use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tree_sitter::Query;

const TYPESCRIPT: &str = r#"
(function_declaration name: (identifier) @name) @definition.function
(generator_function_declaration name: (identifier) @name) @definition.function
(function_signature name: (identifier) @name) @definition.function
(class_declaration name: (type_identifier) @name) @definition.class
(abstract_class_declaration name: (type_identifier) @name) @definition.class
(interface_declaration name: (type_identifier) @name) @definition.interface
(type_alias_declaration name: (type_identifier) @name) @definition.type
(enum_declaration name: (identifier) @name) @definition.enum
(internal_module name: (identifier) @name) @definition.module
(module name: (identifier) @name) @definition.module
(method_definition name: (property_identifier) @name) @definition.method
(method_signature name: (property_identifier) @name) @definition.method
(abstract_method_signature name: (property_identifier) @name) @definition.method
(public_field_definition name: (property_identifier) @name) @definition.field
(property_signature name: (property_identifier) @name) @definition.field
(enum_body name: (property_identifier) @name @definition.variant)
(enum_assignment name: (property_identifier) @name) @definition.variant
(variable_declarator name: (identifier) @name) @definition.variable
(variable_declarator name: (object_pattern (shorthand_property_identifier_pattern) @name @definition.variable))
(variable_declarator name: (object_pattern (pair_pattern value: (identifier) @name @definition.variable)))
(variable_declarator name: (array_pattern (identifier) @name @definition.variable))
(required_parameter pattern: (identifier) @name) @definition.parameter
(optional_parameter pattern: (identifier) @name) @definition.parameter
(arrow_function parameter: (identifier) @name @definition.parameter)
(catch_clause parameter: (identifier) @name @definition.parameter)
(for_in_statement left: (identifier) @name @definition.variable)
(pair key: (property_identifier) @name value: [(arrow_function) (function_expression)]) @definition.method
(assignment_expression
  left: (member_expression property: (property_identifier) @name)
  right: [(arrow_function) (function_expression)]) @definition.function
"#;

const JAVASCRIPT: &str = r#"
(function_declaration name: (identifier) @name) @definition.function
(generator_function_declaration name: (identifier) @name) @definition.function
(class_declaration name: (identifier) @name) @definition.class
(method_definition name: (property_identifier) @name) @definition.method
(field_definition property: (property_identifier) @name) @definition.field
(variable_declarator name: (identifier) @name) @definition.variable
(variable_declarator name: (object_pattern (shorthand_property_identifier_pattern) @name @definition.variable))
(variable_declarator name: (object_pattern (pair_pattern value: (identifier) @name @definition.variable)))
(variable_declarator name: (array_pattern (identifier) @name @definition.variable))
(formal_parameters (identifier) @name @definition.parameter)
(formal_parameters (assignment_pattern left: (identifier) @name @definition.parameter))
(arrow_function parameter: (identifier) @name @definition.parameter)
(catch_clause parameter: (identifier) @name @definition.parameter)
(for_in_statement left: (identifier) @name @definition.variable)
(pair key: (property_identifier) @name value: [(arrow_function) (function_expression)]) @definition.method
(assignment_expression
  left: (member_expression property: (property_identifier) @name)
  right: [(arrow_function) (function_expression)]) @definition.function
"#;

const RUST: &str = r#"
(function_item name: (identifier) @name) @definition.function
(function_signature_item name: (identifier) @name) @definition.function
(struct_item name: (type_identifier) @name) @definition.class
(union_item name: (type_identifier) @name) @definition.class
(enum_item name: (type_identifier) @name) @definition.enum
(type_item name: (type_identifier) @name) @definition.type
(associated_type name: (type_identifier) @name) @definition.type
(trait_item name: (type_identifier) @name) @definition.interface
(impl_item type: (type_identifier) @name) @definition.implementation
(impl_item type: (generic_type type: (type_identifier) @name)) @definition.implementation
(mod_item name: (identifier) @name) @definition.module
(macro_definition name: (identifier) @name) @definition.macro
(const_item name: (identifier) @name) @definition.constant
(static_item name: (identifier) @name) @definition.constant
(enum_variant name: (identifier) @name) @definition.variant
(field_declaration name: (field_identifier) @name) @definition.field
(let_declaration pattern: (identifier) @name) @definition.variable
(let_declaration pattern: (mut_pattern (identifier) @name)) @definition.variable
(let_declaration pattern: (tuple_pattern (identifier) @name @definition.variable))
(parameter pattern: (identifier) @name) @definition.parameter
(parameter pattern: (mut_pattern (identifier) @name)) @definition.parameter
(closure_parameters (identifier) @name @definition.parameter)
(for_expression pattern: (identifier) @name @definition.variable)
"#;

const PYTHON: &str = r#"
(class_definition name: (identifier) @name) @definition.class
(function_definition name: (identifier) @name) @definition.function
(assignment left: (identifier) @name) @definition.variable
(assignment left: (pattern_list (identifier) @name @definition.variable))
(assignment
  left: (attribute object: (identifier) @self attribute: (identifier) @name)
  (#eq? @self "self")) @definition.field
(parameters (identifier) @name @definition.parameter)
(default_parameter name: (identifier) @name) @definition.parameter
(typed_parameter (identifier) @name) @definition.parameter
(typed_default_parameter name: (identifier) @name) @definition.parameter
(for_statement left: (identifier) @name @definition.variable)
"#;

const GO: &str = r#"
(function_declaration name: (identifier) @name) @definition.function
(method_declaration name: (field_identifier) @name) @definition.method
(type_spec name: (type_identifier) @name) @definition.type
(type_alias name: (type_identifier) @name) @definition.type
(const_spec name: (identifier) @name) @definition.constant
(var_spec name: (identifier) @name) @definition.variable
(field_declaration name: (field_identifier) @name) @definition.field
(method_elem name: (field_identifier) @name) @definition.method
(short_var_declaration left: (expression_list (identifier) @name @definition.variable))
(parameter_declaration name: (identifier) @name) @definition.parameter
(range_clause left: (expression_list (identifier) @name @definition.variable))
"#;

const SHELL: &str = r#"
(function_definition name: (word) @name) @definition.function
(variable_assignment name: (variable_name) @name) @definition.variable
"#;

fn source(language: &str) -> Option<String> {
    Some(match language {
        "TypeScript" | "TSX" => TYPESCRIPT.into(),
        "JavaScript" => JAVASCRIPT.into(),
        "Rust" => RUST.into(),
        "Python" => PYTHON.into(),
        "Go" => GO.into(),
        "Shell" => SHELL.into(),
        "C" => tree_sitter_c::TAGS_QUERY.into(),
        "C++" => tree_sitter_cpp::TAGS_QUERY.into(),
        "Java" => tree_sitter_java::TAGS_QUERY.into(),
        "Ruby" => tree_sitter_ruby::TAGS_QUERY.into(),
        "Lua" => tree_sitter_lua::TAGS_QUERY.into(),
        "Swift" => tree_sitter_swift::TAGS_QUERY.into(),
        "C#" => tree_sitter_c_sharp::TAGS_QUERY.into(),
        "PHP" => tree_sitter_php::TAGS_QUERY.into(),
        "Dart" => tree_sitter_dart::TAGS_QUERY.into(),
        "Elixir" => tree_sitter_elixir::TAGS_QUERY.into(),
        "Protocol Buffers" => tree_sitter_proto::TAGS_QUERY.into(),
        "Solidity" => tree_sitter_solidity::TAGS_QUERY.into(),
        _ => return None,
    })
}

pub fn definitions(language: &str) -> Option<&'static Query> {
    static QUERIES: OnceLock<Mutex<HashMap<String, Option<&'static Query>>>> = OnceLock::new();
    let mut queries = QUERIES.get_or_init(Default::default).lock().ok()?;
    *queries.entry(language.to_string()).or_insert_with(|| {
        let query = Query::new(&crate::highlight::grammar(language)?, &source(language)?).ok()?;
        Some(Box::leak(Box::new(query)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_query_builds() {
        for language in [
            "TypeScript",
            "TSX",
            "JavaScript",
            "Rust",
            "Python",
            "Go",
            "Shell",
            "C",
            "C++",
            "Java",
            "Ruby",
            "Lua",
            "Swift",
            "C#",
            "PHP",
            "Dart",
            "Elixir",
            "Protocol Buffers",
            "Solidity",
        ] {
            let grammar = crate::highlight::grammar(language).unwrap();
            if let Err(error) = Query::new(&grammar, &source(language).unwrap()) {
                panic!("{language}: {error}");
            }
        }
    }
}
