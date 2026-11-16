use gpui::prelude::FluentBuilder;
use gpui::{
    Animation, App, Div, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, WindowAppearance, div, px,
};
use nucli_core::icons;
use nucli_core::theme::{self, Theme};

use crate::prefs::{self, Appearance};

pub const TEXT: f32 = 15.0;
pub const SMALL: f32 = 12.0;
pub const ROW: f32 = 48.0;
pub const SPINNER_MS: u128 = 110;
pub const SHIFT: f32 = 24.0;

pub fn theme(window: &Window, cx: &App) -> Theme {
    let prefs = prefs::get(cx);
    let dark = match prefs.appearance {
        Appearance::Dark => true,
        Appearance::Light => false,
        Appearance::System => matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark),
    };
    let theme = theme::resolve(&prefs.theme, dark, &[]);
    status_bar(theme.dark);
    theme
}

#[cfg(target_os = "ios")]
fn status_bar(dark: bool) {
    use gpui_ios::StatusBarContentStyle::{Dark, Light};
    gpui_ios::ios::set_status_bar_style(if dark { Light } else { Dark });
}

#[cfg(not(target_os = "ios"))]
fn status_bar(_: bool) {}

pub fn spinner(time: u128) -> &'static str {
    icons::SPINNER[(time / SPINNER_MS) as usize % icons::SPINNER.len()]
}

pub fn bar(theme: &Theme) -> Div {
    div().flex().flex_none().items_center().gap(px(4.0)).h(px(ROW)).px(px(6.0)).border_b_1().border_color(theme.border)
}

pub fn title(text: impl Into<SharedString>) -> Div {
    div().flex_1().min_w_0().px(px(6.0)).truncate().child(text.into())
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(40.0))
        .h(px(40.0))
        .px(px(8.0))
        .text_color(theme.accent)
        .active(|style| style.bg(theme.hover))
        .child(label.into())
}

pub fn key(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(34.0))
        .h(px(40.0))
        .px(px(6.0))
        .text_size(px(14.0))
        .text_color(theme.accent)
        .active(|style| style.bg(theme.hover))
        .child(label.into())
}

pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(ROW))
        .px(px(16.0))
        .bg(theme.accent)
        .text_color(theme.on_accent)
        .active(|style| style.bg(theme.text).text_color(theme.background))
        .child(label.into())
}

pub fn secondary(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(ROW))
        .px(px(16.0))
        .border_1()
        .border_color(theme.border)
        .text_color(theme.accent)
        .active(|style| style.bg(theme.hover))
        .child(label.into())
}

pub fn section(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(16.0))
        .pt(px(20.0))
        .pb(px(6.0))
        .text_size(px(SMALL))
        .text_color(theme.faint)
        .child(text.into())
        .child(div().flex_1().h(px(1.0)).bg(theme.border))
}

pub fn row(id: impl Into<ElementId>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(12.0))
        .min_h(px(ROW))
        .px(px(16.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(theme.border)
        .active(|style| style.bg(theme.hover))
}

pub fn hint(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div().px(px(16.0)).py(px(8.0)).text_size(px(SMALL)).text_color(theme.muted).child(text.into())
}

pub fn choice(on: bool, theme: &Theme) -> Div {
    div().flex_none().text_color(if on { theme.accent } else { theme.faint }).child(if on { "[x]" } else { "[ ]" })
}

pub fn tabs<const N: usize>(
    labels: [(&'static str, &'static str); N],
    active: usize,
    theme: &Theme,
) -> [Stateful<Div>; N] {
    let mut index = 0;
    labels.map(|(glyph, label)| {
        let on = index == active;
        index += 1;
        div()
            .id(label)
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(32.0))
            .px(px(10.0))
            .text_color(if on { theme.on_accent } else { theme.muted })
            .when(on, |this| this.bg(theme.accent))
            .child(glyph)
            .child(label)
    })
}

pub fn label_value(label: &'static str, value: impl IntoElement, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .min_h(px(ROW))
        .px(px(16.0))
        .border_b_1()
        .border_color(theme.border)
        .child(div().flex_none().w(px(110.0)).text_color(theme.muted).child(label))
        .child(div().flex_1().min_w_0().child(value))
}

pub fn count(number: usize, noun: &str) -> String {
    if number == 1 { format!("1 {noun}") } else { format!("{number} {noun}s") }
}

pub fn motion() -> Animation {
    Animation::new(std::time::Duration::from_millis(220)).with_easing(gpui::ease_out_quint())
}
