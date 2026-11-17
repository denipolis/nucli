use std::collections::BTreeSet;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use fancy_regex::Regex;
use serde::{Deserialize, Serialize};

use crate::find::{self, Hit, Query};

pub const LIMIT: usize = 20000;
const MAX_SIZE: u64 = 4000000;
const BEFORE: usize = 20;
const AFTER: usize = 200;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub find: find::Options,
    pub include: String,
    pub exclude: String,
    pub details: bool,
    pub replace: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub hit: Hit,
    pub preview: String,
    pub range: Range<usize>,
    pub replacement: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileHits {
    pub path: String,
    pub found: Vec<Found>,
}

#[derive(Clone, Debug, Default)]
pub struct Request {
    pub text: String,
    pub options: find::Options,
    pub include: String,
    pub exclude: String,
    pub template: Option<String>,
}

#[derive(Default)]
pub struct Outcome {
    pub files: Vec<FileHits>,
    pub capped: bool,
}

fn patterns(text: &str) -> Vec<&str> {
    let (mut parts, mut depth, mut start) = (Vec::new(), 0usize, 0);
    for (index, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts.into_iter().map(str::trim).filter(|part| !part.is_empty()).collect()
}

fn glob(pattern: &str) -> String {
    let (anchored, pattern) = match pattern.strip_prefix("./").or_else(|| pattern.strip_prefix('/')) {
        Some(rest) => (true, rest),
        None => (false, pattern),
    };
    let pattern = pattern.trim_end_matches('/');
    let mut body = String::from(if anchored || pattern.starts_with("**") { "^" } else { "^(?:.*/)?" });
    let mut chars = pattern.chars().peekable();
    let mut braces = 0;
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.next_if_eq(&'/').is_some() {
                    body.push_str("(?:.*/)?");
                } else {
                    body.push_str(".*");
                }
            }
            '*' => body.push_str("[^/]*"),
            '?' => body.push_str("[^/]"),
            '{' => {
                braces += 1;
                body.push_str("(?:");
            }
            '}' if braces > 0 => {
                braces -= 1;
                body.push(')');
            }
            ',' if braces > 0 => body.push('|'),
            '[' => {
                let class: String = chars.by_ref().take_while(|c| *c != ']').collect();
                let class = class.strip_prefix('!').map_or(class.clone(), |rest| format!("^{rest}"));
                body.push_str(&format!("[{class}]"));
            }
            other => body.push_str(&fancy_regex::escape(&other.to_string())),
        }
    }
    body.push_str(&")".repeat(braces));
    body.push_str("(?:/.*)?$");
    body
}

pub fn globs(text: &str) -> Result<Option<Regex>, String> {
    let bodies: Vec<String> = patterns(text).into_iter().map(glob).collect();
    if bodies.is_empty() {
        return Ok(None);
    }
    Regex::new(&bodies.join("|")).map(Some).map_err(|error| error.to_string())
}

fn lines(text: &str) -> Vec<(usize, &str)> {
    let mut offset = 0;
    text.split_inclusive('\n')
        .map(|line| {
            let start = offset;
            offset += line.len();
            let line = line.strip_suffix('\n').unwrap_or(line);
            (start, line.strip_suffix('\r').unwrap_or(line))
        })
        .collect()
}

fn preview(line: &str, start: usize, end: usize) -> (String, Range<usize>) {
    let indent = (line.len() - line.trim_start().len()).min(start);
    let from = line[..start].char_indices().rev().nth(BEFORE - 1).map_or(0, |(index, _)| index).max(indent);
    let end = line[start..end].char_indices().nth(AFTER).map_or(end, |(index, _)| start + index);
    let to = line[end..].char_indices().nth(AFTER).map_or(line.len(), |(index, _)| end + index);
    let prefix = if from > indent { "…" } else { "" };
    let shift = prefix.len() as isize - from as isize;
    let range = (start as isize + shift) as usize..(end as isize + shift) as usize;
    (format!("{prefix}{}", line[from..to].replace('\t', " ")), range)
}

fn read(full: &Path) -> Option<String> {
    let meta = std::fs::metadata(full).ok()?;
    if !meta.is_file() || meta.len() > MAX_SIZE {
        return None;
    }
    let bytes = std::fs::read(full).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn search_text(text: &str, query: &Query, request: &Request) -> Vec<Found> {
    let lines = lines(text);
    let mut found = Vec::new();
    query.walk_lines(lines.iter().map(|(_, line)| (*line, false)), |hit, captures| {
        let line = lines[hit.start.row].1;
        let end = if hit.end.row == hit.start.row { hit.end.col } else { line.len() };
        let (preview, range) = preview(line, hit.start.col, end);
        let replacement = request.template.as_deref().map(|template| find::expand(template, captures, request.options));
        found.push(Found { hit, preview, range, replacement });
    });
    found
}

pub fn files(root: &Path, request: &Request) -> Result<Vec<String>, String> {
    let include = globs(&request.include)?;
    let exclude = globs(&request.exclude)?;
    let matches = |regex: &Option<Regex>, path: &str| regex.as_ref().map(|regex| regex.is_match(path).unwrap_or(false));
    let mut files = crate::palettes::list_files(root);
    files.retain(|path| matches(&include, path) != Some(false) && matches(&exclude, path) != Some(true));
    Ok(files)
}

pub fn search(root: &Path, request: &Request, cancelled: &(dyn Fn() -> bool + Sync)) -> Result<Outcome, String> {
    Query::new(&request.text, request.options)?;
    let files = files(root, request)?;
    let total = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(4, |count| count.get()).min(8);
    let chunk = files.len().div_ceil(threads).max(1);
    let chunks: Vec<Vec<FileHits>> = std::thread::scope(|scope| {
        let workers: Vec<_> = files
            .chunks(chunk)
            .map(|paths| {
                let total = &total;
                scope.spawn(move || {
                    let Ok(query) = Query::new(&request.text, request.options) else { return Vec::new() };
                    let mut results = Vec::new();
                    for path in paths {
                        if cancelled() || total.load(Ordering::Relaxed) >= LIMIT {
                            break;
                        }
                        let Some(text) = read(&root.join(path)) else { continue };
                        let found = search_text(&text, &query, request);
                        if !found.is_empty() {
                            total.fetch_add(found.len(), Ordering::Relaxed);
                            results.push(FileHits { path: path.clone(), found });
                        }
                    }
                    results
                })
            })
            .collect();
        workers.into_iter().map(|worker| worker.join().unwrap_or_default()).collect()
    });
    let mut outcome = Outcome::default();
    let mut count = 0;
    for mut file in chunks.into_iter().flatten() {
        if count + file.found.len() > LIMIT {
            file.found.truncate(LIMIT - count);
            outcome.capped = true;
        }
        count += file.found.len();
        if !file.found.is_empty() {
            outcome.files.push(file);
        }
        if count >= LIMIT {
            outcome.capped = true;
            break;
        }
    }
    Ok(outcome)
}

pub fn replace(full: &Path, request: &Request, hits: &[Hit]) -> Result<usize, String> {
    let text = read(full).ok_or_else(|| format!("cannot read {}", full.display()))?;
    let query = Query::new(&request.text, request.options)?;
    let template = request.template.as_deref().unwrap_or("");
    let wanted: BTreeSet<_> = hits.iter().map(|hit| (hit.start, hit.end)).collect();
    let lines = lines(&text);
    let mut edits = Vec::new();
    query.walk_lines(lines.iter().map(|(_, line)| (*line, false)), |hit, captures| {
        if wanted.contains(&(hit.start, hit.end)) {
            let range = lines[hit.start.row].0 + hit.start.col..lines[hit.end.row].0 + hit.end.col;
            edits.push((range, find::expand(template, captures, request.options)));
        }
    });
    if edits.is_empty() {
        return Ok(0);
    }
    let mut result = text;
    for (range, replacement) in edits.iter().rev() {
        result.replace_range(range.clone(), replacement);
    }
    std::fs::write(full, result).map_err(|error| error.to_string())?;
    Ok(edits.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matched(patterns: &str, path: &str) -> bool {
        globs(patterns).unwrap().unwrap().is_match(path).unwrap()
    }

    fn request(text: &str, template: Option<&str>, options: find::Options) -> Request {
        Request { text: text.into(), options, template: template.map(String::from), ..Default::default() }
    }

    #[test]
    fn globs_match_like_vscode() {
        assert!(matched("*.rs", "src/main.rs"));
        assert!(!matched("*.rs", "src/main.rsx"));
        assert!(matched("src", "src/a/b.txt"));
        assert!(matched("src", "lib/src/b.txt"));
        assert!(!matched("./src", "lib/src/b.txt"));
        assert!(matched("./src/*.rs", "src/a.rs"));
        assert!(!matched("./src/*.rs", "src/a/b.rs"));
        assert!(matched("src/**/*.rs", "src/a/b/c.rs"));
        assert!(matched("src/**/*.rs", "src/c.rs"));
        assert!(matched("*.{rs,toml}", "Cargo.toml"));
        assert!(matched("docs, *.toml", "Cargo.toml"));
        assert!(matched("file?.[!a]s", "file1.rs"));
        assert!(!matched("file?.[!a]s", "file1.as"));
        assert!(globs(" , ").unwrap().is_none());
    }

    #[test]
    fn previews_trim_and_locate_the_match() {
        assert_eq!(preview("    let foo = 1;", 8, 11), ("let foo = 1;".to_string(), 4..7));
        let long = format!("{}needle tail", "x".repeat(100));
        let (text, range) = preview(&long, 100, 106);
        assert!(text.starts_with('…'));
        assert_eq!(&text[range], "needle");
    }

    #[test]
    fn searches_lines_with_replacements() {
        let options = find::Options { regex: true, ..Default::default() };
        let request = request(r"(\w+)\(\)", Some("$1(x)"), options);
        let query = Query::new(&request.text, options).unwrap();
        let found = search_text("a()\r\n  b() c()\n", &query, &request);
        assert_eq!(found.len(), 3);
        assert_eq!((found[1].hit.start.row, found[1].hit.start.col), (1, 2));
        assert_eq!(found[1].preview, "b() c()");
        assert_eq!(found[2].replacement.as_deref(), Some("c(x)"));
    }

    #[test]
    fn replaces_chosen_hits_in_a_file() {
        let folder = std::env::temp_dir().join(format!("nucli-search-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("a.txt");
        std::fs::write(&file, "foo one\r\nFoo two\nfoo\n").unwrap();
        let request = request("foo", Some("bar"), find::Options { preserve: true, ..Default::default() });
        let query = Query::new("foo", request.options).unwrap();
        let found = search_text(&std::fs::read_to_string(&file).unwrap(), &query, &request);
        assert_eq!(replace(&file, &request, &[found[1].hit]).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "foo one\r\nBar two\nfoo\n");
        let hits: Vec<Hit> = found.iter().map(|found| found.hit).collect();
        assert_eq!(replace(&file, &request, &hits).unwrap(), 2);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "bar one\r\nBar two\nbar\n");
        let multiline = Request { text: r"one\r?\nBar".into(), template: Some("x".into()), ..request };
        let multiline = Request { options: find::Options { regex: true, ..Default::default() }, ..multiline };
        let query = Query::new(&multiline.text, multiline.options).unwrap();
        let hits: Vec<Hit> = search_text(&std::fs::read_to_string(&file).unwrap(), &query, &multiline)
            .iter()
            .map(|found| found.hit)
            .collect();
        assert_eq!(replace(&file, &multiline, &hits).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "bar x two\nbar\n");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
