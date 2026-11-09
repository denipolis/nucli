use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, ScrollStrategy,
    StatefulInteractiveElement, Styled, Subscription, UniformListScrollHandle, Window, deferred, div, px, relative,
    uniform_list,
};

use crate::icons;
use crate::settings::{self, FONT};
use crate::text_input::{InputEvent, TextInput};
use crate::ui;

const VISIBLE_ROWS: usize = 10;

pub struct FontPicker {
    input: Entity<TextInput>,
    fonts: Vec<String>,
    matches: Vec<usize>,
    open: bool,
    previous: Option<gpui::FocusHandle>,
    highlighted: usize,
    scroll: UniformListScrollHandle,
    _subscription: Subscription,
}

impl FontPicker {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new("search fonts", cx));
        let subscription = cx.subscribe_in(&input, window, |picker, _, event, window, cx| match event {
            InputEvent::Changed => picker.filter(cx),
            InputEvent::Up => picker.step(-1, cx),
            InputEvent::Down => picker.step(1, cx),
            InputEvent::Submit => {
                if let Some(&index) = picker.matches.get(picker.highlighted) {
                    picker.pick(index, window, cx);
                }
            }
            InputEvent::Cancel => picker.close(window, cx),
            InputEvent::NextField => {}
        });
        let mut names = cx.text_system().all_font_names();
        names.retain(|name| !name.starts_with('.') && name != FONT);
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        let fonts: Vec<String> = std::iter::once(FONT.to_string()).chain(names).collect();
        FontPicker {
            input,
            matches: (0..fonts.len()).collect(),
            fonts,
            open: false,
            previous: None,
            highlighted: 0,
            scroll: UniformListScrollHandle::new(),
            _subscription: subscription,
        }
    }

    fn filter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text.trim().to_lowercase();
        self.matches =
            (0..self.fonts.len()).filter(|&index| self.fonts[index].to_lowercase().contains(&query)).collect();
        self.highlighted = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        self.highlighted = (self.highlighted as isize + delta).rem_euclid(self.matches.len() as isize) as usize;
        self.scroll.scroll_to_item(self.highlighted, ScrollStrategy::Center);
        cx.notify();
    }

    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.previous = window.focused(cx);
        self.input.update(cx, |input, cx| input.clear(cx));
        self.filter(cx);
        let current = settings::get(cx).font.clone();
        self.highlighted = self.matches.iter().position(|&index| self.fonts[index] == current).unwrap_or(0);
        self.scroll.scroll_to_item(self.highlighted, ScrollStrategy::Center);
        self.input.read(cx).focus(window);
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        match self.previous.take() {
            Some(focus) => window.focus(&focus),
            None => window.blur(),
        }
        cx.notify();
    }

    fn pick(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let family = self.fonts[index].clone();
        settings::update(cx, |settings| settings.font = family);
        self.close(window, cx);
    }
}

impl Render for FontPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let current = settings::get(cx).font.clone();
        let line = px((settings::get(cx).ui_font_size() * 1.5).round() + 4.0);
        let field = div()
            .id("font-field")
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .bg(theme.background)
            .border_1()
            .border_color(if self.open { theme.accent } else { theme.border })
            .hover(|style| style.border_color(theme.muted))
            .child(if self.open {
                div().flex_1().min_w_0().child(self.input.clone())
            } else {
                div().flex_1().min_w_0().truncate().child(current.clone())
            })
            .child(div().flex_none().text_color(theme.muted).child(icons::CHEVRON_DOWN))
            .capture_any_mouse_down(cx.listener(|picker, _, window, cx| {
                if picker.open {
                    picker.close(window, cx);
                } else {
                    picker.show(window, cx);
                }
                cx.stop_propagation();
            }));
        let count = self.matches.len();
        let list = uniform_list(
            "font-list",
            count,
            cx.processor(move |picker: &mut FontPicker, range: std::ops::Range<usize>, _, cx| {
                let theme = ui::theme(cx);
                let current = settings::get(cx).font.clone();
                range
                    .map(|row| {
                        let index = picker.matches[row];
                        let name = picker.fonts[index].clone();
                        let active = row == picker.highlighted;
                        div()
                            .id(("font-row", row))
                            .h(line)
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(8.0))
                            .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
                            .when(!active, |this| this.hover(|style| style.bg(theme.hover)))
                            .child(div().flex_none().w(px(14.0)).when(name == current, |this| this.child(icons::CHECK)))
                            .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                            .child(
                                div()
                                    .flex_none()
                                    .font_family(name)
                                    .text_color(if active { theme.on_accent } else { theme.muted })
                                    .child("Aa 0O1lI {}"),
                            )
                            .on_click(cx.listener(move |picker, _, window, cx| picker.pick(index, window, cx)))
                    })
                    .collect()
            }),
        )
        .track_scroll(self.scroll.clone())
        .h(line * count.clamp(1, VISIBLE_ROWS) as f32);
        let dropdown = div()
            .id("font-dropdown")
            .absolute()
            .top(relative(1.0))
            .left_0()
            .w_full()
            .mt(px(2.0))
            .occlude()
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .on_mouse_down_out(cx.listener(|picker, _, window, cx| picker.close(window, cx)))
            .when(count == 0, |this| this.child(div().px(px(8.0)).text_color(theme.faint).child("no fonts match")))
            .when(count > 0, |this| this.child(list));
        div().relative().w_full().child(field).when(self.open, |this| this.child(deferred(dropdown).with_priority(2)))
    }
}
