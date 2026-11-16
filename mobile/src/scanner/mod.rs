use futures::channel::oneshot::Sender;

#[cfg(target_os = "ios")]
pub mod ios;

#[cfg(target_os = "ios")]
pub fn scan(found: Sender<String>) -> Result<(), String> {
    ios::scan(found)
}

#[cfg(not(target_os = "ios"))]
pub fn scan(_: Sender<String>) -> Result<(), String> {
    Err("Scanning needs a camera. Paste the pairing link or type the code instead.".into())
}
