mod board;
mod computer;
mod computers;
mod device;
mod devtools;
mod field;
mod folders;
mod home;
mod link;
mod menus;
mod mirror;
mod nearby;
mod pair;
mod picker;
mod prefs;
mod project;
mod scanner;
mod settings;
mod sheet;
mod swipe;
mod terminal;
mod transfers;
mod ui;
mod viewer;
mod welcome;
mod workspace;

use futures::StreamExt;
use gpui::{App, AppContext, WindowBounds, WindowOptions};

fn main() {
    let application = gpui_platform::application();
    let (opened, mut urls) = futures::channel::mpsc::unbounded::<Vec<String>>();
    application.on_open_urls(move |list| {
        let _ = opened.unbounded_send(list);
    });
    application.run(|cx: &mut App| {
        if let Err(error) = cx.text_system().add_fonts(nucli_core::fonts::bundled()) {
            eprintln!("fonts: {error}");
        }
        let prefs = prefs::load();
        let link = link::start(prefs.name.clone(), cx);
        cx.set_global(prefs);
        let options = WindowOptions { window_bounds: bounds(cx), ..Default::default() };
        let window = cx.open_window(options, |window, cx| {
            let home = cx.new(|cx| home::Home::new(link, window, cx));
            devtools::start(home.clone(), window, cx);
            home
        });
        match window {
            Ok(window) => {
                cx.spawn(async move |cx| {
                    while let Some(list) = urls.next().await {
                        let _ = window.update(cx, |home, _, cx| {
                            if let Some(url) = list.first() {
                                home.open_url(url, cx);
                            }
                        });
                    }
                })
                .detach();
            }
            Err(error) => eprintln!("window: {error}"),
        }
        cx.activate(true);
    });
}

#[cfg(target_os = "ios")]
fn bounds(_: &App) -> Option<WindowBounds> {
    None
}

#[cfg(not(target_os = "ios"))]
fn bounds(cx: &App) -> Option<WindowBounds> {
    Some(WindowBounds::Windowed(gpui::Bounds::centered(None, gpui::size(gpui::px(390.0), gpui::px(844.0)), cx)))
}
