#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod actions;
mod agents;
mod changes;
mod claude;
mod commands;
mod commits;
#[cfg(debug_assertions)]
mod devtools;
mod file_view;
mod files;
mod fuzzy;
mod git;
mod highlight;
mod history;
mod icons;
mod menu;
mod notify;
mod overview;
mod palette;
mod palettes;
mod panel;
mod paths;
mod persist;
mod process;
mod resize;
mod session;
mod settings;
mod sidebar;
mod splits;
mod splits_view;
mod status;
mod statusbar;
mod store;
mod switcher;
mod terminal;
mod text_input;
mod theme;
mod ui;
mod window;

use std::borrow::Cow;

use gpui::{App, Application};

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

fn first_launch(cx: &mut App) -> Settings {
    match Settings::read() {
        Ok(Some(settings)) => settings,
        Ok(None) => {
            let mut settings = Settings::default();
            settings.agents = agents::detect_agents();
            settings.shells = agents::detect_shells();
            settings.write();
            settings
        }
        Err(error) => {
            cx.set_global(settings::SettingsError(Some(error)));
            Settings::default()
        }
    }
}

fn watch_config(cx: &mut App) {
    cx.spawn(async move |cx| {
        let modified = |path: std::path::PathBuf| std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
        let mut settings_time = modified(paths::settings_file());
        let mut keys_time = modified(paths::keybindings_file());
        loop {
            cx.background_executor().timer(std::time::Duration::from_millis(700)).await;
            let settings_now = modified(paths::settings_file());
            let keys_now = modified(paths::keybindings_file());
            if settings_now != settings_time {
                settings_time = settings_now;
                let _ = cx.update(settings::reload);
            }
            if keys_now != keys_time {
                keys_time = keys_now;
                let _ = cx.update(apply_keys);
            }
        }
    })
    .detach();
}

fn apply_keys(cx: &mut App) {
    commands::apply(cx, text_input::bindings());
}

fn main() {
    Application::new().run(|cx: &mut App| {
        load_fonts(cx);
        let settings = first_launch(cx);
        cx.set_global(settings);
        ui::init(cx);
        apply_keys(cx);
        if cfg!(target_os = "macos") {
            cx.set_menus(commands::menus());
        }
        cx.on_action(|_: &Quit, cx| cx.quit());
        let store = store::init(cx);
        store.update(cx, |store, cx| store.start_polling(cx));
        cx.on_app_quit(|cx| {
            let state = store::store(cx).read(cx).snapshot(true, cx);
            state.write();
            let sessions: Vec<_> = store::store(cx).read(cx).sessions.values().cloned().collect();
            for session in sessions {
                session.update(cx, |session, _| session.kill());
            }
            async {}
        })
        .detach();
        let mut open: Vec<String> = store
            .read(cx)
            .workspaces
            .iter()
            .filter(|workspace| !workspace.closed)
            .map(|workspace| workspace.id.clone())
            .collect();
        if open.is_empty() {
            let last = store.read(cx).last_workspace.clone();
            let fallback = store.read(cx).workspaces.first().map(|workspace| workspace.id.clone());
            open.extend(last.or(fallback));
        }
        let last = store.read(cx).last_workspace.clone();
        open.sort_by_key(|id| Some(id) == last.as_ref());
        for id in open {
            window::open(&id, cx);
        }
        watch_config(cx);
        #[cfg(debug_assertions)]
        devtools::start(cx);
        cx.activate(true);
    });
}
