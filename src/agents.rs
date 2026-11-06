use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::paths;

pub const KNOWN: [&str; 13] = [
    "claude",
    "codex",
    "gemini",
    "opencode",
    "pi",
    "amp",
    "aider",
    "cursor-agent",
    "goose",
    "crush",
    "qwen",
    "copilot",
    "droid",
];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
    Pi,
}

impl AgentKind {
    pub fn from_executable(executable: &str) -> Option<AgentKind> {
        match command_name(executable).as_str() {
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            "pi" => Some(AgentKind::Pi),
            _ => None,
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Pi => "pi",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude Code",
            AgentKind::Codex => "Codex",
            AgentKind::Pi => "pi",
        }
    }

    pub fn config_variable(self) -> &'static str {
        match self {
            AgentKind::Claude => "CLAUDE_CONFIG_DIR",
            AgentKind::Codex => "CODEX_HOME",
            AgentKind::Pi => "PI_CODING_AGENT_DIR",
        }
    }

    pub fn default_config_dir(self) -> &'static str {
        match self {
            AgentKind::Claude => "~/.claude",
            AgentKind::Codex => "~/.codex",
            AgentKind::Pi => "~/.pi/agent",
        }
    }

    pub fn assigns_conversation_id(self) -> bool {
        self != AgentKind::Codex
    }

    pub fn new_arguments(self, id: &str) -> Vec<String> {
        if self.assigns_conversation_id() { vec!["--session-id".into(), id.into()] } else { Vec::new() }
    }

    pub fn resume_arguments(self, id: &str) -> Vec<String> {
        match self {
            AgentKind::Claude => vec!["--resume".into(), id.into()],
            AgentKind::Codex => vec!["resume".into(), id.into()],
            AgentKind::Pi => vec!["--session-id".into(), id.into()],
        }
    }
}

pub fn command_name(executable: &str) -> String {
    let name = executable.rsplit(['/', '\\']).next().unwrap_or(executable).to_lowercase();
    for suffix in [".exe", ".cmd", ".bat", ".ps1"] {
        if let Some(stripped) = name.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    name
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct AgentProfile {
    pub name: String,
    pub executable: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arguments: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub config_dir: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub theme: String,
}

impl AgentProfile {
    pub fn kind(&self) -> Option<AgentKind> {
        AgentKind::from_executable(&self.executable)
    }

    pub fn is_launchable(&self) -> bool {
        !self.executable.trim().is_empty()
    }

    pub fn data_dir(&self) -> Option<PathBuf> {
        let kind = self.kind()?;
        let configured = if self.config_dir.is_empty() {
            self.env.get(kind.config_variable()).cloned().unwrap_or_else(|| kind.default_config_dir().into())
        } else {
            self.config_dir.clone()
        };
        Some(paths::expand(&configured))
    }

    pub fn summary(&self) -> String {
        match self.kind() {
            Some(kind) if !self.config_dir.is_empty() => {
                format!("{} · {}", kind.display_name(), self.config_dir)
            }
            _ if self.arguments.is_empty() => self.executable.clone(),
            _ => format!("{} {}", self.executable, self.arguments),
        }
    }

    pub fn environment(&self) -> Vec<(String, String)> {
        let mut variables: Vec<(String, String)> =
            self.env.iter().filter(|(key, _)| !key.is_empty()).map(|(k, v)| (k.clone(), v.clone())).collect();
        if let Some(kind) = self.kind()
            && !self.config_dir.is_empty()
        {
            variables.retain(|(key, _)| key != kind.config_variable());
            variables.push((kind.config_variable().into(), self.config_dir.clone()));
        }
        variables
    }

    pub fn shell_command(&self, extra: &[String]) -> String {
        let assignments = self
            .environment()
            .into_iter()
            .map(|(key, value)| format!("{key}={}", quote(&paths::expand(&value).to_string_lossy())));
        let program = std::iter::once(quote(&paths::expand(&self.executable).to_string_lossy()))
            .chain((!self.arguments.is_empty()).then(|| self.arguments.clone()))
            .chain(extra.iter().map(|argument| quote(argument)));
        assignments.chain(std::iter::once("exec".to_string())).chain(program).collect::<Vec<_>>().join(" ")
    }

    pub fn windows_command(&self, extra: &[String]) -> String {
        std::iter::once(paths::expand(&self.executable).to_string_lossy().into_owned())
            .chain((!self.arguments.is_empty()).then(|| self.arguments.clone()))
            .chain(extra.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct ShellProfile {
    pub name: String,
    pub path: String,
}

impl ShellProfile {
    pub fn is_launchable(&self) -> bool {
        let path = paths::expand(&self.path);
        path.is_file() || (!path.is_absolute() && find_in_path(&self.path).is_some())
    }
}

pub fn login_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| Path::new(shell).is_file())
        .unwrap_or_else(|| if cfg!(target_os = "macos") { "/bin/zsh".into() } else { "/bin/bash".into() })
}

pub fn detect_shells() -> Vec<ShellProfile> {
    let mut shells: Vec<ShellProfile> = Vec::new();
    let mut add = |name: String, path: String| {
        if shells.iter().any(|shell| shell.path == path) {
            return;
        }
        let name = if shells.iter().any(|shell| shell.name == name) { path.clone() } else { name };
        shells.push(ShellProfile { name, path });
    };
    if cfg!(windows) {
        let program_files = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let candidates = [
            ("PowerShell", find_in_path("pwsh.exe")),
            (
                "Windows PowerShell",
                Some(PathBuf::from(format!("{system}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"))),
            ),
            ("Command Prompt", Some(PathBuf::from(format!("{system}\\System32\\cmd.exe")))),
            ("Git Bash", Some(PathBuf::from(format!("{program_files}\\Git\\bin\\bash.exe")))),
            ("WSL", Some(PathBuf::from(format!("{system}\\System32\\wsl.exe")))),
        ];
        for (name, path) in candidates {
            if let Some(path) = path.filter(|path| path.is_file()) {
                add(name.into(), path.to_string_lossy().into_owned());
            }
        }
        return shells;
    }
    let listed = std::fs::read_to_string("/etc/shells").unwrap_or_default();
    let paths = std::iter::once(login_shell()).chain(
        listed.lines().map(str::trim).filter(|line| line.starts_with('/')).map(String::from).collect::<Vec<_>>(),
    );
    for path in paths {
        if is_executable(Path::new(&path)) {
            add(command_name(&path), path);
        }
    }
    shells
}

pub fn detect_agents() -> Vec<AgentProfile> {
    let path = login_path();
    let directories = search_directories(&path);
    let in_path: HashSet<&PathBuf> = path.iter().collect();
    KNOWN
        .iter()
        .flat_map(|name| {
            let Some((dir, file)) = directories.iter().find_map(|dir| executable_in(dir, name).map(|file| (dir, file)))
            else {
                return Vec::new();
            };
            let executable = if in_path.contains(dir) { name.to_string() } else { file.to_string_lossy().into_owned() };
            variants(name, &executable)
        })
        .collect()
}

fn variants(name: &str, executable: &str) -> Vec<AgentProfile> {
    let markers: &[&str] = match name {
        "claude" => &["projects", "history.jsonl", "settings.json", ".credentials.json"],
        "codex" => &["sessions", "config.toml", "auth.json"],
        _ => &[],
    };
    let plain = AgentProfile { name: name.into(), executable: executable.into(), ..Default::default() };
    if markers.is_empty() {
        return vec![plain];
    }
    let home = paths::home();
    let default_dir = format!(".{name}");
    let mut dirs: Vec<String> = std::fs::read_dir(&home)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|entry| *entry == default_dir || entry.starts_with(&format!("{default_dir}-")))
                .filter(|entry| {
                    home.join(entry).is_dir() && markers.iter().any(|marker| home.join(entry).join(marker).exists())
                })
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    let extra: Vec<AgentProfile> = dirs
        .iter()
        .filter(|dir| **dir != default_dir)
        .map(|dir| AgentProfile {
            name: dir[1..].to_string(),
            executable: executable.into(),
            config_dir: format!("~/{dir}"),
            ..Default::default()
        })
        .collect();
    if dirs.contains(&default_dir) || extra.is_empty() { std::iter::once(plain).chain(extra).collect() } else { extra }
}

fn login_path() -> Vec<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("PATH").map(|path| std::env::split_paths(&path).collect()).unwrap_or_default();
    }
    let output = Command::new(login_shell())
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .env_clear()
        .env("HOME", paths::home())
        .env("USER", std::env::var("USER").unwrap_or_default())
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(output) => {
            String::from_utf8_lossy(&output.stdout).split(':').filter(|p| !p.is_empty()).map(PathBuf::from).collect()
        }
        Err(_) => Vec::new(),
    }
}

fn search_directories(path: &[PathBuf]) -> Vec<PathBuf> {
    let home = paths::home();
    let nvm = home.join(".nvm/versions/node");
    let mut node: Vec<PathBuf> = std::fs::read_dir(&nvm)
        .map(|entries| entries.flatten().map(|entry| entry.path().join("bin")).collect())
        .unwrap_or_default();
    node.sort();
    node.reverse();
    let mut known: Vec<PathBuf> = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        home.join(".local/bin"),
        home.join(".claude/local"),
        home.join(".bun/bin"),
        home.join(".npm-global/bin"),
        home.join(".volta/bin"),
        home.join("Library/pnpm"),
        home.join(".local/share/pnpm"),
        home.join(".deno/bin"),
        home.join(".cargo/bin"),
        home.join("go/bin"),
        home.join(".opencode/bin"),
        home.join(".amp/bin"),
    ];
    if cfg!(windows) {
        if let Some(appdata) = dirs::config_dir() {
            known.push(appdata.join("npm"));
        }
        known.push(home.join(".local\\bin"));
    }
    let mut seen = HashSet::new();
    path.iter().cloned().chain(known).chain(node).filter(|dir| seen.insert(dir.clone())).collect()
}

fn executable_in(dir: &Path, name: &str) -> Option<PathBuf> {
    let candidates: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat"].iter().map(|ext| format!("{name}.{ext}")).collect()
    } else {
        vec![name.to_string()]
    };
    candidates.into_iter().map(|file| dir.join(file)).find(|file| is_executable(file))
}

pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let file = dir.join(name);
        is_executable(&file).then_some(file)
    })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata().map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line() {
        let mut profile = AgentProfile {
            name: "work".into(),
            executable: "claude".into(),
            arguments: "--model opus".into(),
            config_dir: "/tmp/it's".into(),
            ..Default::default()
        };
        profile.env.insert("CLAUDE_CONFIG_DIR".into(), "/ignored".into());
        profile.env.insert("A".into(), "1".into());
        assert_eq!(profile.kind(), Some(AgentKind::Claude));
        assert_eq!(
            profile.shell_command(&["--session-id".into(), "x y".into()]),
            "A='1' CLAUDE_CONFIG_DIR='/tmp/it'\\''s' exec 'claude' --model opus '--session-id' 'x y'"
        );
        assert_eq!(command_name("C:\\npm\\claude.CMD"), "claude");
        assert_eq!(AgentKind::from_executable("/usr/bin/codex"), Some(AgentKind::Codex));
    }
}
