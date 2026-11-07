use std::sync::{Mutex, OnceLock};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

static CLICKS: OnceLock<Mutex<UnboundedSender<String>>> = OnceLock::new();

pub fn clicks() -> UnboundedReceiver<String> {
    let (sender, receiver) = unbounded();
    let _ = CLICKS.set(Mutex::new(sender));
    #[cfg(target_os = "macos")]
    center::listen();
    receiver
}

fn clicked(id: String) {
    if let Some(sender) = CLICKS.get().and_then(|sender| sender.lock().ok()) {
        let _ = sender.unbounded_send(id);
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod center {
    use std::ffi::{CStr, CString, c_char};

    use block::{Block, ConcreteBlock};
    use objc::declare::ClassDecl;
    use objc::runtime::{BOOL, NO, Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};

    #[link(name = "UserNotifications", kind = "framework")]
    unsafe extern "C" {}

    const ALERT_AND_SOUND: usize = (1 << 2) | (1 << 1);
    const BANNER_LIST_AND_SOUND: usize = (1 << 4) | (1 << 3) | (1 << 1);

    fn string(text: &str) -> *mut Object {
        let text = CString::new(text.replace('\0', "")).unwrap_or_default();
        unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] }
    }

    fn center() -> Option<*mut Object> {
        unsafe {
            let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
            let id: *mut Object = msg_send![bundle, bundleIdentifier];
            if id.is_null() {
                return None;
            }
            Some(msg_send![class!(UNUserNotificationCenter), currentNotificationCenter])
        }
    }

    extern "C" fn did_receive(_: &Object, _: Sel, _: *mut Object, response: *mut Object, done: *mut Object) {
        unsafe {
            let notification: *mut Object = msg_send![response, notification];
            let request: *mut Object = msg_send![notification, request];
            let id: *mut Object = msg_send![request, identifier];
            let text: *const c_char = msg_send![id, UTF8String];
            super::clicked(CStr::from_ptr(text).to_string_lossy().into_owned());
            (*(done as *const Block<(), ()>)).call(());
        }
    }

    extern "C" fn will_present(_: &Object, _: Sel, _: *mut Object, _: *mut Object, done: *mut Object) {
        unsafe { (*(done as *const Block<(usize,), ()>)).call((BANNER_LIST_AND_SOUND,)) };
    }

    pub fn listen() {
        let Some(center) = center() else { return };
        let Some(mut class) = ClassDecl::new("NucliNotificationDelegate", class!(NSObject)) else { return };
        unsafe {
            class.add_method(
                sel!(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:),
                did_receive as extern "C" fn(&Object, Sel, *mut Object, *mut Object, *mut Object),
            );
            class.add_method(
                sel!(userNotificationCenter:willPresentNotification:withCompletionHandler:),
                will_present as extern "C" fn(&Object, Sel, *mut Object, *mut Object, *mut Object),
            );
            let delegate: *mut Object = msg_send![class.register(), new];
            let _: () = msg_send![center, setDelegate: delegate];
        }
    }

    pub fn post(title: &str, text: &str, id: &str) {
        let Some(center) = center() else { return };
        let (title, text, id, address) = (title.to_string(), text.to_string(), id.to_string(), center as usize);
        let granted = ConcreteBlock::new(move |granted: BOOL, _: *mut Object| {
            if granted == NO {
                return;
            }
            unsafe {
                let content: *mut Object = msg_send![class!(UNMutableNotificationContent), new];
                let _: () = msg_send![content, setTitle: string(&title)];
                let _: () = msg_send![content, setBody: string(&text)];
                let sound: *mut Object = msg_send![class!(UNNotificationSound), defaultSound];
                let _: () = msg_send![content, setSound: sound];
                let none = std::ptr::null_mut::<Object>();
                let request: *mut Object = msg_send![
                    class!(UNNotificationRequest),
                    requestWithIdentifier: string(&id)
                    content: content
                    trigger: none
                ];
                let _: () =
                    msg_send![address as *mut Object, addNotificationRequest: request withCompletionHandler: none];
                let _: () = msg_send![content, release];
            }
        })
        .copy();
        unsafe {
            let _: () =
                msg_send![center, requestAuthorizationWithOptions: ALERT_AND_SOUND completionHandler: &*granted];
        }
    }
}

#[cfg(target_os = "macos")]
pub use center::post;

#[cfg(not(target_os = "macos"))]
pub fn post(title: &str, text: &str, id: &str) {
    use notify_rust::{Notification, NotificationResponse};
    let (title, text, id) = (title.to_string(), text.to_string(), id.to_string());
    std::thread::spawn(move || {
        let shown = Notification::new().summary(&title).body(&text).action("default", "Show").show();
        if let Ok(handle) = shown {
            let _ = handle.wait_for_response(|response: &NotificationResponse| {
                if matches!(response, NotificationResponse::Default | NotificationResponse::Action(_)) {
                    clicked(id);
                }
            });
        }
    });
}

#[cfg(windows)]
pub fn alert(text: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let wide = |value: &str| value.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (text, caption) = (wide(text), wide("Nucli"));
    let shown =
        std::thread::spawn(move || unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10) });
    let _ = shown.join();
}

#[cfg(not(windows))]
pub fn alert(_text: &str) {}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
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
