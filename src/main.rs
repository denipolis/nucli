#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agents;
mod claude;
mod commands;
#[cfg(debug_assertions)]
mod devtools;
mod git;
mod highlight;
mod history;
mod icons;
mod paths;
mod process;
mod session;
mod settings;
mod terminal;
mod theme;
mod ui;
mod window;

use std::borrow::Cow;

use gpui::{App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size};

use crate::commands::Quit;
use crate::settings::Settings;

fn load_fonts(cx: &mut App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/MapleMono-NF-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/MapleMono-NF-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/MapleMono-NF-Italic.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/MapleMono-NF-BoldItalic.ttf")),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("fonts: {error}");
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        load_fonts(cx);
        let settings = Settings::read().ok().flatten().unwrap_or_default();
        cx.set_global(settings);
        ui::init(cx);
        commands::apply(cx, Vec::new());
        if cfg!(target_os = "macos") {
            cx.set_menus(commands::menus());
        }
        cx.on_action(|_: &Quit, cx| cx.quit());
        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(paths::app_name().into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(10.0), px(10.0))),
            }),
            app_id: Some("uk.polishch.nucli".into()),
            window_min_size: Some(size(px(480.0), px(320.0))),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| cx.new(|cx| window::WorkspaceView::new(window, cx))).ok();
        #[cfg(debug_assertions)]
        devtools::start(cx);
        cx.activate(true);
    });
}
