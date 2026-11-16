const SERVICE: &str = "Nucli Uplink";

fn entry(account: &str) -> Option<keyring::Entry> {
    if cfg!(all(target_os = "macos", debug_assertions)) {
        return None;
    }
    keyring::Entry::new(SERVICE, account).ok()
}

pub fn load(account: &str) -> Option<String> {
    entry(account)?.get_password().ok()
}

pub fn store(account: &str, secret: &str) -> bool {
    load(account).as_deref() == Some(secret) || entry(account).is_some_and(|entry| entry.set_password(secret).is_ok())
}

#[cfg(test)]
pub fn forget(account: &str) {
    if let Some(entry) = entry(account) {
        let _ = entry.delete_credential();
    }
}
