use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;

use nucli_grammars::*;
use tree_sitter::Language;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    Keyword,
    String,
    Number,
    Comment,
    Type,
    Function,
    Constant,
    Attribute,
    Punctuation,
    Property,
    Tag,
    Variable,
}

const MAX_SIZE: usize = 2 * 1024 * 1024;

const CAPTURES: [(&str, Option<Token>); 42] = [
    ("keyword", Some(Token::Keyword)),
    ("conditional", Some(Token::Keyword)),
    ("repeat", Some(Token::Keyword)),
    ("include", Some(Token::Keyword)),
    ("exception", Some(Token::Keyword)),
    ("storageclass", Some(Token::Keyword)),
    ("text.title", Some(Token::Keyword)),
    ("markup.heading", Some(Token::Keyword)),
    ("string", Some(Token::String)),
    ("character", Some(Token::String)),
    ("text.literal", Some(Token::String)),
    ("markup.raw", Some(Token::String)),
    ("number", Some(Token::Number)),
    ("float", Some(Token::Number)),
    ("constant.numeric", Some(Token::Number)),
    ("comment", Some(Token::Comment)),
    ("type", Some(Token::Type)),
    ("constructor", Some(Token::Type)),
    ("namespace", Some(Token::Type)),
    ("module", Some(Token::Type)),
    ("function", Some(Token::Function)),
    ("method", Some(Token::Function)),
    ("text.uri", Some(Token::Function)),
    ("text.reference", Some(Token::Function)),
    ("markup.link", Some(Token::Function)),
    ("constant", Some(Token::Constant)),
    ("boolean", Some(Token::Constant)),
    ("attribute", Some(Token::Attribute)),
    ("label", Some(Token::Attribute)),
    ("annotation", Some(Token::Attribute)),
    ("decorator", Some(Token::Attribute)),
    ("punctuation", Some(Token::Punctuation)),
    ("operator", Some(Token::Punctuation)),
    ("delimiter", Some(Token::Punctuation)),
    ("property", Some(Token::Property)),
    ("field", Some(Token::Property)),
    ("variable.member", Some(Token::Property)),
    ("tag", Some(Token::Tag)),
    ("variable.builtin", Some(Token::Variable)),
    ("variable.parameter", Some(Token::Variable)),
    ("parameter", Some(Token::Variable)),
    ("embedded", None),
];

const MARKDOWN_INJECTIONS: &str = r#"
(fenced_code_block
  (info_string (language) @injection.language)
  (code_fence_content) @injection.content
  (#set! injection.include-children))

((html_block) @injection.content
  (#set! injection.language "html")
  (#set! injection.include-children))
"#;

const PHP_INJECTIONS: &str = r#"
((text) @injection.content
  (#set! injection.language "html")
  (#set! injection.combined))
"#;

const SVELTE_INJECTIONS: &str = r#"
((script_element (raw_text) @injection.content)
  (#set! injection.language "typescript"))

((style_element (raw_text) @injection.content)
  (#set! injection.language "css"))

((svelte_raw_text) @injection.content
  (#set! injection.language "typescript"))
"#;

pub const PLAIN_TEXT: &str = "Plain Text";

pub const LANGUAGES: [(&str, &str); 39] = [
    ("Astro", "a.astro"),
    ("C", "a.c"),
    ("C#", "a.cs"),
    ("C++", "a.cpp"),
    ("CMake", "CMakeLists.txt"),
    ("CSS", "a.css"),
    ("Dart", "a.dart"),
    ("Diff", "a.diff"),
    ("Dockerfile", "Dockerfile"),
    ("Elixir", "a.ex"),
    ("Go", "a.go"),
    ("HTML", "a.html"),
    ("INI", "a.ini"),
    ("Java", "a.java"),
    ("JavaScript", "a.js"),
    ("JSON", "a.json"),
    ("Kotlin", "a.kt"),
    ("Lua", "a.lua"),
    ("Makefile", "Makefile"),
    ("Markdown", "a.md"),
    ("Nix", "a.nix"),
    ("PHP", "a.php"),
    ("PowerShell", "a.ps1"),
    ("Prisma", "a.prisma"),
    ("Protocol Buffers", "a.proto"),
    ("Python", "a.py"),
    ("Ruby", "a.rb"),
    ("Rust", "a.rs"),
    ("Shell", "a.sh"),
    ("Solidity", "a.sol"),
    ("SQL", "a.sql"),
    ("Svelte", "a.svelte"),
    ("Swift", "a.swift"),
    ("TOML", "a.toml"),
    ("TSX", "a.tsx"),
    ("TypeScript", "a.ts"),
    ("XML", "a.xml"),
    ("YAML", "a.yml"),
    ("Zig", "a.zig"),
];

pub fn known_language(name: &str) -> Option<&'static str> {
    if name == PLAIN_TEXT {
        return Some(PLAIN_TEXT);
    }
    LANGUAGES.iter().map(|(known, _)| *known).find(|known| *known == name)
}

pub fn sample_file(name: &str) -> Option<&'static str> {
    LANGUAGES.iter().find(|(known, _)| *known == name).map(|(_, sample)| *sample)
}

pub fn language_name(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    language(&name)
        .or_else(|| name.starts_with(".env.").then_some("Shell"))
        .or_else(|| language(&path.extension()?.to_str()?.to_lowercase()))
}

pub fn highlight(name: &str, text: &str) -> Option<Vec<(Range<usize>, Token)>> {
    if text.len() > MAX_SIZE {
        return None;
    }
    let root = config(name)?;
    let mut highlighter = Highlighter::new();
    let events = highlighter
        .highlight(root, text.as_bytes(), None, None, |name: &str| config(language(&name.to_lowercase())?))
        .ok()?;
    let mut stack = Vec::new();
    let mut ranges: Vec<(Range<usize>, Token)> = Vec::new();
    for event in events {
        match event.ok()? {
            HighlightEvent::HighlightStart(highlight) => stack.push(CAPTURES[highlight.0].1),
            HighlightEvent::HighlightEnd => {
                stack.pop();
            }
            HighlightEvent::Source { start, end } => {
                let Some(&Some(token)) = stack.last() else { continue };
                match ranges.last_mut() {
                    Some((range, last)) if range.end == start && *last == token => range.end = end,
                    _ => ranges.push((start..end, token)),
                }
            }
        }
    }
    Some(ranges)
}

fn language(key: &str) -> Option<&'static str> {
    Some(match key {
        "rs" | "rust" => "Rust",
        "js" | "mjs" | "cjs" | "jsx" | "javascript" => "JavaScript",
        "ts" | "mts" | "cts" | "typescript" => "TypeScript",
        "tsx" => "TSX",
        "py" | "pyi" | "python" => "Python",
        "go" | "golang" => "Go",
        "json" | "jsonc" => "JSON",
        "toml" | "cargo.lock" => "TOML",
        "yml" | "yaml" => "YAML",
        "sh" | "bash" | "zsh" | "shell" | ".bashrc" | ".bash_profile" | ".zshrc" | ".zprofile" | ".zshenv"
        | ".profile" | ".env" => "Shell",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "c++" => "C++",
        "css" => "CSS",
        "html" | "htm" => "HTML",
        "md" | "markdown" => "Markdown",
        "java" => "Java",
        "rb" | "ruby" | "gemfile" | "rakefile" => "Ruby",
        "swift" => "Swift",
        "lua" => "Lua",
        "mk" | "make" | "makefile" | "gnumakefile" => "Makefile",
        "kt" | "kts" => "Kotlin",
        "dockerfile" | "containerfile" => "Dockerfile",
        "cs" => "C#",
        "fs" | "fsx" => "F#",
        "php" => "PHP",
        "sql" => "SQL",
        "xml" | "plist" | "xib" | "storyboard" | "svg" => "XML",
        "scss" => "SCSS",
        "sass" => "Sass",
        "less" => "Less",
        "vue" => "Vue",
        "svelte" => "Svelte",
        "astro" => "Astro",
        "scala" => "Scala",
        "dart" => "Dart",
        "ex" | "exs" => "Elixir",
        "erl" | "hrl" => "Erlang",
        "hs" => "Haskell",
        "ml" | "mli" => "OCaml",
        "clj" | "cljs" | "edn" => "Clojure",
        "zig" | "zon" => "Zig",
        "nim" => "Nim",
        "nix" => "Nix",
        "r" => "R",
        "jl" => "Julia",
        "pl" | "pm" => "Perl",
        "m" | "mm" => "Objective-C",
        "ps1" | "psm1" => "PowerShell",
        "bat" | "cmd" => "Batch",
        "fish" => "Fish",
        "graphql" | "gql" => "GraphQL",
        "proto" => "Protocol Buffers",
        "prisma" => "Prisma",
        "sol" => "Solidity",
        "tf" | "tfvars" => "Terraform",
        "gradle" | "groovy" => "Groovy",
        "cmake" | "cmakelists.txt" => "CMake",
        "ini" | "cfg" | "conf" | ".editorconfig" | ".gitconfig" => "INI",
        "csv" => "CSV",
        "diff" | "patch" => "Diff",
        _ => return None,
    })
}

fn config(name: &str) -> Option<&'static HighlightConfiguration> {
    static CONFIGS: [OnceLock<Option<HighlightConfiguration>>; LANGUAGES.len()] =
        [const { OnceLock::new() }; LANGUAGES.len()];
    let index = LANGUAGES.iter().position(|(known, _)| *known == name)?;
    CONFIGS[index].get_or_init(|| build(name)).as_ref()
}

pub fn grammar(name: &str) -> Option<Language> {
    nucli_grammars::language(name).map(Language::new)
}

fn build(name: &str) -> Option<HighlightConfiguration> {
    let js = tree_sitter_javascript::HIGHLIGHT_QUERY;
    let jsx = tree_sitter_javascript::JSX_HIGHLIGHT_QUERY;
    let ts = tree_sitter_typescript::HIGHLIGHTS_QUERY;
    let js_locals = tree_sitter_javascript::LOCALS_QUERY;
    let ts_locals = tree_sitter_typescript::LOCALS_QUERY;
    let (highlights, injections, locals): (String, &str, String) = match name {
        "Rust" => (tree_sitter_rust::HIGHLIGHTS_QUERY.into(), tree_sitter_rust::INJECTIONS_QUERY, String::new()),
        "JavaScript" => (format!("{js}\n{jsx}"), tree_sitter_javascript::INJECTIONS_QUERY, js_locals.into()),
        "TypeScript" => {
            (format!("{js}\n{ts}"), tree_sitter_javascript::INJECTIONS_QUERY, format!("{js_locals}\n{ts_locals}"))
        }
        "TSX" => (
            format!("{js}\n{jsx}\n{ts}"),
            tree_sitter_javascript::INJECTIONS_QUERY,
            format!("{js_locals}\n{ts_locals}"),
        ),
        "Python" => (tree_sitter_python::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Go" => (
            format!(
                "(field_identifier) @property\n{}",
                tree_sitter_go::HIGHLIGHTS_QUERY.replace("(field_identifier) @property\n(identifier) @variable\n", "")
            ),
            "",
            String::new(),
        ),
        "JSON" => (tree_sitter_json::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "TOML" => (tree_sitter_toml_ng::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "YAML" => (tree_sitter_yaml::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Shell" => (tree_sitter_bash::HIGHLIGHT_QUERY.into(), "", String::new()),
        "C" => (tree_sitter_c::HIGHLIGHT_QUERY.into(), "", String::new()),
        "C++" => {
            (format!("{}\n{}", tree_sitter_c::HIGHLIGHT_QUERY, tree_sitter_cpp::HIGHLIGHT_QUERY), "", String::new())
        }
        "CSS" => (tree_sitter_css::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "HTML" => (tree_sitter_html::HIGHLIGHTS_QUERY.into(), tree_sitter_html::INJECTIONS_QUERY, String::new()),
        "Markdown" => (tree_sitter_md::HIGHLIGHT_QUERY_BLOCK.into(), MARKDOWN_INJECTIONS, String::new()),
        "Java" => (tree_sitter_java::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Ruby" => (tree_sitter_ruby::HIGHLIGHTS_QUERY.into(), "", tree_sitter_ruby::LOCALS_QUERY.into()),
        "Swift" => (
            tree_sitter_swift::HIGHLIGHTS_QUERY.into(),
            tree_sitter_swift::INJECTIONS_QUERY,
            tree_sitter_swift::LOCALS_QUERY.into(),
        ),
        "Lua" => (
            tree_sitter_lua::HIGHLIGHTS_QUERY.into(),
            tree_sitter_lua::INJECTIONS_QUERY,
            tree_sitter_lua::LOCALS_QUERY.into(),
        ),
        "Makefile" => (tree_sitter_make::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Astro" => {
            (tree_sitter_astro_next::HIGHLIGHTS_QUERY.into(), tree_sitter_astro_next::INJECTIONS_QUERY, String::new())
        }
        "C#" => (tree_sitter_c_sharp::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "CMake" => (
            tree_sitter_cmake::HIGHLIGHTS_QUERY
                .replace("#lua-match?", "#match?")
                .replace("%u@][%u%d_", "A-Z@][A-Z0-9_"),
            "",
            String::new(),
        ),
        "Dart" => (tree_sitter_dart::HIGHLIGHTS_QUERY.into(), "", tree_sitter_dart::LOCALS_QUERY.into()),
        "Diff" => (tree_sitter_diff::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Dockerfile" => (
            tree_sitter_containerfile::HIGHLIGHTS_QUERY.into(),
            tree_sitter_containerfile::INJECTIONS_QUERY,
            String::new(),
        ),
        "Elixir" => (tree_sitter_elixir::HIGHLIGHTS_QUERY.into(), tree_sitter_elixir::INJECTIONS_QUERY, String::new()),
        "INI" => (tree_sitter_ini::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Kotlin" => (tree_sitter_kotlin_sg::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Nix" => (tree_sitter_nix::HIGHLIGHTS_QUERY.into(), tree_sitter_nix::INJECTIONS_QUERY, String::new()),
        "PHP" => (tree_sitter_php::HIGHLIGHTS_QUERY.into(), PHP_INJECTIONS, String::new()),
        "PowerShell" => (tree_sitter_powershell::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Prisma" => (tree_sitter_prisma::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Protocol Buffers" => (tree_sitter_proto::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Solidity" => (
            tree_sitter_solidity::HIGHLIGHT_QUERY.replace("@type .))", "@type))"),
            "",
            tree_sitter_solidity::LOCALS_QUERY.into(),
        ),
        "SQL" => (tree_sitter_sequel::HIGHLIGHTS_QUERY.into(), "", String::new()),
        "Svelte" => (
            format!("{}\n{}", tree_sitter_html::HIGHLIGHTS_QUERY, tree_sitter_svelte_ng::HIGHLIGHTS_QUERY),
            SVELTE_INJECTIONS,
            String::new(),
        ),
        "XML" => (tree_sitter_xml::XML_HIGHLIGHT_QUERY.into(), "", String::new()),
        "Zig" => (tree_sitter_zig::HIGHLIGHTS_QUERY.replace("#lua-match?", "#match?"), "", String::new()),
        _ => return None,
    };
    let highlights = highlights.replace(" @spell", "").replace(" @nospell", "");
    let mut config = HighlightConfiguration::new(grammar(name)?, name, &highlights, injections, &locals).ok()?;
    config.configure(&CAPTURES.map(|(capture, _)| capture));
    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(path: &str, text: &str) -> Vec<(String, Token)> {
        highlight(language_name(Path::new(path)).unwrap(), text)
            .unwrap()
            .into_iter()
            .map(|(range, token)| (text[range].to_string(), token))
            .collect()
    }

    #[test]
    fn detects_languages() {
        assert_eq!(language_name(Path::new("src/main.rs")), Some("Rust"));
        assert_eq!(language_name(Path::new("App.TSX")), Some("TSX"));
        assert_eq!(language_name(Path::new("Makefile")), Some("Makefile"));
        assert_eq!(language_name(Path::new("Cargo.lock")), Some("TOML"));
        assert_eq!(language_name(Path::new("/home/me/.zshrc")), Some("Shell"));
        assert_eq!(language_name(Path::new("Gemfile")), Some("Ruby"));
        assert_eq!(language_name(Path::new("Dockerfile")), Some("Dockerfile"));
        assert_eq!(language_name(Path::new("build.gradle.kts")), Some("Kotlin"));
        assert_eq!(language_name(Path::new(".env.local")), Some("Shell"));
        assert_eq!(language_name(Path::new("icon.svg")), Some("XML"));
        assert_eq!(language_name(Path::new("LICENSE")), None);
        assert_eq!(language_name(Path::new("archive.xyz")), None);
        assert!(highlight("Scala", "object A").is_none());
    }

    #[test]
    fn every_grammar_builds() {
        for (name, _) in LANGUAGES {
            assert!(config(name).is_some(), "{name}");
        }
    }

    #[test]
    fn highlights_tokens() {
        let text = "// hi\nfn main() { let x = \"a\\n\"; }\n";
        let found = tokens("main.rs", text);
        assert!(found.contains(&("// hi".into(), Token::Comment)));
        assert!(found.contains(&("fn".into(), Token::Keyword)));
        assert!(found.contains(&("main".into(), Token::Function)));
        assert!(found.contains(&("\"a\\n\"".into(), Token::String)));
        assert!(!found.iter().any(|(text, _)| text == "x"));
        let ranges = highlight("Rust", text).unwrap();
        assert!(ranges.windows(2).all(|pair| pair[0].0.end <= pair[1].0.start));
        let found = tokens("main.go", "func main() { fmt.Println(1.5) }");
        assert!(found.contains(&("main".into(), Token::Function)));
        assert!(found.contains(&("Println".into(), Token::Function)));
        assert!(found.contains(&("1.5".into(), Token::Number)));
        let found = tokens("Program.cs", "class A { string F() { return \"s\"; } }");
        assert!(found.contains(&("class".into(), Token::Keyword)));
        assert!(found.contains(&("F".into(), Token::Function)));
        assert!(found.contains(&("\"s\"".into(), Token::String)));
        let found = tokens("query.sql", "-- c\nSELECT 'a' FROM t;");
        assert!(found.contains(&("-- c".into(), Token::Comment)));
        assert!(found.contains(&("SELECT".into(), Token::Keyword)));
        assert!(found.contains(&("'a'".into(), Token::String)));
        let found = tokens("main.zig", "// c\nconst MAX = 2;\nconst x = 1;");
        assert!(found.contains(&("// c".into(), Token::Comment)));
        assert!(found.contains(&("MAX".into(), Token::Constant)));
        assert!(!found.iter().any(|(text, _)| text == "x"));
    }

    #[test]
    fn highlights_injections() {
        let found = tokens("index.html", "<script>let a = 1;</script>");
        assert!(found.contains(&("script".into(), Token::Tag)));
        assert!(found.contains(&("let".into(), Token::Keyword)));
        let found = tokens("README.md", "# Title\n\n```rust\nfn f() { let x = 1; }\n```\n");
        assert!(found.contains(&("Title".into(), Token::Keyword)));
        assert!(found.contains(&("fn".into(), Token::Keyword)));
        assert!(found.contains(&("let".into(), Token::Keyword)));
        let found =
            tokens("App.svelte", "<script lang=\"ts\">let x: number = 1;</script>\n<style>p { color: red; }</style>");
        assert!(found.contains(&("number".into(), Token::Type)));
        assert!(found.contains(&("color".into(), Token::Property)));
        let found = tokens("index.php", "<p><?php echo \"s\"; ?></p>");
        assert!(found.contains(&("p".into(), Token::Tag)));
        assert!(found.contains(&("echo".into(), Token::Keyword)));
    }

    #[test]
    fn rejects_large_text() {
        assert!(highlight("JSON", &" ".repeat(MAX_SIZE + 1)).is_none());
    }
}
