mod imports;
mod modules;
mod queries;

use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tree_sitter::{Node, Parser, QueryCursor, StreamingIterator, Tree};

use imports::{Import, Imported};

#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub range: Range<usize>,
    pub name_range: Range<usize>,
    pub scope: Option<Range<usize>>,
    pub container: Option<String>,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub path: PathBuf,
    pub line: usize,
    pub cols: Range<usize>,
    pub name: String,
    pub kind: &'static str,
    pub container: Option<String>,
    pub preview: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Locations(Vec<Location>),
    Definition(String),
    Url(String),
    Nothing(String),
}

pub struct Request<'a> {
    pub project: &'a Path,
    pub file: &'a Path,
    pub language: Option<&'a str>,
    pub text: &'a str,
    pub offset: usize,
    pub cancelled: &'a (dyn Fn() -> bool + Sync),
}

const MAX_FILE: u64 = 1500000;
const MAX_DEPTH: usize = 6;

pub fn parse(language: &str, text: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&crate::highlight::grammar(language)?).ok()?;
    parser.parse(text, None)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

pub fn word_at(line: &str, col: usize) -> Option<Range<usize>> {
    let col = col.min(line.len());
    let start = line[..col].char_indices().rev().take_while(|(_, c)| is_word(*c)).last().map_or(col, |(i, _)| i);
    let end = line[col..].char_indices().find(|(_, c)| !is_word(*c)).map_or(line.len(), |(i, _)| col + i);
    (start < end && !line[start..end].chars().all(|c| c.is_ascii_digit())).then_some(start..end)
}

pub fn url_at(line: &str, col: usize) -> Option<Range<usize>> {
    let mut search = 0;
    while let Some(found) = line[search..].find("http").map(|index| index + search) {
        let rest = &line[found..];
        let scheme = if rest.starts_with("https://") {
            8
        } else if rest.starts_with("http://") {
            7
        } else {
            search = found + 4;
            continue;
        };
        let length = rest
            .char_indices()
            .find(|(index, c)| *index >= scheme && (c.is_whitespace() || "\"'<>`|{}\\^".contains(*c)))
            .map_or(rest.len(), |(index, _)| index);
        let mut end = found + length;
        while end > found + scheme && line[..end].ends_with(['.', ',', ';', ':', '!', '?', ')', ']']) {
            let unbalanced = line[..end].ends_with(')')
                && line[found..end].matches('(').count() >= line[found..end].matches(')').count();
            if unbalanced {
                break;
            }
            end -= 1;
        }
        if col >= found && col < end && end > found + scheme {
            return Some(found..end);
        }
        search = end.max(found + 1);
    }
    None
}

pub fn path_at(line: &str, col: usize) -> Option<Range<usize>> {
    let allowed = |c: char| c.is_alphanumeric() || "_-./~@+%".contains(c);
    let col = col.min(line.len());
    let start = line[..col].char_indices().rev().take_while(|(_, c)| allowed(*c)).last().map_or(col, |(i, _)| i);
    let end = line[col..].char_indices().find(|(_, c)| !allowed(*c)).map_or(line.len(), |(i, _)| col + i);
    let mut range = start..end;
    while range.end > range.start && line[..range.end].ends_with(['.', ',']) {
        range.end -= 1;
    }
    let token = &line[range.clone()];
    (token.contains('/') || token.contains('.')).then_some(range)
}

fn location_in(path: &Path, text: &str, range: Range<usize>, name: &str, kind: &'static str) -> Location {
    let line = text[..range.start].matches('\n').count();
    let line_start = text[..range.start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[range.start..].find('\n').map_or(text.len(), |index| range.start + index);
    let preview: String = text[line_start..line_end].trim().chars().take(160).collect();
    let end = range.end.min(line_end).max(range.start);
    Location {
        path: path.to_path_buf(),
        line,
        cols: range.start - line_start..end - line_start,
        name: name.to_string(),
        kind,
        container: None,
        preview,
    }
}

fn file_start(path: &Path) -> Location {
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let preview = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| text.lines().next().map(|line| line.trim().chars().take(160).collect()))
        .unwrap_or_default();
    Location { path: path.to_path_buf(), line: 0, cols: 0..0, name, kind: "file", container: None, preview }
}

fn refine(node: Node, kind: &'static str) -> &'static str {
    if node.kind() == "variable_declarator"
        && let Some(value) = node.child_by_field_name("value")
        && matches!(value.kind(), "arrow_function" | "function_expression" | "function")
    {
        return "function";
    }
    kind
}

fn local_scope(node: Node, kind: &str) -> Option<Range<usize>> {
    if kind != "variable" && kind != "parameter" {
        return None;
    }
    let mut current = node.parent();
    while let Some(scope) = current {
        let parent = scope.parent()?;
        let name = scope.kind();
        let function = ["function", "method", "closure", "lambda", "arrow", "func_literal", "constructor"]
            .iter()
            .any(|word| name.contains(word))
            && !name.contains("call")
            && !name.contains("type");
        let block = name.ends_with("block") || name.ends_with("body");
        if function || (kind == "variable" && block) {
            let owner = parent.kind();
            let member =
                ["class", "interface", "struct", "impl", "trait", "enum"].iter().any(|word| owner.contains(word))
                    || name == "class_body";
            return (!member || function).then(|| scope.byte_range());
        }
        current = Some(parent);
    }
    None
}

pub fn symbols(language: &str, tree: &Tree, text: &str) -> Vec<Symbol> {
    let Some(query) = queries::definitions(language) else { return Vec::new() };
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());
    let mut found: Vec<Symbol> = Vec::new();
    let mut seen = HashSet::new();
    while let Some(item) = matches.next() {
        let (mut name, mut definition) = (None, None);
        for capture in item.captures() {
            let capture_name = names[capture.index as usize];
            if capture_name == "name" {
                name = Some(capture.node);
            } else if let Some(kind) = capture_name.strip_prefix("definition.") {
                definition = Some((capture.node, kind));
            }
        }
        let (Some(name), Some((node, kind))) = (name, definition) else { continue };
        if !seen.insert(name.start_byte()) {
            continue;
        }
        let kind = refine(node, kind);
        found.push(Symbol {
            name: text[name.byte_range()].to_string(),
            kind,
            range: node.byte_range(),
            name_range: name.byte_range(),
            scope: local_scope(node, kind),
            container: None,
            depth: 0,
        });
    }
    found.sort_by_key(|symbol| (symbol.range.start, std::cmp::Reverse(symbol.range.end)));
    let mut stack: Vec<usize> = Vec::new();
    for index in 0..found.len() {
        if found[index].scope.is_some() {
            continue;
        }
        while let Some(&top) = stack.last() {
            if found[top].range.end > found[index].range.start && found[top].range.end >= found[index].range.end {
                break;
            }
            stack.pop();
        }
        if let Some(&top) = stack.last() {
            let (container, kind, depth) = (found[top].name.clone(), found[top].kind, found[top].depth);
            let symbol = &mut found[index];
            symbol.container = Some(container);
            symbol.depth = depth + 1;
            if symbol.kind == "function" && matches!(kind, "class" | "interface" | "implementation") {
                symbol.kind = "method";
            }
        }
        stack.push(index);
    }
    found
}

pub fn outline(language: &str, text: &str) -> Vec<Symbol> {
    let Some(tree) = parse(language, text) else { return Vec::new() };
    symbols(language, &tree, text).into_iter().filter(|symbol| symbol.scope.is_none()).collect()
}

fn family(language: &str) -> &str {
    match language {
        "TypeScript" | "TSX" | "JavaScript" => "JavaScript",
        "C" | "C++" => "C",
        other => other,
    }
}

fn identifier(node: Node, text: &str) -> bool {
    node.is_named() && node.child_count() == 0 && text[node.byte_range()].chars().all(is_word)
}

fn node_at<'a>(tree: &'a Tree, text: &str, offset: usize) -> Option<Node<'a>> {
    let root = tree.root_node();
    let at = |offset: usize| root.descendant_for_byte_range(offset, offset);
    let node = at(offset)?;
    if identifier(node, text) || offset == 0 {
        return Some(node);
    }
    match at(offset - 1) {
        Some(before) if identifier(before, text) => Some(before),
        _ => Some(node),
    }
}

fn string_node(node: Node) -> Option<Node> {
    let mut current = Some(node);
    let mut found = None;
    for _ in 0..3 {
        let Some(candidate) = current else { break };
        let kind = candidate.kind();
        if (kind.contains("string") && !kind.contains("template")) || kind == "system_lib_string" {
            found = Some(candidate);
        }
        current = candidate.parent();
    }
    found
}

fn ancestor<'a>(node: Node<'a>, kinds: &[&str], levels: usize) -> Option<Node<'a>> {
    let mut current = node.parent();
    for _ in 0..levels {
        let candidate = current?;
        if kinds.contains(&candidate.kind()) {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

struct FileList {
    root: PathBuf,
    time: std::time::Instant,
    files: std::sync::Arc<Vec<String>>,
}

static FILES: Mutex<Option<FileList>> = Mutex::new(None);

pub fn project_files(project: &Path) -> std::sync::Arc<Vec<String>> {
    if let Ok(cache) = FILES.lock()
        && let Some(list) = cache.as_ref()
        && list.root == project
        && list.time.elapsed().as_secs() < 10
    {
        return list.files.clone();
    }
    let files = std::sync::Arc::new(crate::palettes::list_files(project));
    if let Ok(mut cache) = FILES.lock() {
        *cache = Some(FileList { root: project.to_path_buf(), time: std::time::Instant::now(), files: files.clone() });
    }
    files
}

struct Parsed {
    language: &'static str,
    text: String,
    tree: Tree,
}

impl Parsed {
    fn read(path: &Path) -> Option<Parsed> {
        let language = crate::highlight::language_name(path)?;
        if std::fs::metadata(path).ok()?.len() > MAX_FILE {
            return None;
        }
        let text = std::fs::read_to_string(path).ok()?;
        let tree = parse(language, &text)?;
        Some(Parsed { language, text, tree })
    }
}

struct Resolver<'a> {
    project: &'a Path,
    files: &'a dyn Fn() -> std::sync::Arc<Vec<String>>,
    cancelled: &'a (dyn Fn() -> bool + Sync),
}

impl Resolver<'_> {
    fn module_files(&self, language: &str, from: &Path, spec: &str) -> Vec<PathBuf> {
        let project = self.project;
        match family(language) {
            "JavaScript" => modules::javascript(project, from, spec).into_iter().collect(),
            "Python" => modules::python(project, from, spec).into_iter().collect(),
            "Go" => modules::go(project, from, spec),
            "Java" => {
                let base = spec.replace('.', "/");
                let suffixes = [format!("{base}.java"), format!("{base}.kt")];
                modules::by_suffix(project, &(self.files)(), &suffixes).into_iter().collect()
            }
            _ => Vec::new(),
        }
    }

    fn rust_target(&self, from: &Path, segments: &[&str], depth: usize) -> Option<Vec<Location>> {
        let (file, used) = modules::rust(self.project, from, segments)?;
        let rest = &segments[used..];
        let Some(name) = rest.last() else { return Some(vec![file_start(&file)]) };
        let found = self.exported(&file, name, rest.get(rest.len().wrapping_sub(2)).copied(), depth + 1);
        Some(if found.is_empty() { vec![file_start(&file)] } else { found })
    }

    fn import_target(&self, language: &str, from: &Path, import: &Import, depth: usize) -> Vec<Location> {
        if depth > MAX_DEPTH {
            return Vec::new();
        }
        if language == "Rust" {
            let segments: Vec<&str> = import.source.split("::").collect();
            return self.rust_target(from, &segments, depth).unwrap_or_default();
        }
        if import.source.is_empty() {
            return self.exported(from, &import.binding, None, depth + 1);
        }
        let files = self.module_files(language, from, &import.source);
        let mut found = Vec::new();
        for file in &files {
            found.extend(match &import.imported {
                Imported::Module => Vec::new(),
                Imported::Default => self.default_export(file, depth + 1),
                Imported::Name(name) => self.exported(file, name, None, depth + 1),
            });
        }
        if found.is_empty()
            && let Imported::Name(name) = &import.imported
            && family(language) == "Python"
        {
            let spec = if import.source.ends_with('.') {
                format!("{}{name}", import.source)
            } else {
                format!("{}.{name}", import.source)
            };
            found.extend(modules::python(self.project, from, &spec).map(|file| file_start(&file)));
        }
        if found.is_empty() {
            found.extend(files.iter().map(|file| file_start(file)));
        }
        found
    }

    fn exported(&self, file: &Path, name: &str, container: Option<&str>, depth: usize) -> Vec<Location> {
        if depth > MAX_DEPTH {
            return Vec::new();
        }
        let Some(parsed) = Parsed::read(file) else { return Vec::new() };
        let symbols = symbols(parsed.language, &parsed.tree, &parsed.text);
        let matching: Vec<&Symbol> = symbols.iter().filter(|symbol| target(symbol, name)).collect();
        let preferred: Vec<&Symbol> = match container {
            Some(container) => {
                matching.iter().copied().filter(|symbol| symbol.container.as_deref() == Some(container)).collect()
            }
            None => matching.iter().copied().filter(|symbol| symbol.depth == 0).collect(),
        };
        let chosen = if preferred.is_empty() { matching } else { preferred };
        if !chosen.is_empty() {
            return chosen.into_iter().map(|symbol| self.location(file, &parsed.text, symbol)).collect();
        }
        let imports = imports::imports(parsed.language, parsed.tree.root_node(), &parsed.text);
        for import in imports.iter().filter(|import| import.binding == name) {
            let found = self.import_target(parsed.language, file, import, depth);
            if !found.is_empty() {
                return found;
            }
        }
        for import in imports.iter().filter(|import| import.binding == "*") {
            let found = match parsed.language {
                "Rust" => {
                    let path = format!("{}::{name}", import.source);
                    let segments: Vec<&str> = path.split("::").collect();
                    self.rust_target(file, &segments, depth).unwrap_or_default()
                }
                _ => {
                    let files = self.module_files(parsed.language, file, &import.source);
                    files.iter().flat_map(|module| self.exported(module, name, None, depth + 1)).collect()
                }
            };
            if !found.is_empty() && found.iter().any(|location| location.kind != "file") {
                return found;
            }
        }
        Vec::new()
    }

    fn default_export(&self, file: &Path, depth: usize) -> Vec<Location> {
        let Some(parsed) = Parsed::read(file) else { return vec![file_start(file)] };
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        for node in root.named_children(&mut cursor) {
            if node.kind() != "export_statement" || !parsed.text[node.byte_range()].contains("default") {
                continue;
            }
            if let Some(declaration) = node.child_by_field_name("declaration")
                && let Some(name) = declaration.child_by_field_name("name")
            {
                let text = &parsed.text[name.byte_range()];
                return vec![location_in(file, &parsed.text, name.byte_range(), text, "export")];
            }
            if let Some(value) = node.child_by_field_name("value")
                && value.kind() == "identifier"
            {
                let found = self.exported(file, &parsed.text[value.byte_range()], None, depth + 1);
                if !found.is_empty() {
                    return found;
                }
            }
            return vec![location_in(file, &parsed.text, node.byte_range(), "default", "export")];
        }
        vec![file_start(file)]
    }

    fn location(&self, path: &Path, text: &str, symbol: &Symbol) -> Location {
        let mut location = location_in(path, text, symbol.name_range.clone(), &symbol.name, symbol.kind);
        location.container = symbol.container.clone();
        location
    }

    fn project_wide(
        &self,
        current: &Path,
        language: &str,
        name: &str,
        member: bool,
        qualifier: Option<&str>,
    ) -> Vec<Location> {
        let files = (self.files)();
        let wanted = family(language).to_string();
        let candidates: Vec<PathBuf> = files
            .iter()
            .map(|path| self.project.join(path))
            .filter(|path| {
                path != current
                    && crate::highlight::language_name(path).is_some_and(|language| family(language) == wanted)
            })
            .collect();
        let threads = std::thread::available_parallelism().map_or(4, |count| count.get()).min(8);
        let chunk = candidates.len().div_ceil(threads).max(1);
        let cancelled = self.cancelled;
        let found: Vec<(Location, usize)> = std::thread::scope(|scope| {
            let workers: Vec<_> = candidates
                .chunks(chunk)
                .map(|paths| {
                    scope.spawn(move || {
                        let mut found = Vec::new();
                        for path in paths {
                            if cancelled() {
                                break;
                            }
                            if std::fs::metadata(path).map_or(true, |meta| meta.len() > MAX_FILE) {
                                continue;
                            }
                            let Ok(text) = std::fs::read_to_string(path) else { continue };
                            if !contains_word(&text, name) {
                                continue;
                            }
                            let Some(language) = crate::highlight::language_name(path) else { continue };
                            let Some(tree) = parse(language, &text) else { continue };
                            for symbol in symbols(language, &tree, &text) {
                                if target(&symbol, name) {
                                    let depth = symbol.depth;
                                    let mut location =
                                        location_in(path, &text, symbol.name_range.clone(), name, symbol.kind);
                                    location.container = symbol.container;
                                    found.push((location, depth));
                                }
                            }
                            if found.len() > 400 {
                                break;
                            }
                        }
                        found
                    })
                })
                .collect();
            workers.into_iter().flat_map(|worker| worker.join().unwrap_or_default()).collect()
        });
        rank(current, found, member, qualifier)
    }
}

fn target(symbol: &Symbol, name: &str) -> bool {
    symbol.name == name && symbol.scope.is_none() && symbol.kind != "implementation"
}

fn contains_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    text.match_indices(word).any(|(index, _)| {
        let before = index.checked_sub(1).map(|at| bytes[at] as char);
        let after = bytes.get(index + word.len()).map(|byte| *byte as char);
        !before.is_some_and(is_word) && !after.is_some_and(is_word)
    })
}

fn rank(current: &Path, found: Vec<(Location, usize)>, member: bool, qualifier: Option<&str>) -> Vec<Location> {
    let dir = current.parent().unwrap_or(current);
    let mut scored: Vec<(i32, Location)> = found
        .into_iter()
        .map(|(location, depth)| {
            let mut score = 0;
            if qualifier.is_some() && location.container.as_deref() == qualifier {
                score += 100;
            }
            if location.path.parent() == Some(dir) {
                score += 20;
            }
            let shared = location.path.components().zip(dir.components()).take_while(|(a, b)| a == b).count();
            score += shared as i32;
            let text = location.path.to_string_lossy().to_lowercase();
            if ["test", "spec", "mock", "fixture", "example", "/vendor/", ".d.ts"]
                .iter()
                .any(|word| text.contains(word))
            {
                score -= 30;
            }
            match (member, depth) {
                (false, 0) => score += 40,
                (true, depth) if depth > 0 => score += 40,
                _ => {}
            }
            (score, location)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.path.cmp(&b.1.path)).then(a.1.line.cmp(&b.1.line)));
    let Some(best) = scored.first().map(|(score, _)| *score) else { return Vec::new() };
    let strong = best >= 100 || (best >= 40 && scored.iter().filter(|(score, _)| *score >= best - 10).count() == 1);
    scored
        .into_iter()
        .filter(|(score, _)| !strong || *score >= best - 10)
        .take(50)
        .map(|(_, location)| location)
        .collect()
}

fn member_parts<'a>(node: Node<'a>, text: &str) -> Option<String> {
    let parent = node.parent()?;
    let kinds = [
        "member_expression",
        "field_expression",
        "attribute",
        "selector_expression",
        "nested_type_identifier",
        "nested_identifier",
        "qualified_identifier",
        "method_invocation",
        "field_access",
        "call",
        "dot_index_expression",
        "method_index_expression",
    ];
    if !kinds.contains(&parent.kind()) {
        return None;
    }
    let is_member = ["property", "field", "attribute", "name", "method"]
        .iter()
        .any(|field| parent.child_by_field_name(field).is_some_and(|child| child.id() == node.id()));
    if !is_member {
        return None;
    }
    ["object", "value", "operand", "module", "scope", "argument", "receiver", "table"]
        .iter()
        .find_map(|field| parent.child_by_field_name(field))
        .filter(|object| object.id() != node.id())
        .map(|object| text[object.byte_range()].trim().to_string())
}

fn rust_path(node: Node, text: &str) -> Option<String> {
    let mut current = node;
    let mut path = text[node.byte_range()].to_string();
    let mut in_path = false;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "scoped_identifier" | "scoped_type_identifier" | "scoped_use_list" => {
                in_path = true;
                let name = parent.child_by_field_name("name");
                let list = parent.child_by_field_name("list");
                let prefix = parent.child_by_field_name("path");
                let extends = name.is_some_and(|name| name.id() == current.id())
                    || list.is_some_and(|list| list.id() == current.id());
                if extends && let Some(prefix) = prefix {
                    path = format!("{}::{path}", &text[prefix.byte_range()]);
                }
            }
            "use_list" | "use_as_clause" => in_path = true,
            "use_declaration" => return Some(path),
            "generic_type" | "generic_function" => {}
            _ => break,
        }
        current = parent;
    }
    in_path.then_some(path)
}

fn python_module(node: Node, text: &str) -> Option<String> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "dotted_name" | "relative_import" | "import_prefix" => current = parent,
            "aliased_import" if parent.child_by_field_name("name").is_some_and(|name| name.id() == current.id()) => {
                current = parent;
            }
            "import_statement" => return Some(text[current.start_byte()..node.end_byte()].to_string()),
            "import_from_statement" => {
                let module = parent.child_by_field_name("module_name")?;
                return (module.id() == current.id()).then(|| text[module.start_byte()..node.end_byte()].to_string());
            }
            _ => return None,
        }
    }
    None
}

pub fn resolve(request: &Request, files: &dyn Fn() -> std::sync::Arc<Vec<String>>) -> Outcome {
    let Request { project, file, language, text, offset, cancelled } = *request;
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |index| offset + index);
    let (line, col) = (&text[line_start..line_end], offset - line_start);
    if let Some(range) = url_at(line, col) {
        return Outcome::Url(line[range].to_string());
    }
    let word = word_at(line, col).map(|range| line[range].to_string()).unwrap_or_default();
    let path_outcome = || {
        let range = path_at(line, col)?;
        modules::path(project, file, &line[range]).map(|found| Outcome::Locations(vec![file_start(&found)]))
    };
    let resolver = Resolver { project, files, cancelled };
    let parsed = language.and_then(|language| Some((language, parse(language, text)?)));
    let Some((language, tree)) = parsed else {
        return path_outcome().unwrap_or(Outcome::Nothing(word));
    };
    let Some(node) = node_at(&tree, text, offset) else { return Outcome::Nothing(word) };
    if let Some(string) = string_node(node) {
        let spec = imports::string_content(string, text).trim_start_matches('<').trim_end_matches('>').to_string();
        let found = string_target(&resolver, language, file, string, &spec, text);
        if !found.is_empty() {
            return Outcome::Locations(found);
        }
        return path_outcome().unwrap_or(Outcome::Nothing(spec));
    }
    if !identifier(node, text) {
        return path_outcome().unwrap_or(Outcome::Nothing(word));
    }
    let name = text[node.byte_range()].to_string();
    let symbols = symbols(language, &tree, text);
    let imports = imports::imports(language, tree.root_node(), text);
    if symbols.iter().any(|symbol| symbol.name_range == node.byte_range() && symbol.kind != "implementation") {
        if node
            .parent()
            .is_some_and(|parent| parent.kind() == "mod_item" && parent.child_by_field_name("body").is_none())
            && let Some(module) = modules::rust_child(file, &name)
        {
            return Outcome::Locations(vec![file_start(&module)]);
        }
        return Outcome::Definition(name);
    }
    let found = identifier_target(&resolver, language, file, text, node, &name, &symbols, &imports);
    if found.is_empty() { Outcome::Nothing(name) } else { Outcome::Locations(found) }
}

fn string_target(
    resolver: &Resolver,
    language: &str,
    file: &Path,
    string: Node,
    spec: &str,
    text: &str,
) -> Vec<Location> {
    let project = resolver.project;
    let found: Vec<PathBuf> = match family(language) {
        "JavaScript" => {
            let import =
                ancestor(string, &["import_statement", "export_statement", "import_require_clause"], 2).is_some();
            let call = ancestor(string, &["call_expression"], 2).is_some_and(|call| {
                call.child_by_field_name("function").is_some_and(|function| {
                    let name = &text[function.byte_range()];
                    name == "require"
                        || name == "import"
                        || name.ends_with(".mock")
                        || name.ends_with("require.resolve")
                })
            });
            if import || call || spec.starts_with('.') {
                modules::javascript(project, file, spec).into_iter().collect()
            } else {
                Vec::new()
            }
        }
        "Go" if ancestor(string, &["import_spec"], 1).is_some() => modules::go(project, file, spec),
        "C" if ancestor(string, &["preproc_include"], 1).is_some() => {
            modules::include(project, file, spec, || (resolver.files)().to_vec()).into_iter().collect()
        }
        "Lua" | "Ruby" => {
            let call = ancestor(string, &["function_call", "call"], 3);
            let callee = call
                .and_then(|call| call.child_by_field_name("name").or_else(|| call.child_by_field_name("method")))
                .map(|name| &text[name.byte_range()]);
            match (language, callee) {
                ("Lua", Some("require")) => modules::lua(project, file, spec).into_iter().collect(),
                ("Ruby", Some("require_relative")) => modules::ruby(project, file, spec, true).into_iter().collect(),
                ("Ruby", Some("require" | "load")) => modules::ruby(project, file, spec, false).into_iter().collect(),
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    };
    let found = if found.is_empty() { modules::path(project, file, spec).into_iter().collect() } else { found };
    found.iter().map(|path| file_start(path)).collect()
}

#[allow(clippy::too_many_arguments)]
fn identifier_target(
    resolver: &Resolver,
    language: &str,
    file: &Path,
    text: &str,
    node: Node,
    name: &str,
    symbols: &[Symbol],
    imports: &[Import],
) -> Vec<Location> {
    let offset = node.start_byte();
    let here = |symbol: &Symbol| resolver.location(file, text, symbol);
    if language == "Rust"
        && let Some(path) = rust_path(node, text)
    {
        let mut segments: Vec<String> = path.split("::").map(|segment| segment.trim().to_string()).collect();
        segments.retain(|segment| !segment.is_empty());
        if let Some(import) = imports.iter().find(|import| Some(&import.binding) == segments.first()) {
            let mut full: Vec<String> = import.source.split("::").map(String::from).collect();
            full.extend(segments.drain(1..));
            segments = full;
        }
        let parts: Vec<&str> = segments.iter().map(String::as_str).collect();
        if let Some(found) = resolver.rust_target(file, &parts, 0) {
            return found;
        }
        let external = parts.first().is_some_and(|first| {
            first.starts_with(|c: char| c.is_lowercase()) && !matches!(*first, "crate" | "self" | "super")
        });
        if external && parts.len() > 1 {
            return Vec::new();
        }
        if let Some(qualifier) = parts.len().checked_sub(2).map(|index| parts[index]) {
            let qualifier =
                if qualifier == "Self" { enclosing_type(symbols, offset) } else { Some(qualifier.to_string()) };
            let local: Vec<Location> = symbols
                .iter()
                .filter(|symbol| symbol.name == name && symbol.container.is_some() && symbol.container == qualifier)
                .map(here)
                .collect();
            if !local.is_empty() {
                return local;
            }
            return resolver.project_wide(file, language, name, true, qualifier.as_deref());
        }
    }
    if family(language) == "Python"
        && let Some(module) = python_module(node, text)
        && let Some(found) = modules::python(resolver.project, file, &module)
    {
        return vec![file_start(&found)];
    }
    if family(language) == "JavaScript" && ancestor(node, &["import_statement", "export_statement"], 4).is_some() {
        let source = ancestor(node, &["import_statement", "export_statement"], 4)
            .and_then(|statement| statement.child_by_field_name("source"))
            .map(|source| imports::string_content(source, text));
        let found: Vec<Location> = imports
            .iter()
            .filter(|import| Some(&import.source) == source.as_ref())
            .filter(|import| import.binding == name || import.imported == Imported::Name(name.to_string()))
            .flat_map(|import| resolver.import_target(language, file, import, 0))
            .collect();
        if !found.is_empty() {
            return found;
        }
    }
    if let Some(qualifier) = member_parts(node, text) {
        let base = qualifier.split(['.', '(', '[', '!', '?']).next().unwrap_or(&qualifier).to_string();
        let namespace = |import: &&Import| {
            import.binding == base && matches!(import.imported, Imported::Module | Imported::Default)
        };
        if let Some(import) = imports.iter().find(namespace) {
            let files = resolver.module_files(language, file, &import.source);
            let found: Vec<Location> =
                files.iter().flat_map(|module| resolver.exported(module, name, None, 1)).collect();
            return if found.is_empty() { files.iter().map(|module| file_start(module)).collect() } else { found };
        }
        let own = matches!(base.as_str(), "this" | "self" | "Self" | "cls" | "super");
        let container = if own {
            enclosing_type(symbols, offset)
        } else {
            inferred_type(symbols, text, &base, offset).or_else(|| Some(base.clone()))
        };
        let members: Vec<&Symbol> = symbols.iter().filter(|symbol| target(symbol, name)).collect();
        let exact: Vec<Location> = members
            .iter()
            .filter(|symbol| symbol.container.is_some() && symbol.container == container)
            .map(|s| here(s))
            .collect();
        if !exact.is_empty() {
            return exact;
        }
        if own || !members.is_empty() {
            let nested: Vec<Location> = members.iter().filter(|symbol| symbol.depth > 0).map(|s| here(s)).collect();
            if !nested.is_empty() {
                return nested;
            }
        }
        return resolver.project_wide(file, language, name, true, container.as_deref());
    }
    let mut locals: Vec<&Symbol> = symbols
        .iter()
        .filter(|symbol| symbol.name == name && symbol.scope.as_ref().is_some_and(|scope| scope.contains(&offset)))
        .collect();
    locals.sort_by_key(|symbol| {
        let scope = symbol.scope.clone().unwrap_or_default();
        (scope.end - scope.start, symbol.name_range.start > offset, offset.abs_diff(symbol.name_range.start))
    });
    if let Some(local) = locals.first() {
        return vec![here(local)];
    }
    let same: Vec<&Symbol> = symbols.iter().filter(|symbol| target(symbol, name)).collect();
    let top: Vec<Location> =
        same.iter().filter(|symbol| symbol.depth == 0 || symbol.kind == "module").map(|s| here(s)).collect();
    if !top.is_empty() {
        return top;
    }
    if let Some(import) =
        imports.iter().find(|import| import.binding == name && !(import.export && !import.source.is_empty()))
    {
        return resolver.import_target(language, file, import, 0);
    }
    if !same.is_empty() {
        return same.iter().map(|s| here(s)).collect();
    }
    for import in imports.iter().filter(|import| import.binding == "*") {
        let found: Vec<Location> = match language {
            "Rust" => {
                let path = format!("{}::{name}", import.source);
                let segments: Vec<&str> = path.split("::").collect();
                resolver.rust_target(file, &segments, 0).unwrap_or_default()
            }
            _ => resolver
                .module_files(language, file, &import.source)
                .iter()
                .flat_map(|module| resolver.exported(module, name, None, 1))
                .collect(),
        };
        if found.iter().any(|location| location.kind != "file") {
            return found;
        }
    }
    resolver.project_wide(file, language, name, false, None)
}

fn type_name(text: &str) -> Option<String> {
    let text = text.trim_start().trim_start_matches(['&', '*', '(']);
    let text = ["mut ", "dyn ", "impl ", "readonly ", "const "]
        .iter()
        .fold(text, |text, prefix| text.strip_prefix(prefix).unwrap_or(text))
        .trim_start();
    let path: String = text.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '.')).collect();
    path.rsplit([':', '.']).find(|part| part.starts_with(|c: char| c.is_uppercase())).map(String::from)
}

fn inferred_type(symbols: &[Symbol], text: &str, base: &str, offset: usize) -> Option<String> {
    let declaration = symbols
        .iter()
        .filter(|symbol| {
            symbol.name == base
                && symbol.name_range.start < offset
                && matches!(symbol.kind, "variable" | "parameter" | "constant" | "field")
                && symbol.scope.as_ref().is_none_or(|scope| scope.contains(&offset))
        })
        .max_by_key(|symbol| symbol.name_range.start)?;
    let after = &text[declaration.name_range.end..declaration.range.end.max(declaration.name_range.end)];
    let after = after.trim_start().trim_start_matches('?').trim_start_matches('!');
    if let Some(annotation) = after.strip_prefix(':').filter(|rest| !rest.starts_with(':')) {
        return type_name(annotation.split('=').next().unwrap_or(annotation));
    }
    let value = after.strip_prefix('=').or_else(|| after.split_once(" = ").map(|(_, value)| value))?.trim_start();
    let value = value.strip_prefix("await ").unwrap_or(value);
    type_name(value.strip_prefix("new ").unwrap_or(value))
}

fn enclosing_type(symbols: &[Symbol], offset: usize) -> Option<String> {
    symbols
        .iter()
        .filter(|symbol| {
            symbol.range.contains(&offset)
                && matches!(symbol.kind, "class" | "interface" | "implementation" | "enum" | "type")
        })
        .max_by_key(|symbol| symbol.range.start)
        .map(|symbol| symbol.name.clone())
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::PathBuf;

    pub struct Mock(pub PathBuf);

    impl Drop for Mock {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub fn mock(files: &[(&str, &str)]) -> Mock {
        let root = std::env::temp_dir().join(format!("nucli-navigate-{}", uuid::Uuid::new_v4()));
        for (path, text) in files {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        Mock(root)
    }
}

#[cfg(test)]
mod tests;
