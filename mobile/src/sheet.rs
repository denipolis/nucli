use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnimationExt, AnyElement, ClipboardItem, Context, Focusable, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px, relative,
};
use nucli_core::theme::{ItemColor, Theme};

use crate::home::Home;
use crate::ui;

pub type Run = Rc<dyn Fn(&mut Home, &mut Window, &mut Context<Home>)>;
pub type Answer = Rc<dyn Fn(String, &mut Home, &mut Window, &mut Context<Home>)>;
pub type Paint = Rc<dyn Fn(Option<ItemColor>, &mut Home, &mut Window, &mut Context<Home>)>;

pub enum Item {
    Action { label: String, danger: bool, run: Run },
    Header(String),
    Colors { current: Option<ItemColor>, run: Paint },
}

pub enum Sheet {
    Menu { title: String, items: Vec<Item> },
    Confirm { title: String, detail: String, button: String, run: Run },
    Ask { title: String, run: Answer },
}

pub fn action(label: impl Into<String>, run: impl Fn(&mut Home, &mut Window, &mut Context<Home>) + 'static) -> Item {
    Item::Action { label: label.into(), danger: false, run: Rc::new(run) }
}

pub fn copy(label: &str, text: impl Into<String>) -> Item {
    let text = text.into();
    action(label, move |home, _, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        home.say("copied");
    })
}

pub fn ask(
    label: &str,
    title: impl Into<String>,
    value: impl Into<String>,
    run: impl Fn(String, &mut Home, &mut Window, &mut Context<Home>) + 'static,
) -> Item {
    let (title, value, run): (String, String, Answer) = (title.into(), value.into(), Rc::new(run));
    action(format!("{label}…"), move |home, window, cx| {
        home.ask(title.clone(), value.clone(), run.clone(), window, cx)
    })
}

pub fn confirm(
    label: &str,
    title: impl Into<String>,
    detail: impl Into<String>,
    button: &str,
    run: impl Fn(&mut Home, &mut Window, &mut Context<Home>) + 'static,
) -> Item {
    let (title, detail, button, run): (String, String, String, Run) =
        (title.into(), detail.into(), button.into(), Rc::new(run));
    Item::Action {
        label: format!("{label}…"),
        danger: true,
        run: Rc::new(move |home, _, cx| {
            home.sheet = Some(Sheet::Confirm {
                title: title.clone(),
                detail: detail.clone(),
                button: button.clone(),
                run: run.clone(),
            });
            cx.notify();
        }),
    }
}

impl Home {
    pub fn ask(&mut self, title: String, value: String, run: Answer, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |field, cx| field.set(value, cx));
        self.sheet = Some(Sheet::Ask { title, run });
        window.focus(&self.input.focus_handle(cx), cx);
        cx.notify();
    }

    pub fn answer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Sheet::Ask { run, .. }) = self.sheet.take() else { return };
        let text = self.input.read(cx).text.trim().to_string();
        window.blur(cx);
        if !text.is_empty() {
            run(text, self, window, cx);
        }
        cx.notify();
    }

    fn close_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sheet = None;
        window.blur(cx);
        cx.notify();
    }

    pub fn sheet_view(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sheet = self.sheet.as_ref()?;
        let (title, mut body) = match sheet {
            Sheet::Menu { title, items } => (title.clone(), self.items(items, theme, cx)),
            Sheet::Confirm { title, detail, button, .. } => (
                title.clone(),
                div().flex().flex_col().child(ui::hint(detail.clone(), theme)).child(
                    ui::row("sheet-confirm", theme).text_color(theme.red).child(button.clone()).on_click(cx.listener(
                        |home, _, window, cx| {
                            if let Some(Sheet::Confirm { run, .. }) = home.sheet.take() {
                                run(home, window, cx);
                            }
                            cx.notify();
                        },
                    )),
                ),
            ),
            Sheet::Ask { title, .. } => (
                title.clone(),
                div().flex().flex_col().gap(px(10.0)).p(px(16.0)).child(self.input.clone()).child(
                    ui::primary("sheet-ok", "ok", theme)
                        .on_click(cx.listener(|home, _, window, cx| home.answer(window, cx))),
                ),
            ),
        };
        body = body.child(
            ui::row("sheet-cancel", theme)
                .text_color(theme.muted)
                .child("cancel")
                .on_click(cx.listener(|home, _, window, cx| home.close_sheet(window, cx))),
        );
        let dim = theme.background;
        let panel = div()
            .track_focus(&self.sheet_focus)
            .flex()
            .flex_col()
            .max_h(relative(0.8))
            .bg(theme.background)
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_none()
                    .px(px(16.0))
                    .pt(px(14.0))
                    .pb(px(8.0))
                    .text_size(px(ui::SMALL))
                    .text_color(theme.faint)
                    .truncate()
                    .child(title),
            )
            .child(div().id("sheet").flex_1().min_h_0().overflow_y_scroll().child(body))
            .with_animation("sheet-rise", ui::motion(), |this, delta| {
                this.relative().top(px((1.0 - delta) * ui::SHIFT * 2.0)).opacity(delta)
            });
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .occlude()
                .child(
                    div()
                        .id("sheet-backdrop")
                        .flex_1()
                        .on_click(cx.listener(|home, _, window, cx| home.close_sheet(window, cx)))
                        .with_animation("sheet-dim", ui::motion(), move |this, delta| {
                            this.bg(dim.opacity(0.7 * delta))
                        }),
                )
                .child(panel)
                .into_any_element(),
        )
    }

    fn items(&self, items: &[Item], theme: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let mut list = div().flex().flex_col();
        for (index, item) in items.iter().enumerate() {
            list = match item {
                Item::Header(text) => list.child(ui::section(text.clone(), theme)),
                Item::Action { label, danger, run } => {
                    let run = run.clone();
                    list.child(
                        ui::row(SharedString::from(format!("sheet-item-{index}")), theme)
                            .when(*danger, |this| this.text_color(theme.red))
                            .child(label.clone())
                            .on_click(cx.listener(move |home, _, window, cx| {
                                home.sheet = None;
                                window.blur(cx);
                                run(home, window, cx);
                                cx.notify();
                            })),
                    )
                }
                Item::Colors { current, run } => {
                    let mut row = div().flex().flex_wrap().gap(px(6.0)).px(px(16.0)).py(px(10.0));
                    let choices = std::iter::once(None).chain(ItemColor::ALL.into_iter().map(Some));
                    for (choice, color) in choices.enumerate() {
                        let run = run.clone();
                        let on = *current == color;
                        row = row.child(
                            div()
                                .id(SharedString::from(format!("sheet-color-{choice}")))
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(32.0))
                                .border_1()
                                .border_color(if on { theme.text } else { theme.border })
                                .when_some(color, |this, color| this.bg(color.color(theme.dark)))
                                .when(color.is_none(), |this| this.text_color(theme.faint).child("×"))
                                .on_click(cx.listener(move |home, _, window, cx| {
                                    home.sheet = None;
                                    run(color, home, window, cx);
                                    cx.notify();
                                })),
                        );
                    }
                    list.child(row.border_b_1().border_color(theme.border))
                }
            };
        }
        list
    }
}
