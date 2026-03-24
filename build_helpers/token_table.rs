use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Token category in tokens.toml
#[derive(Deserialize)]
struct TokensDef {
    keywords: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    keyword_aliases: BTreeMap<String, String>,
    operators: BTreeMap<String, Vec<String>>,
    #[allow(dead_code)]
    delimiters: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    #[allow(dead_code)]
    special: BTreeMap<String, Vec<String>>,
}

#[derive(Deserialize)]
struct PrecedenceLevel {
    name: String,
    precedence: u32,
    operators: Vec<String>,
    associativity: String,
}

#[derive(Deserialize)]
struct PrecedenceDef {
    level: Vec<PrecedenceLevel>,
}

pub fn generate_token_table(out_dir: &Path) {
    let tokens_path = Path::new("grammar/tokens.toml");
    let prec_path = Path::new("grammar/precedence.toml");
    if !tokens_path.exists() {
        return;
    }

    let tokens_content = fs::read_to_string(tokens_path).unwrap();
    let tokens: TokensDef = toml::from_str(&tokens_content).unwrap();

    // Collect all keywords with their categories
    let mut keyword_entries: Vec<(String, String)> = Vec::new(); // (keyword, category)
    for (category, words) in &tokens.keywords {
        for word in words {
            keyword_entries.push((word.clone(), category.clone()));
        }
    }
    keyword_entries.sort();

    // Build keyword → TokenType name mapping
    fn keyword_to_token_type(kw: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = true;
        for ch in kw.chars() {
            if ch == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }
        result
    }

    // Generate keyword map entries
    let mut keyword_map_lines = String::new();
    for (kw, _cat) in &keyword_entries {
        let tt = keyword_to_token_type(kw);
        keyword_map_lines.push_str(&format!(
            "        m.insert(\"{kw}\", TokenType::{tt});\n"
        ));
    }
    // Add aliases
    for (alias, target) in &tokens.keyword_aliases {
        let tt = keyword_to_token_type(target);
        keyword_map_lines.push_str(&format!(
            "        m.insert(\"{alias}\", TokenType::{tt});\n"
        ));
    }

    // Generate keyword list for tree-sitter (grouped by category)
    let mut ts_keyword_lines = String::new();
    for (category, words) in &tokens.keywords {
        let words_str: Vec<String> = words.iter().map(|w| format!("\"{}\"", w)).collect();
        ts_keyword_lines.push_str(&format!(
            "    // {category}\n    {words},\n",
            words = words_str.join(", "),
        ));
    }
    // Add aliases
    let alias_strs: Vec<String> = tokens.keyword_aliases.keys().map(|a| format!("\"{}\"", a)).collect();
    if !alias_strs.is_empty() {
        ts_keyword_lines.push_str(&format!(
            "    // aliases\n    {},\n", alias_strs.join(", ")
        ));
    }

    // Generate TextMate keyword scopes
    let mut tm_keywords = String::new();
    for (category, words) in &tokens.keywords {
        let scope = match category.as_str() {
            "control" => "keyword.control.almide",
            "declaration" => "keyword.declaration.almide",
            "modifier" => "storage.modifier.almide",
            "value" => "constant.language.almide",
            "flow" => "keyword.control.flow.almide",
            _ => "keyword.other.almide",
        };
        let words_str = words.join("|");
        tm_keywords.push_str(&format!(
            "    // scope: {scope}\n    // pattern: \\\\b({words_str})\\\\b\n"
        ));
    }

    // All keywords as a flat list
    let all_keywords: Vec<&str> = keyword_entries.iter().map(|(k, _)| k.as_str()).collect();
    let all_kw_str: Vec<String> = all_keywords.iter().map(|k| format!("\"{}\"", k)).collect();

    // Collect all operators
    let mut all_operators: Vec<(String, String)> = Vec::new();
    for (category, ops) in &tokens.operators {
        for op in ops {
            all_operators.push((op.clone(), category.clone()));
        }
    }

    // Generate precedence table
    let mut prec_lines = String::new();
    if prec_path.exists() {
        let prec_content = fs::read_to_string(&prec_path).unwrap();
        let prec: PrecedenceDef = toml::from_str(&prec_content).unwrap();
        for level in &prec.level {
            let ops_str: Vec<String> = level.operators.iter().map(|o| format!("\"{}\"", o)).collect();
            prec_lines.push_str(&format!(
                "    // precedence {}: {} ({}) — {}\n",
                level.precedence, level.name, level.associativity,
                ops_str.join(", "),
            ));
        }
        println!("cargo:rerun-if-changed={}", prec_path.display());
    }

    // Write the generated token table
    let token_table = format!(
        r#"// AUTO-GENERATED by build.rs from almide-grammar — DO NOT EDIT
//
// This file provides:
//   - build_keyword_map_generated() for the lexer
//   - ALL_KEYWORDS list
//   - Keyword categories and precedence table as comments for reference

use std::collections::HashMap;
use crate::lexer::TokenType;

/// Build the keyword → TokenType map from grammar/tokens.toml
pub fn build_keyword_map_generated() -> HashMap<&'static str, TokenType> {{
    let mut m = HashMap::new();
{keyword_map_lines}    m
}}

/// All keywords as a flat list (for validation, tree-sitter, TextMate generation)
pub const ALL_KEYWORDS: &[&str] = &[{all_kw}];

/*
── Tree-sitter keyword list ──────────────────────────────────────────
{ts_keyword_lines}
── TextMate grammar scopes ───────────────────────────────────────────
{tm_keywords}
── Operator precedence table ─────────────────────────────────────────
{prec_lines}*/
"#,
        keyword_map_lines = keyword_map_lines,
        all_kw = all_kw_str.join(", "),
        ts_keyword_lines = ts_keyword_lines,
        tm_keywords = tm_keywords,
        prec_lines = prec_lines,
    );

    fs::write(out_dir.join("token_table.rs"), token_table).unwrap();

    // ── Generate tree-sitter keywords file ─────────────────────────────
    let mut ts_rules = String::new();
    ts_rules.push_str("// AUTO-GENERATED by build.rs from grammar/tokens.toml — DO NOT EDIT\n");
    ts_rules.push_str("// Copy these keyword rules into tree-sitter-almide/grammar.js\n\n");

    for (category, words) in &tokens.keywords {
        ts_rules.push_str(&format!("    // {category} keywords\n"));
        for word in words {
            ts_rules.push_str(&format!(
                "    {word}_keyword: $ => '{word}',\n"
            ));
        }
        ts_rules.push('\n');
    }
    // Alias keywords
    for (alias, target) in &tokens.keyword_aliases {
        ts_rules.push_str(&format!(
            "    // alias: {alias} → {target}\n"
        ));
    }

    ts_rules.push_str("\n    // keyword() list for tree-sitter word rule:\n");
    ts_rules.push_str("    // keyword: $ => choice(\n");
    for (kw, _) in &keyword_entries {
        ts_rules.push_str(&format!("    //   $.{kw}_keyword,\n"));
    }
    ts_rules.push_str("    // ),\n");

    fs::write(out_dir.join("tree_sitter_keywords.txt"), ts_rules).unwrap();

    // ── Generate TextMate grammar patterns ─────────────────────────────
    let mut tm_grammar = String::new();
    tm_grammar.push_str("// AUTO-GENERATED by build.rs from grammar/tokens.toml — DO NOT EDIT\n");
    tm_grammar.push_str("// Use these patterns in vscode-almide/syntaxes/almide.tmLanguage.json\n\n");

    for (category, words) in &tokens.keywords {
        let scope = match category.as_str() {
            "control" => "keyword.control.almide",
            "declaration" => "keyword.declaration.almide",
            "modifier" => "storage.modifier.almide",
            "value" => "constant.language.almide",
            "flow" => "keyword.control.flow.almide",
            _ => "keyword.other.almide",
        };
        let pattern = words.join("|");
        tm_grammar.push_str(&format!(
            r#"{{
  "name": "{scope}",
  "match": "\\b({pattern})\\b"
}},
"#
        ));
    }
    // Aliases (Ok, Err, Some, None → constant.language)
    if !tokens.keyword_aliases.is_empty() {
        let alias_pattern: Vec<&String> = tokens.keyword_aliases.keys().collect();
        let pattern = alias_pattern.iter().map(|a| a.as_str()).collect::<Vec<_>>().join("|");
        tm_grammar.push_str(&format!(
            r#"{{
  "name": "constant.language.almide",
  "match": "\\b({pattern})\\b"
}},
"#
        ));
    }

    // Operators
    let mut op_patterns: Vec<String> = Vec::new();
    for (_cat, ops) in &tokens.operators {
        for op in ops {
            // Escape regex special chars
            let escaped: String = op.chars().map(|c| {
                if "+-*/%^|.=!<>()[]{}?\\".contains(c) {
                    format!("\\{}", c)
                } else {
                    c.to_string()
                }
            }).collect();
            op_patterns.push(escaped);
        }
    }
    // Sort by length descending so longer operators match first
    op_patterns.sort_by(|a, b| b.len().cmp(&a.len()));
    let op_pattern = op_patterns.join("|");
    tm_grammar.push_str(&format!(
        r#"{{
  "name": "keyword.operator.almide",
  "match": "{op_pattern}"
}},
"#
    ));

    fs::write(out_dir.join("textmate_patterns.txt"), tm_grammar).unwrap();

    // ── Generate precedence reference ──────────────────────────────────
    if prec_path.exists() {
        let prec_content = fs::read_to_string(&prec_path).unwrap();
        let prec: PrecedenceDef = toml::from_str(&prec_content).unwrap();

        let mut prec_ref = String::new();
        prec_ref.push_str("// AUTO-GENERATED by build.rs from grammar/precedence.toml — DO NOT EDIT\n");
        prec_ref.push_str("// Tree-sitter precedence rules:\n\n");

        for level in &prec.level {
            let prec_fn = match level.associativity.as_str() {
                "left" => "prec.left",
                "right" => "prec.right",
                "none" => "prec",
                _ => "prec",
            };
            prec_ref.push_str(&format!(
                "// {name}: {fn}({prec}, ...)\n// operators: {ops}\n\n",
                name = level.name,
                fn = prec_fn,
                prec = level.precedence,
                ops = level.operators.join(", "),
            ));
        }

        fs::write(out_dir.join("tree_sitter_precedence.txt"), prec_ref).unwrap();
    }

    println!("cargo:rerun-if-changed={}", tokens_path.display());
}
