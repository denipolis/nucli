use std::time::Duration;

use gpui::{App, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, Window, point, px};

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
                let _ = handle.update(cx, |_, window, cx| match line.strip_prefix("tap ") {
                    Some(point) => tap(point, window, cx),
                    None => home.update(cx, |home, cx| home.control(&line, window, cx)),
                });
            }
        }
    })
    .detach();
}

fn tap(text: &str, window: &mut Window, cx: &mut App) {
    let mut numbers = text.split(' ').filter_map(|part| part.parse::<f32>().ok());
    let (Some(x), Some(y)) = (numbers.next(), numbers.next()) else { return };
    let position = point(px(x), px(y));
    let button = MouseButton::Left;
    for event in [
        PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button, position, click_count: 1, ..Default::default() }),
        PlatformInput::MouseUp(MouseUpEvent { button, position, click_count: 1, ..Default::default() }),
    ] {
        window.dispatch_event(event, cx);
    }
}
