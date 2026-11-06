use std::collections::HashMap;

use std::rc::Rc;

use gpui::{
    Action, App, DummyKeyboardMapper, Global, KeyBinding, KeyBindingContextPredicate, Menu, MenuItem, NoAction,
    SystemMenuType, actions,
};

use crate::paths;

actions!(
    nucli,
    [
        NewWorkspace,
        RenameWorkspace,
        DeleteWorkspace,
        NewTab,
        NewTabPicker,
        NewTabInFolder,
        NewTabWithoutProject,
        OpenProject,
        QuickOpen,
        CommandPalette,
        Close,
        CloseProject,
        RenameTab,
        SplitRight,
        SplitDown,
        CloseSplit,
        FocusLeft,
        FocusRight,
        FocusUp,
        FocusDown,
        ToggleTerminal,
        NewTerminal,
        TabSwitcher,
        TabSwitcherBack,
        NextTab,
        PreviousTab,
        SelectTab1,
        SelectTab2,
        SelectTab3,
        SelectTab4,
        SelectTab5,
        SelectTab6,
        SelectTab7,
        SelectTab8,
        SelectTab9,
        ToggleSidebar,
        TogglePanel,
        MovePanel,
        FilterSidebar,
        ShowProjects,
        ShowAgents,
        ShowHistory,
        ShowFiles,
        ShowChanges,
        ShowCommits,
        ToggleFullScreen,
        FontBigger,
        FontSmaller,
        FontReset,
        Clear,
        Find,
        FindNext,
        FindPrevious,
        Undo,
        Redo,
        Cut,
        Copy,
        Paste,
        SelectAll,
        Save,
        NextChange,
        PreviousChange,
        HideFileView,
        OpenSettings,
        OpenSettingsFile,
        OpenKeybindings,
        OpenKeybindingsFile,
        SelectTheme,
        DetectAgents,
        DetectShells,
        ToggleNotifications,
        Minimize,
        Zoom,
        Hide,
        HideOthers,
        ShowAll,
        About,
        Quit,
    ]
);

pub struct Command {
    pub name: &'static str,
    pub title: &'static str,
    pub mac: &'static str,
    pub other: &'static str,
    pub context: Option<&'static str>,
    pub action: fn() -> Box<dyn Action>,
}

macro_rules! command {
    ($name:literal, $title:literal, $action:ident, $mac:literal, $other:literal) => {
        Command { name: $name, title: $title, mac: $mac, other: $other, context: None, action: || Box::new($action) }
    };
    ($name:literal, $title:literal, $action:ident, $mac:literal, $other:literal, $context:literal) => {
        Command {
            name: $name,
            title: $title,
            mac: $mac,
            other: $other,
            context: Some($context),
            action: || Box::new($action),
        }
    };
}

pub const COMMANDS: &[Command] = &[
    command!("new_workspace", "New Workspace", NewWorkspace, "cmd-n", "ctrl-shift-n"),
    command!("rename_workspace", "Rename Workspace", RenameWorkspace, "", ""),
    command!("delete_workspace", "Delete Workspace", DeleteWorkspace, "", ""),
    command!("new_tab", "New Tab", NewTab, "cmd-t", "ctrl-shift-t"),
    command!("new_tab_picker", "New Tab…", NewTabPicker, "cmd-shift-t", "ctrl-shift-space"),
    command!("new_tab_in_folder", "New Tab in Folder…", NewTabInFolder, "cmd-alt-shift-t", "ctrl-alt-shift-t"),
    command!("new_tab_without_project", "New Tab Without Project…", NewTabWithoutProject, "", ""),
    command!("open_project", "Open Project…", OpenProject, "cmd-o", "ctrl-shift-o"),
    command!("quick_open", "Quick Open", QuickOpen, "cmd-p", "ctrl-shift-e"),
    command!("command_palette", "Command Palette", CommandPalette, "cmd-shift-p", "ctrl-shift-p"),
    command!("close", "Close", Close, "cmd-w", "ctrl-shift-w"),
    command!("close_project", "Close Project", CloseProject, "cmd-shift-w", "ctrl-alt-shift-w"),
    command!("rename_tab", "Rename Tab", RenameTab, "", ""),
    command!("split_right", "Split Right", SplitRight, "cmd-d", "ctrl-shift-d"),
    command!("split_down", "Split Down", SplitDown, "cmd-shift-d", "ctrl-alt-shift-d"),
    command!("close_split", "Close Split", CloseSplit, "cmd-alt-w", "ctrl-alt-w"),
    command!("focus_left", "Focus Left Split", FocusLeft, "cmd-alt-left", "ctrl-alt-shift-left"),
    command!("focus_right", "Focus Right Split", FocusRight, "cmd-alt-right", "ctrl-alt-shift-right"),
    command!("focus_up", "Focus Upper Split", FocusUp, "cmd-alt-up", "ctrl-alt-shift-up"),
    command!("focus_down", "Focus Lower Split", FocusDown, "cmd-alt-down", "ctrl-alt-shift-down"),
    command!("toggle_terminal", "Toggle Terminal Panel", ToggleTerminal, "ctrl-`", "ctrl-`"),
    command!("new_terminal", "New Terminal", NewTerminal, "ctrl-shift-`", "ctrl-shift-`"),
    command!("tab_switcher", "Tab Switcher", TabSwitcher, "ctrl-tab", "ctrl-tab"),
    command!("tab_switcher_back", "Tab Switcher Back", TabSwitcherBack, "ctrl-shift-tab", "ctrl-shift-tab"),
    command!("next_tab", "Next Tab", NextTab, "cmd-shift-]", "ctrl-pagedown"),
    command!("previous_tab", "Previous Tab", PreviousTab, "cmd-shift-[", "ctrl-pageup"),
    command!("select_tab_1", "Tab 1", SelectTab1, "cmd-1", "alt-1"),
    command!("select_tab_2", "Tab 2", SelectTab2, "cmd-2", "alt-2"),
    command!("select_tab_3", "Tab 3", SelectTab3, "cmd-3", "alt-3"),
    command!("select_tab_4", "Tab 4", SelectTab4, "cmd-4", "alt-4"),
    command!("select_tab_5", "Tab 5", SelectTab5, "cmd-5", "alt-5"),
    command!("select_tab_6", "Tab 6", SelectTab6, "cmd-6", "alt-6"),
    command!("select_tab_7", "Tab 7", SelectTab7, "cmd-7", "alt-7"),
    command!("select_tab_8", "Tab 8", SelectTab8, "cmd-8", "alt-8"),
    command!("select_tab_9", "Tab 9", SelectTab9, "cmd-9", "alt-9"),
    command!("toggle_sidebar", "Toggle Sidebar", ToggleSidebar, "cmd-b", "ctrl-shift-b"),
    command!("toggle_panel", "Toggle Panel", TogglePanel, "cmd-alt-b", "ctrl-alt-b"),
    command!("move_panel", "Move Panel to Other Side", MovePanel, "", ""),
    command!("filter_sidebar", "Filter Sidebar", FilterSidebar, "cmd-alt-f", "ctrl-alt-f"),
    command!("show_projects", "Sidebar: Projects", ShowProjects, "cmd-alt-1", "ctrl-alt-1"),
    command!("show_agents", "Sidebar: Agents", ShowAgents, "cmd-alt-2", "ctrl-alt-2"),
    command!("show_history", "Sidebar: History", ShowHistory, "cmd-alt-3", "ctrl-alt-3"),
    command!("show_files", "Panel: Files", ShowFiles, "ctrl-cmd-1", "ctrl-shift-1"),
    command!("show_changes", "Panel: Changes", ShowChanges, "ctrl-cmd-2", "ctrl-shift-2"),
    command!("show_commits", "Panel: History", ShowCommits, "ctrl-cmd-3", "ctrl-shift-3"),
    command!("toggle_full_screen", "Toggle Full Screen", ToggleFullScreen, "ctrl-cmd-f", "f11"),
    command!("font_bigger", "Bigger Font", FontBigger, "cmd-=", "ctrl-="),
    command!("font_smaller", "Smaller Font", FontSmaller, "cmd--", "ctrl--"),
    command!("font_reset", "Reset Font Size", FontReset, "cmd-0", "ctrl-0"),
    command!("clear", "Clear Terminal", Clear, "cmd-k", "ctrl-shift-k", "Terminal"),
    command!("find", "Find", Find, "cmd-f", "ctrl-shift-f"),
    command!("find_next", "Find Next", FindNext, "cmd-g", "f3"),
    command!("find_previous", "Find Previous", FindPrevious, "cmd-shift-g", "shift-f3"),
    command!("undo", "Undo", Undo, "cmd-z", "ctrl-z", "FileView"),
    command!("redo", "Redo", Redo, "cmd-shift-z", "ctrl-shift-z", "FileView"),
    command!("cut", "Cut", Cut, "cmd-x", "ctrl-x", "FileView"),
    command!("copy", "Copy", Copy, "cmd-c", "ctrl-shift-c", "Terminal"),
    command!("paste", "Paste", Paste, "cmd-v", "ctrl-shift-v", "Terminal"),
    command!("select_all", "Select All", SelectAll, "cmd-a", "ctrl-shift-a", "Terminal"),
    command!("save", "Save File", Save, "cmd-s", "ctrl-s", "FileView"),
    command!("next_change", "Next Change", NextChange, "f7", "f7", "FileView"),
    command!("previous_change", "Previous Change", PreviousChange, "shift-f7", "shift-f7", "FileView"),
    command!("hide_file_view", "Hide File View", HideFileView, "escape", "escape", "FileView"),
    command!("open_settings", "Open Settings", OpenSettings, "cmd-,", "ctrl-,"),
    command!("open_settings_file", "Open Settings File", OpenSettingsFile, "", ""),
    command!("open_keybindings", "Open Keybindings", OpenKeybindings, "", ""),
    command!("open_keybindings_file", "Open Keybindings File", OpenKeybindingsFile, "", ""),
    command!("select_theme", "Select Theme", SelectTheme, "", ""),
    command!("detect_agents", "Detect Agents", DetectAgents, "", ""),
    command!("detect_shells", "Detect Shells", DetectShells, "", ""),
    command!("toggle_notifications", "Toggle Notifications", ToggleNotifications, "", ""),
    command!("minimize", "Minimize", Minimize, "cmd-m", ""),
    command!("zoom", "Zoom", Zoom, "", ""),
    command!("hide", "Hide Nucli", Hide, "cmd-h", ""),
    command!("hide_others", "Hide Others", HideOthers, "cmd-alt-h", ""),
    command!("show_all", "Show All", ShowAll, "", ""),
    command!("about", "About Nucli", About, "", ""),
    command!("quit", "Quit", Quit, "cmd-q", "ctrl-shift-q"),
];

pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.name == name)
}

pub fn default_key(command: &Command) -> &'static str {
    if cfg!(target_os = "macos") { command.mac } else { command.other }
}

#[derive(Default)]
pub struct Keys {
    pub by_name: HashMap<&'static str, String>,
    pub error: Option<String>,
}

impl Global for Keys {}

pub fn key_for(name: &str, cx: &App) -> String {
    cx.try_global::<Keys>().and_then(|keys| keys.by_name.get(name)).map(|key| display(key)).unwrap_or_default()
}

pub fn display(key: &str) -> String {
    let mac = cfg!(target_os = "macos");
    let parts: Vec<&str> = if key.ends_with("--") {
        let mut parts: Vec<&str> = key.trim_end_matches("--").split('-').filter(|part| !part.is_empty()).collect();
        parts.push("-");
        parts
    } else {
        key.split('-').collect()
    };
    let (last, modifiers) = parts.split_last().unwrap_or((&"", &[]));
    let mut ordered: Vec<&str> = Vec::new();
    for wanted in ["ctrl", "alt", "shift", "cmd"] {
        if modifiers.contains(&wanted) {
            ordered.push(wanted);
        }
    }
    let name = match *last {
        "left" => "←".to_string(),
        "right" => "→".to_string(),
        "up" => "↑".to_string(),
        "down" => "↓".to_string(),
        "enter" => {
            if mac {
                "↩".into()
            } else {
                "Enter".into()
            }
        }
        "escape" => "Esc".into(),
        "tab" => "Tab".into(),
        "space" => "Space".into(),
        "backspace" => "⌫".into(),
        "pageup" => "PgUp".into(),
        "pagedown" => "PgDn".into(),
        other => other.to_uppercase(),
    };
    if mac {
        let symbols: String = ordered
            .iter()
            .map(|modifier| match *modifier {
                "ctrl" => "⌃",
                "alt" => "⌥",
                "shift" => "⇧",
                _ => "⌘",
            })
            .collect();
        format!("{symbols}{name}")
    } else {
        let mut words: Vec<String> = ordered
            .iter()
            .map(|modifier| match *modifier {
                "ctrl" => "Ctrl".to_string(),
                "alt" => "Alt".to_string(),
                "shift" => "Shift".to_string(),
                _ => "Super".to_string(),
            })
            .collect();
        words.push(name);
        words.join("+")
    }
}

pub fn read_overrides() -> Result<Vec<(String, Option<String>)>, String> {
    let text = match std::fs::read_to_string(paths::keybindings_file()) {
        Ok(text) => text,
        Err(_) => return Ok(Vec::new()),
    };
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&text).map_err(|error| format!("keybindings.json: {error}"))?;
    Ok(map.into_iter().map(|(key, value)| (key, value.as_str().map(String::from))).collect())
}

fn rebound(map: &mut serde_json::Map<String, serde_json::Value>, name: &str, default: &str, key: Option<&str>) {
    map.retain(|bound, value| value.as_str() != Some(name) && !(bound == default && value.is_null()));
    if let Some(key) = key.filter(|key| *key != default) {
        map.insert(key.to_string(), serde_json::Value::String(name.to_string()));
        if !default.is_empty() {
            map.entry(default.to_string()).or_insert(serde_json::Value::Null);
        }
    }
}

pub fn rebind(name: &str, key: Option<&str>) {
    let Some(command) = find(name) else { return };
    let mut map = match std::fs::read_to_string(paths::keybindings_file()) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(map) => map,
            Err(_) => return,
        },
        Err(_) => serde_json::Map::new(),
    };
    rebound(&mut map, name, default_key(command), key);
    if let Ok(json) = serde_json::to_string_pretty(&map) {
        let _ = paths::write_atomic(&paths::keybindings_file(), format!("{json}\n").as_bytes());
    }
}

pub fn shifted_alias(key: &str) -> Option<String> {
    let (head, last) = key.rsplit_once('-')?;
    let symbol = match last {
        "1" => "!",
        "2" => "@",
        "3" => "#",
        "4" => "$",
        "5" => "%",
        "6" => "^",
        "7" => "&",
        "8" => "*",
        "9" => "(",
        "0" => ")",
        "`" => "~",
        "[" => "{",
        "]" => "}",
        "=" => "+",
        ";" => ":",
        "'" => "\"",
        "," => "<",
        "." => ">",
        "/" => "?",
        "\\" => "|",
        _ => return None,
    };
    let mut modifiers: Vec<&str> = head.split('-').collect();
    let before = modifiers.len();
    modifiers.retain(|modifier| *modifier != "shift");
    if modifiers.len() == before {
        return None;
    }
    modifiers.push(symbol);
    Some(modifiers.join("-"))
}

fn bindings_for(key: &str, action: fn() -> Box<dyn Action>, context: Option<&str>) -> Vec<KeyBinding> {
    let alias = shifted_alias(key);
    std::iter::once(key).chain(alias.as_deref()).filter_map(|key| binding(key, action(), context)).collect()
}

pub fn binding(key: &str, action: Box<dyn Action>, context: Option<&str>) -> Option<KeyBinding> {
    let predicate = match context {
        Some(context) => Some(Rc::new(KeyBindingContextPredicate::parse(context).ok()?)),
        None => None,
    };
    KeyBinding::load(key, action, predicate, false, None, &DummyKeyboardMapper).ok()
}

pub fn apply(cx: &mut App, extra: Vec<KeyBinding>) {
    cx.clear_key_bindings();
    cx.bind_keys(extra);
    let mut keys = Keys::default();
    let mut bindings = Vec::new();
    for command in COMMANDS {
        let key = default_key(command);
        if !key.is_empty() {
            keys.by_name.insert(command.name, key.to_string());
            bindings.extend(bindings_for(key, command.action, command.context));
        }
    }
    match read_overrides() {
        Ok(overrides) => {
            for (key, name) in overrides {
                match name.as_deref().and_then(find) {
                    Some(command) => {
                        keys.by_name.insert(command.name, key.clone());
                        bindings.extend(bindings_for(&key, command.action, command.context));
                    }
                    None => {
                        keys.by_name.retain(|_, bound| *bound != key);
                        for context in [None, Some("Terminal"), Some("FileView")] {
                            bindings.extend(bindings_for(&key, || Box::new(NoAction), context));
                        }
                    }
                }
            }
        }
        Err(error) => keys.error = Some(error),
    }
    cx.bind_keys(bindings);
    cx.set_global(keys);
}

pub fn menus() -> Vec<Menu> {
    let items = |names: &[&str]| -> Vec<MenuItem> {
        names
            .iter()
            .map(|name| match find(name) {
                Some(command) => {
                    MenuItem::Action { name: command.title.into(), action: (command.action)(), os_action: None }
                }
                None => MenuItem::separator(),
            })
            .collect()
    };
    vec![
        Menu {
            name: "Nucli".into(),
            items: items(&["about", "-", "open_settings", "-"])
                .into_iter()
                .chain([MenuItem::os_submenu("Services", SystemMenuType::Services), MenuItem::separator()])
                .chain(items(&["hide", "hide_others", "show_all", "-", "quit"]))
                .collect(),
        },
        Menu {
            name: "File".into(),
            items: items(&[
                "new_workspace",
                "rename_workspace",
                "delete_workspace",
                "-",
                "new_tab",
                "new_tab_picker",
                "new_tab_in_folder",
                "new_tab_without_project",
                "open_project",
                "quick_open",
                "-",
                "split_right",
                "split_down",
                "close_split",
                "toggle_terminal",
                "new_terminal",
                "-",
                "rename_tab",
                "save",
                "close",
                "close_project",
            ]),
        },
        Menu {
            name: "Edit".into(),
            items: items(&[
                "undo",
                "redo",
                "-",
                "cut",
                "copy",
                "paste",
                "select_all",
                "-",
                "find",
                "find_next",
                "find_previous",
                "-",
                "clear",
            ]),
        },
        Menu {
            name: "View".into(),
            items: items(&[
                "command_palette",
                "-",
                "toggle_sidebar",
                "toggle_panel",
                "move_panel",
                "filter_sidebar",
                "-",
                "show_projects",
                "show_agents",
                "show_history",
                "show_files",
                "show_changes",
                "show_commits",
                "-",
                "next_change",
                "previous_change",
                "-",
                "focus_left",
                "focus_right",
                "focus_up",
                "focus_down",
                "-",
                "font_bigger",
                "font_smaller",
                "font_reset",
                "-",
                "toggle_full_screen",
            ]),
        },
        Menu {
            name: "Window".into(),
            items: items(&[
                "minimize",
                "zoom",
                "-",
                "next_tab",
                "previous_tab",
                "-",
                "select_tab_1",
                "select_tab_2",
                "select_tab_3",
                "select_tab_4",
                "select_tab_5",
                "select_tab_6",
                "select_tab_7",
                "select_tab_8",
                "select_tab_9",
            ]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebinds_keys() {
        let mut map = serde_json::Map::new();
        rebound(&mut map, "new_tab", "cmd-t", Some("cmd-j"));
        assert_eq!(map.get("cmd-j").and_then(|value| value.as_str()), Some("new_tab"));
        assert!(map.get("cmd-t").is_some_and(|value| value.is_null()));
        rebound(&mut map, "new_tab", "cmd-t", Some("cmd-y"));
        assert!(!map.contains_key("cmd-j"));
        map.insert("cmd-t".into(), serde_json::Value::String("other".into()));
        rebound(&mut map, "new_tab", "cmd-t", None);
        assert!(!map.contains_key("cmd-y"));
        assert_eq!(map.get("cmd-t").and_then(|value| value.as_str()), Some("other"));
        rebound(&mut map, "other", "", None);
        assert!(map.is_empty());
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<&str> = COMMANDS.iter().map(|command| command.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len());
    }

    #[test]
    fn aliases_shifted_symbols() {
        assert_eq!(shifted_alias("ctrl-shift-2").as_deref(), Some("ctrl-@"));
        assert_eq!(shifted_alias("cmd-shift-]").as_deref(), Some("cmd-}"));
        assert_eq!(shifted_alias("ctrl-shift-`").as_deref(), Some("ctrl-~"));
        assert_eq!(shifted_alias("ctrl-shift-t"), None);
        assert_eq!(shifted_alias("cmd-1"), None);
    }

    #[test]
    fn display_keys() {
        if cfg!(target_os = "macos") {
            assert_eq!(display("cmd-shift-t"), "⇧⌘T");
            assert_eq!(display("cmd--"), "⌘-");
        } else {
            assert_eq!(display("ctrl-shift-t"), "Ctrl+Shift+T");
            assert_eq!(display("ctrl--"), "Ctrl+-");
        }
    }
}
