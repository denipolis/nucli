use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cwd: Option<PathBuf>,
}

static SYSTEM: Mutex<Option<System>> = Mutex::new(None);

fn file_name(path: &str) -> String {
    let lower = path.rsplit(['/', '\\']).next().unwrap_or(path).to_lowercase();
    for suffix in [".exe", ".js", ".mjs", ".cjs", ".py"] {
        if let Some(stripped) = lower.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    lower
}

pub fn command_name(command: &[String], fallback: &str) -> String {
    let program = command.first().map(|program| file_name(program)).unwrap_or_else(|| file_name(fallback));
    let interpreted = matches!(program.as_str(), "node" | "bun" | "deno" | "python" | "python3");
    if interpreted && let Some(script) = command.iter().skip(1).find(|argument| !argument.starts_with('-')) {
        return file_name(script);
    }
    program.trim_start_matches('-').to_string()
}

pub fn inspect(pid: u32, root: Option<u32>) -> Option<ProcessInfo> {
    let mut guard = SYSTEM.lock().ok()?;
    if guard.is_none() {
        *guard = System::new().ok();
    }
    let system = guard.as_mut()?;
    let kind = ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_cwd(UpdateKind::Always);
    let target = if cfg!(windows) {
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
        newest_descendant(system, Pid::from_u32(root.unwrap_or(pid)))
    } else {
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true, kind);
        Pid::from_u32(pid)
    };
    let process = system.process(target)?;
    let command: Vec<String> = process.cmd().iter().map(|part| part.to_string_lossy().into_owned()).collect();
    Some(ProcessInfo {
        pid: target.as_u32(),
        name: command_name(&command, &process.name().to_string_lossy()),
        cwd: process.cwd().map(Path::to_path_buf),
    })
}

fn newest_descendant(system: &System, root: Pid) -> Pid {
    let mut current = root;
    loop {
        let child = system
            .processes()
            .values()
            .filter(|process| process.parent() == Some(current))
            .max_by_key(|process| process.start_time())
            .map(|process| process.pid());
        match child {
            Some(child) => current = child,
            None => return current,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_commands() {
        let parts = |items: &[&str]| items.iter().map(|item| item.to_string()).collect::<Vec<_>>();
        assert_eq!(command_name(&parts(&["/usr/bin/node", "/opt/bin/claude", "--resume"]), "node"), "claude");
        assert_eq!(command_name(&parts(&["-zsh"]), "zsh"), "zsh");
        assert_eq!(command_name(&parts(&["C:\\bin\\codex.exe"]), ""), "codex");
        assert_eq!(command_name(&[], "vim"), "vim");
    }
}
