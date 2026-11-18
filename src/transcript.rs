use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Claude,
    Codex,
    Pi,
    Gemini,
}

pub use nucli_core::chips::Entry;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Parsed {
    pub conversation: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub tokens: Option<u64>,
    pub peak: u64,
    pub window: Option<u64>,
    pub memory: Vec<Entry>,
    pub skills: Vec<Entry>,
    pub used_skills: Vec<Entry>,
    pub servers: BTreeMap<String, String>,
    pub calls: BTreeMap<String, usize>,
    pub compactions: usize,
    pub limits: Vec<crate::usage::Limit>,
}

#[derive(Default)]
struct Tail {
    offset: u64,
    parsed: Parsed,
}

static TAILS: Mutex<Option<HashMap<PathBuf, Tail>>> = Mutex::new(None);

pub fn read(path: &Path, format: Format) -> Option<Parsed> {
    let len = std::fs::metadata(path).ok()?.len();
    let mut guard = TAILS.lock().unwrap_or_else(PoisonError::into_inner);
    let tails = guard.get_or_insert_with(HashMap::new);
    if tails.len() >= 64 && !tails.contains_key(path) {
        tails.clear();
    }
    let tail = tails.entry(path.to_path_buf()).or_default();
    if len < tail.offset {
        *tail = Tail::default();
    }
    if len > tail.offset {
        let mut file = File::open(path).ok()?;
        file.seek(SeekFrom::Start(tail.offset)).ok()?;
        let mut bytes = Vec::new();
        file.take(len - tail.offset).read_to_end(&mut bytes).ok()?;
        if let Some(end) = bytes.iter().rposition(|byte| *byte == b'\n') {
            for line in String::from_utf8_lossy(&bytes[..end]).lines() {
                parse_line(format, line, &mut tail.parsed);
            }
            tail.offset += end as u64 + 1;
        }
    }
    Some(tail.parsed.clone())
}

pub fn parse_line(format: Format, line: &str, parsed: &mut Parsed) {
    let wanted = match format {
        Format::Claude => {
            line.contains("\"usage\"")
                || line.contains("\"attachment\"")
                || line.contains("Base directory for this skill")
                || line.contains("compact_boundary")
        }
        Format::Codex => !line.contains("\"function_call_output\"") && !line.contains("\"reasoning\""),
        Format::Pi => !line.contains("\"role\":\"toolResult\""),
        Format::Gemini => !line.starts_with("{\"$"),
    };
    if !wanted {
        return;
    }
    let Ok(json) = serde_json::from_str::<Value>(line) else { return };
    match format {
        Format::Claude => claude(&json, parsed),
        Format::Codex => codex(&json, parsed),
        Format::Pi => pi(&json, parsed),
        Format::Gemini => gemini(&json, parsed),
    }
}

impl Parsed {
    fn set_tokens(&mut self, tokens: u64) {
        self.tokens = Some(tokens);
        self.peak = self.peak.max(tokens);
    }

    fn call(&mut self, server: &str) {
        *self.calls.entry(server.to_string()).or_default() += 1;
    }

    fn server(&mut self, name: &str) {
        let key = server_key(name);
        if !key.is_empty() && self.servers.get(&key).is_none_or(|display| display == &key) {
            self.servers.insert(key, name.to_string());
        }
    }

    fn use_skill(&mut self, name: String, path: Option<PathBuf>) {
        if name.is_empty() || self.used_skills.iter().any(|skill| skill.name == name) {
            return;
        }
        self.used_skills.push(Entry { name, path, used: true, ..Default::default() });
    }

    fn add_memory(&mut self, path: &str, detail: &str) {
        if path.is_empty() || self.memory.iter().any(|entry| entry.path.as_deref() == Some(Path::new(path))) {
            return;
        }
        self.memory.push(memory_entry(Path::new(path), detail));
    }
}

pub fn memory_entry(path: &Path, detail: &str) -> Entry {
    Entry {
        name: path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
        path: Some(path.to_path_buf()),
        detail: detail.to_string(),
        ..Default::default()
    }
}

pub fn server_key(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

pub fn mcp_server(tool: &str) -> Option<&str> {
    let rest = tool.strip_prefix("mcp__")?;
    let server = rest.split("__").next()?;
    (!server.is_empty() && rest.len() > server.len()).then_some(server)
}

fn text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts.iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

pub fn skill_name(dir: &Path) -> String {
    let parts: Vec<String> = dir
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let base = parts.last().cloned().unwrap_or_default();
    let plugin = parts
        .windows(2)
        .position(|pair| pair[0] == "plugins" && pair[1] == "cache")
        .and_then(|index| parts.get(index + 3))
        .filter(|_| parts.len() > 1);
    match plugin {
        Some(plugin) => format!("{plugin}:{base}"),
        None => base,
    }
}

fn skill_dir(text: &str) -> Option<PathBuf> {
    let line = text.lines().find_map(|line| line.trim().strip_prefix("Base directory for this skill:"))?;
    let dir = line.trim();
    (!dir.is_empty()).then(|| PathBuf::from(dir))
}

pub fn listing(content: &str) -> Vec<Entry> {
    content
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .filter_map(|line| {
            let (name, description) = line.split_once(": ").unwrap_or((line.trim_end_matches(':'), ""));
            let name = name.trim();
            (!name.is_empty() && !name.contains(' ')).then(|| Entry {
                name: name.to_string(),
                description: description.trim().to_string(),
                ..Default::default()
            })
        })
        .collect()
}

fn claude(json: &Value, parsed: &mut Parsed) {
    if json["isSidechain"] == true {
        return;
    }
    match json["type"].as_str().unwrap_or_default() {
        "assistant" => {
            let message = &json["message"];
            if let Some(model) = message["model"].as_str().filter(|model| !model.starts_with('<')) {
                parsed.model = Some(model.to_string());
            }
            let usage = &message["usage"];
            let tokens: u64 = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
                .iter()
                .filter_map(|key| usage[key].as_u64())
                .sum();
            if tokens > 0 {
                parsed.set_tokens(tokens);
            }
            for block in message["content"].as_array().into_iter().flatten() {
                if block["type"] == "tool_use"
                    && let Some(server) = block["name"].as_str().and_then(mcp_server)
                {
                    parsed.call(server);
                }
            }
        }
        "attachment" => claude_attachment(&json["attachment"], parsed),
        "user" if json["isMeta"] == true => {
            if let Some(dir) = skill_dir(&text(&json["message"]["content"])) {
                parsed.use_skill(skill_name(&dir), Some(dir.join("SKILL.md")));
            }
        }
        "system" if json["subtype"] == "compact_boundary" => {
            parsed.compactions += 1;
            if let Some(tokens) = json["compactMetadata"]["preTokens"].as_u64() {
                parsed.peak = parsed.peak.max(tokens);
            }
        }
        _ => {}
    }
}

fn claude_attachment(attachment: &Value, parsed: &mut Parsed) {
    let names = |key: &str| -> Vec<String> {
        attachment[key].as_array().into_iter().flatten().filter_map(|name| name.as_str().map(String::from)).collect()
    };
    match attachment["type"].as_str().unwrap_or_default() {
        "instructions" => {
            let nested: Vec<Entry> = parsed.memory.drain(..).filter(|entry| entry.detail == "nested").collect();
            for file in attachment["files"].as_array().into_iter().flatten() {
                let detail = file["type"].as_str().unwrap_or_default().to_lowercase();
                parsed.add_memory(file["path"].as_str().unwrap_or_default(), &detail);
            }
            for entry in nested {
                if !parsed.memory.iter().any(|existing| existing.path == entry.path) {
                    parsed.memory.push(entry);
                }
            }
        }
        "nested_memory" => parsed.add_memory(attachment["path"].as_str().unwrap_or_default(), "nested"),
        "skill_listing" => {
            let skills = listing(attachment["content"].as_str().unwrap_or_default());
            if attachment["isInitial"] == false {
                for skill in skills {
                    if !parsed.skills.iter().any(|existing| existing.name == skill.name) {
                        parsed.skills.push(skill);
                    }
                }
            } else if !skills.is_empty() {
                parsed.skills = skills;
            }
        }
        "invoked_skills" => {
            for skill in attachment["skills"].as_array().into_iter().flatten() {
                let dir = skill_dir(skill["content"].as_str().unwrap_or_default());
                let name = skill["name"]
                    .as_str()
                    .map(String::from)
                    .or_else(|| dir.as_deref().map(skill_name))
                    .unwrap_or_default();
                parsed.use_skill(name, dir.map(|dir| dir.join("SKILL.md")));
            }
        }
        "mcp_instructions_delta" => {
            for name in names("addedNames") {
                parsed.server(&name);
            }
            for name in names("removedNames") {
                parsed.servers.remove(&server_key(&name));
            }
        }
        "deferred_tools_delta" => {
            for name in names("addedNames") {
                if let Some(server) = mcp_server(&name) {
                    match server.strip_prefix("claude_ai_") {
                        Some(rest) => parsed.server(&format!("claude.ai {}", rest.replace('_', " "))),
                        None => parsed.server(server),
                    }
                }
            }
        }
        _ => {}
    }
}

fn codex(json: &Value, parsed: &mut Parsed) {
    let payload = &json["payload"];
    match json["type"].as_str().unwrap_or_default() {
        "session_meta" => parsed.conversation = payload["id"].as_str().map(String::from),
        "turn_context" => {
            if let Some(model) = payload["model"].as_str() {
                parsed.model = Some(model.to_string());
            }
        }
        "compacted" => parsed.compactions += 1,
        "event_msg" if payload["type"] == "token_count" => {
            let info = &payload["info"];
            let last = &info["last_token_usage"];
            let tokens = last["total_tokens"].as_u64().unwrap_or_else(|| {
                last["input_tokens"].as_u64().unwrap_or(0) + last["output_tokens"].as_u64().unwrap_or(0)
            });
            if tokens > 0 {
                parsed.set_tokens(tokens);
            }
            if let Some(window) = info["model_context_window"].as_u64() {
                parsed.window = Some(window);
            }
            let at = json["timestamp"].as_str().and_then(crate::usage::parse_time);
            let limits = crate::usage::rollout(&payload["rate_limits"], at);
            if !limits.is_empty() {
                parsed.limits = limits;
            }
        }
        "response_item" => match payload["type"].as_str().unwrap_or_default() {
            "function_call" => {
                let namespace = payload["namespace"].as_str().unwrap_or_default();
                let name = format!("{namespace}{}", payload["name"].as_str().unwrap_or_default());
                if let Some(server) = mcp_server(&name) {
                    parsed.call(server);
                }
            }
            "message" if payload["role"] == "user" => {
                let content = text(&payload["content"]);
                for block in content.split("<skill>").skip(1) {
                    let tag = |name: &str| {
                        let start = block.find(&format!("<{name}>"))? + name.len() + 2;
                        let end = block[start..].find(&format!("</{name}>"))? + start;
                        Some(block[start..end].trim().to_string())
                    };
                    if let Some(name) = tag("name") {
                        parsed.use_skill(name, tag("path").map(PathBuf::from));
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Vec<&'a str> {
    text.split(open).skip(1).filter_map(|part| part.split_once(close).map(|(inside, _)| inside)).collect()
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn pi(json: &Value, parsed: &mut Parsed) {
    match json["type"].as_str().unwrap_or_default() {
        "session" => parsed.conversation = json["id"].as_str().map(String::from),
        "model_change" => {
            parsed.model = json["modelId"].as_str().map(String::from).or(parsed.model.take());
            parsed.provider = json["provider"].as_str().map(String::from).or(parsed.provider.take());
        }
        "compaction" => parsed.compactions += 1,
        "message" => {
            let message = &json["message"];
            match message["role"].as_str().unwrap_or_default() {
                "system" => {
                    let sections = &message["sections"];
                    let context = sections["project_context"].as_str().unwrap_or_default();
                    let paths = between(context, "<project_instructions path=\"", "\"");
                    if !paths.is_empty() {
                        parsed.memory.clear();
                        for path in paths {
                            parsed.add_memory(&unescape(path), "");
                        }
                    }
                    let skills: Vec<Entry> =
                        between(sections["skills"].as_str().unwrap_or_default(), "<skill>", "</skill>")
                            .into_iter()
                            .map(|block| {
                                let tag = |name: &str| {
                                    between(block, &format!("<{name}>"), &format!("</{name}>"))
                                        .first()
                                        .map(|value| unescape(value.trim()))
                                };
                                Entry {
                                    name: tag("name").unwrap_or_default(),
                                    path: tag("location").map(PathBuf::from),
                                    description: tag("description").unwrap_or_default(),
                                    ..Default::default()
                                }
                            })
                            .filter(|entry| !entry.name.is_empty())
                            .collect();
                    if !skills.is_empty() {
                        parsed.skills = skills;
                    }
                }
                "assistant" => {
                    if let Some(model) = message["model"].as_str() {
                        parsed.model = Some(model.to_string());
                    }
                    if let Some(provider) = message["provider"].as_str() {
                        parsed.provider = Some(provider.to_string());
                    }
                    let usage = &message["usage"];
                    let tokens = usage["totalTokens"].as_u64().filter(|total| *total > 0).unwrap_or_else(|| {
                        ["input", "output", "cacheRead", "cacheWrite"]
                            .iter()
                            .filter_map(|key| usage[key].as_u64())
                            .sum()
                    });
                    if tokens > 0 {
                        parsed.set_tokens(tokens);
                    }
                    for block in message["content"].as_array().into_iter().flatten() {
                        let path = block["arguments"]["path"].as_str().unwrap_or_default();
                        if block["type"] == "toolCall" && path.ends_with("SKILL.md") {
                            let path = PathBuf::from(path);
                            let name = parsed
                                .skills
                                .iter()
                                .find(|skill| skill.path.as_ref() == Some(&path))
                                .map(|skill| skill.name.clone())
                                .unwrap_or_else(|| path.parent().map(skill_name).unwrap_or_default());
                            parsed.use_skill(name, Some(path));
                        }
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn gemini(json: &Value, parsed: &mut Parsed) {
    if let Some(id) = json["sessionId"].as_str().filter(|_| json.get("projectHash").is_some()) {
        parsed.conversation = Some(id.to_string());
    }
    if json["type"] != "gemini" {
        return;
    }
    if let Some(model) = json["model"].as_str() {
        parsed.model = Some(model.to_string());
    }
    if let Some(tokens) = json["tokens"]["input"].as_u64().filter(|tokens| *tokens > 0) {
        parsed.set_tokens(tokens);
    }
    for call in json["toolCalls"].as_array().into_iter().flatten() {
        if let Some(server) = call["name"].as_str().and_then(mcp_server) {
            parsed.call(server);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(format: Format, lines: &[&str]) -> Parsed {
        let mut parsed = Parsed::default();
        for line in lines {
            parse_line(format, line, &mut parsed);
        }
        parsed
    }

    #[test]
    fn reads_claude_transcripts() {
        let parsed = parse(
            Format::Claude,
            &[
                r#"{"type":"attachment","attachment":{"type":"instructions","files":[{"path":"/home/u/.claude/CLAUDE.md","type":"User"},{"path":"/work/CLAUDE.md","type":"Project"}]}}"#,
                r#"{"type":"attachment","attachment":{"type":"nested_memory","path":"/work/src/CLAUDE.md"}}"#,
                r#"{"type":"attachment","attachment":{"type":"skill_listing","content":"- summary: Short answers\n- caveman:caveman: Terse mode\nmore text\n- update-config: Edit settings"}}"#,
                r#"{"type":"attachment","attachment":{"type":"mcp_instructions_delta","addedNames":["claude.ai Docs","codegraph"]}}"#,
                r#"{"type":"attachment","attachment":{"type":"deferred_tools_delta","addedNames":["WebFetch","mcp__claude_ai_Docs__read","mcp__redis__get"]}}"#,
                r#"{"type":"user","isMeta":true,"message":{"content":[{"type":"text","text":"Base directory for this skill: /home/u/.claude/plugins/cache/caveman/caveman/2.7.0/skills/caveman\n\n# Caveman"}]}}"#,
                r#"{"type":"assistant","message":{"model":"claude-opus-5-5","usage":{"input_tokens":2,"cache_creation_input_tokens":100,"cache_read_input_tokens":900,"output_tokens":50},"content":[{"type":"tool_use","name":"mcp__redis__get"},{"type":"tool_use","name":"Bash"}]}}"#,
                r#"{"type":"assistant","isSidechain":true,"message":{"model":"claude-haiku","usage":{"input_tokens":5000}}}"#,
                r#"{"type":"assistant","message":{"model":"<synthetic>","usage":{"input_tokens":0}}}"#,
                r#"{"type":"system","subtype":"compact_boundary","compactMetadata":{"preTokens":250000}}"#,
                r#"{"type":"attachment","attachment":{"type":"instructions","files":[{"path":"/work/CLAUDE.md","type":"Project"}]}}"#,
            ],
        );
        assert_eq!(parsed.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(parsed.tokens, Some(1002));
        assert_eq!(parsed.peak, 250000);
        assert_eq!(parsed.compactions, 1);
        let memory: Vec<_> = parsed.memory.iter().map(|entry| (entry.name.as_str(), entry.detail.as_str())).collect();
        assert_eq!(memory, [("CLAUDE.md", "project"), ("CLAUDE.md", "nested")]);
        let skills: Vec<_> = parsed.skills.iter().map(|skill| skill.name.as_str()).collect();
        assert_eq!(skills, ["summary", "caveman:caveman", "update-config"]);
        assert_eq!(parsed.skills[1].description, "Terse mode");
        let mut delta = parsed.clone();
        parse_line(
            Format::Claude,
            r#"{"type":"attachment","attachment":{"type":"skill_listing","isInitial":false,"content":"- fresh: New"}}"#,
            &mut delta,
        );
        assert_eq!(delta.skills.len(), 4);
        assert_eq!(parsed.used_skills[0].name, "caveman:caveman");
        assert_eq!(
            parsed.used_skills[0].path.as_deref(),
            Some(Path::new("/home/u/.claude/plugins/cache/caveman/caveman/2.7.0/skills/caveman/SKILL.md"))
        );
        let servers: Vec<_> = parsed.servers.iter().map(|(key, name)| (key.as_str(), name.as_str())).collect();
        assert_eq!(servers, [("claude_ai_Docs", "claude.ai Docs"), ("codegraph", "codegraph"), ("redis", "redis")]);
        assert_eq!(parsed.calls.get("redis"), Some(&1));
    }

    #[test]
    fn reads_codex_rollouts() {
        let parsed = parse(
            Format::Codex,
            &[
                r#"{"type":"session_meta","payload":{"id":"c-1","cwd":"/work"}}"#,
                r#"{"type":"turn_context","payload":{"model":"gpt-5-codex"}}"#,
                r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#,
                r#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":9000,"output_tokens":500,"total_tokens":9500},"model_context_window":272000}}}"#,
                r#"{"type":"response_item","payload":{"type":"function_call","name":"mcp__github__get_issue","call_id":"1"}}"#,
                r#"{"type":"response_item","payload":{"type":"function_call","namespace":"mcp__linear__","name":"list","call_id":"2"}}"#,
                r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"1","output":"mcp__x__y"}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<skill>\n<name>pdf</name>\n<path>/home/u/.agents/skills/pdf/SKILL.md</path>\nbody</skill>"}]}}"#,
                r#"{"type":"compacted","payload":{"message":""}}"#,
            ],
        );
        assert_eq!(parsed.conversation.as_deref(), Some("c-1"));
        assert_eq!(parsed.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!((parsed.tokens, parsed.window), (Some(9500), Some(272000)));
        assert_eq!(parsed.calls.keys().collect::<Vec<_>>(), ["github", "linear"]);
        assert_eq!(parsed.used_skills[0].name, "pdf");
        assert_eq!(parsed.compactions, 1);
    }

    #[test]
    fn reads_pi_sessions() {
        let parsed = parse(
            Format::Pi,
            &[
                r#"{"type":"session","id":"p-1","cwd":"/work"}"#,
                r#"{"type":"model_change","provider":"local","modelId":"qwen"}"#,
                r#"{"type":"message","message":{"role":"system","sections":{"project_context":"<project_instructions path=\"/home/u/AGENTS.md\">\nx\n</project_instructions>","skills":"<skill>\n<name>brainstorming</name>\n<description>Use &quot;this&quot;</description>\n<location>/s/brainstorming/SKILL.md</location>\n</skill>"}}}"#,
                r#"{"type":"message","message":{"role":"assistant","model":"qwen","provider":"local","usage":{"input":10,"output":5,"cacheRead":100,"cacheWrite":0,"totalTokens":115},"content":[{"type":"toolCall","name":"read","arguments":{"path":"/s/brainstorming/SKILL.md"}}]}}"#,
                r#"{"type":"message","message":{"role":"toolResult","content":"huge"}}"#,
            ],
        );
        assert_eq!(parsed.conversation.as_deref(), Some("p-1"));
        assert_eq!((parsed.model.as_deref(), parsed.provider.as_deref()), (Some("qwen"), Some("local")));
        assert_eq!(parsed.tokens, Some(115));
        assert_eq!(parsed.memory[0].path.as_deref(), Some(Path::new("/home/u/AGENTS.md")));
        assert_eq!(parsed.skills[0].description, "Use \"this\"");
        assert_eq!(parsed.used_skills[0].name, "brainstorming");
    }

    #[test]
    fn reads_gemini_sessions() {
        let parsed = parse(
            Format::Gemini,
            &[
                r#"{"sessionId":"g-1","projectHash":"abc","startTime":"2026-01-01T00:00:00Z"}"#,
                r#"{"id":"1","type":"user","content":"hi"}"#,
                r#"{"id":"2","type":"gemini","model":"gemini-3-pro","tokens":{"input":4000,"output":10,"total":4010}}"#,
                r#"{"$set":{"memoryScratchpad":"x"}}"#,
            ],
        );
        assert_eq!(parsed.conversation.as_deref(), Some("g-1"));
        assert_eq!(parsed.model.as_deref(), Some("gemini-3-pro"));
        assert_eq!(parsed.tokens, Some(4000));
    }

    #[test]
    fn names_skills_and_servers() {
        assert_eq!(
            skill_name(Path::new("/c/plugins/cache/market/superpowers/7.0.0/skills/plans")),
            "superpowers:plans"
        );
        assert_eq!(skill_name(Path::new("/home/u/.claude/skills/summary")), "summary");
        assert_eq!(mcp_server("mcp__claude_ai_Gmail__send"), Some("claude_ai_Gmail"));
        assert_eq!(mcp_server("mcp__broken"), None);
        assert_eq!(mcp_server("Bash"), None);
        assert_eq!(server_key("claude.ai Claude Docs"), "claude_ai_Claude_Docs");
    }

    #[test]
    fn reads_only_new_lines() {
        let path = std::env::temp_dir().join("nucli-transcript-tail.jsonl");
        let usage =
            |tokens: u64| format!(r#"{{"type":"assistant","message":{{"usage":{{"input_tokens":{tokens}}}}}}}"#);
        std::fs::write(&path, format!("{}\n{}", usage(10), &usage(20)[..20])).unwrap();
        assert_eq!(read(&path, Format::Claude).unwrap().tokens, Some(10));
        std::fs::write(&path, format!("{}\n{}\n", usage(10), usage(20))).unwrap();
        assert_eq!(read(&path, Format::Claude).unwrap().tokens, Some(20));
        std::fs::write(&path, format!("{}\n", usage(5))).unwrap();
        assert_eq!(read(&path, Format::Claude).unwrap().tokens, Some(5));
        std::fs::remove_file(path).unwrap();
    }
}
