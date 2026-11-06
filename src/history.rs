use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agents::{AgentKind, AgentProfile};
use crate::paths;

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub profile: String,
    pub kind: AgentKind,
    pub dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub modified: SystemTime,
    pub kind: AgentKind,
    pub profile: String,
    pub cwd: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
struct Meta {
    modified: SystemTime,
    id: Option<String>,
    cwd: Option<PathBuf>,
    title: Option<String>,
}

static CACHE: Mutex<Option<HashMap<PathBuf, Meta>>> = Mutex::new(None);
static CHANGED: AtomicBool = AtomicBool::new(false);

pub fn sources(profiles: &[AgentProfile]) -> Vec<Source> {
    let mut sources: Vec<Source> = Vec::new();
    for profile in profiles {
        let (Some(kind), Some(dir)) = (profile.kind(), profile.data_dir()) else { continue };
        if !sources.iter().any(|source| source.dir == dir) {
            sources.push(Source { profile: profile.name.clone(), kind, dir });
        }
    }
    sources
}

pub fn list(cwd: &Path, sources: &[Source], limit: usize) -> Vec<Conversation> {
    let found = sources
        .iter()
        .flat_map(|source| {
            let read = |(path, modified): (PathBuf, SystemTime)| conversation(source, &path, modified);
            let folder = match source.kind {
                AgentKind::Claude => claude_project_dir(&source.dir, cwd),
                AgentKind::Pi => pi_session_dir(&source.dir, cwd),
                AgentKind::Codex => {
                    return codex_files(&source.dir)
                        .into_iter()
                        .filter_map(read)
                        .filter(|c| c.cwd == cwd)
                        .take(limit)
                        .collect::<Vec<_>>();
                }
            };
            newest(&folder, 0, limit.saturating_mul(2)).into_iter().filter_map(read).take(limit).collect()
        })
        .collect();
    finish(found, limit)
}

pub fn all(sources: &[Source], limit: usize) -> Vec<Conversation> {
    let found = sources
        .iter()
        .flat_map(|source| {
            let files = match source.kind {
                AgentKind::Claude => newest(&source.dir.join("projects"), 1, limit),
                AgentKind::Codex => codex_files(&source.dir).into_iter().take(limit).collect(),
                AgentKind::Pi => newest(&source.dir.join("sessions"), 1, limit),
            };
            files.into_iter().filter_map(|(path, modified)| conversation(source, &path, modified)).collect::<Vec<_>>()
        })
        .collect();
    finish(found, limit)
}

pub fn claude_project_dir(dir: &Path, cwd: &Path) -> PathBuf {
    let encoded: String =
        cwd.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    dir.join("projects").join(encoded)
}

pub fn pi_session_dir(dir: &Path, cwd: &Path) -> PathBuf {
    let text = cwd.to_string_lossy();
    let relative = text.strip_prefix(['/', '\\']).unwrap_or(&text);
    let encoded: String =
        relative.chars().filter(|c| *c != ':').map(|c| if c == '/' || c == '\\' { '-' } else { c }).collect();
    dir.join("sessions").join(format!("--{encoded}--"))
}

pub fn claude_conversation_exists(dir: &Path, cwd: &Path, id: &str) -> bool {
    claude_project_dir(dir, cwd).join(format!("{id}.jsonl")).is_file()
}

pub fn newest_codex_conversation(
    dir: &Path,
    cwd: &Path,
    started_after: SystemTime,
    excluding: &HashSet<String>,
) -> Option<String> {
    codex_files(dir).into_iter().take_while(|(_, modified)| *modified >= started_after).find_map(|(path, modified)| {
        let meta = cached(&path, modified, codex_meta);
        let id = meta.id?;
        (meta.cwd.as_deref() == Some(cwd) && !excluding.contains(&id)).then_some(id)
    })
}

pub fn relative_age(time: SystemTime) -> String {
    let seconds = SystemTime::now().duration_since(time).unwrap_or_default().as_secs();
    let (value, unit) = match seconds {
        0..60 => return "now".into(),
        60..3600 => (seconds / 60, "m"),
        3600..86400 => (seconds / 3600, "h"),
        86400..604800 => (seconds / 86400, "d"),
        604800..2592000 => (seconds / 604800, "w"),
        2592000..31536000 => (seconds / 2592000, "mo"),
        _ => (seconds / 31536000, "y"),
    };
    format!("{value}{unit}")
}

fn conversation(source: &Source, path: &Path, modified: SystemTime) -> Option<Conversation> {
    let read: fn(&Path, SystemTime) -> Meta = match source.kind {
        AgentKind::Claude => claude_meta,
        AgentKind::Codex => codex_meta,
        AgentKind::Pi => pi_meta,
    };
    let meta = cached(path, modified, read);
    let (id, title) = match source.kind {
        AgentKind::Claude => (path.file_stem()?.to_string_lossy().into_owned(), meta.title?),
        AgentKind::Codex => (meta.id?, meta.title.unwrap_or_else(|| "Codex session".into())),
        AgentKind::Pi => (meta.id?, meta.title?),
    };
    Some(Conversation { id, title, modified, kind: source.kind, profile: source.profile.clone(), cwd: meta.cwd? })
}

fn finish(mut found: Vec<Conversation>, limit: usize) -> Vec<Conversation> {
    found.sort_by_key(|conversation| std::cmp::Reverse(conversation.modified));
    found.truncate(limit);
    save_cache();
    found
}

fn codex_files(dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    newest(&dir.join("sessions"), usize::MAX, 300)
}

fn newest(dir: &Path, depth: usize, limit: usize) -> Vec<(PathBuf, SystemTime)> {
    let mut files = Vec::new();
    jsonl_files(dir, depth, &mut files);
    files.sort_by_key(|file| std::cmp::Reverse(file.1));
    files.truncate(limit);
    files
}

fn jsonl_files(dir: &Path, depth: usize, files: &mut Vec<(PathBuf, SystemTime)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else { continue };
        let path = entry.path();
        if metadata.is_dir() {
            if depth > 0 {
                jsonl_files(&path, depth - 1, files);
            }
        } else if path.extension().is_some_and(|extension| extension == "jsonl")
            && let Ok(modified) = metadata.modified()
        {
            files.push((path, modified));
        }
    }
}

fn cached(path: &Path, modified: SystemTime, read: fn(&Path, SystemTime) -> Meta) -> Meta {
    let hit = with_cache(|cache| cache.get(path).filter(|meta| meta.modified == modified).cloned());
    if let Some(meta) = hit {
        return meta;
    }
    let meta = read(path, modified);
    with_cache(|cache| cache.insert(path.to_path_buf(), meta.clone()));
    CHANGED.store(true, Ordering::Relaxed);
    meta
}

fn with_cache<T>(action: impl FnOnce(&mut HashMap<PathBuf, Meta>) -> T) -> T {
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    action(cache.get_or_insert_with(|| {
        fs::read(cache_file()).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }))
}

fn save_cache() {
    let cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(cache) = cache.as_ref()
        && CHANGED.swap(false, Ordering::Relaxed)
        && let Ok(bytes) = serde_json::to_vec(cache)
    {
        let _ = paths::write_atomic(&cache_file(), &bytes);
    }
}

fn cache_file() -> PathBuf {
    paths::cache_dir().join("history.json")
}

fn read_prefix(path: &Path, limit: u64) -> String {
    let mut bytes = Vec::new();
    if let Ok(file) = File::open(path) {
        let _ = file.take(limit).read_to_end(&mut bytes);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn claude_meta(path: &Path, modified: SystemTime) -> Meta {
    let text = read_prefix(path, 33554432);
    let (mut custom, mut generated, mut prompt, mut cwd) = (None, None, None, None);
    for line in text.lines() {
        let wanted = (cwd.is_none() && line.contains("\"cwd\""))
            || line.contains("\"custom-title\"")
            || line.contains("\"ai-title\"")
            || (prompt.is_none() && line.contains("\"type\":\"user\""));
        if !wanted {
            continue;
        }
        let Ok(json) = serde_json::from_str::<Value>(line) else { continue };
        if cwd.is_none() {
            cwd = json["cwd"].as_str().map(PathBuf::from);
        }
        match json["type"].as_str() {
            Some("custom-title") => custom = json["customTitle"].as_str().map(String::from).or(custom),
            Some("ai-title") => generated = json["aiTitle"].as_str().map(String::from).or(generated),
            Some("user") if prompt.is_none() && json["isMeta"] != true && json["isCompactSummary"] != true => {
                prompt = prompt_text(&json["message"]["content"]);
            }
            _ => {}
        }
    }
    Meta { modified, id: None, cwd, title: custom.or(generated).or(prompt) }
}

fn codex_meta(path: &Path, modified: SystemTime) -> Meta {
    let mut meta = Meta { modified, id: None, cwd: None, title: None };
    for line in read_prefix(path, 262144).lines() {
        let Ok(json) = serde_json::from_str::<Value>(line) else { continue };
        let payload = &json["payload"];
        match json["type"].as_str() {
            Some("session_meta") => {
                meta.id = payload["id"].as_str().map(String::from);
                meta.cwd = payload["cwd"].as_str().map(PathBuf::from);
            }
            Some("event_msg") if meta.title.is_none() && payload["type"] == "user_message" => {
                meta.title = prompt_text(&payload["message"]);
            }
            _ => {}
        }
        if meta.id.is_some() && meta.title.is_some() {
            break;
        }
    }
    meta
}

fn pi_meta(path: &Path, modified: SystemTime) -> Meta {
    let mut meta = Meta { modified, id: None, cwd: None, title: None };
    for line in read_prefix(path, 1048576).lines() {
        let Ok(json) = serde_json::from_str::<Value>(line) else { continue };
        match json["type"].as_str() {
            Some("session") => {
                meta.id = json["id"].as_str().map(String::from);
                meta.cwd = json["cwd"].as_str().map(PathBuf::from);
            }
            Some("message") if meta.title.is_none() && json["message"]["role"] == "user" => {
                meta.title = prompt_text(&json["message"]["content"]);
            }
            _ => {}
        }
        if meta.id.is_some() && meta.title.is_some() {
            break;
        }
    }
    meta
}

fn prompt_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(text) => text.as_str(),
        Value::Array(parts) => parts.iter().find(|part| part["type"] == "text")?["text"].as_str()?,
        _ => return None,
    };
    let text = text.trim();
    if text.is_empty() || text.starts_with('<') {
        return None;
    }
    Some(text.chars().take(200).collect::<String>().replace(['\r', '\n'], " "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nucli-history-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, lines: &[&str], age: u64) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, lines.join("\n")).unwrap();
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(age)).unwrap();
    }

    fn source(kind: AgentKind, dir: &Path) -> Source {
        Source { profile: format!("{kind:?}"), kind, dir: dir.to_path_buf() }
    }

    #[test]
    fn encodes_folders() {
        let dir = Path::new("/data");
        assert_eq!(claude_project_dir(dir, Path::new("/Users/a/my.app")), dir.join("projects/-Users-a-my-app"));
        assert_eq!(claude_project_dir(dir, Path::new("C:\\x")), dir.join("projects/C--x"));
        assert_eq!(pi_session_dir(dir, Path::new("/Users/a/b")), dir.join("sessions/--Users-a-b--"));
        assert_eq!(pi_session_dir(dir, Path::new("C:\\x\\y")), dir.join("sessions/--C-x-y--"));
    }

    #[test]
    fn reads_conversations_of_every_agent() {
        let root = temp_dir("all");
        let cwd = Path::new("/work/app");
        let claude = root.join("claude");
        let project = claude_project_dir(&claude, cwd);
        write(
            &project.join("named.jsonl"),
            &[
                r#"{"type":"user","isMeta":true,"cwd":"/work/app","message":{"content":"meta"}}"#,
                r#"{"type":"user","cwd":"/work/app","message":{"content":[{"type":"image"},{"type":"text","text":"  fix\nthe bug  "}]}}"#,
                r#"{"type":"ai-title","aiTitle":"Generated"}"#,
                "not json",
                r#"{"type":"custom-title","customTitle":"First"}"#,
                r#"{"type":"custom-title","customTitle":"Renamed"}"#,
            ],
            30,
        );
        write(
            &project.join("prompt.jsonl"),
            &[
                r#"{"type":"user","cwd":"/work/app","message":{"content":"<command-name>/clear</command-name>"}}"#,
                &format!(r#"{{"type":"user","cwd":"/work/app","message":{{"content":"{}"}}}}"#, "é".repeat(300)),
            ],
            20,
        );
        write(&project.join("empty.jsonl"), &[r#"{"type":"summary","summary":"x"}"#], 10);
        write(&project.join("older/subagents/agent-1.jsonl"), &[r#"{"type":"user","message":{"content":"x"}}"#], 0);
        let codex = root.join("codex");
        write(
            &codex.join("sessions/2026/11/05/rollout-a.jsonl"),
            &[
                r#"{"type":"session_meta","payload":{"id":"codex-a","cwd":"/work/app"}}"#,
                r#"{"type":"event_msg","payload":{"type":"user_message","message":"Add tests"}}"#,
            ],
            40,
        );
        write(
            &codex.join("sessions/2026/11/04/rollout-b.jsonl"),
            &[r#"{"type":"session_meta","payload":{"id":"codex-b","cwd":"/elsewhere"}}"#],
            50,
        );
        let pi = root.join("pi");
        write(
            &pi_session_dir(&pi, cwd).join("one.jsonl"),
            &[
                r#"{"type":"session","id":"pi-1","cwd":"/work/app"}"#,
                r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"Hello pi"}]}}"#,
            ],
            5,
        );
        let sources =
            [source(AgentKind::Claude, &claude), source(AgentKind::Codex, &codex), source(AgentKind::Pi, &pi)];

        let found: Vec<(String, String)> =
            list(cwd, &sources, 10).into_iter().map(|c| (c.id, c.title.chars().take(12).collect())).collect();
        assert_eq!(
            found,
            [
                ("pi-1".into(), "Hello pi".into()),
                ("prompt".into(), "é".repeat(12)),
                ("named".into(), "Renamed".into()),
                ("codex-a".into(), "Add tests".into()),
            ]
        );
        assert_eq!(list(cwd, &sources, 10)[1].title.chars().count(), 200);
        let everything = all(&sources, 10);
        assert_eq!(everything.len(), 5);
        assert_eq!(everything[4].title, "Codex session");
        assert_eq!(everything[4].cwd, Path::new("/elsewhere"));
        assert_eq!(all(&sources, 2).len(), 2);
        assert!(claude_conversation_exists(&claude, cwd, "named"));
        assert!(!claude_conversation_exists(&claude, cwd, "missing"));

        let hour_ago = SystemTime::now() - Duration::from_secs(3600);
        let none = HashSet::new();
        assert_eq!(newest_codex_conversation(&codex, cwd, hour_ago, &none).as_deref(), Some("codex-a"));
        assert_eq!(newest_codex_conversation(&codex, cwd, hour_ago, &HashSet::from(["codex-a".into()])), None);
        assert_eq!(newest_codex_conversation(&codex, cwd, SystemTime::now(), &none), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deduplicates_sources() {
        let profile = |name: &str, executable: &str, config_dir: &str| AgentProfile {
            name: name.into(),
            executable: executable.into(),
            config_dir: config_dir.into(),
            ..Default::default()
        };
        let found = sources(&[
            profile("first", "claude", "/c"),
            profile("second", "/bin/claude", "/c"),
            profile("other", "aider", "/c"),
            profile("work", "codex", "/x"),
        ]);
        assert_eq!(
            found,
            [
                Source { profile: "first".into(), kind: AgentKind::Claude, dir: "/c".into() },
                Source { profile: "work".into(), kind: AgentKind::Codex, dir: "/x".into() },
            ]
        );
    }

    #[test]
    fn formats_ages() {
        let ago = |seconds: u64| relative_age(SystemTime::now() - Duration::from_secs(seconds));
        assert_eq!(ago(5), "now");
        assert_eq!(ago(300), "5m");
        assert_eq!(ago(3 * 3600 + 10), "3h");
        assert_eq!(ago(2 * 86400 + 10), "2d");
        assert_eq!(ago(29 * 86400), "4w");
        assert_eq!(ago(7 * 2592000 + 10), "7mo");
        assert_eq!(ago(2 * 31536000 + 10), "2y");
        assert_eq!(relative_age(SystemTime::now() + Duration::from_secs(100)), "now");
    }
}
