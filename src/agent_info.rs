use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::history;
use crate::paths;
use crate::transcript::{self, Entry, Format, Parsed};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct AgentInfo {
    pub agent: String,
    pub model: Option<String>,
    pub tokens: Option<u64>,
    pub window: Option<u64>,
    pub skills: Vec<Entry>,
    pub memory: Vec<Entry>,
    pub mcp: Vec<Entry>,
    pub started: Option<SystemTime>,
    pub transcript: Option<PathBuf>,
    pub conversation: Option<String>,
    pub compactions: usize,
    pub compacting: bool,
    pub limits: Vec<crate::usage::Limit>,
}

pub struct Request {
    pub agent: String,
    pub pid: u32,
    pub cwd: PathBuf,
    pub started: Option<SystemTime>,
    pub conversation: Option<String>,
    pub dirs: Vec<PathBuf>,
    pub usage: bool,
}

struct Layout {
    agent: &'static str,
    config: &'static str,
    format: Option<Format>,
    global_memory: &'static [&'static str],
    memory: &'static [&'static str],
    to_root: bool,
    global_skills: &'static [&'static str],
    skills: &'static [&'static str],
    mcp: &'static [(&'static str, &'static str)],
}

const LAYOUTS: [Layout; 12] = [
    Layout {
        agent: "claude",
        config: "~/.claude",
        format: Some(Format::Claude),
        global_memory: &["{config}/CLAUDE.md", "{config}/rules/"],
        memory: &["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md", ".claude/rules/"],
        to_root: true,
        global_skills: &["{config}/skills"],
        skills: &[".claude/skills"],
        mcp: &[("./.mcp.json", "mcpServers")],
    },
    Layout {
        agent: "codex",
        config: "~/.codex",
        format: Some(Format::Codex),
        global_memory: &["{config}/AGENTS.override.md|{config}/AGENTS.md"],
        memory: &["AGENTS.override.md|AGENTS.md"],
        to_root: false,
        global_skills: &["~/.agents/skills", "{config}/skills", "/etc/codex/skills"],
        skills: &[".agents/skills"],
        mcp: &[("{config}/config.toml", "mcp_servers")],
    },
    Layout {
        agent: "pi",
        config: "~/.pi/agent",
        format: Some(Format::Pi),
        global_memory: &["{config}/AGENTS.md"],
        memory: &["AGENTS.md|CLAUDE.md"],
        to_root: true,
        global_skills: &["{config}/skills", "~/.agents/skills"],
        skills: &[".pi/skills", ".agents/skills"],
        mcp: &[],
    },
    Layout {
        agent: "gemini",
        config: "~/.gemini",
        format: Some(Format::Gemini),
        global_memory: &["{config}/GEMINI.md"],
        memory: &["GEMINI.md"],
        to_root: false,
        global_skills: &["{config}/skills", "~/.agents/skills"],
        skills: &[".gemini/skills", ".agents/skills"],
        mcp: &[("{config}/settings.json", "mcpServers"), ("./.gemini/settings.json", "mcpServers")],
    },
    Layout {
        agent: "qwen",
        config: "~/.qwen",
        format: None,
        global_memory: &["{config}/QWEN.md"],
        memory: &["QWEN.md"],
        to_root: false,
        global_skills: &["{config}/skills"],
        skills: &[".qwen/skills"],
        mcp: &[("{config}/settings.json", "mcpServers"), ("./.qwen/settings.json", "mcpServers")],
    },
    Layout {
        agent: "opencode",
        config: "~/.config/opencode",
        format: None,
        global_memory: &["{config}/AGENTS.md|~/.claude/CLAUDE.md"],
        memory: &["AGENTS.md|CLAUDE.md"],
        to_root: false,
        global_skills: &["{config}/skills", "~/.claude/skills", "~/.agents/skills"],
        skills: &[".opencode/skills", ".claude/skills", ".agents/skills"],
        mcp: &[("{config}/opencode.json", "mcp"), ("./opencode.json", "mcp")],
    },
    Layout {
        agent: "copilot",
        config: "~/.copilot",
        format: None,
        global_memory: &["{config}/copilot-instructions.md"],
        memory: &["AGENTS.md", ".github/copilot-instructions.md", ".github/instructions/"],
        to_root: false,
        global_skills: &["{config}/skills"],
        skills: &[".github/skills", ".claude/skills"],
        mcp: &[("{config}/mcp-config.json", "mcpServers")],
    },
    Layout {
        agent: "amp",
        config: "~/.config/amp",
        format: None,
        global_memory: &["{config}/AGENTS.md"],
        memory: &["AGENTS.md|AGENT.md|CLAUDE.md"],
        to_root: false,
        global_skills: &["~/.config/agents/skills"],
        skills: &[".agents/skills"],
        mcp: &[("{config}/settings.json", "amp.mcpServers")],
    },
    Layout {
        agent: "droid",
        config: "~/.factory",
        format: None,
        global_memory: &["{config}/AGENTS.md"],
        memory: &["AGENTS.md"],
        to_root: false,
        global_skills: &["{config}/skills"],
        skills: &[".factory/skills"],
        mcp: &[("{config}/mcp.json", "mcpServers"), ("./.factory/mcp.json", "mcpServers")],
    },
    Layout {
        agent: "cursor-agent",
        config: "~/.cursor",
        format: None,
        global_memory: &[],
        memory: &["AGENTS.md", "CLAUDE.md", ".cursor/rules/"],
        to_root: false,
        global_skills: &["{config}/skills"],
        skills: &[".cursor/skills"],
        mcp: &[("{config}/mcp.json", "mcpServers"), ("./.cursor/mcp.json", "mcpServers")],
    },
    Layout {
        agent: "crush",
        config: "~/.config/crush",
        format: None,
        global_memory: &[],
        memory: &["CRUSH.md|crush.md", "AGENTS.md", "CLAUDE.md"],
        to_root: false,
        global_skills: &[],
        skills: &[],
        mcp: &[("{config}/crush.json", "mcp"), ("./crush.json", "mcp"), ("./.crush.json", "mcp")],
    },
    Layout {
        agent: "goose",
        config: "~/.config/goose",
        format: None,
        global_memory: &["{config}/.goosehints"],
        memory: &[".goosehints", "AGENTS.md"],
        to_root: false,
        global_skills: &[],
        skills: &[],
        mcp: &[],
    },
];

#[derive(Clone, Default)]
struct Found {
    memory: Vec<Entry>,
    skills: Vec<Entry>,
    servers: Vec<(String, PathBuf)>,
    windows: HashMap<String, u64>,
    long_context: bool,
}

type Discovered = HashMap<(String, PathBuf, PathBuf), (Instant, Found)>;

static DISCOVERED: Mutex<Option<Discovered>> = Mutex::new(None);
type Located = HashMap<(u32, Option<String>), PathBuf>;

static LOCATED: Mutex<Option<Located>> = Mutex::new(None);

const ONE_MILLION: u64 = 1000000;

pub fn probe(request: &Request) -> AgentInfo {
    let mut info = AgentInfo { agent: request.agent.clone(), started: request.started, ..Default::default() };
    let Some(layout) = LAYOUTS.iter().find(|layout| layout.agent == request.agent) else { return info };
    let dirs = if request.dirs.is_empty() { vec![paths::expand(layout.config)] } else { request.dirs.clone() };
    let transcript = layout.format.and_then(|format| locate(format, request, &dirs));
    let config =
        transcript.as_ref().and_then(|path| dirs.iter().find(|dir| path.starts_with(dir))).unwrap_or(&dirs[0]).clone();
    let parsed = transcript
        .as_ref()
        .zip(layout.format)
        .and_then(|(path, format)| transcript::read(path, format))
        .unwrap_or_default();
    let found = discover(layout, &config, &request.cwd);
    info.window = window(&parsed, &found);
    info.model = parsed.model.clone();
    info.tokens = parsed.tokens;
    info.compactions = parsed.compactions;
    info.conversation = parsed.conversation.clone().or_else(|| request.conversation.clone());
    info.memory = if parsed.memory.is_empty() { found.memory.clone() } else { parsed.memory.clone() };
    info.skills = skills(&parsed, &found);
    info.mcp = servers(&parsed, &found);
    if request.usage {
        info.limits = crate::usage::limits(layout.agent, &config, &parsed);
    }
    info.transcript = transcript;
    info
}

fn window(parsed: &Parsed, found: &Found) -> Option<u64> {
    let model = parsed.model.as_deref().unwrap_or_default();
    let known = parsed
        .window
        .or_else(|| (model.ends_with("[1m]") || found.long_context).then_some(ONE_MILLION))
        .or_else(|| found.windows.get(model).copied())
        .or_else(|| family_window(model));
    match known {
        Some(window) if parsed.peak > window && window < ONE_MILLION => Some(ONE_MILLION),
        other => other,
    }
}

fn family_window(model: &str) -> Option<u64> {
    let model = model.to_lowercase();
    if ["claude", "opus", "sonnet", "haiku"].iter().any(|name| model.contains(name)) {
        Some(200000)
    } else if model.contains("gemini") {
        Some(1048576)
    } else if model.contains("gpt-4.1") {
        Some(1047576)
    } else if model.contains("gpt-5") {
        Some(400000)
    } else {
        None
    }
}

fn skills(parsed: &Parsed, found: &Found) -> Vec<Entry> {
    let mut skills: Vec<Entry> = if parsed.skills.is_empty() {
        found.skills.clone()
    } else {
        parsed
            .skills
            .iter()
            .map(|listed| match found.skills.iter().find(|skill| skill.name == listed.name) {
                Some(skill) => Entry {
                    path: listed.path.clone().or_else(|| skill.path.clone()),
                    detail: skill.detail.clone(),
                    description: if listed.description.is_empty() {
                        skill.description.clone()
                    } else {
                        listed.description.clone()
                    },
                    ..listed.clone()
                },
                None if listed.path.is_some() => listed.clone(),
                None => Entry { detail: "built-in".into(), ..listed.clone() },
            })
            .collect()
    };
    for used in &parsed.used_skills {
        match skills.iter_mut().find(|skill| skill.name == used.name) {
            Some(skill) => {
                skill.used = true;
                if skill.path.is_none() {
                    skill.path = used.path.clone();
                }
            }
            None => skills.push(used.clone()),
        }
    }
    skills.sort_by_key(|skill| !skill.used);
    skills
}

fn servers(parsed: &Parsed, found: &Found) -> Vec<Entry> {
    let mut servers: BTreeMap<String, Entry> = BTreeMap::new();
    for (name, file) in &found.servers {
        servers.entry(transcript::server_key(name)).or_insert_with(|| Entry {
            name: name.clone(),
            path: Some(file.clone()),
            ..Default::default()
        });
    }
    for (key, name) in &parsed.servers {
        let entry = servers.entry(key.clone()).or_default();
        if entry.name.is_empty() || entry.name == *key {
            entry.name = name.clone();
        }
    }
    for (key, calls) in &parsed.calls {
        let entry = servers.entry(key.clone()).or_default();
        if entry.name.is_empty() {
            entry.name = key.clone();
        }
        entry.used = true;
        entry.detail = if *calls == 1 { "1 call".into() } else { format!("{calls} calls") };
    }
    let mut list: Vec<Entry> = servers.into_values().collect();
    list.sort_by_key(|server| !server.used);
    list
}

fn resolve(template: &str, config: &Path, root: &Path) -> PathBuf {
    if let Some(rest) = template.strip_prefix("{config}/") {
        config.join(rest)
    } else if let Some(rest) = template.strip_prefix("./") {
        root.join(rest)
    } else {
        paths::expand(template)
    }
}

pub fn project_root(cwd: &Path) -> PathBuf {
    cwd.ancestors().find(|dir| dir.join(".git").exists()).unwrap_or(cwd).to_path_buf()
}

fn walked(cwd: &Path, to_root: bool) -> Vec<PathBuf> {
    let stop = if to_root { None } else { Some(project_root(cwd)) };
    let mut dirs = Vec::new();
    for dir in cwd.ancestors() {
        dirs.push(dir.to_path_buf());
        if stop.as_deref() == Some(dir) {
            break;
        }
    }
    dirs.reverse();
    dirs
}

fn files_matching(pattern: &str, base: impl Fn(&str) -> PathBuf) -> Vec<PathBuf> {
    if pattern.ends_with('/') {
        let folder = base(pattern);
        let mut files: Vec<PathBuf> = fs::read_dir(&folder)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.extension().is_some_and(|extension| extension == "md" || extension == "mdc"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        return files;
    }
    pattern.split('|').map(&base).find(|path| path.is_file()).into_iter().collect()
}

fn discover(layout: &Layout, config: &Path, cwd: &Path) -> Found {
    let key = (layout.agent.to_string(), config.to_path_buf(), cwd.to_path_buf());
    let hit = DISCOVERED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .get(&key)
        .filter(|(time, _)| time.elapsed() < Duration::from_secs(10))
        .map(|(_, found)| found.clone());
    if let Some(found) = hit {
        return found;
    }
    let found = scan(layout, config, cwd);
    let mut guard = DISCOVERED.lock().unwrap_or_else(PoisonError::into_inner);
    let cache = guard.get_or_insert_with(HashMap::new);
    if cache.len() >= 64 {
        cache.clear();
    }
    cache.insert(key, (Instant::now(), found.clone()));
    found
}

fn scan(layout: &Layout, config: &Path, cwd: &Path) -> Found {
    let root = project_root(cwd);
    let mut found = Found::default();
    for pattern in layout.global_memory {
        for path in files_matching(pattern, |part| resolve(part, config, &root)) {
            found.memory.push(transcript::memory_entry(&path, "user"));
        }
    }
    let dirs = walked(cwd, layout.to_root);
    for dir in &dirs {
        for pattern in layout.memory {
            for path in files_matching(pattern, |part| dir.join(part)) {
                if !found.memory.iter().any(|entry| entry.path.as_ref() == Some(&path)) {
                    let detail = if pattern.contains("local") { "local" } else { "project" };
                    found.memory.push(transcript::memory_entry(&path, detail));
                }
            }
        }
    }
    for folder in layout.skills.iter().rev() {
        for dir in dirs.iter().rev() {
            skills_in(&dir.join(folder), "project", "", &mut found.skills, 1);
        }
    }
    for folder in layout.global_skills {
        skills_in(&resolve(folder, config, &root), "user", "", &mut found.skills, 1);
    }
    if layout.agent == "claude" {
        for dir in dirs.iter().rev() {
            commands_in(&dir.join(".claude/commands"), "project", "", &mut found.skills);
        }
        commands_in(&config.join("commands"), "user", "", &mut found.skills);
        claude_plugins(config, &mut found.skills);
        claude_servers(config, cwd, &root, &mut found.servers);
        claude_windows(config, &mut found);
    }
    if layout.agent == "pi" {
        pi_windows(config, &mut found);
    }
    for (template, key) in layout.mcp {
        let file = resolve(template, config, &root);
        let names = if file.extension().is_some_and(|extension| extension == "toml") {
            toml_tables(&fs::read_to_string(&file).unwrap_or_default(), key)
        } else {
            read_json(&file)[*key].as_object().map(|servers| servers.keys().cloned().collect()).unwrap_or_default()
        };
        for name in names {
            if !found.servers.iter().any(|(existing, _)| *existing == name) {
                found.servers.push((name, file.clone()));
            }
        }
    }
    found
}

fn read_json(path: &Path) -> Value {
    fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

pub fn toml_tables(text: &str, key: &str) -> Vec<String> {
    let prefix = format!("[{key}.");
    let mut names: Vec<String> = Vec::new();
    for line in text.lines().map(str::trim) {
        let Some(rest) = line.strip_prefix(&prefix) else { continue };
        let name = match rest.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next().unwrap_or_default(),
            None => rest.split(['.', ']']).next().unwrap_or_default(),
        };
        if !name.is_empty() && !names.iter().any(|existing| existing == name) {
            names.push(name.to_string());
        }
    }
    names
}

pub fn frontmatter(text: &str) -> (Option<String>, String) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, String::new());
    }
    let (mut name, mut description) = (None, String::new());
    for line in lines.take_while(|line| line.trim() != "---") {
        let value = |key: &str| line.strip_prefix(key).map(|value| value.trim().trim_matches(['"', '\'']).to_string());
        if let Some(value) = value("name:") {
            name = Some(value).filter(|value| !value.is_empty());
        } else if let Some(value) = value("description:").filter(|value| !value.starts_with(['>', '|'])) {
            description = value;
        }
    }
    (name, description)
}

fn skills_in(folder: &Path, detail: &str, prefix: &str, skills: &mut Vec<Entry>, depth: usize) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
    dirs.sort();
    for dir in dirs {
        let file = dir.join("SKILL.md");
        if !file.is_file() {
            if depth > 0 {
                skills_in(&dir, detail, prefix, skills, depth - 1);
            }
            continue;
        }
        let mut head = String::new();
        if let Ok(opened) = File::open(&file) {
            let _ = opened.take(4096).read_to_string(&mut head);
        }
        let (name, description) = frontmatter(&head);
        let name = name.unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().into_owned());
        let name = if prefix.is_empty() { name } else { format!("{prefix}:{name}") };
        if !skills.iter().any(|skill| skill.name == name) {
            skills.push(Entry { name, path: Some(file), detail: detail.into(), description, used: false });
        }
    }
}

fn commands_in(folder: &Path, detail: &str, prefix: &str, skills: &mut Vec<Entry>) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    files.sort();
    for file in files {
        let stem = file.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let name = if prefix.is_empty() { stem } else { format!("{prefix}:{stem}") };
        if !skills.iter().any(|skill| skill.name == name) {
            let (_, description) = frontmatter(&fs::read_to_string(&file).unwrap_or_default());
            skills.push(Entry { name, path: Some(file), detail: detail.into(), description, used: false });
        }
    }
}

fn claude_plugins(config: &Path, skills: &mut Vec<Entry>) {
    let installed = read_json(&config.join("plugins/installed_plugins.json"));
    let enabled = read_json(&config.join("settings.json"))["enabledPlugins"].clone();
    for (key, installs) in installed["plugins"].as_object().into_iter().flatten() {
        if enabled.is_object() && enabled[key] != true {
            continue;
        }
        let plugin = key.split('@').next().unwrap_or(key);
        let Some(path) = installs[0]["installPath"].as_str() else { continue };
        skills_in(&Path::new(path).join("skills"), "plugin", plugin, skills, 0);
        commands_in(&Path::new(path).join("commands"), "plugin", plugin, skills);
    }
}

fn claude_servers(config: &Path, cwd: &Path, root: &Path, servers: &mut Vec<(String, PathBuf)>) {
    let file = if config == paths::expand("~/.claude") {
        paths::expand("~/.claude.json")
    } else {
        config.join(".claude.json")
    };
    let json = read_json(&file);
    let project = |path: &Path| json["projects"][path.to_string_lossy().as_ref()]["mcpServers"].clone();
    for list in [json["mcpServers"].clone(), project(cwd), project(root)] {
        for name in list.as_object().into_iter().flat_map(|servers| servers.keys()) {
            if !servers.iter().any(|(existing, _)| existing == name) {
                servers.push((name.clone(), file.clone()));
            }
        }
    }
}

fn claude_windows(config: &Path, found: &mut Found) {
    let model = read_json(&config.join("settings.json"))["model"].as_str().unwrap_or_default().to_string();
    found.long_context = model.ends_with("[1m]");
    let Ok(entries) = fs::read_dir(config.join("cache/model-catalog")) else { return };
    for entry in entries.flatten() {
        let json = read_json(&entry.path());
        for model in json["catalog"]["config"]["models"].as_array().into_iter().flatten() {
            if let (Some(id), Some(window)) = (model["id"].as_str(), model["runtime"]["max_input_tokens"].as_u64()) {
                found.windows.insert(id.to_string(), window);
            }
        }
    }
}

fn pi_windows(config: &Path, found: &mut Found) {
    let json = read_json(&config.join("models.json"));
    for provider in json["providers"].as_object().into_iter().flat_map(|providers| providers.values()) {
        for model in provider["models"].as_array().into_iter().flatten() {
            if let (Some(id), Some(window)) = (model["id"].as_str(), model["contextWindow"].as_u64()) {
                found.windows.insert(id.to_string(), window);
            }
        }
    }
}

fn locate(format: Format, request: &Request, dirs: &[PathBuf]) -> Option<PathBuf> {
    let since = request.started.map(|time| time - Duration::from_secs(5)).unwrap_or(UNIX_EPOCH);
    let id = request.conversation.as_deref();
    if format == Format::Claude {
        let folders: Vec<PathBuf> = dirs.iter().map(|dir| history::claude_project_dir(dir, &request.cwd)).collect();
        return match id {
            Some(id) => folders.iter().map(|folder| folder.join(format!("{id}.jsonl"))).find(|file| file.is_file()),
            None => folders.iter().find_map(|folder| newest_since(folder, since, |_| true)),
        };
    }
    let key = (request.pid, request.conversation.clone());
    let cached = LOCATED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .get(&key)
        .filter(|path| path.is_file())
        .cloned();
    if cached.is_some() {
        return cached;
    }
    let found = dirs.iter().find_map(|dir| match format {
        Format::Codex => codex_rollout(dir, &request.cwd, id, since),
        Format::Pi => {
            let folder = history::pi_session_dir(dir, &request.cwd);
            match id {
                Some(id) => newest_since(&folder, UNIX_EPOCH, |name| name.ends_with(&format!("_{id}.jsonl"))),
                None => newest_since(&folder, since, |_| true),
            }
        }
        Format::Gemini => {
            let slug = read_json(&dir.join("projects.json"))["projects"][request.cwd.to_string_lossy().as_ref()]
                .as_str()
                .map(String::from)?;
            newest_since(&dir.join("tmp").join(slug).join("chats"), since, |name| name.starts_with("session-"))
        }
        Format::Claude => None,
    })?;
    let mut guard = LOCATED.lock().unwrap_or_else(PoisonError::into_inner);
    let located = guard.get_or_insert_with(HashMap::new);
    if located.len() >= 64 {
        located.clear();
    }
    located.insert(key, found.clone());
    Some(found)
}

fn newest_since(folder: &Path, since: SystemTime, accept: impl Fn(&str) -> bool) -> Option<PathBuf> {
    fs::read_dir(folder)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let modified = entry.metadata().ok()?.modified().ok()?;
            (name.ends_with(".jsonl") && accept(&name) && modified >= since).then(|| (modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn codex_rollout(dir: &Path, cwd: &Path, id: Option<&str>, since: SystemTime) -> Option<PathBuf> {
    let day = |time: SystemTime| time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64 / 86400;
    let (first, last) = (day(since) - 1, day(SystemTime::now()) + 1);
    let mut files: Vec<(SystemTime, PathBuf)> = (first..=last)
        .flat_map(|days| {
            let (year, month, day) = civil(days);
            let folder =
                dir.join("sessions").join(format!("{year:04}")).join(format!("{month:02}")).join(format!("{day:02}"));
            fs::read_dir(folder).into_iter().flatten().flatten().filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, entry.path()))
            })
        })
        .filter(|(modified, path)| *modified >= since && path.extension().is_some_and(|extension| extension == "jsonl"))
        .collect();
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    let named = |path: &Path, id: &str| path.file_stem().is_some_and(|stem| stem.to_string_lossy().ends_with(id));
    if let Some(id) = id {
        return files
            .iter()
            .map(|(_, path)| path.clone())
            .find(|path| named(path, id))
            .or_else(|| history::codex_files(dir).into_iter().map(|(path, _)| path).find(|path| named(path, id)));
    }
    files.into_iter().map(|(_, path)| path).find(|path| {
        let mut head = String::new();
        if let Ok(file) = File::open(path) {
            let _ = file.take(262144).read_to_string(&mut head);
        }
        let first = head.lines().next().and_then(|line| serde_json::from_str::<Value>(line).ok()).unwrap_or_default();
        first["payload"]["cwd"].as_str().map(Path::new) == Some(cwd)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nucli-agent-info-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn computes_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20405), (2025, 11, 13));
        assert_eq!(civil(-1), (1969, 12, 31));
    }

    #[test]
    fn parses_config_snippets() {
        let toml = "[mcp_servers.github]\ncommand = \"x\"\n[mcp_servers.github.env]\nA = \"1\"\n[mcp_servers.\"my.server\"]\n[other]";
        assert_eq!(toml_tables(toml, "mcp_servers"), ["github", "my.server"]);
        let (name, description) = frontmatter("---\nname: pdf\ndescription: \"Read PDFs\"\n---\n# Body");
        assert_eq!((name.as_deref(), description.as_str()), (Some("pdf"), "Read PDFs"));
        assert_eq!(frontmatter("# no frontmatter").0, None);
    }

    #[test]
    fn discovers_codex_files() {
        let root = temp_dir("codex");
        let (home, repo) = (root.join("codex-home"), root.join("repo"));
        let cwd = repo.join("app");
        write(&repo.join(".git/HEAD"), "ref");
        write(&home.join("AGENTS.md"), "global");
        write(&repo.join("AGENTS.md"), "repo");
        write(&cwd.join("AGENTS.override.md"), "override");
        write(&cwd.join("AGENTS.md"), "ignored");
        write(&repo.join(".agents/skills/pdf/SKILL.md"), "---\nname: pdf\ndescription: PDFs\n---");
        write(&home.join("config.toml"), "[mcp_servers.github]\ncommand = \"gh\"");
        let started = SystemTime::now() - Duration::from_secs(60);
        let (year, month, day) = civil(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64 / 86400);
        let rollout = home.join(format!("sessions/{year:04}/{month:02}/{day:02}/rollout-2025-c-9.jsonl"));
        let cwd_text = cwd.to_string_lossy().replace('\\', "\\\\");
        write(
            &rollout,
            &format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"c-9\",\"cwd\":\"{cwd_text}\"}}}}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"last_token_usage\":{{\"total_tokens\":5000}},\"model_context_window\":200000}}}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"function_call\",\"name\":\"mcp__github__issue\"}}}}\n"
            ),
        );
        let request = Request {
            agent: "codex".into(),
            pid: 4242,
            cwd: cwd.clone(),
            started: Some(started),
            conversation: None,
            dirs: vec![home.clone()],
            usage: false,
        };
        let info = probe(&request);
        assert_eq!(info.transcript.as_deref(), Some(rollout.as_path()));
        assert_eq!((info.tokens, info.window), (Some(5000), Some(200000)));
        assert_eq!(info.conversation.as_deref(), Some("c-9"));
        let memory: Vec<_> = info.memory.iter().map(|entry| entry.path.clone().unwrap()).collect();
        assert_eq!(memory, [home.join("AGENTS.md"), repo.join("AGENTS.md"), cwd.join("AGENTS.override.md")]);
        assert_eq!(info.skills[0].name, "pdf");
        assert_eq!(info.mcp[0].name, "github");
        assert_eq!(info.mcp[0].detail, "1 call");
        assert!(info.mcp[0].used);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn merges_claude_listing_with_disk() {
        let root = temp_dir("claude");
        let (config, cwd) = (root.join("config"), root.join("work"));
        write(&cwd.join(".git/HEAD"), "ref");
        write(&config.join("skills/summary/SKILL.md"), "---\nname: summary\n---");
        write(&cwd.join(".mcp.json"), r#"{"mcpServers":{"redis":{}}}"#);
        write(
            &config.join("cache/model-catalog/x.json"),
            r#"{"catalog":{"config":{"models":[{"id":"claude-next","runtime":{"max_input_tokens":1000000}}]}}}"#,
        );
        let transcript = history::claude_project_dir(&config, &cwd).join("s-1.jsonl");
        write(
            &transcript,
            concat!(
                r#"{"type":"attachment","attachment":{"type":"skill_listing","content":"- summary: Short\n- update-config: Settings"}}"#,
                "\n",
                r#"{"type":"assistant","message":{"model":"claude-next","usage":{"input_tokens":300000},"content":[{"type":"tool_use","name":"mcp__redis__get"}]}}"#,
                "\n",
            ),
        );
        let request = Request {
            agent: "claude".into(),
            pid: 1,
            cwd: cwd.clone(),
            started: None,
            conversation: Some("s-1".into()),
            dirs: vec![config.clone()],
            usage: false,
        };
        let info = probe(&request);
        assert_eq!(info.window, Some(1000000));
        assert_eq!(info.tokens, Some(300000));
        let skills: Vec<_> = info.skills.iter().map(|skill| (skill.name.as_str(), skill.detail.as_str())).collect();
        assert_eq!(skills, [("summary", "user"), ("update-config", "built-in")]);
        assert_eq!(info.mcp.len(), 1);
        assert_eq!(info.mcp[0].path.as_deref(), Some(cwd.join(".mcp.json").as_path()));
        assert!(info.mcp[0].used);
        assert_eq!(info.memory, Vec::new());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn grows_unknown_windows() {
        let parsed = Parsed { model: Some("claude-sonnet".into()), peak: 250000, ..Default::default() };
        assert_eq!(window(&parsed, &Found::default()), Some(1000000));
        let parsed = Parsed { model: Some("local-model".into()), ..Default::default() };
        assert_eq!(window(&parsed, &Found::default()), None);
    }
}
