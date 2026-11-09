use gpui::{App, Global};
use serde::{Deserialize, Serialize};

use crate::agents::{AgentProfile, ShellProfile};
use crate::paths;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum CursorShape {
    #[default]
    Block,
    Bar,
    Underline,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub appearance: Appearance,
    pub font: String,
    pub font_size: f32,
    pub ui_font_size: f32,
    pub line_height: f32,
    pub cursor: CursorShape,
    pub cursor_blink: bool,
    pub ligatures: bool,
    pub restore_tabs: bool,
    pub notifications: bool,
    pub agents: Vec<AgentProfile>,
    pub shells: Vec<ShellProfile>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "nucli".into(),
            appearance: Appearance::System,
            font: FONT.into(),
            font_size: 13.0,
            ui_font_size: 13.0,
            line_height: NATURAL_LINE_HEIGHT,
            cursor: CursorShape::Block,
            cursor_blink: true,
            ligatures: true,
            restore_tabs: true,
            notifications: true,
            agents: Vec::new(),
            shells: Vec::new(),
        }
    }
}

impl Global for Settings {}

#[derive(Default)]
pub struct SettingsError(pub Option<String>);

impl Global for SettingsError {}

pub const FONT: &str = "Maple Mono NF";
pub const NATURAL_LINE_HEIGHT: f32 = 1.3;

impl Settings {
    pub fn read() -> Result<Option<Settings>, String> {
        match std::fs::read_to_string(paths::settings_file()) {
            Ok(text) => serde_json::from_str(&text).map(Some).map_err(|error| format!("settings.json: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("settings.json: {error}")),
        }
    }

    pub fn write(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = paths::write_atomic(&paths::settings_file(), format!("{json}\n").as_bytes());
        }
    }

    pub fn font_size(&self) -> f32 {
        self.font_size.clamp(6.0, 48.0)
    }

    pub fn ui_font_size(&self) -> f32 {
        self.ui_font_size.clamp(8.0, 24.0)
    }

    pub fn line_height(&self) -> f32 {
        self.line_height.clamp(1.0, 2.0)
    }
}

pub fn get(cx: &App) -> &Settings {
    cx.global::<Settings>()
}

pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    let mut settings = cx.global::<Settings>().clone();
    change(&mut settings);
    settings.write();
    cx.set_global(settings);
}

pub fn reload(cx: &mut App) {
    match Settings::read() {
        Ok(Some(settings)) => {
            if &settings != cx.global::<Settings>() {
                cx.set_global(settings);
            }
            cx.set_global(SettingsError(None));
        }
        Ok(None) => cx.set_global(SettingsError(None)),
        Err(error) => cx.set_global(SettingsError(Some(error))),
    }
}
