use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::history;

#[derive(Clone, Debug, PartialEq)]
pub struct ClaudeProcess {
    pub session_id: String,
    pub status: String,
    pub cwd: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Subagent {
    pub id: String,
    pub description: String,
    pub kind: String,
}

pub fn process(pid: u32, dirs: &[PathBuf]) -> Option<ClaudeProcess> {
    dirs.iter().find_map(|dir| {
        let json = read_json(&dir.join("sessions").join(format!("{pid}.json")))?;
        Some(ClaudeProcess {
            session_id: json["sessionId"].as_str()?.to_string(),
            status: json["status"].as_str().unwrap_or_default().to_string(),
            cwd: json["cwd"].as_str().map(PathBuf::from),
        })
    })
}

pub fn subagents_dir(dir: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    history::claude_project_dir(dir, cwd).join(session_id).join("subagents")
}

pub fn running_subagents(folder: &Path) -> Vec<Subagent> {
    let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
    let now = SystemTime::now();
    let mut running: Vec<(SystemTime, Subagent)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let base = name.strip_suffix(".jsonl")?;
            let id = base.strip_prefix("agent-")?;
            let modified = entry.metadata().ok()?.modified().ok()?;
            if now.duration_since(modified).unwrap_or_default() >= Duration::from_secs(600) || finished(&entry.path()) {
                return None;
            }
            let meta = read_json(&folder.join(format!("{base}.meta.json"))).unwrap_or_default();
            let description = meta["description"].as_str().unwrap_or(id).to_string();
            Some((
                modified,
                Subagent { id: id.into(), description, kind: meta["agentType"].as_str().unwrap_or_default().into() },
            ))
        })
        .collect();
    running.sort_by_key(|(modified, _)| *modified);
    running.into_iter().map(|(_, subagent)| subagent).collect()
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn finished(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else { return true };
    let size = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(size.saturating_sub(65536))).is_err() || file.read_to_end(&mut bytes).is_err() {
        return false;
    }
    let text = String::from_utf8_lossy(&bytes);
    let Some(last) = text.lines().map(str::trim).rfind(|line| !line.is_empty()) else { return false };
    let Ok(json) = serde_json::from_str::<Value>(last) else { return false };
    json["toolEndsTurn"] == true || (json["type"] == "assistant" && json["message"]["stop_reason"] == "end_turn")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nucli-claude-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str, age: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(age)).unwrap();
    }

    #[test]
    fn reads_process_status() {
        let root = temp_dir("process");
        let (first, second) = (root.join("first"), root.join("second"));
        write(&first.join("sessions/42.json"), "{\"status\":\"busy\"}", 0);
        write(&first.join("sessions/42.abc.key"), "secret", 0);
        write(&second.join("sessions/42.json"), r#"{"sessionId":"s1","status":"idle","cwd":"/work"}"#, 0);
        assert_eq!(
            process(42, &[first.clone(), second]),
            Some(ClaudeProcess { session_id: "s1".into(), status: "idle".into(), cwd: Some("/work".into()) })
        );
        assert_eq!(process(7, &[first]), None);
        assert_eq!(
            subagents_dir(Path::new("/c"), Path::new("/work/my.app"), "s1"),
            Path::new("/c/projects/-work-my-app/s1/subagents")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lists_running_subagents() {
        let folder = temp_dir("subagents");
        let busy = r#"{"type":"assistant","message":{"stop_reason":"tool_use"}}"#;
        write(&folder.join("agent-new.jsonl"), &format!("{}\n{busy}\n\n", "x".repeat(70000)), 5);
        write(&folder.join("agent-new.meta.json"), r#"{"description":"Search code","agentType":"Explore"}"#, 5);
        write(&folder.join("agent-old.jsonl"), "{\"type\":\"user\"}\n{\"type\":\"assistant\",\"message\":", 60);
        write(&folder.join("agent-done.jsonl"), r#"{"type":"assistant","message":{"stop_reason":"end_turn"}}"#, 1);
        write(&folder.join("agent-tool.jsonl"), r#"{"type":"user","toolEndsTurn":true}"#, 1);
        write(&folder.join("agent-stale.jsonl"), busy, 601);
        write(&folder.join("notes.jsonl"), busy, 1);
        assert_eq!(
            running_subagents(&folder),
            [
                Subagent { id: "old".into(), description: "old".into(), kind: String::new() },
                Subagent { id: "new".into(), description: "Search code".into(), kind: "Explore".into() },
            ]
        );
        assert!(running_subagents(&folder.join("missing")).is_empty());
        fs::remove_dir_all(folder).unwrap();
    }
}
