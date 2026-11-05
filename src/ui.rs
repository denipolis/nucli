use std::rc::Rc;

use gpui::{App, Global, Hsla, SharedString, Window};

use crate::settings::{self, Appearance, Settings};
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

pub fn label(text: impl Into<SharedString>) -> SharedString {
    text.into()
}

pub fn caps(text: &str) -> SharedString {
    text.to_uppercase().into()
}

pub fn mix(a: Hsla, b: Hsla, amount: f32) -> Hsla {
    let a = a.to_rgb();
    let b = b.to_rgb();
    gpui::Rgba {
        r: a.r + (b.r - a.r) * amount,
        g: a.g + (b.g - a.g) * amount,
        b: a.b + (b.b - a.b) * amount,
        a: a.a + (b.a - a.a) * amount,
    }
    .into()
}
