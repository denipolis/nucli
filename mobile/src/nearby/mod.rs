use std::sync::Arc;

use nucli_core::uplink::net::Shared;

#[cfg(target_os = "ios")]
mod ios;

#[cfg(target_os = "ios")]
pub use ios::Browser;

#[cfg(not(target_os = "ios"))]
pub use nucli_core::uplink::net::Discovery as Browser;

#[cfg(target_os = "ios")]
pub fn start(shared: &Arc<Shared>) -> Option<Browser> {
    ios::start(shared)
}

#[cfg(not(target_os = "ios"))]
pub fn start(shared: &Arc<Shared>) -> Option<Browser> {
    Browser::start(shared)
}
