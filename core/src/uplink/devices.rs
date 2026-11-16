use std::path::Path;

use serde::{Deserialize, Serialize};

use super::vault;
use super::wire::{self, Identity, Key};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    #[default]
    Control,
    View,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Device {
    pub key: String,
    pub name: String,
    pub os: String,
    pub access: Access,
    pub blocked: bool,
    pub addresses: Vec<String>,
    pub manual: Vec<String>,
    pub paired: u64,
    pub seen: u64,
}

impl Device {
    pub fn public(&self) -> Option<Key> {
        wire::decode_key(&self.key)
    }

    pub fn learn(&mut self, address: String) {
        self.addresses.retain(|known| *known != address);
        self.addresses.insert(0, address);
        self.addresses.truncate(4);
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct DeviceFile {
    secret: String,
    public: String,
    devices: Vec<Device>,
}

pub fn load(path: &Path) -> (Identity, Vec<Device>) {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let stored: Option<DeviceFile> = serde_json::from_str(&text).ok();
            if let Some(stored) = stored
                && let Some(public) = wire::decode_key(&stored.public)
                && let Some(secret) = wire::decode_key(&stored.secret)
                    .or_else(|| vault::load(&stored.public).and_then(|text| wire::decode_key(&text)))
            {
                let mut devices = stored.devices;
                devices.retain(|device| device.public().is_some_and(|key| key != public));
                let identity = Identity { secret, public };
                if !stored.secret.is_empty() {
                    save(path, &identity, &devices);
                }
                return (identity, devices);
            }
            set_aside(path);
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => set_aside(path),
        Err(_) => {}
    }
    let identity = Identity::generate();
    save(path, &identity, &[]);
    (identity, Vec::new())
}

fn set_aside(path: &Path) {
    let broken = path.with_extension("json.broken");
    eprintln!("uplink: {} is unreadable, moved to {}", path.display(), broken.display());
    let _ = std::fs::rename(path, broken);
}

pub fn save(path: &Path, identity: &Identity, devices: &[Device]) {
    let secret = wire::encode_key(&identity.secret);
    let public = wire::encode_key(&identity.public);
    let file = DeviceFile {
        secret: if vault::store(&public, &secret) { String::new() } else { secret },
        public,
        devices: devices.to_vec(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&file)
        && let Err(error) = write_private(path, format!("{json}\n").as_bytes())
    {
        eprintln!("uplink: could not save {}: {error}", path.display());
    }
}

fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&temp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_identity_private_and_stable() {
        let folder = std::env::temp_dir().join(format!("nucli-uplink-{}", uuid::Uuid::new_v4()));
        let path = folder.join("uplink.json");
        let (identity, devices) = load(&path);
        assert!(devices.is_empty());
        let other = Identity::generate();
        let device = Device { key: wire::encode_key(&other.public), name: "laptop".into(), ..Default::default() };
        save(&path, &identity, std::slice::from_ref(&device));
        let (again, devices) = load(&path);
        assert_eq!(again.public, identity.public);
        assert_eq!(again.secret, identity.secret);
        assert_eq!(devices, vec![device]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        vault::forget(&wire::encode_key(&identity.public));
        let _ = std::fs::remove_dir_all(folder);
    }
}
