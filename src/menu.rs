use std::rc::Rc;

use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, SharedString,
    StatefulInteractiveElement, Styled, Window, anchored, deferred, div, px,
};

use crate::theme::ItemColor;
use crate::ui;
use crate::window::WorkspaceView;

pub type Run = Rc<dyn Fn(&mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>)>;

pub type ColorRun = Rc<dyn Fn(Option<ItemColor>, &mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>)>;

#[derive(Clone)]
pub enum Item {
    Action { label: SharedString, key: String, run: Run },
    Colors { current: Option<ItemColor>, run: ColorRun },
    Header(SharedString),
    Separator,
}

pub fn action(
    label: impl Into<SharedString>,
    run: impl Fn(&mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>) + 'static,
) -> Item {
    Item::Action { label: label.into(), key: String::new(), run: Rc::new(run) }
}

pub fn command(label: impl Into<SharedString>, name: &str, cx: &App) -> Item {
    let command = crate::commands::find(name);
    let key = crate::commands::key_for(name, cx);
    Item::Action {
        label: label.into(),
        key,
        run: Rc::new(move |_, window, cx| {
            if let Some(command) = command {
                window.dispatch_action((command.action)(), cx);
            }
        }),
    }
}

pub fn from_commands(names: &[&str], cx: &App) -> Vec<Item> {
    names
        .iter()
        .map(|name| match crate::commands::find(name) {
            Some(found) => command(found.title, name, cx),
            None => Item::Separator,
        })
        .collect()
}

pub struct Menu {
    pub position: Point<Pixels>,
    pub items: Vec<Item>,
}

pub fn render(menu: &Menu, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let mut list = div()
        .id("menu")
        .min_w(px(220.0))
        .py(px(4.0))
        .bg(theme.elevated)
        .border_1()
        .border_color(theme.border)
        .occlude()
        .on_mouse_down_out(cx.listener(|view, _, _, cx| {
            view.menu = None;
            cx.notify();
        }));
    for (index, item) in menu.items.iter().enumerate() {
        let element: AnyElement = match item {
            Item::Action { label, key, run } => {
                let run = run.clone();
                div()
                    .id(("menu-item", index))
                    .flex()
                    .justify_between()
                    .gap(px(24.0))
                    .px(px(10.0))
                    .py(px(2.0))
                    .hover(|style| style.bg(theme.accent).text_color(theme.on_accent))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.menu = None;
                        run(view, window, cx);
                        cx.notify();
                    }))
                    .child(label.clone())
                    .child(div().text_color(theme.faint).child(key.clone()))
                    .into_any_element()
            }
            Item::Colors { current, run } => {
                let mut row = div().flex().gap(px(4.0)).px(px(10.0)).py(px(4.0));
                let none_run = run.clone();
                row = row.child(
                    div()
                        .id(("color-none", index))
                        .w(px(14.0))
                        .h(px(14.0))
                        .border_1()
                        .border_color(if current.is_none() { theme.text } else { theme.border })
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.0))
                        .child("×")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.menu = None;
                            none_run(None, view, window, cx);
                        })),
                );
                for color in ItemColor::ALL {
                    let run = run.clone();
                    let selected = *current == Some(color);
                    row = row.child(
                        div()
                            .id(SharedString::from(format!("color-{}", color.title())))
                            .w(px(14.0))
                            .h(px(14.0))
                            .bg(color.color(theme.dark))
                            .border_1()
                            .border_color(if selected { theme.text } else { color.color(theme.dark) })
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.menu = None;
                                run(Some(color), view, window, cx);
                            })),
                    );
                }
                row.into_any_element()
            }
            Item::Header(label) => {
                div().px(px(10.0)).pt(px(4.0)).text_color(theme.faint).child(ui::title(label)).into_any_element()
            }
            Item::Separator => div().my(px(4.0)).h(px(1.0)).bg(theme.border).into_any_element(),
        };
        list = list.child(element);
    }
    deferred(anchored().position(menu.position).snap_to_window_with_margin(px(8.0)).child(list))
        .with_priority(2)
        .into_any_element()
}
