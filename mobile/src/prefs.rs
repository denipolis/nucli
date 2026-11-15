use std::path::PathBuf;

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Prefs {
    pub name: String,
    pub appearance: Appearance,
    pub theme: String,
    pub font_size: f32,
    pub onboarded: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            name: if cfg!(target_os = "ios") { "iPhone" } else { "Nucli Uplink" }.into(),
            appearance: Appearance::System,
            theme: "nucli".into(),
            font_size: 13.0,
            onboarded: false,
        }
    }
}

impl Global for Prefs {}

pub const FONT_SIZES: [f32; 7] = [10.0, 11.0, 12.0, 13.0, 14.0, 16.0, 18.0];

pub fn dir() -> PathBuf {
    let folder = if cfg!(debug_assertions) { "Nucli Uplink Dev" } else { "Nucli Uplink" };
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join(folder)
}

fn file() -> PathBuf {
    dir().join("settings.json")
}

pub fn load() -> Prefs {
    std::fs::read_to_string(file()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

pub fn get(cx: &App) -> &Prefs {
    cx.global::<Prefs>()
}

pub fn update(cx: &mut App, change: impl FnOnce(&mut Prefs)) {
    let mut prefs = get(cx).clone();
    change(&mut prefs);
    if let Ok(text) = serde_json::to_string_pretty(&prefs) {
        let _ = std::fs::create_dir_all(dir());
        let _ = std::fs::write(file(), format!("{text}\n"));
    }
    cx.set_global(prefs);
    cx.refresh_windows();
}
