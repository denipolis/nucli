use std::path::Path;

const SERVICE: &str = "Nucli Uplink";

fn entry(path: &Path) -> Option<keyring::Entry> {
    if cfg!(all(target_os = "macos", debug_assertions)) {
        return None;
    }
    keyring::Entry::new(SERVICE, &path.to_string_lossy()).ok()
}

pub fn load(path: &Path) -> Option<String> {
    entry(path)?.get_password().ok()
}

pub fn store(path: &Path, secret: &str) -> bool {
    load(path).as_deref() == Some(secret) || entry(path).is_some_and(|entry| entry.set_password(secret).is_ok())
}

#[cfg(test)]
pub fn forget(path: &Path) {
    if let Some(entry) = entry(path) {
        let _ = entry.delete_credential();
    }
}
