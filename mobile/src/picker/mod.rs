use std::path::PathBuf;

use futures::channel::oneshot;
use gpui::App;

#[cfg(target_os = "ios")]
mod ios;

#[cfg(target_os = "ios")]
pub fn pick(_: &mut App) -> oneshot::Receiver<Vec<PathBuf>> {
    ios::pick()
}

#[cfg(not(target_os = "ios"))]
pub fn pick(cx: &mut App) -> oneshot::Receiver<Vec<PathBuf>> {
    let (sender, receiver) = oneshot::channel();
    let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some("Upload".into()),
    });
    cx.background_executor()
        .spawn(async move {
            let files = match picked.await {
                Ok(Ok(Some(files))) => files,
                _ => Vec::new(),
            };
            let _ = sender.send(files);
        })
        .detach();
    receiver
}

#[cfg(target_os = "ios")]
pub const PLACE: &str = "Files › Nucli Uplink";

#[cfg(not(target_os = "ios"))]
pub const PLACE: &str = "Downloads";

#[cfg(target_os = "ios")]
pub fn downloads() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join("Documents")
}

#[cfg(not(target_os = "ios"))]
pub fn downloads() -> PathBuf {
    dirs::download_dir().or_else(dirs::home_dir).unwrap_or_default()
}
