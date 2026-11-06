use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::Command;

use plist::Value;

use crate::agents::{AgentProfile, ShellProfile};
use crate::persist::{ProjectFile, StateFile, WorkspaceFile};
use crate::settings::{Appearance, CursorShape, Settings};

fn text(value: &Value, key: &str) -> String {
    value.as_dictionary().and_then(|dict| dict.get(key)).and_then(Value::as_string).unwrap_or_default().to_string()
}

fn window_id(value: &Value) -> Option<i64> {
    let dict = value.as_dictionary()?;
    dict.get("window")?.as_signed_integer().or_else(|| dict.get("window")?.as_real().map(|number| number as i64))
}

fn profile(value: &Value) -> Option<AgentProfile> {
    let executable = text(value, "executable");
    if executable.is_empty() {
        return None;
    }
    let env: BTreeMap<String, String> = value
        .as_dictionary()?
        .get("environment")
        .and_then(Value::as_array)
        .map(|pairs| {
            pairs
                .iter()
                .filter_map(|pair| {
                    let pair = pair.as_array()?;
                    Some((pair.first()?.as_string()?.to_string(), pair.get(1)?.as_string()?.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(AgentProfile {
        name: text(value, "name"),
        executable,
        arguments: text(value, "arguments"),
        config_dir: text(value, "configDirectory"),
        env,
        theme: text(value, "theme"),
    })
}

pub fn parse(root: &Value) -> Option<(Settings, StateFile)> {
    let dict = root.as_dictionary()?;
    let mut settings = Settings::default();
    if let Some(profiles) = dict.get("profiles").and_then(Value::as_array) {
        settings.agents = profiles.iter().filter_map(profile).collect();
    }
    if let Some(shells) = dict.get("shells").and_then(Value::as_array) {
        settings.shells = shells
            .iter()
            .map(|shell| ShellProfile { name: text(shell, "name"), path: text(shell, "path") })
            .filter(|shell| !shell.path.is_empty())
            .collect();
    }
    if let Some(theme) = dict.get("theme").and_then(Value::as_string) {
        settings.theme = match theme {
            "nucli-dark" | "nucli-light" | "nucli" => "nucli".to_string(),
            other => other.to_string(),
        };
    }
    if let Some(size) = dict.get("fontSize").and_then(Value::as_real) {
        settings.font_size = size as f32;
    }
    if let Some(on) = dict.get("notifications").and_then(Value::as_boolean) {
        settings.notifications = on;
    }
    if let Some(on) = dict.get("restoresTabs").and_then(Value::as_boolean) {
        settings.restore_tabs = on;
    }
    if let Some(spacing) = dict.get("lineSpacing").and_then(Value::as_real) {
        settings.line_height = spacing as f32;
    }
    if let Some(on) = dict.get("cursorBlinks").and_then(Value::as_boolean) {
        settings.cursor_blink = on;
    }
    if let Some(on) = dict.get("ligatures").and_then(Value::as_boolean) {
        settings.ligatures = on;
    }
    settings.cursor = match dict.get("cursorShape").and_then(Value::as_string) {
        Some("bar") => CursorShape::Bar,
        Some("underline") => CursorShape::Underline,
        _ => CursorShape::Block,
    };
    settings.appearance = match dict.get("appearance").and_then(Value::as_string) {
        Some("light") => Appearance::Light,
        Some("dark") => Appearance::Dark,
        _ => Appearance::System,
    };
    let mut workspaces: Vec<WorkspaceFile> = Vec::new();
    let mut by_window: HashMap<i64, usize> = HashMap::new();
    for item in dict.get("workspaceList").and_then(Value::as_array).into_iter().flatten() {
        let closed =
            item.as_dictionary().and_then(|dict| dict.get("closed")).and_then(Value::as_boolean).unwrap_or(false);
        if let Some(window) = window_id(item) {
            by_window.insert(window, workspaces.len());
        }
        workspaces.push(WorkspaceFile {
            id: uuid::Uuid::new_v4().to_string(),
            name: text(item, "name"),
            closed,
            ..Default::default()
        });
    }
    for item in dict.get("projects").and_then(Value::as_array).into_iter().flatten() {
        let path = text(item, "path");
        if path.is_empty() {
            continue;
        }
        let scratch =
            item.as_dictionary().and_then(|dict| dict.get("scratch")).and_then(Value::as_boolean).unwrap_or(false);
        if scratch {
            continue;
        }
        if workspaces.is_empty() {
            workspaces.push(WorkspaceFile {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Personal".into(),
                ..Default::default()
            });
        }
        let index = window_id(item).and_then(|window| by_window.get(&window).copied()).unwrap_or(0);
        let name = text(item, "name");
        let color = serde_json::from_value(serde_json::Value::String(text(item, "color"))).ok();
        workspaces[index].projects.push(ProjectFile {
            path: PathBuf::from(path),
            name: (!name.is_empty()).then_some(name),
            color,
            ..Default::default()
        });
    }
    if !workspaces.iter().any(|workspace| !workspace.closed)
        && let Some(first) = workspaces.first_mut()
    {
        first.closed = false;
    }
    Some((settings, StateFile { workspaces, ..Default::default() }))
}

pub fn read() -> Option<(Settings, StateFile)> {
    let output = Command::new("defaults").args(["export", "uk.polishch.nucli", "-"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse(&Value::from_reader_xml(output.stdout.as_slice()).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_preferences() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>theme</key><string>vitesse-dark</string>
<key>fontSize</key><real>14</real>
<key>notifications</key><false/>
<key>profiles</key><array><dict>
<key>name</key><string>work</string><key>executable</key><string>claude</string>
<key>configDirectory</key><string>~/.claude-work</string>
<key>environment</key><array><array><string>A</string><string>1</string></array></array>
</dict><dict><key>name</key><string>old</string><key>command</key><string>x</string></dict></array>
<key>shells</key><array><dict><key>name</key><string>zsh</string><key>path</key><string>/bin/zsh</string></dict></array>
<key>workspaceList</key><array>
<dict><key>name</key><string>Personal</string><key>window</key><integer>0</integer></dict>
<dict><key>name</key><string>Work</string><key>window</key><integer>1</integer><key>closed</key><true/></dict>
</array>
<key>projects</key><array>
<dict><key>path</key><string>/a</string><key>window</key><integer>1</integer><key>name</key><string>A</string><key>color</key><string>red</string></dict>
<dict><key>path</key><string>/b</string><key>window</key><integer>0</integer></dict>
<dict><key>path</key><string>/u</string><key>scratch</key><true/></dict>
</array>
</dict></plist>"#;
        let value = Value::from_reader_xml(xml.as_bytes()).unwrap();
        let (settings, state) = parse(&value).unwrap();
        assert_eq!(settings.theme, "vitesse-dark");
        assert_eq!(settings.font_size, 14.0);
        assert!(!settings.notifications);
        assert_eq!(settings.agents.len(), 1);
        assert_eq!(settings.agents[0].config_dir, "~/.claude-work");
        assert_eq!(settings.agents[0].env.get("A").map(String::as_str), Some("1"));
        assert_eq!(settings.shells[0].path, "/bin/zsh");
        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.workspaces[0].projects[0].path, PathBuf::from("/b"));
        assert_eq!(state.workspaces[1].projects[0].name.as_deref(), Some("A"));
        assert_eq!(state.workspaces[1].projects[0].color, Some(crate::theme::ItemColor::Red));
        assert!(state.workspaces[1].closed);
    }
}
