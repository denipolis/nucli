use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use gpui::BackgroundExecutor;

use super::net::Outbox;

pub const PIECE: usize = 262144;
const WAIT: Duration = Duration::from_millis(10);

pub enum Piece {
    Data(Vec<u8>),
    End,
}

pub fn free_name(path: &Path) -> PathBuf {
    if path.symlink_metadata().is_err() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = path.extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default();
    (1..)
        .map(|count| {
            let suffix = if count == 1 { " copy".to_string() } else { format!(" copy {count}") };
            path.with_file_name(format!("{stem}{suffix}{extension}"))
        })
        .find(|candidate| candidate.symlink_metadata().is_err())
        .unwrap_or_else(|| path.to_path_buf())
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0', ':'])
}

pub async fn send(
    outbox: &Outbox,
    id: u64,
    path: &Path,
    stop: &AtomicBool,
    executor: &BackgroundExecutor,
) -> Result<(), String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut buffer = vec![0u8; PIECE];
    loop {
        while outbox.backed_up() && !outbox.is_closed() && !stop.load(Ordering::Acquire) {
            executor.timer(WAIT).await;
        }
        if stop.load(Ordering::Acquire) {
            return Err("The transfer was stopped.".into());
        }
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(());
        }
        if !outbox.chunk(id, &buffer[..read]) {
            return Err("The connection closed.".into());
        }
    }
}

pub fn receive(
    target: PathBuf,
    exact: bool,
    limit: u64,
    executor: &BackgroundExecutor,
) -> (mpsc::UnboundedSender<Piece>, oneshot::Receiver<Result<PathBuf, String>>) {
    let (sender, mut pieces) = mpsc::unbounded::<Piece>();
    let (done, finished) = oneshot::channel();
    executor
        .spawn(async move {
            let name = target.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            let temp = target.with_file_name(format!(".{name}.nucli-part"));
            let result = async {
                let _ = std::fs::remove_file(&temp);
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temp)
                    .map_err(|error| error.to_string())?;
                let mut written = 0u64;
                while let Some(piece) = pieces.next().await {
                    match piece {
                        Piece::Data(data) => {
                            written += data.len() as u64;
                            if written > limit {
                                return Err("The file is larger than announced.".to_string());
                            }
                            file.write_all(&data).map_err(|error| error.to_string())?;
                        }
                        Piece::End => {
                            file.flush().map_err(|error| error.to_string())?;
                            drop(file);
                            let path = if exact { target.clone() } else { free_name(&target) };
                            std::fs::rename(&temp, &path).map_err(|error| error.to_string())?;
                            return Ok(path);
                        }
                    }
                }
                Err("The transfer stopped.".to_string())
            }
            .await;
            if result.is_err() {
                let _ = std::fs::remove_file(&temp);
            }
            let _ = done.send(result);
        })
        .detach();
    (sender, finished)
}

pub fn stopper() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_free_paths() {
        assert!(valid_name("a.txt") && valid_name(".env"));
        assert!(!valid_name("") && !valid_name("..") && !valid_name("a/b") && !valid_name("a\\b"));
        assert!(!valid_name("C:x"));
        let base = std::env::temp_dir().join(format!("nucli-transfer-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        assert_eq!(free_name(&base.join("new.txt")), base.join("new.txt"));
        std::fs::write(base.join("a.txt"), "a").unwrap();
        assert_eq!(free_name(&base.join("a.txt")), base.join("a copy.txt"));
        let _ = std::fs::remove_dir_all(base);
    }
}
