use std::time::Duration;

use gpui::{App, Keystroke, Modifiers};

use crate::commands;

pub fn start(cx: &mut App) {
    let Some(path) = std::env::var_os("NUCLI_CONTROL") else { return };
    cx.spawn(async move |cx| {
        let mut done = 0;
        loop {
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let lines: Vec<String> = text.lines().map(String::from).collect();
            for line in lines.iter().skip(done) {
                let line = line.clone();
                let _ = cx.update(|cx| run(&line, cx));
                cx.background_executor().timer(Duration::from_millis(60)).await;
            }
            done = lines.len().max(done);
        }
    })
    .detach();
}

fn run(line: &str, cx: &mut App) {
    let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
    let Some(window) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
    let _ = window.update(cx, |_, window, cx| match command {
        "keys" => {
            for key in rest.split_whitespace() {
                if let Ok(keystroke) = Keystroke::parse(key) {
                    window.dispatch_keystroke(keystroke, cx);
                }
            }
        }
        "type" => {
            for character in rest.chars() {
                let key = character.to_string();
                let keystroke = Keystroke { modifiers: Modifiers::default(), key: key.clone(), key_char: Some(key) };
                window.dispatch_keystroke(keystroke, cx);
            }
        }
        "capture" => {
            let mut parts = rest.split_whitespace();
            if let (Some(id), Some(path)) = (parts.next().and_then(|id| id.parse().ok()), parts.next()) {
                capture(id, path);
            }
        }
        "action" => {
            if let Some(command) = commands::find(rest.trim()) {
                window.dispatch_action((command.action)(), cx);
            }
        }
        _ => {}
    });
}

#[cfg(target_os = "macos")]
fn capture(id: u32, path: &str) {
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    use core_graphics::window::{create_image, kCGWindowImageBoundsIgnoreFraming, kCGWindowListOptionIncludingWindow};
    let Some(image) =
        create_image(
            CGRect::new(&CGPoint::new(f64::INFINITY, f64::INFINITY), &CGSize::new(0.0, 0.0)),
            kCGWindowListOptionIncludingWindow, id, kCGWindowImageBoundsIgnoreFraming)
    else {
        eprintln!("capture failed");
        return;
    };
    let (width, height, stride) = (image.width(), image.height(), image.bytes_per_row());
    let data = image.data();
    let bytes = data.bytes();
    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        for column in 0..width {
            let offset = row * stride + column * 4;
            pixels.extend_from_slice(&[bytes[offset + 2], bytes[offset + 1], bytes[offset], 255]);
        }
    }
    if let Some(buffer) = image::RgbaImage::from_raw(width as u32, height as u32, pixels) {
        let _ = buffer.save(path);
    }
}

#[cfg(not(target_os = "macos"))]
fn capture(_id: u32, _path: &str) {}
