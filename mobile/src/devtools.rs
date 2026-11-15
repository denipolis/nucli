use std::time::Duration;

use gpui::{App, Entity, Window};

use crate::home::Home;

pub fn start(home: Entity<Home>, window: &mut Window, cx: &mut App) {
    let Some(path) = std::env::var_os("NUCLI_CONTROL") else { return };
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let mut done = 0;
        loop {
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<String> = text.lines().skip(done).map(String::from).collect();
            done += lines.len();
            for line in lines {
                let home = home.clone();
                let _ = handle.update(cx, |_, window, cx| {
                    home.update(cx, |home, cx| home.control(&line, window, cx));
                });
            }
        }
    })
    .detach();
}
