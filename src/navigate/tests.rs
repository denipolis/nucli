use std::path::Path;
use std::sync::Arc;

use super::testing::mock;
use super::*;

fn files(root: &Path) -> Arc<Vec<String>> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path.strip_prefix(root).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    found.sort();
    Arc::new(found)
}

fn at(root: &Path, file: &str, marker: &str, nth: usize) -> Outcome {
    let full = root.join(file);
    let text = std::fs::read_to_string(&full).unwrap();
    let offset = text.match_indices(marker).nth(nth).unwrap().0 + 1;
    let language = crate::highlight::language_name(&full);
    let request = Request { project: root, file: &full, language, text: &text, offset, cancelled: &|| false };
    resolve(&request, &|| files(root))
}

fn targets(root: &Path, outcome: Outcome) -> Vec<String> {
    match outcome {
        Outcome::Locations(found) => found
            .into_iter()
            .map(|location| {
                let path = location.path.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                format!("{path}:{}:{}", location.line + 1, location.name)
            })
            .collect(),
        other => vec![format!("{other:?}")],
    }
}

#[test]
fn finds_words_and_urls() {
    assert_eq!(word_at("let foo_bar = 1;", 6), Some(4..11));
    assert_eq!(word_at("a + 12", 5), None);
    let line = "see https://example.com/a_(b). ok";
    assert_eq!(url_at(line, 10).map(|range| &line[range]), Some("https://example.com/a_(b)"));
    assert_eq!(url_at(line, 1), None);
    let line = "import x from \"./lib/util.ts\";";
    assert_eq!(path_at(line, 18).map(|range| &line[range]), Some("./lib/util.ts"));
}

#[test]
fn typescript_navigation() {
    let dir = mock(&[
        (
            "src/app.ts",
            "import Widget, { Button as Btn, helper } from './ui';\nimport * as math from './math';\n\
             import type { Config } from '@/config';\n\nconst local = 1;\n\
             export function run(config: Config) {\n  const value = helper(local);\n  \
             return new Btn(math.add(value, 2), Widget);\n}\n\nclass Thing {\n  go() { return this.stop(); }\n  stop() {}\n}\n",
        ),
        (
            "src/ui/index.ts",
            "export { Button } from './button';\nexport * from './helpers';\nexport default Panel;\nclass Panel {}\n",
        ),
        ("src/ui/button.tsx", "export class Button {\n  constructor() {}\n}\n"),
        ("src/ui/helpers.ts", "export const helper = (x: number) => x;\n"),
        (
            "src/view.tsx",
            "import styles from './view.module.css';\nimport { Button as Btn } from './ui';\n\n\
             export const View = () => <Btn className={styles.box} />;\n",
        ),
        ("src/view.module.css", ".box { color: red; }\n"),
        ("src/math.ts", "export function add(a: number, b: number) {\n  return a + b;\n}\n"),
        ("src/config.ts", "export interface Config {\n  name: string;\n}\n"),
        ("tsconfig.json", "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./src/*\"] } } }"),
    ]);
    let root = dir.0.as_path();
    let file = "src/app.ts";
    assert_eq!(targets(root, at(root, file, "'./ui'", 0)), vec!["src/ui/index.ts:1:index.ts"]);
    assert_eq!(targets(root, at(root, file, " Btn(", 0)), vec!["src/ui/button.tsx:1:Button"]);
    assert_eq!(targets(root, at(root, file, " helper(", 0)), vec!["src/ui/helpers.ts:1:helper"]);
    assert_eq!(targets(root, at(root, file, ".add(", 0)), vec!["src/math.ts:1:add"]);
    assert_eq!(targets(root, at(root, file, " Config)", 0)), vec!["src/config.ts:1:Config"]);
    assert_eq!(targets(root, at(root, file, " Widget)", 0)), vec!["src/ui/index.ts:4:Panel"]);
    assert_eq!(targets(root, at(root, file, "(local)", 0)), vec!["src/app.ts:5:local"]);
    assert_eq!(targets(root, at(root, file, "(value,", 0)), vec!["src/app.ts:7:value"]);
    assert_eq!(targets(root, at(root, file, ".stop()", 0)), vec!["src/app.ts:13:stop"]);
    assert_eq!(targets(root, at(root, file, " Button as", 0)), vec!["src/ui/button.tsx:1:Button"]);
    assert_eq!(at(root, file, " run(", 0), Outcome::Definition("run".into()));
    let view = "src/view.tsx";
    assert_eq!(targets(root, at(root, view, "<Btn", 0)), vec!["src/ui/button.tsx:1:Button"]);
    assert_eq!(targets(root, at(root, view, ".box", 0)), vec!["src/view.module.css:1:view.module.css"]);
}

#[test]
fn rust_navigation() {
    let dir = mock(&[
        ("Cargo.toml", "[package]\nname = \"app\"\n"),
        (
            "src/main.rs",
            "mod store;\nmod view;\nuse crate::store::{self, Store};\n\nfn main() {\n    let total = 1;\n    \
             let s = Store::new(total);\n    store::helper();\n    view::render(&s);\n}\n",
        ),
        (
            "src/store.rs",
            "pub struct Store {}\n\nimpl Store {\n    pub fn new(x: u32) -> Self { Store {} }\n}\n\npub fn helper() {}\n",
        ),
        (
            "src/view.rs",
            "pub fn render(store: &crate::store::Store) {}\n\nfn other() {\n    let map = std::collections::HashMap::new();\n}\n",
        ),
    ]);
    let root = dir.0.as_path();
    let file = "src/main.rs";
    assert_eq!(targets(root, at(root, file, " store;", 0)), vec!["src/store.rs:1:store.rs"]);
    assert_eq!(targets(root, at(root, file, " Store}", 0)), vec!["src/store.rs:1:Store"]);
    assert_eq!(targets(root, at(root, file, ":new", 0)), vec!["src/store.rs:4:new"]);
    assert_eq!(targets(root, at(root, file, ":helper", 0)), vec!["src/store.rs:7:helper"]);
    assert_eq!(targets(root, at(root, file, "(total)", 0)), vec!["src/main.rs:6:total"]);
    assert_eq!(targets(root, at(root, file, ":render", 0)), vec!["src/view.rs:1:render"]);
    assert_eq!(targets(root, at(root, file, " view::", 0)), vec!["src/view.rs:1:view.rs"]);
    assert_eq!(targets(root, at(root, "src/view.rs", ":new(", 0)), vec!["Nothing(\"new\")"]);
}

#[test]
fn python_and_go_navigation() {
    let dir = mock(&[
        ("pkg/__init__.py", "from .models import User\n"),
        (
            "pkg/models.py",
            "class User:\n    def __init__(self):\n        self.name = 'a'\n\n    def greet(self):\n        return self.name\n",
        ),
        ("main.py", "from pkg import User\nimport pkg.models as m\n\nu = User()\nm.User()\n"),
        ("go.mod", "module example.com/app\n"),
        (
            "main.go",
            "package main\n\nimport \"example.com/app/db\"\n\nfunc main() {\n\tconn := db.Open()\n\t_ = conn\n}\n",
        ),
        ("db/db.go", "package db\n\nfunc Open() int {\n\treturn 1\n}\n"),
    ]);
    let root = dir.0.as_path();
    assert_eq!(targets(root, at(root, "main.py", "User()", 0)), vec!["pkg/models.py:1:User"]);
    assert_eq!(targets(root, at(root, "main.py", ".User()", 0)), vec!["pkg/models.py:1:User"]);
    assert_eq!(targets(root, at(root, "main.py", ".models", 0)), vec!["pkg/models.py:1:models.py"]);
    assert_eq!(targets(root, at(root, "pkg/models.py", ".name\n", 0)), vec!["pkg/models.py:3:name"]);
    assert_eq!(targets(root, at(root, "main.go", ".Open", 0)), vec!["db/db.go:3:Open"]);
    assert_eq!(targets(root, at(root, "main.go", "\"example", 0)), vec!["db/db.go:1:db.go"]);
    assert_eq!(targets(root, at(root, "main.go", " conn\n", 0)), vec!["main.go:6:conn"]);
}

#[test]
fn outlines_nest() {
    let text = "class A {\n  run() {}\n}\nfunction b() { const x = 1; }\n";
    let found: Vec<(String, &str, usize)> =
        outline("TypeScript", text).into_iter().map(|symbol| (symbol.name, symbol.kind, symbol.depth)).collect();
    assert_eq!(found, vec![("A".into(), "class", 0), ("run".into(), "method", 1), ("b".into(), "function", 0)]);
}
