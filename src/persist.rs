use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::session::Kind;
use crate::theme::ItemColor;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum SettingsSection {
    #[default]
    General,
    Appearance,
    Terminal,
    Agents,
    Shells,
    Keybindings,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum SidebarTab {
    #[default]
    Projects,
    Agents,
    History,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum PanelTab {
    #[default]
    Files,
    Changes,
    Commits,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Left,
    Right,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct Ui {
    pub sidebar_tab: SidebarTab,
    pub panel_tab: PanelTab,
    pub panel_side: Side,
    pub panel_hidden: bool,
    pub settings_section: SettingsSection,
    pub find: crate::find::Options,
}

impl Default for Ui {
    fn default() -> Self {
        Ui {
            sidebar_tab: SidebarTab::Projects,
            panel_tab: PanelTab::Files,
            panel_side: Side::Left,
            panel_hidden: false,
            settings_section: SettingsSection::General,
            find: crate::find::Options::default(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct WindowState {
    pub bounds: Option<[f32; 4]>,
    pub sidebar_width: f32,
    pub panel_width: f32,
    pub terminal_height: f32,
    pub sidebar_hidden: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        WindowState {
            bounds: None,
            sidebar_width: 260.0,
            panel_width: 280.0,
            terminal_height: 240.0,
            sidebar_hidden: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
#[serde(default)]
pub struct SavedTab {
    pub id: String,
    pub kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ItemColor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
#[serde(default)]
pub struct ProjectFile {
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ItemColor>,
    pub collapsed: bool,
    pub tabs: Vec<SavedTab>,
    pub terminals: Vec<SavedTab>,
    pub active_tab: Option<String>,
    pub terminal_shown: bool,
    pub open_files: Vec<String>,
    pub active_file: Option<String>,
    pub file_shown: bool,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub languages: std::collections::BTreeMap<String, String>,
    pub recent_files: Vec<String>,
    pub last_launcher: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum Selected {
    Project(PathBuf),
    Tab(String),
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "axis", rename_all = "lowercase")]
pub enum Layout {
    Pane { tab: String },
    Row { children: Vec<Layout> },
    Column { children: Vec<Layout> },
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
#[serde(default)]
pub struct WorkspaceFile {
    pub id: String,
    pub name: String,
    pub closed: bool,
    pub window: WindowState,
    pub selected: Option<Selected>,
    pub projects: Vec<ProjectFile>,
    pub scratch: ProjectFile,
    pub layouts: Vec<Layout>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
#[serde(default)]
pub struct StateFile {
    pub workspaces: Vec<WorkspaceFile>,
    pub last_launcher: Option<String>,
    pub last_shell: Option<String>,
    pub last_folder: Option<PathBuf>,
    pub last_workspace: Option<String>,
    pub ui: Ui,
    pub open_with: BTreeMap<String, Vec<String>>,
    pub skip_close_confirm_until: u64,
}

impl StateFile {
    pub fn read() -> Option<StateFile> {
        let text = std::fs::read_to_string(paths::state_file()).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn write(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = paths::write_atomic(&paths::state_file(), json.as_bytes());
        }
    }
}

pub fn scrollback_file(id: &str) -> PathBuf {
    paths::scrollback_dir().join(format!("{id}.txt"))
}

pub fn read_scrollback(id: &str) -> Option<String> {
    std::fs::read_to_string(scrollback_file(id)).ok()
}

pub fn prune_scrollback(state: &StateFile) {
    let keep: std::collections::HashSet<String> = state
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.projects.iter().chain(std::iter::once(&workspace.scratch)))
        .flat_map(|project| project.tabs.iter().chain(&project.terminals))
        .map(|tab| format!("{}.txt", tab.id))
        .collect();
    let Ok(entries) = std::fs::read_dir(paths::scrollback_dir()) else { return };
    for entry in entries.flatten() {
        if !keep.contains(&entry.file_name().to_string_lossy().into_owned()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips() {
        let state = StateFile {
            workspaces: vec![WorkspaceFile {
                id: "a".into(),
                name: "Personal".into(),
                selected: Some(Selected::Tab("t".into())),
                layouts: vec![Layout::Row {
                    children: vec![Layout::Pane { tab: "t".into() }, Layout::Pane { tab: "u".into() }],
                }],
                projects: vec![ProjectFile {
                    path: "/tmp/x".into(),
                    tabs: vec![SavedTab { id: "t".into(), kind: Some(Kind::Agent), ..Default::default() }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<StateFile>(&json).unwrap(), state);
        assert_eq!(serde_json::from_str::<StateFile>("{}").unwrap(), StateFile::default());
    }
}
