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
    if command == "open" {
        let handle = crate::store::store(cx).read(cx).windows.values().next().copied();
        if let Some(handle) = handle {
            let path = std::path::PathBuf::from(rest.trim());
            let _ = handle.update(cx, |view, window, cx| crate::actions::open_folder(view, path, window, cx));
        }
        return;
    }
    #[cfg(target_os = "macos")]
    if command == "event" {
        let event = rest.to_string();
        cx.spawn(async move |_| send_event(&event)).detach();
        return;
    }
    #[cfg(target_os = "macos")]
    if command == "menu" {
        let path = rest.trim().to_string();
        inspect_app(&path, false);
        cx.spawn(async move |_| inspect_app(&path, true)).detach();
        return;
    }
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
        "dump" => {
            let state = crate::store::store(cx).read(cx).snapshot(false, cx);
            eprintln!("DUMP {}", serde_json::to_string(&state).unwrap_or_default());
            let store = crate::store::store(cx);
            for (id, session) in store.read(cx).sessions.iter() {
                let session = session.read(cx);
                eprintln!(
                    "SESSION {id} kind={:?} title={} fg={:?} activity={:?} attention={} exit={:?} grid={}x{}",
                    session.kind,
                    session.display_title(),
                    session.foreground,
                    session.activity,
                    session.attention,
                    session.exit,
                    session.terminal.size.cols,
                    session.terminal.size.rows
                );
            }
        }
        "text" => {
            let store = crate::store::store(cx);
            if let Some(session) = store.read(cx).sessions.get(rest.trim()) {
                eprintln!("TEXT {}", session.read(cx).terminal.text(40));
            }
        }
        "draw" => window.draw(cx).clear(),
        "activate" => {
            cx.activate(true);
            window.activate_window();
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
#[allow(unexpected_cfgs)]
fn send_event(line: &str) {
    use core_graphics::geometry::{CGPoint, CGRect};
    use objc::runtime::{BOOL, NO, Object};
    use objc::{class, msg_send, sel, sel_impl};
    let parts: Vec<&str> = line.split_whitespace().collect();
    let [number, kind, rest @ ..] = parts.as_slice() else { return };
    let Ok(number) = number.parse::<isize>() else { return };
    let time: f64 = 0.0;
    let null: *mut Object = std::ptr::null_mut();
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let window: *mut Object = msg_send![app, windowWithWindowNumber: number];
        if window.is_null() {
            return;
        }
        if *kind == "front" {
            let _: () = msg_send![window, orderFrontRegardless];
            return;
        }
        if *kind == "key" {
            let [text, code, flags] = rest else { return };
            let text = match *text {
                "esc" => "\u{1b}",
                "ret" => "\r",
                "space" => " ",
                other => other,
            };
            let Ok(text) = std::ffi::CString::new(text) else { return };
            let chars: *mut Object = msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()];
            let event: *mut Object = msg_send![class!(NSEvent),
                keyEventWithType: 10usize
                location: CGPoint::new(0.0, 0.0)
                modifierFlags: flags.parse::<usize>().unwrap_or(0)
                timestamp: time
                windowNumber: number
                context: null
                characters: chars
                charactersIgnoringModifiers: chars
                isARepeat: NO
                keyCode: code.parse::<u16>().unwrap_or(0)];
            let handled: BOOL = msg_send![window, performKeyEquivalent: event];
            let menu: *mut Object = msg_send![app, mainMenu];
            let claimed: BOOL = if handled == NO { msg_send![menu, performKeyEquivalent: event] } else { handled };
            eprintln!("EVENT window={} menu={}", handled != NO, claimed != NO && handled == NO);
            if claimed == NO {
                let _: () = msg_send![window, sendEvent: event];
            }
            return;
        }
        let content: *mut Object = msg_send![window, contentView];
        if *kind == "text" || *kind == "mark" {
            #[repr(C)]
            struct Range(usize, usize);
            let Ok(text) = std::ffi::CString::new(rest.join(" ")) else { return };
            let chars: *mut Object = msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()];
            let none = || Range(isize::MAX as usize, 0);
            let responder: *mut Object = msg_send![window, firstResponder];
            if *kind == "text" {
                let _: () = msg_send![responder, insertText: chars replacementRange: none()];
            } else {
                let _: () =
                    msg_send![responder, setMarkedText: chars selectedRange: Range(0, 0) replacementRange: none()];
            }
            return;
        }
        let [x, y] = rest else { return };
        let (Ok(x), Ok(y)) = (x.parse::<f64>(), y.parse::<f64>()) else { return };
        let frame: CGRect = msg_send![content, frame];
        let code: usize = match *kind {
            "down" => 1,
            "up" => 2,
            "rdown" => 3,
            "rup" => 4,
            "move" => 5,
            _ => 6,
        };
        let event: *mut Object = msg_send![class!(NSEvent),
            mouseEventWithType: code
            location: CGPoint::new(x, frame.size.height - y)
            modifierFlags: 0usize
            timestamp: time
            windowNumber: number
            context: null
            eventNumber: 0isize
            clickCount: 1isize
            pressure: 1.0f32];
        let event: *mut Object = if code == 3 || code == 4 {
            #[link(name = "CoreGraphics", kind = "framework")]
            unsafe extern "C" {
                fn CGEventSetIntegerValueField(event: *mut std::ffi::c_void, field: u32, value: i64);
            }
            let native: *mut std::ffi::c_void = msg_send![event, CGEvent];
            CGEventSetIntegerValueField(native, 3, 1);
            msg_send![class!(NSEvent), eventWithCGEvent: native]
        } else {
            event
        };
        let _: () = msg_send![window, sendEvent: event];
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn inspect_app(path: &str, perform: bool) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe fn text(value: *mut Object) -> String {
        if value.is_null() {
            return String::new();
        }
        let bytes: *const std::ffi::c_char = unsafe { msg_send![value, UTF8String] };
        unsafe { std::ffi::CStr::from_ptr(bytes) }.to_string_lossy().into_owned()
    }
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let main: *mut Object = msg_send![app, mainMenu];
        let tops: isize = msg_send![main, numberOfItems];
        for top in 0..tops {
            let item: *mut Object = msg_send![main, itemAtIndex: top];
            let menu: *mut Object = msg_send![item, submenu];
            let name = text(msg_send![item, title]);
            if menu.is_null() {
                continue;
            }
            let rows: isize = msg_send![menu, numberOfItems];
            for row in 0..rows {
                let item: *mut Object = msg_send![menu, itemAtIndex: row];
                let title = text(msg_send![item, title]);
                let full = format!("{name}/{title}");
                if perform {
                    if full == path {
                        let _: () = msg_send![menu, performActionForItemAtIndex: row];
                    }
                } else if !title.is_empty() {
                    let key = text(msg_send![item, keyEquivalent]);
                    let mask: usize = msg_send![item, keyEquivalentModifierMask];
                    eprintln!("MENU {full} key={key:?} mask={mask:#x}");
                }
            }
        }
        if !perform {
            let tile: *mut Object = msg_send![app, dockTile];
            eprintln!("BADGE {:?}", text(msg_send![tile, badgeLabel]));
        }
    }
}

#[cfg(target_os = "macos")]
fn capture(id: u32, path: &str) {
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    use core_graphics::window::{create_image, kCGWindowImageBoundsIgnoreFraming, kCGWindowListOptionIncludingWindow};
    let Some(image) = create_image(
        CGRect::new(&CGPoint::new(f64::INFINITY, f64::INFINITY), &CGSize::new(0.0, 0.0)),
        kCGWindowListOptionIncludingWindow,
        id,
        kCGWindowImageBoundsIgnoreFraming,
    ) else {
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
