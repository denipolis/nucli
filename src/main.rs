#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod actions;
mod agents;
mod changes;
mod claude;
mod commands;
mod commits;
#[cfg(debug_assertions)]
mod devtools;
mod editor;
mod file_view;
mod files;
mod find;
mod fuzzy;
mod git;
mod highlight;
mod history;
mod icons;
#[cfg(target_os = "macos")]
mod import;
mod menu;
mod notify;
mod overview;
mod palette;
mod palettes;
mod panel;
mod paths;
mod persist;
mod process;
mod prompt;
mod resize;
mod session;
mod settings;
mod settings_view;
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
mod titlebar;
mod ui;
mod watcher;
mod window;

use std::borrow::Cow;

use gpui::{App, Application};

use crate::commands::Quit;
use crate::settings::Settings;

fn start_application() -> Application {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let started = std::panic::catch_unwind(Application::new);
    std::panic::set_hook(hook);
    started.unwrap_or_else(|panic| {
        let reason = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("unknown error");
        let reason = reason.rsplit("value: ").next().unwrap_or(reason);
        let text = format!("Nucli could not start. Check your graphics drivers.\n\n{reason}");
        eprintln!("{text}");
        notify::alert(&text);
        std::process::exit(1);
    })
}

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
            let settings = imported().unwrap_or_else(|| Settings {
                agents: agents::detect_agents(),
                shells: agents::detect_shells(),
                ..Settings::default()
            });
            settings.write();
            settings
        }
        Err(error) => {
            cx.set_global(settings::SettingsError(Some(error)));
            Settings::default()
        }
    }
}

#[cfg(target_os = "macos")]
fn imported() -> Option<Settings> {
    let (mut settings, state) = import::read()?;
    if settings.agents.is_empty() {
        settings.agents = agents::detect_agents();
    }
    if settings.shells.is_empty() {
        settings.shells = agents::detect_shells();
    }
    if !paths::state_file().exists() {
        state.write();
    }
    Some(settings)
}

#[cfg(not(target_os = "macos"))]
fn imported() -> Option<Settings> {
    None
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
    let mut bindings = text_input::bindings();
    bindings.extend(editor::bindings());
    commands::apply(cx, bindings);
}

fn folder_from_url(url: &str) -> Option<std::path::PathBuf> {
    let path = url.strip_prefix("file://")?;
    let mut decoded = Vec::with_capacity(path.len());
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let escape = (bytes[index] == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match escape {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    Some(std::path::PathBuf::from(String::from_utf8_lossy(&decoded).into_owned()))
}

fn open_folders(folders: Vec<std::path::PathBuf>, cx: &mut App) {
    let store = store::store(cx);
    let target = store
        .read(cx)
        .last_workspace
        .clone()
        .filter(|id| store.read(cx).windows.contains_key(id))
        .or_else(|| store.read(cx).windows.keys().next().cloned());
    let Some(handle) = target.and_then(|id| window::open(&id, cx)) else { return };
    let _ = handle.update(cx, |view, window, cx| {
        window.activate_window();
        for folder in folders.into_iter().filter(|folder| folder.is_dir()) {
            actions::open_folder(view, folder, window, cx);
        }
    });
}

fn main() {
    let application = start_application();
    let (opened, mut opened_events) = futures::channel::mpsc::unbounded::<Vec<std::path::PathBuf>>();
    application.on_open_urls(move |urls| {
        let _ = opened.unbounded_send(urls.iter().filter_map(|url| folder_from_url(url)).collect());
    });
    application.run(|cx: &mut App| {
        load_fonts(cx);
        let settings = first_launch(cx);
        cx.set_global(settings);
        ui::init(cx);
        cx.set_prompt_builder(prompt::build);
        apply_keys(cx);
        if cfg!(target_os = "macos") {
            cx.set_menus(commands::menus());
        }
        cx.on_action(|_: &Quit, cx| cx.defer(actions::request_quit));
        let store = store::init(cx);
        store.update(cx, |store, cx| {
            store.start_polling(cx);
            store.start_watching(cx);
        });
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
        let arguments: Vec<std::path::PathBuf> = std::env::args_os()
            .skip(1)
            .map(std::path::PathBuf::from)
            .filter_map(|path| std::fs::canonicalize(path).ok())
            .collect();
        if !arguments.is_empty() {
            open_folders(arguments, cx);
        }
        cx.spawn(async move |cx| {
            use futures::StreamExt;
            while let Some(folders) = opened_events.next().await {
                let _ = cx.update(|cx| open_folders(folders, cx));
            }
        })
        .detach();
        #[cfg(debug_assertions)]
        devtools::start(cx);
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_folder_urls() {
        assert_eq!(super::folder_from_url("file:///a%20b/c").unwrap(), std::path::PathBuf::from("/a b/c"));
        assert!(super::folder_from_url("https://x").is_none());
    }
}
