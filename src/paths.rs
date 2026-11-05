use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const DEV: bool = cfg!(debug_assertions);

pub fn app_name() -> &'static str {
    if DEV { "Nucli Dev" } else { "Nucli" }
}

fn folder_name() -> &'static str {
    if cfg!(target_os = "linux") { if DEV { "nucli-dev" } else { "nucli" } } else { app_name() }
}

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn config_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if cfg!(windows) {
            dirs::config_dir().unwrap_or_else(home).join(app_name())
        } else {
            let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"));
            base.join(if DEV { "nucli-dev" } else { "nucli" })
        }
    })
}

pub fn data_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let base = if cfg!(windows) { dirs::data_local_dir() } else { dirs::data_dir() };
        base.unwrap_or_else(home).join(folder_name())
    })
}

pub fn cache_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if cfg!(windows) {
            data_dir().join("cache")
        } else {
            dirs::cache_dir().unwrap_or_else(home).join(folder_name())
        }
    })
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn keybindings_file() -> PathBuf {
    config_dir().join("keybindings.json")
}

pub fn state_file() -> PathBuf {
    data_dir().join("state.json")
}

pub fn scrollback_dir() -> PathBuf {
    data_dir().join("restore")
}

pub fn display(path: &Path) -> String {
    let home = home();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        Err(_) => path.display().to_string(),
    }
}

pub fn expand(path: &str) -> PathBuf {
    let home = home();
    let home = home.to_string_lossy();
    let expanded = path.replace("${HOME}", &home).replace("$HOME", &home);
    match expanded.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') => {
            PathBuf::from(format!("{home}{rest}"))
        }
        _ => PathBuf::from(expanded),
    }
}

pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, contents)?;
    std::fs::rename(&temp, path)
}
