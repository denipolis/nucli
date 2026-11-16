use std::sync::{Mutex, OnceLock};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

#[cfg(target_os = "ios")]
mod ios;

#[cfg(target_os = "ios")]
pub use ios::{active, keep, post};

static CLICKS: OnceLock<Mutex<UnboundedSender<String>>> = OnceLock::new();

pub fn clicks() -> UnboundedReceiver<String> {
    let (sender, receiver) = unbounded();
    let _ = CLICKS.set(Mutex::new(sender));
    #[cfg(target_os = "ios")]
    ios::listen();
    receiver
}

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn clicked(id: String) {
    if let Some(sender) = CLICKS.get().and_then(|sender| sender.lock().ok()) {
        let _ = sender.unbounded_send(id);
    }
}

#[cfg(not(target_os = "ios"))]
pub fn active() -> bool {
    true
}

#[cfg(not(target_os = "ios"))]
pub fn keep(_on: bool) {}

#[cfg(not(target_os = "ios"))]
pub fn post(_title: &str, _place: &str, _text: &str, _id: &str) {}
