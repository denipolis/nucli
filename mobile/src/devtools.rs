use std::time::Duration;

use gpui::{
    App, Entity, KeyUpEvent, Keystroke, LongPressEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    OperatingSystem, PlatformInput, TouchEvent, TouchId, TouchPhase, Window, point, px,
};

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
                if let Some(points) = line.strip_prefix("fling ") {
                    let numbers: Vec<f32> = points.split(' ').filter_map(|part| part.parse().ok()).collect();
                    let [x, y, distance] = numbers[..] else { continue };
                    for step in 0..=6 {
                        let phase = match step {
                            0 => TouchPhase::Started,
                            6 => TouchPhase::Ended,
                            _ => TouchPhase::Moved,
                        };
                        let position = point(px(x), px(y + distance * step.min(5) as f32 / 5.0));
                        let touch =
                            TouchEvent { id: TouchId(1), phase, position, predicted_position: None, force: None };
                        let _ =
                            handle.update(cx, |_, window, cx| window.dispatch_event(PlatformInput::Touch(touch), cx));
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                    }
                    continue;
                }
                let home = home.clone();
                let _ = handle.update(cx, |_, window, cx| {
                    if let Some(point) = line.strip_prefix("tap ") {
                        tap(point, window, cx);
                    } else if let Some(point) = line.strip_prefix("hold ") {
                        hold(point, window, cx);
                    } else if let Some(keys) = line.strip_prefix("press ") {
                        press(keys, window, cx);
                    } else {
                        home.update(cx, |home, cx| home.control(&line, window, cx));
                    }
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

fn hold(text: &str, window: &mut Window, cx: &mut App) {
    let mut numbers = text.split(' ').filter_map(|part| part.parse::<f32>().ok());
    let (Some(x), Some(y)) = (numbers.next(), numbers.next()) else { return };
    let position = point(px(x), px(y));
    for phase in [TouchPhase::Started, TouchPhase::Ended] {
        let event = LongPressEvent { phase, start_position: position, position };
        window.dispatch_event(PlatformInput::LongPress(event), cx);
    }
}

fn press(keys: &str, window: &mut Window, cx: &mut App) {
    for text in keys.split(' ') {
        let Ok(keystroke) = Keystroke::parse_with_os(text, OperatingSystem::Mac) else { continue };
        window.dispatch_keystroke(keystroke.clone(), cx);
        window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke }), cx);
    }
}
