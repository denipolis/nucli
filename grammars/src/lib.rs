use tree_sitter_language::LanguageFn;

pub use {
    tree_sitter_astro_next, tree_sitter_bash, tree_sitter_c, tree_sitter_c_sharp, tree_sitter_cmake,
    tree_sitter_containerfile, tree_sitter_cpp, tree_sitter_css, tree_sitter_dart, tree_sitter_diff,
    tree_sitter_elixir, tree_sitter_go, tree_sitter_html, tree_sitter_ini, tree_sitter_java, tree_sitter_javascript,
    tree_sitter_json, tree_sitter_kotlin_sg, tree_sitter_lua, tree_sitter_make, tree_sitter_md, tree_sitter_nix,
    tree_sitter_php, tree_sitter_powershell, tree_sitter_prisma, tree_sitter_proto, tree_sitter_python,
    tree_sitter_ruby, tree_sitter_rust, tree_sitter_sequel, tree_sitter_solidity, tree_sitter_svelte_ng,
    tree_sitter_swift, tree_sitter_toml_ng, tree_sitter_typescript, tree_sitter_xml, tree_sitter_yaml, tree_sitter_zig,
};

macro_rules! grammars {
    ($($file:literal $name:literal $language:expr;)*) => {
        fn linked(name: &str) -> Option<LanguageFn> {
            match name {
                $(#[cfg(feature = $file)] $name => Some($language),)*
                _ => None,
            }
        }

        fn file(name: &str) -> Option<&'static str> {
            match name {
                $($name => Some($file),)*
                _ => None,
            }
        }
    };
}

grammars! {
    "rust" "Rust" tree_sitter_rust::LANGUAGE;
    "javascript" "JavaScript" tree_sitter_javascript::LANGUAGE;
    "typescript" "TypeScript" tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
    "typescript" "TSX" tree_sitter_typescript::LANGUAGE_TSX;
    "python" "Python" tree_sitter_python::LANGUAGE;
    "go" "Go" tree_sitter_go::LANGUAGE;
    "json" "JSON" tree_sitter_json::LANGUAGE;
    "toml" "TOML" tree_sitter_toml_ng::LANGUAGE;
    "yaml" "YAML" tree_sitter_yaml::LANGUAGE;
    "bash" "Shell" tree_sitter_bash::LANGUAGE;
    "c" "C" tree_sitter_c::LANGUAGE;
    "cpp" "C++" tree_sitter_cpp::LANGUAGE;
    "css" "CSS" tree_sitter_css::LANGUAGE;
    "html" "HTML" tree_sitter_html::LANGUAGE;
    "markdown" "Markdown" tree_sitter_md::LANGUAGE;
    "java" "Java" tree_sitter_java::LANGUAGE;
    "ruby" "Ruby" tree_sitter_ruby::LANGUAGE;
    "swift" "Swift" tree_sitter_swift::LANGUAGE;
    "lua" "Lua" tree_sitter_lua::LANGUAGE;
    "make" "Makefile" tree_sitter_make::LANGUAGE;
    "astro" "Astro" tree_sitter_astro_next::LANGUAGE;
    "c-sharp" "C#" tree_sitter_c_sharp::LANGUAGE;
    "cmake" "CMake" tree_sitter_cmake::LANGUAGE;
    "dart" "Dart" tree_sitter_dart::LANGUAGE;
    "diff" "Diff" tree_sitter_diff::LANGUAGE;
    "containerfile" "Dockerfile" tree_sitter_containerfile::LANGUAGE;
    "elixir" "Elixir" tree_sitter_elixir::LANGUAGE;
    "ini" "INI" tree_sitter_ini::LANGUAGE;
    "kotlin" "Kotlin" tree_sitter_kotlin_sg::LANGUAGE;
    "nix" "Nix" tree_sitter_nix::LANGUAGE;
    "php" "PHP" tree_sitter_php::LANGUAGE_PHP;
    "powershell" "PowerShell" tree_sitter_powershell::LANGUAGE;
    "prisma" "Prisma" tree_sitter_prisma::LANGUAGE;
    "proto" "Protocol Buffers" tree_sitter_proto::LANGUAGE;
    "solidity" "Solidity" tree_sitter_solidity::LANGUAGE;
    "sql" "SQL" tree_sitter_sequel::LANGUAGE;
    "svelte" "Svelte" tree_sitter_svelte_ng::LANGUAGE;
    "xml" "XML" tree_sitter_xml::LANGUAGE_XML;
    "zig" "Zig" tree_sitter_zig::LANGUAGE;
}

pub fn language(name: &str) -> Option<LanguageFn> {
    linked(name).or_else(|| load(file(name)?, name))
}

#[unsafe(no_mangle)]
unsafe extern "C" fn nucli_grammar(name: *const u8, length: usize) -> Option<unsafe extern "C" fn() -> *const ()> {
    let name = std::str::from_utf8(unsafe { std::slice::from_raw_parts(name, length) }).ok()?;
    linked(name).map(LanguageFn::into_raw)
}

#[cfg(windows)]
fn load(file: &str, name: &str) -> Option<LanguageFn> {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::core::{HSTRING, s};
    type Entry = unsafe extern "C" fn(*const u8, usize) -> Option<unsafe extern "C" fn() -> *const ()>;
    let path = std::env::current_exe().ok()?.with_file_name("grammars").join(format!("{file}.dll"));
    unsafe {
        let library = LoadLibraryW(&HSTRING::from(path.as_path())).ok()?;
        let entry = std::mem::transmute::<unsafe extern "system" fn() -> isize, Entry>(GetProcAddress(
            library,
            s!("nucli_grammar"),
        )?);
        entry(name.as_ptr(), name.len()).map(|raw| LanguageFn::from_raw(raw))
    }
}

#[cfg(not(windows))]
fn load(_: &str, _: &str) -> Option<LanguageFn> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn exports_linked_grammars() {
        let found = |name: &str| unsafe { super::nucli_grammar(name.as_ptr(), name.len()) }.is_some();
        assert_eq!(found("Rust"), cfg!(feature = "rust"));
        assert_eq!(found("TSX"), cfg!(feature = "typescript"));
        assert!(!found("Nope"));
        assert_eq!(super::file("C#"), Some("c-sharp"));
    }
}
