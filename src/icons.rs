use crate::theme::ItemColor;

pub const FOLDER: &str = "\u{e5ff}";
pub const FOLDER_OPEN: &str = "\u{e5fe}";
pub const FILE: &str = "\u{ea7b}";
pub const TERMINAL: &str = "\u{f489}";
pub const AGENT: &str = "\u{ec10}";
pub const BRANCH: &str = "\u{e725}";
pub const COMMIT: &str = "\u{f417}";
pub const CHECK: &str = "\u{f00c}";
pub const CROSS: &str = "\u{f00d}";
pub const QUESTION: &str = "\u{f128}";
pub const CLOSE: &str = "\u{ea76}";
pub const ADD: &str = "\u{ea60}";
pub const REMOVE: &str = "\u{eb3b}";
pub const CHEVRON_DOWN: &str = "\u{eab4}";
pub const CHEVRON_RIGHT: &str = "\u{eab6}";
pub const CHEVRON_UP: &str = "\u{eab7}";
pub const HOME: &str = "\u{eb06}";
pub const SIDEBAR: &str = "\u{ebf3}";
pub const PANEL: &str = "\u{ebf2}";
pub const LAYOUT: &str = "\u{eaf0}";
pub const HISTORY: &str = "\u{ea82}";
pub const DOT: &str = "\u{f444}";
pub const ARROW_UP: &str = "\u{eaa1}";
pub const ARROW_DOWN: &str = "\u{ea9a}";
pub const LINK: &str = "\u{eb14}";
pub const DISCARD: &str = "\u{eae2}";
pub const MAXIMIZE: &str = "\u{eb4c}";
pub const WINDOW: &str = "\u{eb7f}";
pub const REVEAL: &str = "\u{eaf7}";
pub const BELL: &str = "\u{eaa2}";
pub const COMMAND: &str = "\u{eb63}";
pub const TAG: &str = "\u{ea66}";
pub const CLOUD: &str = "\u{ebaa}";
pub const THEME: &str = "\u{eb5c}";

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn file(name: &str) -> (&'static str, ItemColor) {
pub fn agent(command: &str) -> (&'static str, ItemColor) {
    match command {
        "claude" => ("\u{f06c4}", ItemColor::Orange),
        "codex" => ("\u{f02d9}", ItemColor::Mint),
        "gemini" => ("\u{f0ae2}", ItemColor::Blue),
        "opencode" => ("\u{eac4}", ItemColor::Gray),
        "pi" => ("\u{f03ff}", ItemColor::Purple),
        "amp" => ("\u{f140b}", ItemColor::Yellow),
        "aider" => ("\u{f01a6}", ItemColor::Teal),
        "cursor-agent" => ("\u{f01ff}", ItemColor::Indigo),
        "goose" => ("\u{f01e5}", ItemColor::Brown),
        "crush" => ("\u{f004}", ItemColor::Pink),
        "qwen" => ("Q", ItemColor::Purple),
        "copilot" => ("\u{f4b8}", ItemColor::Blue),
        "droid" => ("\u{f0032}", ItemColor::Green),
        _ => (AGENT, ItemColor::Gray),
    }
}

    let lower = name.to_lowercase();
    let by_name = match lower.as_str() {
        "dockerfile" | "containerfile" => Some(("\u{e650}", ItemColor::Blue)),
        "makefile" | "justfile" => Some(("\u{e673}", ItemColor::Orange)),
        "license" | "license.md" | "license.txt" | "copying" => Some(("\u{e60a}", ItemColor::Yellow)),
        ".gitignore" | ".gitattributes" | ".gitmodules" => Some(("\u{e65d}", ItemColor::Orange)),
        "package.json" | "package-lock.json" | ".npmrc" => Some(("\u{e616}", ItemColor::Red)),
        "cargo.lock" | "package.resolved" | "yarn.lock" | "pnpm-lock.yaml" | "bun.lock" | "flake.lock" => {
            Some(("\u{e672}", ItemColor::Gray))
        }
        _ => None,
    };
    if let Some(found) = by_name {
        return found;
    }
    let extension = lower.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");
    match extension {
        "rs" => ("\u{e68b}", ItemColor::Orange),
        "swift" => ("\u{e699}", ItemColor::Orange),
        "py" | "pyi" => ("\u{e606}", ItemColor::Blue),
        "js" | "mjs" | "cjs" => ("\u{e60c}", ItemColor::Yellow),
        "jsx" | "tsx" => ("\u{e625}", ItemColor::Teal),
        "ts" | "mts" | "cts" => ("\u{e628}", ItemColor::Blue),
        "json" | "jsonc" | "json5" => ("\u{e60b}", ItemColor::Yellow),
        "md" | "markdown" | "mdx" => ("\u{e609}", ItemColor::Blue),
        "go" => ("\u{e627}", ItemColor::Teal),
        "c" | "h" => ("\u{e649}", ItemColor::Blue),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "mm" | "m" => ("\u{e646}", ItemColor::Indigo),
        "cs" => ("\u{e648}", ItemColor::Purple),
        "java" => ("\u{e66d}", ItemColor::Red),
        "kt" | "kts" => ("\u{e634}", ItemColor::Purple),
        "rb" => ("\u{e605}", ItemColor::Red),
        "php" => ("\u{e608}", ItemColor::Indigo),
        "lua" => ("\u{e620}", ItemColor::Blue),
        "zig" => ("\u{e6a9}", ItemColor::Orange),
        "dart" => ("\u{e64c}", ItemColor::Teal),
        "scala" | "sc" => ("\u{e68e}", ItemColor::Red),
        "ex" | "exs" => ("\u{e62d}", ItemColor::Purple),
        "hs" => ("\u{e61f}", ItemColor::Purple),
        "ml" | "mli" => ("\u{e67a}", ItemColor::Orange),
        "clj" | "cljs" | "edn" => ("\u{e642}", ItemColor::Green),
        "pl" | "pm" => ("\u{e67e}", ItemColor::Blue),
        "r" => ("\u{e68a}", ItemColor::Blue),
        "jl" => ("\u{e624}", ItemColor::Purple),
        "vue" => ("\u{e6a0}", ItemColor::Green),
        "svelte" => ("\u{e697}", ItemColor::Orange),
        "html" | "htm" => ("\u{e60e}", ItemColor::Orange),
        "css" => ("\u{e614}", ItemColor::Blue),
        "scss" | "sass" => ("\u{e603}", ItemColor::Pink),
        "less" => ("\u{e60b}", ItemColor::Indigo),
        "graphql" | "gql" => ("\u{e662}", ItemColor::Pink),
        "xml" | "plist" | "xib" | "storyboard" => ("\u{e619}", ItemColor::Orange),
        "yml" | "yaml" => ("\u{e6a8}", ItemColor::Purple),
        "toml" | "ini" | "cfg" | "conf" | "env" | "properties" => ("\u{e615}", ItemColor::Gray),
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" => ("\u{e691}", ItemColor::Green),
        "sql" | "db" | "sqlite" => ("\u{e64d}", ItemColor::Yellow),
        "csv" | "tsv" => ("\u{e64a}", ItemColor::Green),
        "tex" => ("\u{e69b}", ItemColor::Teal),
        "wasm" => ("\u{e6a1}", ItemColor::Purple),
        "gradle" => ("\u{e660}", ItemColor::Teal),
        "lock" => ("\u{e672}", ItemColor::Gray),
        "svg" => ("\u{e698}", ItemColor::Yellow),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "icns" | "tif" | "tiff" | "heic" => {
            ("\u{e60d}", ItemColor::Purple)
        }
        "pdf" => ("\u{e67d}", ItemColor::Red),
        "ttf" | "otf" | "woff" | "woff2" => ("\u{e659}", ItemColor::Gray),
        "mp4" | "mov" | "mkv" | "webm" | "avi" => ("\u{e69f}", ItemColor::Pink),
        "mp3" | "wav" | "flac" | "ogg" | "m4a" => ("\u{e638}", ItemColor::Pink),
        "zip" | "gz" | "tgz" | "xz" | "bz2" | "7z" | "rar" | "tar" | "dmg" => ("\u{e6aa}", ItemColor::Brown),
        "txt" | "log" => ("\u{e64e}", ItemColor::Gray),
        _ => ("\u{e64e}", ItemColor::Gray),
    }
}
