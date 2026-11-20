use tree_sitter::Node;

#[derive(Clone, Debug, PartialEq)]
pub enum Imported {
    Module,
    Default,
    Name(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub binding: String,
    pub imported: Imported,
    pub source: String,
    pub export: bool,
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

pub fn string_content(node: Node, source: &str) -> String {
    let raw = text(node, source);
    raw.trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string()
}

pub fn imports(language: &str, root: Node, source: &str) -> Vec<Import> {
    let mut found = Vec::new();
    match language {
        "TypeScript" | "TSX" | "JavaScript" => {
            for node in children(root) {
                javascript(node, source, &mut found);
            }
        }
        "Python" => {
            for node in children(root) {
                python(node, source, &mut found);
            }
        }
        "Rust" => rust_items(root, source, &mut found),
        "Go" => go(root, source, &mut found),
        "Java" => java(root, source, &mut found),
        _ => {}
    }
    found
}

fn push(found: &mut Vec<Import>, binding: &str, imported: Imported, source: &str, export: bool) {
    found.push(Import { binding: binding.to_string(), imported, source: source.to_string(), export });
}

fn javascript(node: Node, source: &str, found: &mut Vec<Import>) {
    let from = node.child_by_field_name("source").map(|spec| string_content(spec, source));
    match node.kind() {
        "import_statement" => {
            for part in children(node) {
                if part.kind() == "import_require_clause"
                    && let (Some(name), Some(spec)) = (part.named_child(0), part.child_by_field_name("source"))
                {
                    push(found, text(name, source), Imported::Module, &string_content(spec, source), false);
                }
                let Some(from) = from.as_deref() else { continue };
                if part.kind() != "import_clause" {
                    continue;
                }
                for clause in children(part) {
                    match clause.kind() {
                        "identifier" => push(found, text(clause, source), Imported::Default, from, false),
                        "namespace_import" => {
                            if let Some(name) = clause.named_child(0) {
                                push(found, text(name, source), Imported::Module, from, false);
                            }
                        }
                        "named_imports" => {
                            for specifier in children(clause) {
                                let Some(name) = specifier.child_by_field_name("name") else { continue };
                                let binding = specifier.child_by_field_name("alias").unwrap_or(name);
                                let name = string_content(name, source);
                                push(found, &string_content(binding, source), Imported::Name(name), from, false);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        "export_statement" => {
            let mut exported = false;
            for part in children(node) {
                match part.kind() {
                    "export_clause" => {
                        exported = true;
                        for specifier in children(part) {
                            let Some(name) = specifier.child_by_field_name("name") else { continue };
                            let binding = specifier.child_by_field_name("alias").unwrap_or(name);
                            let name = string_content(name, source);
                            let imported = if name == "default" { Imported::Default } else { Imported::Name(name) };
                            push(
                                found,
                                &string_content(binding, source),
                                imported,
                                from.as_deref().unwrap_or(""),
                                true,
                            );
                        }
                    }
                    "namespace_export" => {
                        exported = true;
                        if let (Some(name), Some(from)) = (part.named_child(0), &from) {
                            push(found, &string_content(name, source), Imported::Module, from, true);
                        }
                    }
                    _ => {}
                }
            }
            if !exported && let Some(from) = from {
                push(found, "*", Imported::Module, &from, true);
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            for declarator in children(node).into_iter().filter(|child| child.kind() == "variable_declarator") {
                let (Some(name), Some(value)) =
                    (declarator.child_by_field_name("name"), declarator.child_by_field_name("value"))
                else {
                    continue;
                };
                let Some(spec) = required(value, source) else { continue };
                match name.kind() {
                    "identifier" => push(found, text(name, source), Imported::Module, &spec, false),
                    "object_pattern" => {
                        for property in children(name) {
                            match property.kind() {
                                "shorthand_property_identifier_pattern" => {
                                    let name = text(property, source);
                                    push(found, name, Imported::Name(name.to_string()), &spec, false);
                                }
                                "pair_pattern" => {
                                    if let (Some(key), Some(value)) =
                                        (property.child_by_field_name("key"), property.child_by_field_name("value"))
                                    {
                                        let imported = Imported::Name(text(key, source).to_string());
                                        push(found, text(value, source), imported, &spec, false);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn required(value: Node, source: &str) -> Option<String> {
    if value.kind() != "call_expression" {
        return None;
    }
    let function = value.child_by_field_name("function")?;
    if text(function, source) != "require" {
        return None;
    }
    let argument = value.child_by_field_name("arguments")?.named_child(0)?;
    (argument.kind() == "string").then(|| string_content(argument, source))
}

fn python(node: Node, source: &str, found: &mut Vec<Import>) {
    match node.kind() {
        "import_statement" => {
            for name in children(node) {
                match name.kind() {
                    "dotted_name" => {
                        let module = text(name, source);
                        let first = module.split('.').next().unwrap_or(module);
                        push(found, first, Imported::Module, first, false);
                    }
                    "aliased_import" => {
                        if let (Some(module), Some(alias)) =
                            (name.child_by_field_name("name"), name.child_by_field_name("alias"))
                        {
                            push(found, text(alias, source), Imported::Module, text(module, source), false);
                        }
                    }
                    _ => {}
                }
            }
        }
        "import_from_statement" => {
            let Some(module) = node.child_by_field_name("module_name") else { return };
            let from = text(module, source);
            for name in children(node) {
                if name.id() == module.id() {
                    continue;
                }
                match name.kind() {
                    "dotted_name" => {
                        let imported = text(name, source);
                        push(found, imported, Imported::Name(imported.to_string()), from, false);
                    }
                    "aliased_import" => {
                        if let (Some(imported), Some(alias)) =
                            (name.child_by_field_name("name"), name.child_by_field_name("alias"))
                        {
                            let imported = Imported::Name(text(imported, source).to_string());
                            push(found, text(alias, source), imported, from, false);
                        }
                    }
                    "wildcard_import" => push(found, "*", Imported::Module, from, false),
                    _ => {}
                }
            }
        }
        "if_statement" | "try_statement" | "block" | "else_clause" | "except_clause" => {
            for child in children(node) {
                python(child, source, found);
            }
        }
        _ => {}
    }
}

fn rust_items(root: Node, source: &str, found: &mut Vec<Import>) {
    for node in children(root) {
        if node.kind() == "use_declaration" {
            let export = children(node).iter().any(|child| child.kind() == "visibility_modifier");
            if let Some(argument) = node.child_by_field_name("argument") {
                rust_tree(argument, "", source, export, found);
            }
        }
    }
}

fn join(prefix: &str, path: &str) -> String {
    if prefix.is_empty() { path.to_string() } else { format!("{prefix}::{path}") }
}

fn rust_tree(node: Node, prefix: &str, source: &str, export: bool, found: &mut Vec<Import>) {
    match node.kind() {
        "identifier" | "scoped_identifier" | "crate" | "self" | "super" => {
            let path = join(prefix, text(node, source));
            let binding = path.rsplit("::").next().unwrap_or(&path).to_string();
            let (path, binding) = match binding.as_str() {
                "self" => {
                    let parent = path.trim_end_matches("::self").to_string();
                    let binding = parent.rsplit("::").next().unwrap_or(&parent).to_string();
                    (parent, binding)
                }
                _ => (path, binding),
            };
            push(found, &binding, Imported::Module, &path, export);
        }
        "use_as_clause" => {
            if let (Some(path), Some(alias)) = (node.child_by_field_name("path"), node.child_by_field_name("alias")) {
                push(found, text(alias, source), Imported::Module, &join(prefix, text(path, source)), export);
            }
        }
        "scoped_use_list" => {
            let inner = node.child_by_field_name("path").map(|path| join(prefix, text(path, source)));
            let inner = inner.unwrap_or_else(|| prefix.to_string());
            if let Some(list) = node.child_by_field_name("list") {
                rust_tree(list, &inner, source, export, found);
            }
        }
        "use_list" => {
            for child in children(node) {
                rust_tree(child, prefix, source, export, found);
            }
        }
        "use_wildcard" => {
            let path = node.named_child(0).map(|path| join(prefix, text(path, source)));
            push(found, "*", Imported::Module, &path.unwrap_or_else(|| prefix.to_string()), export);
        }
        _ => {}
    }
}

fn go(root: Node, source: &str, found: &mut Vec<Import>) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "source_file" | "import_declaration" | "import_spec_list" => stack.extend(children(node).into_iter().rev()),
            "import_spec" => {
                let Some(path) = node.child_by_field_name("path") else { continue };
                let path = string_content(path, source);
                let binding = match node.child_by_field_name("name") {
                    Some(name) => text(name, source).to_string(),
                    None => go_package(&path),
                };
                push(found, &binding, Imported::Module, &path, false);
            }
            _ => {}
        }
    }
}

pub fn go_package(path: &str) -> String {
    let mut parts = path.rsplit('/');
    let last = parts.next().unwrap_or(path);
    let versioned = last.len() > 1 && last.starts_with('v') && last[1..].chars().all(|c| c.is_ascii_digit());
    let name = if versioned { parts.next().unwrap_or(last) } else { last };
    name.trim_start_matches("go-").replace(['-', '.'], "_")
}

fn java(root: Node, source: &str, found: &mut Vec<Import>) {
    for node in children(root) {
        if node.kind() != "import_declaration" {
            continue;
        }
        let statik = text(node, source).contains("static ");
        let Some(path) = children(node).into_iter().find(|child| child.kind() == "scoped_identifier") else { continue };
        let path = text(path, source);
        let Some((parent, last)) = path.rsplit_once('.') else { continue };
        if statik {
            push(found, last, Imported::Name(last.to_string()), parent, false);
        } else {
            push(found, last, Imported::Module, path, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(language: &str, source: &str) -> Vec<Import> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&crate::highlight::grammar(language).unwrap()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        imports(language, tree.root_node(), source)
    }

    fn import(binding: &str, imported: Imported, source: &str, export: bool) -> Import {
        Import { binding: binding.into(), imported, source: source.into(), export }
    }

    #[test]
    fn typescript_imports() {
        let found = parse(
            "TypeScript",
            "import D, { A, B as C } from './m';\nimport * as ns from \"pkg\";\nimport type { T } from '@/t';\n\
             export { X as Y } from './x';\nexport * from './all';\nconst { r, s: t } = require('./req');\n\
             const lib = require('lib');\nimport fs = require('fs');\n",
        );
        assert_eq!(
            found,
            vec![
                import("D", Imported::Default, "./m", false),
                import("A", Imported::Name("A".into()), "./m", false),
                import("C", Imported::Name("B".into()), "./m", false),
                import("ns", Imported::Module, "pkg", false),
                import("T", Imported::Name("T".into()), "@/t", false),
                import("Y", Imported::Name("X".into()), "./x", true),
                import("*", Imported::Module, "./all", true),
                import("r", Imported::Name("r".into()), "./req", false),
                import("t", Imported::Name("s".into()), "./req", false),
                import("lib", Imported::Module, "lib", false),
                import("fs", Imported::Module, "fs", false),
            ]
        );
    }

    #[test]
    fn python_imports() {
        let found = parse("Python", "import os.path\nimport numpy as np\nfrom .models import User, Group as G\n");
        assert_eq!(
            found,
            vec![
                import("os", Imported::Module, "os", false),
                import("np", Imported::Module, "numpy", false),
                import("User", Imported::Name("User".into()), ".models", false),
                import("G", Imported::Name("Group".into()), ".models", false),
            ]
        );
    }

    #[test]
    fn rust_imports() {
        let found = parse(
            "Rust",
            "use crate::store::{self, FilePlace};\npub use super::a::B as C;\nuse std::path::{Path, PathBuf};\nuse x::*;\n",
        );
        assert_eq!(
            found,
            vec![
                import("store", Imported::Module, "crate::store", false),
                import("FilePlace", Imported::Module, "crate::store::FilePlace", false),
                import("C", Imported::Module, "super::a::B", true),
                import("Path", Imported::Module, "std::path::Path", false),
                import("PathBuf", Imported::Module, "std::path::PathBuf", false),
                import("*", Imported::Module, "x", false),
            ]
        );
    }

    #[test]
    fn go_and_java_imports() {
        let found =
            parse("Go", "package a\nimport (\n  \"fmt\"\n  y \"example.com/m/v2\"\n  \"example.com/go-yaml\"\n)\n");
        assert_eq!(
            found,
            vec![
                import("fmt", Imported::Module, "fmt", false),
                import("y", Imported::Module, "example.com/m/v2", false),
                import("yaml", Imported::Module, "example.com/go-yaml", false),
            ]
        );
        let found = parse("Java", "import com.a.Thing;\nimport static com.a.Util.helper;\nclass A {}\n");
        assert_eq!(
            found,
            vec![
                import("Thing", Imported::Module, "com.a.Thing", false),
                import("helper", Imported::Name("helper".into()), "com.a.Util", false),
            ]
        );
    }
}
