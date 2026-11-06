use std::rc::Rc;

use gpui::{
    AnyView, App, AppContext, Context, Global, IntoElement, ParentElement, Render, SharedString, Styled, Window, div,
    px,
};

use crate::settings::{self, Appearance, FONT, Settings};
use crate::theme::{self, Theme};

pub struct ActiveTheme(pub Rc<Theme>);

impl Global for ActiveTheme {}

pub fn init(cx: &mut App) {
    refresh_theme(cx);
    cx.observe_global::<Settings>(refresh_theme).detach();
}

pub fn system_dark(cx: &App) -> bool {
    match settings::get(cx).appearance {
        Appearance::Dark => true,
        Appearance::Light => false,
        Appearance::System => {
            matches!(cx.window_appearance(), gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark)
        }
    }
}

pub fn refresh_theme(cx: &mut App) {
    let id = settings::get(cx).theme.clone();
    let theme = theme::resolve(&id, system_dark(cx));
    let changed = cx.try_global::<ActiveTheme>().is_none_or(|active| active.0.id != theme.id);
    if changed {
        cx.set_global(ActiveTheme(Rc::new(theme)));
        cx.refresh_windows();
    }
}

pub fn theme(cx: &App) -> Rc<Theme> {
    cx.global::<ActiveTheme>().0.clone()
}

pub fn watch_appearance<T: 'static>(window: &mut Window, cx: &mut gpui::Context<T>) {
    cx.observe_window_appearance(window, |_, _, cx| refresh_theme(cx)).detach();
}

pub fn title(text: &str) -> SharedString {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect::<String>().into(),
        None => SharedString::default(),
    }
}

struct Tooltip(SharedString);

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme(cx);
        let size = settings::get(cx).ui_font_size();
        div()
            .font_family(FONT)
            .text_size(px(size))
            .line_height(px((size * 1.5).round()))
            .px(px(6.0))
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border)
            .text_color(theme.text)
            .child(self.0.clone())
    }
}

pub fn tooltip(text: &'static str) -> impl Fn(&mut Window, &mut App) -> AnyView {
    move |_, cx| cx.new(|_| Tooltip(text.into())).into()
}
