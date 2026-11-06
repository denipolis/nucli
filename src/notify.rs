use std::sync::{Mutex, OnceLock};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

static CLICKS: OnceLock<Mutex<UnboundedSender<String>>> = OnceLock::new();

pub fn clicks() -> UnboundedReceiver<String> {
    let (sender, receiver) = unbounded();
    let _ = CLICKS.set(Mutex::new(sender));
    receiver
}

#[allow(dead_code)]
fn clicked(id: String) {
    if let Some(sender) = CLICKS.get().and_then(|sender| sender.lock().ok()) {
        let _ = sender.unbounded_send(id);
    }
}

#[cfg(target_os = "macos")]
pub fn post(title: &str, text: &str, id: &str) {
    use mac_notification_sys::{Notification, NotificationResponse, set_application};
    let (title, text, id) = (title.to_string(), text.to_string(), id.to_string());
    std::thread::spawn(move || {
        let _ = set_application("uk.polishch.nucli");
        let response = Notification::new().title(&title).message(&text).wait_for_click(true).send();
        if matches!(response, Ok(NotificationResponse::Click)) {
            clicked(id);
        }
    });
}

#[cfg(target_os = "linux")]
pub fn post(title: &str, text: &str, id: &str) {
    let (title, text, id) = (title.to_string(), text.to_string(), id.to_string());
    std::thread::spawn(move || {
        let shown = notify_rust::Notification::new().summary(&title).body(&text).action("default", "Show").show();
        if let Ok(handle) = shown {
            handle.wait_for_action(|action| {
                if action == "default" {
                    clicked(id);
                }
            });
        }
    });
}

#[cfg(target_os = "windows")]
pub fn post(title: &str, text: &str, _id: &str) {
    let (title, text) = (title.to_string(), text.to_string());
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new().summary(&title).body(&text).show();
    });
}

#[cfg(target_os = "macos")]
pub fn badge(count: usize) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    let text = std::ffi::CString::new(if count > 0 { count.to_string() } else { String::new() }).unwrap_or_default();
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let tile: *mut Object = msg_send![app, dockTile];
        let label: *mut Object = msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()];
        let _: () = msg_send![tile, setBadgeLabel: label];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn badge(_count: usize) {}
