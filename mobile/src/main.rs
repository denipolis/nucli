mod devtools;
mod home;
mod link;
mod mirror;

use gpui::{App, AppContext, Window, WindowAppearance, WindowBounds, WindowOptions};
use nucli_core::theme::{self, Theme};

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        if let Err(error) = cx.text_system().add_fonts(nucli_core::fonts::bundled()) {
            eprintln!("fonts: {error}");
        }
        let link = link::start(cx);
        let options = WindowOptions { window_bounds: bounds(cx), ..Default::default() };
        if let Err(error) = cx.open_window(options, |window, cx| {
            let home = cx.new(|cx| home::Home::new(link, window, cx));
            devtools::start(home.clone(), window, cx);
            home
        }) {
            eprintln!("window: {error}");
        }
        cx.activate(true);
    });
}

pub fn theme(window: &Window) -> Theme {
    let dark = matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark);
    status_bar(dark);
    theme::resolve("nucli", dark, &[])
}

#[cfg(target_os = "ios")]
fn status_bar(dark: bool) {
    use gpui_ios::StatusBarContentStyle::{Dark, Light};
    gpui_ios::ios::set_status_bar_style(if dark { Light } else { Dark });
}

#[cfg(not(target_os = "ios"))]
fn status_bar(_: bool) {}

#[cfg(target_os = "ios")]
fn bounds(_: &App) -> Option<WindowBounds> {
    None
}

#[cfg(not(target_os = "ios"))]
fn bounds(cx: &App) -> Option<WindowBounds> {
    Some(WindowBounds::Windowed(gpui::Bounds::centered(None, gpui::size(gpui::px(390.0), gpui::px(844.0)), cx)))
}
