use std::path::{Component, Path, PathBuf};

const JS_EXTENSIONS: [&str; 13] =
    ["ts", "tsx", "d.ts", "js", "jsx", "mjs", "cjs", "mts", "cts", "json", "vue", "svelte", "astro"];

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other),
        }
    }
    result
}

fn file(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then(|| normalize(&path))
}

fn with_suffix(base: &Path, suffix: &str) -> PathBuf {
    let mut text = base.as_os_str().to_os_string();
    text.push(suffix);
    PathBuf::from(text)
}

fn strip_comments(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            result.push(c);
            if c == '\\' {
                result.extend(chars.next());
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                result.push(c);
            }
            ('/', Some('/')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        result.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for next in chars.by_ref() {
                    if last == '*' && next == '/' {
                        break;
                    }
                    last = next;
                }
            }
            _ => result.push(c),
        }
    }
    result
}

pub fn strip_jsonc(text: &str) -> String {
    let text = strip_comments(text);
    let mut result = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for (index, c) in text.char_indices() {
        if in_string {
            in_string = escaped || c != '"';
            escaped = !escaped && c == '\\';
        } else if c == '"' {
            in_string = true;
        } else if c == ',' && text[index + 1..].trim_start().starts_with(['}', ']']) {
            continue;
        }
        result.push(c);
    }
    result
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&strip_jsonc(&text)).ok()
}

fn package_entry(dir: &Path) -> Option<PathBuf> {
    let json = read_json(&dir.join("package.json"))?;
    let exports = json.get("exports").and_then(|exports| match exports {
        serde_json::Value::String(path) => Some(path.clone()),
        serde_json::Value::Object(map) => {
            let root = map.get(".").unwrap_or(exports);
            match root {
                serde_json::Value::String(path) => Some(path.clone()),
                serde_json::Value::Object(conditions) => ["types", "import", "default", "require"]
                    .iter()
                    .find_map(|key| conditions.get(*key).and_then(|value| value.as_str()).map(String::from)),
                _ => None,
            }
        }
        _ => None,
    });
    let fields = ["types", "typings", "module", "main"];
    let entries = fields.iter().filter_map(|key| json.get(*key).and_then(|value| value.as_str()).map(String::from));
    entries.chain(exports).find_map(|entry| {
        let target = dir.join(entry);
        js_plain(&target).or_else(|| js_index(&target))
    })
}

fn js_plain(base: &Path) -> Option<PathBuf> {
    if base.is_file() {
        return Some(normalize(base));
    }
    let extension = base.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    if let Some(swapped) = match extension {
        "js" => Some(["ts", "tsx"].as_slice()),
        "jsx" => Some(["tsx"].as_slice()),
        "mjs" => Some(["mts"].as_slice()),
        "cjs" => Some(["cts"].as_slice()),
        _ => None,
    } {
        for ext in swapped {
            if let Some(found) = file(base.with_extension(ext)) {
                return Some(found);
            }
        }
    }
    for ext in JS_EXTENSIONS {
        if let Some(found) = file(with_suffix(base, &format!(".{ext}"))) {
            return Some(found);
        }
    }
    None
}

fn js_index(dir: &Path) -> Option<PathBuf> {
    JS_EXTENSIONS.iter().find_map(|ext| file(dir.join(format!("index.{ext}"))))
}

fn js_file(base: &Path) -> Option<PathBuf> {
    js_plain(base).or_else(|| base.is_dir().then(|| package_entry(base).or_else(|| js_index(base))).flatten())
}

struct TsConfig {
    base: Option<PathBuf>,
    paths: Vec<(String, Vec<String>, PathBuf)>,
}

fn tsconfig(path: &Path, depth: usize) -> Option<TsConfig> {
    let json = read_json(path)?;
    let dir = path.parent()?;
    let mut config = match json.get("extends").and_then(|value| value.as_str()) {
        Some(parent) if depth < 3 && parent.starts_with('.') => {
            let parent = dir.join(parent);
            let parent = if parent.extension().is_some() { parent } else { with_suffix(&parent, ".json") };
            tsconfig(&parent, depth + 1).unwrap_or(TsConfig { base: None, paths: Vec::new() })
        }
        _ => TsConfig { base: None, paths: Vec::new() },
    };
    let options = json.get("compilerOptions");
    if let Some(base) = options.and_then(|options| options.get("baseUrl")).and_then(|value| value.as_str()) {
        config.base = Some(dir.join(base));
    }
    if let Some(paths) = options.and_then(|options| options.get("paths")).and_then(|value| value.as_object()) {
        let root = config.base.clone().unwrap_or_else(|| dir.to_path_buf());
        config.paths = paths
            .iter()
            .map(|(pattern, targets)| {
                let targets = targets
                    .as_array()
                    .map(|list| list.iter().filter_map(|value| value.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                (pattern.clone(), targets, root.clone())
            })
            .collect();
    }
    Some(config)
}

fn ancestors<'a>(start: &'a Path, project: &'a Path) -> impl Iterator<Item = &'a Path> {
    let stop = project.parent();
    start.ancestors().take_while(move |dir| Some(*dir) != stop && !dir.as_os_str().is_empty())
}

fn js_alias(project: &Path, from: &Path, spec: &str) -> Option<PathBuf> {
    for dir in ancestors(from, project) {
        for name in ["tsconfig.json", "jsconfig.json"] {
            let Some(config) = tsconfig(&dir.join(name), 0) else { continue };
            for (pattern, targets, root) in &config.paths {
                let rest = match pattern.split_once('*') {
                    Some((head, tail)) => {
                        spec.strip_prefix(head).and_then(|rest| rest.strip_suffix(tail)).map(String::from)
                    }
                    None => (pattern == spec).then(String::new),
                };
                let Some(rest) = rest else { continue };
                for target in targets {
                    if let Some(found) = js_file(&root.join(target.replace('*', &rest))) {
                        return Some(found);
                    }
                }
            }
            if let Some(found) = config.base.as_ref().and_then(|base| js_file(&base.join(spec))) {
                return Some(found);
            }
        }
    }
    None
}

pub fn javascript(project: &Path, file: &Path, spec: &str) -> Option<PathBuf> {
    let from = file.parent()?;
    if spec.starts_with("./") || spec.starts_with("../") || spec == "." || spec == ".." {
        return js_file(&from.join(spec));
    }
    if let Some(found) = js_alias(project, from, spec) {
        return Some(found);
    }
    if let Some(rest) = spec.strip_prefix('/').or_else(|| spec.strip_prefix("~/"))
        && let Some(found) = js_file(&project.join(rest))
    {
        return Some(found);
    }
    let types = match spec.strip_prefix('@').and_then(|rest| rest.split_once('/')) {
        Some((scope, name)) => format!("@types/{scope}__{}", name.split('/').next().unwrap_or(name)),
        None => format!("@types/{}", spec.split('/').next().unwrap_or(spec)),
    };
    for dir in from.ancestors().take(12) {
        let modules = dir.join("node_modules");
        if !modules.is_dir() {
            continue;
        }
        if let Some(found) = js_file(&modules.join(spec)) {
            return Some(found);
        }
        if let Some(found) = js_file(&modules.join(&types)) {
            return Some(found);
        }
    }
    None
}

fn python_module(base: &Path, parts: &[&str]) -> Option<PathBuf> {
    let joined: PathBuf = parts.iter().collect();
    let path = base.join(joined);
    if parts.is_empty() {
        return file(base.join("__init__.py"));
    }
    file(path.with_extension("py"))
        .or_else(|| file(path.with_extension("pyi")))
        .or_else(|| file(path.join("__init__.py")))
        .or_else(|| file(path.join("__init__.pyi")))
}

pub fn python(project: &Path, file: &Path, spec: &str) -> Option<PathBuf> {
    let dots = spec.chars().take_while(|c| *c == '.').count();
    let parts: Vec<&str> = spec[dots..].split('.').filter(|part| !part.is_empty()).collect();
    let from = file.parent()?;
    if dots > 0 {
        let base = from.ancestors().nth(dots - 1)?;
        return python_module(base, &parts);
    }
    if parts.is_empty() {
        return None;
    }
    let roots =
        ancestors(from, project).map(Path::to_path_buf).chain(["src", "lib", "app"].map(|dir| project.join(dir)));
    for root in roots {
        if let Some(found) = python_module(&root, &parts) {
            return Some(found);
        }
    }
    None
}

fn crate_dir(file: &Path) -> Option<PathBuf> {
    file.ancestors().skip(1).find(|dir| dir.join("Cargo.toml").is_file()).map(Path::to_path_buf)
}

fn crate_root(dir: &Path) -> Option<PathBuf> {
    ["src/lib.rs", "src/main.rs"].iter().map(|name| dir.join(name)).find(|path| path.is_file())
}

fn module_dir(file: &Path) -> Option<PathBuf> {
    let name = file.file_name()?.to_str()?;
    let parent = file.parent()?;
    Some(if matches!(name, "mod.rs" | "lib.rs" | "main.rs") { parent.to_path_buf() } else { file.with_extension("") })
}

fn module_file(dir: &Path, crate_src: &Path) -> Option<PathBuf> {
    if dir == crate_src {
        return crate_root(crate_src.parent()?);
    }
    file(dir.with_extension("rs")).or_else(|| file(dir.join("mod.rs")))
}

fn workspace_crate(project: &Path, name: &str) -> Option<PathBuf> {
    let wanted = name.replace('-', "_");
    let mut manifests = vec![project.join("Cargo.toml")];
    if let Ok(entries) = std::fs::read_dir(project) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                manifests.push(path.join("Cargo.toml"));
                if let Ok(inner) = std::fs::read_dir(&path) {
                    manifests.extend(inner.flatten().map(|entry| entry.path().join("Cargo.toml")));
                }
            }
        }
    }
    manifests.into_iter().find_map(|manifest| {
        let text = std::fs::read_to_string(&manifest).ok()?;
        let package = text.split("[package]").nth(1)?;
        let line = package.lines().find(|line| line.trim_start().starts_with("name"))?;
        let value = line.split_once('=')?.1.trim().trim_matches('"');
        (value.replace('-', "_") == wanted).then(|| manifest.parent().map(Path::to_path_buf)).flatten()
    })
}

pub fn rust(project: &Path, file: &Path, segments: &[&str]) -> Option<(PathBuf, usize)> {
    let crate_dir = crate_dir(file)?;
    let crate_src = crate_dir.join("src");
    let (mut dir, mut current, mut index) = match *segments.first()? {
        "crate" => (crate_src.clone(), crate_root(&crate_dir)?, 1),
        "self" => (module_dir(file)?, file.to_path_buf(), 1),
        "super" => {
            let mut dir = module_dir(file)?;
            let mut index = 0;
            while segments.get(index) == Some(&"super") {
                dir = dir.parent()?.to_path_buf();
                index += 1;
            }
            let current = module_file(&dir, &crate_src)?;
            (dir, current, index)
        }
        name => match workspace_crate(project, name) {
            Some(other) => (other.join("src"), crate_root(&other)?, 1),
            None => (module_dir(file)?, file.to_path_buf(), 0),
        },
    };
    let start = index;
    while let Some(segment) = segments.get(index) {
        let next = dir.join(segment);
        let Some(found) = module_file(&next, &crate_src) else { break };
        current = found;
        dir = next;
        index += 1;
    }
    (index > start || start > 0).then_some((current, index))
}

pub fn rust_child(file: &Path, name: &str) -> Option<PathBuf> {
    let dir = module_dir(file)?;
    let crate_src = crate_dir(file)?.join("src");
    module_file(&dir.join(name), &crate_src)
}

pub fn go(project: &Path, file: &Path, spec: &str) -> Vec<PathBuf> {
    let Some(from) = file.parent() else { return Vec::new() };
    let mut dirs = Vec::new();
    if let Some(root) = from.ancestors().find(|dir| dir.join("go.mod").is_file()) {
        let module = std::fs::read_to_string(root.join("go.mod")).ok().and_then(|text| {
            text.lines().find_map(|line| line.trim().strip_prefix("module ").map(|name| name.trim().to_string()))
        });
        if let Some(rest) = module
            .as_deref()
            .and_then(|module| spec.strip_prefix(module))
            .filter(|rest| rest.is_empty() || rest.starts_with('/'))
        {
            dirs.push(root.join(rest.trim_start_matches('/')));
        }
        dirs.push(root.join("vendor").join(spec));
    }
    dirs.push(project.join("vendor").join(spec));
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
                name.ends_with(".go") && !name.ends_with("_test.go") && path.is_file()
            })
            .collect();
        if !files.is_empty() {
            files.sort();
            return files;
        }
    }
    Vec::new()
}

pub fn by_suffix(project: &Path, files: &[String], suffixes: &[String]) -> Option<PathBuf> {
    let mut best: Option<&String> = None;
    for path in files {
        let hit = suffixes.iter().any(|suffix| path == suffix || path.ends_with(&format!("/{suffix}")));
        if hit && best.is_none_or(|current| path.len() < current.len()) {
            best = Some(path);
        }
    }
    best.map(|path| project.join(path))
}

pub fn include(project: &Path, file: &Path, spec: &str, files: impl FnOnce() -> Vec<String>) -> Option<PathBuf> {
    let from = file.parent()?;
    let bases = ancestors(from, project).map(Path::to_path_buf).chain([project.join("include"), project.join("src")]);
    for base in bases {
        if let Some(found) = self::file(base.join(spec)) {
            return Some(found);
        }
    }
    by_suffix(project, &files(), &[spec.to_string()])
}

pub fn lua(project: &Path, file: &Path, spec: &str) -> Option<PathBuf> {
    let relative = spec.replace('.', "/");
    let from = file.parent()?;
    for base in ancestors(from, project).map(Path::to_path_buf).chain([project.join("lua"), project.join("src")]) {
        let found = self::file(base.join(format!("{relative}.lua")))
            .or_else(|| self::file(base.join(&relative).join("init.lua")));
        if found.is_some() {
            return found;
        }
    }
    None
}

pub fn ruby(project: &Path, file: &Path, spec: &str, relative: bool) -> Option<PathBuf> {
    let name = if spec.ends_with(".rb") { spec.to_string() } else { format!("{spec}.rb") };
    if relative {
        return self::file(file.parent()?.join(name));
    }
    ["lib", "app", ""].iter().find_map(|base| self::file(project.join(base).join(&name)))
}

pub fn path(project: &Path, file: &Path, text: &str) -> Option<PathBuf> {
    let text = text.trim();
    if text.is_empty() || text.len() > 400 || text.contains(['\n', '*', '?', '<', '>', '|']) || text.contains("://") {
        return None;
    }
    let text = text.split(['#', '?']).next().unwrap_or(text);
    let candidate = Path::new(text);
    if let Some(home) = text.strip_prefix("~/") {
        return dirs::home_dir().and_then(|home_dir| self::file(home_dir.join(home)));
    }
    if candidate.is_absolute() {
        return self::file(candidate.to_path_buf()).or_else(|| self::file(project.join(text.trim_start_matches('/'))));
    }
    let from = file.parent()?;
    self::file(from.join(text)).or_else(|| self::file(project.join(text)))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::navigate::testing::mock;

    #[test]
    fn strips_jsonc() {
        let text = "{\n // c\n \"a\": \"//x\", /* b */ \"p\": [1, 2,], // note\n \"q\": \"a,}\",\n}";
        let value: serde_json::Value = serde_json::from_str(&strip_jsonc(text)).unwrap();
        assert_eq!(value["a"], "//x");
        assert_eq!(value["p"].as_array().unwrap().len(), 2);
        assert_eq!(value["q"], "a,}");
    }

    #[test]
    fn resolves_javascript() {
        let dir = mock(&[
            ("src/app.ts", ""),
            ("src/util/index.ts", ""),
            ("src/esm.ts", ""),
            ("src/view.tsx", ""),
            ("src/lib/math.ts", ""),
            (
                "tsconfig.json",
                "{ // paths\n \"compilerOptions\": { \"baseUrl\": \".\", \"paths\": { \"@/*\": [\"src/*\"], },},}",
            ),
            ("node_modules/pkg/package.json", "{\"types\": \"dist/index.d.ts\"}"),
            ("node_modules/pkg/dist/index.d.ts", ""),
            ("node_modules/@types/node/index.d.ts", ""),
            ("node_modules/dot/package.json", "{\"main\": \".\"}"),
            ("node_modules/dot/index.js", ""),
        ]);
        let root = dir.0.as_path();
        let app = root.join("src/app.ts");
        let resolve =
            |spec: &str| javascript(root, &app, spec).map(|path| path.strip_prefix(root).unwrap().to_path_buf());
        assert_eq!(resolve("./util"), Some("src/util/index.ts".into()));
        assert_eq!(resolve("./esm.js"), Some("src/esm.ts".into()));
        assert_eq!(resolve("./view"), Some("src/view.tsx".into()));
        assert_eq!(resolve("@/lib/math"), Some("src/lib/math.ts".into()));
        assert_eq!(resolve("src/lib/math"), Some("src/lib/math.ts".into()));
        assert_eq!(resolve("pkg"), Some("node_modules/pkg/dist/index.d.ts".into()));
        assert_eq!(resolve("node"), Some("node_modules/@types/node/index.d.ts".into()));
        assert_eq!(resolve("dot"), Some("node_modules/dot/index.js".into()));
        assert_eq!(resolve("./missing"), None);
    }

    #[test]
    fn resolves_python_and_rust() {
        let dir = mock(&[
            ("pkg/__init__.py", ""),
            ("pkg/models.py", ""),
            ("pkg/sub/views.py", ""),
            ("Cargo.toml", "[package]\nname = \"app\"\n"),
            ("src/main.rs", ""),
            ("src/store.rs", ""),
            ("src/editor.rs", ""),
            ("src/editor/find_bar.rs", ""),
            ("src/net/mod.rs", ""),
            ("core/Cargo.toml", "[package]\nname = \"app-core\"\n"),
            ("core/src/lib.rs", ""),
            ("core/src/icons.rs", ""),
        ]);
        let root = dir.0.as_path();
        let views = root.join("pkg/sub/views.py");
        let relative = |path: PathBuf| path.strip_prefix(root).unwrap().to_path_buf();
        assert_eq!(python(root, &views, "..models").map(relative), Some("pkg/models.py".into()));
        assert_eq!(python(root, &views, "pkg.models").map(relative), Some("pkg/models.py".into()));
        assert_eq!(python(root, &views, "pkg").map(relative), Some("pkg/__init__.py".into()));
        let finder = root.join("src/editor/find_bar.rs");
        let editor = root.join("src/editor.rs");
        let rust = |file: &Path, path: &str| {
            let segments: Vec<&str> = path.split("::").collect();
            super::rust(root, file, &segments).map(|(path, used)| (relative(path), used))
        };
        assert_eq!(rust(&finder, "crate::store::Store"), Some(("src/store.rs".into(), 2)));
        assert_eq!(rust(&finder, "super::Editor"), Some(("src/editor.rs".into(), 1)));
        assert_eq!(rust(&editor, "find_bar::FindBar"), Some(("src/editor/find_bar.rs".into(), 1)));
        assert_eq!(rust(&editor, "crate::net"), Some(("src/net/mod.rs".into(), 2)));
        assert_eq!(rust(&editor, "app_core::icons::FOLDER"), Some(("core/src/icons.rs".into(), 2)));
        assert_eq!(rust(&editor, "std::path::Path"), None);
        assert_eq!(rust_child(&root.join("src/main.rs"), "store").map(relative), Some("src/store.rs".into()));
    }

    #[test]
    fn resolves_go_and_paths() {
        let dir = mock(&[
            ("go.mod", "module example.com/app\n\ngo 1.22\n"),
            ("main.go", ""),
            ("internal/db/db.go", ""),
            ("internal/db/db_test.go", ""),
            ("docs/guide.md", ""),
        ]);
        let root = dir.0.as_path();
        let main = root.join("main.go");
        assert_eq!(go(root, &main, "example.com/app/internal/db"), vec![root.join("internal/db/db.go")]);
        assert!(go(root, &main, "example.com/apple/internal/db").is_empty());
        assert_eq!(path(root, &main, "docs/guide.md"), Some(root.join("docs/guide.md")));
        assert_eq!(path(root, &main, "./docs/guide.md#intro"), Some(root.join("docs/guide.md")));
        assert_eq!(path(root, &main, "https://x.dev/a"), None);
    }
}
