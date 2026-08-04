//! PowerShell parser plugin — full-parse mode.
//!
//! Handles `.ps1`, `.psm1`, `.psd1` files.
//! The plugin parses source with tree-sitter-powershell inside Rust/Wasm.

use intentdiff_plugin_sdk::{
    cst::CstNode,
    hash::structural_hash_with_memo,
    tree::{SemanticNode, SemanticNodeBuilder},
};

wit_bindgen::generate!({
    path: "wit/plugin.wit",
    world: "parser-plugin",
});

use crate::exports::intentdiff::plugin::parser::ExamplePair;
use crate::exports::intentdiff::plugin::parser::Guest;
use crate::exports::intentdiff::plugin::parser::LanguageInfoRecord;
use crate::exports::intentdiff::plugin::parser::ParserMode;

const PLUGIN_METADATA: &str = include_str!("../plugin_metadata.info");

fn language_info_for(ids: Vec<String>) -> Vec<LanguageInfoRecord> {
    let metadata = intentdiff_plugin_sdk::metadata::parse_plugin_metadata(PLUGIN_METADATA);
    ids.into_iter()
        .map(|language_id| {
            let info = metadata.language_or_default(&language_id);
            LanguageInfoRecord {
                language_id: info.language_id,
                language_name: info.language_name,
                language_short_name: info.language_short_name,
                monaco_language: info.monaco_language,
                default_filename: info.default_filename,
                language_file_extensions: info.language_file_extensions,
                author: metadata.author().to_string(),
                plugin_version: metadata.plugin_version().to_string(),
                last_updated: metadata.last_updated().to_string(),
            }
        })
        .collect()
}
struct PowerShellParser;

const TRIVIA: &[&str] = &["comment", "whitespace"];

const SEMANTIC_TYPES: &[&str] = &[
    // Root
    "program",
    // Definitions
    "function_statement",
    "class_statement",
    "enum_statement",
    "method_statement",
    // Parameters
    "param_block",
    "parameter",
    // Statements
    "assignment_statement",
    "if_statement",
    "foreach_statement",
    "for_statement",
    "while_statement",
    "do_while_statement",
    "do_until_statement",
    "switch_statement",
    "try_statement",
    "catch_clause",
    "finally_clause",
    "trap_statement",
    "return_statement",
    "throw_statement",
    "break_statement",
    "continue_statement",
    "pipeline_statement",
    "command",
    // Expressions
    "variable",
    "string_literal",
    "number_literal",
    "boolean_literal",
    "array_literal_expression",
    "hash_literal_expression",
    "invocation_expression",
    "member_access",
    "element_access",
    "type_literal",
    // Attributes & data
    "attribute",
    "script_block",
];

fn is_semantic(node_type: &str) -> bool {
    SEMANTIC_TYPES.contains(&node_type)
}

fn label_for(node: &CstNode) -> String {
    if node.is_leaf() {
        return node.text_or_empty().to_string();
    }
    // Literal containers label with their captured source text (SDK-shared, issue #47).
    if let Some(label) = intentdiff_plugin_sdk::ts_convert::literal_label(node) {
        return label;
    }
    match node.node_type.as_str() {
        "function_statement" | "method_statement" => {
            for child in &node.children {
                if child.node_type == "function_name"
                    || child.node_type == "method_name"
                    || child.node_type == "identifier"
                {
                    return child.text_or_empty().to_string();
                }
            }
        }
        "class_statement" | "enum_statement" => {
            for child in &node.children {
                if child.node_type == "class_name"
                    || child.node_type == "enum_name"
                    || child.node_type == "identifier"
                    || child.node_type == "type_identifier"
                {
                    return child.text_or_empty().to_string();
                }
            }
        }
        "assignment_statement" => {
            for child in &node.children {
                if child.node_type == "variable" {
                    return child.text_or_empty().to_string();
                }
            }
        }
        "command" => {
            // First word/identifier is the command name
            for child in &node.children {
                if child.node_type == "command_name"
                    || child.node_type == "identifier"
                    || child.node_type == "word"
                {
                    return child.text_or_empty().to_string();
                }
            }
        }
        _ => {}
    }
    for child in &node.children {
        if child.node_type == "identifier" || child.node_type == "word" {
            return child.text_or_empty().to_string();
        }
    }
    node.node_type.clone()
}

fn is_class_like(node_type: &str) -> bool {
    matches!(node_type, "class_statement" | "enum_statement")
}

fn is_method_like(node_type: &str) -> bool {
    matches!(node_type, "function_statement" | "method_statement")
}

fn convert(
    node: &CstNode,
    id_prefix: &str,
    parent_class: Option<&str>,
    memo: &mut std::collections::HashMap<usize, String>,
) -> Option<SemanticNode> {
    convert_semantic_classed(
        node,
        id_prefix,
        parent_class,
        memo,
        &|t| TRIVIA.contains(&t),
        &is_semantic,
        &is_class_like,
        &is_method_like,
        &label_for,
    )
}



use intentdiff_plugin_sdk::ts_convert::{convert_semantic_classed, node_to_cst};

fn parse_source(source: &str) -> Result<CstNode, String> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter_powershell::LANGUAGE.into();
    parser
        .set_language(&lang)
        .map_err(|_| "Failed to load PowerShell grammar".to_string())?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "Parse failed".to_string())?;
    Ok(node_to_cst(tree.root_node(), source.as_bytes()))
}

fn process_impl(source: &str) -> String {
    let root: CstNode = match parse_source(source) {
        Ok(n) => n,
        Err(e) => return format!(r#"{{"error":"{}"}}"#, e),
    };
    let mut memo: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    let sem = match convert(&root, "0", None, &mut memo) {
        Some(n) => n,
        None => return r#"{"error":"Empty semantic tree"}"#.to_string(),
    };
    match serde_json::to_string(&sem) {
        Ok(s) => s,
        Err(e) => format!(r#"{{"error":"Serialisation error: {}"}}"#, e),
    }
}

impl Guest for PowerShellParser {
    fn get_parser_mode() -> ParserMode {
        ParserMode::FullParse
    }
    fn grammar_id() -> String {
        "powershell".to_string()
    }
    fn detect_language(filename: String, _content: String) -> String {
        let lower = filename.to_lowercase();
        if lower.ends_with(".ps1") || lower.ends_with(".psm1") || lower.ends_with(".psd1") {
            return "powershell".to_string();
        }
        String::new()
    }
    fn preprocess_source(source: String) -> String {
        source
    }
    fn example(_language: String) -> ExamplePair {
        ExamplePair {
            old: "function Greet($Name) {\n    Write-Host \"Hello, $Name\"\n}\n\nfunction Add-Numbers($A, $B) {\n    return $A + $B\n}\n".to_string(),
            new: "function Greet {\n    param(\n        [string]$Name = 'World'\n    )\n    Write-Host \"Hello, $Name!\"\n}\n\nfunction Add-Numbers {\n    param(\n        [int]$A,\n        [int]$B\n    )\n    return $A + $B\n}\n\nfunction Multiply-Numbers {\n    param(\n        [int]$A,\n        [int]$B\n    )\n    return $A * $B\n}\n".to_string(),
        }
    }
    fn process(input: String, _language: String, _filename: String) -> String {
        process_impl(&input)
    }
    fn trivia_node_types() -> Vec<String> {
        TRIVIA.iter().map(|s| s.to_string()).collect()
    }
    fn language_ids() -> Vec<String> {
        vec!["powershell".to_string()]
    }
    fn language_info() -> Vec<LanguageInfoRecord> {
        language_info_for(Self::language_ids())
    }
    fn priority() -> i32 {
        0
    }
}

export!(PowerShellParser);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::intentdiff::plugin::parser::Guest;
    use intentdiff_plugin_sdk::testing as t;

    #[test]
    fn grammar_id_nonempty() {
        assert!(!PowerShellParser::grammar_id().is_empty());
    }

    #[test]
    fn language_ids_contain_grammar_id() {
        let gid = PowerShellParser::grammar_id();
        let ids = PowerShellParser::language_ids();
        assert!(
            ids.contains(&gid),
            "language_ids {:?} must contain {:?}",
            ids,
            gid
        );
    }

    #[test]
    fn detect_language_known_ext() {
        let r = PowerShellParser::detect_language("test.ps1".to_string(), "".to_string());
        assert_eq!(r.as_str(), "powershell");
    }

    #[test]
    fn detect_language_unknown_ext() {
        let r = PowerShellParser::detect_language(
            "test.xyz_notareal_ext_9z8y".to_string(),
            "".to_string(),
        );
        assert_eq!(r.as_str(), "");
    }

    #[test]
    fn parser_mode_is_full_parse() {
        assert!(matches!(
            PowerShellParser::get_parser_mode(),
            ParserMode::FullParse
        ));
    }

    #[test]
    fn process_impl_empty_returns_valid_json() {
        let out = process_impl("");
        t::assert_valid_json(&out, "process(empty)");
    }

    #[test]
    fn process_impl_whitespace_returns_valid_json() {
        let out = process_impl("   \n  ");
        t::assert_valid_json(&out, "process(whitespace)");
    }

    #[test]
    fn process_impl_parses_raw_powershell_source() {
        let out = process_impl(
            "function Greet {\n    param([string]$Name)\n    Write-Host \"Hello, $Name\"\n}\n\nfunction Add-Numbers($A, $B) {\n    return $A + $B\n}\n",
        );
        t::assert_valid_json(&out, "process(raw powershell)");
        let root: SemanticNode = serde_json::from_str(&out).unwrap();
        let labels = labels_for_type(&root, "function_statement");
        assert!(labels.contains(&"Greet".to_string()), "{labels:?}");
        assert!(labels.contains(&"Add-Numbers".to_string()), "{labels:?}");
        assert!(
            labels_for_type(&root, "command").contains(&"Write-Host".to_string()),
            "{root:?}"
        );
    }

    #[test]
    fn process_impl_handles_nested_script_blocks_without_recursion_failure() {
        let mut source = String::new();
        source.push_str("function Invoke-Outer {\n");
        source.push_str("    $items = 1..40\n");
        source.push_str("    $items | ForEach-Object {\n");
        for index in 0..40 {
            source.push_str(&format!(
                "        if ($_ -eq {index}) {{ Write-Host \"item {index}\" }}\n"
            ));
        }
        source.push_str("    }\n}\n");

        let out = process_impl(&source);
        t::assert_valid_json(&out, "process(nested powershell)");
        assert!(
            !out.contains("recursion") && !out.contains("fuel"),
            "unexpected recursion/fuel marker: {out}"
        );
        let root: SemanticNode = serde_json::from_str(&out).unwrap();
        assert!(
            labels_for_type(&root, "function_statement").contains(&"Invoke-Outer".to_string()),
            "{root:?}"
        );
    }

    fn labels_for_type(node: &SemanticNode, node_type: &str) -> Vec<String> {
        let mut labels = Vec::new();
        push_labels_for_type(node, node_type, &mut labels);
        labels
    }

    fn push_labels_for_type(node: &SemanticNode, node_type: &str, labels: &mut Vec<String>) {
        if node.node_type == node_type {
            labels.push(node.label.clone());
        }
        for child in &node.children {
            push_labels_for_type(child, node_type, labels);
        }
    }
}
