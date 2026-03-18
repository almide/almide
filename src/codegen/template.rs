//! TOML-driven template renderer (Layer 3).
//!
//! Templates define ONLY syntax — no semantic logic.
//! Semantic decisions are made by Nanopass passes (Layer 2)
//! and recorded as TargetAttrs on IR nodes.
//!
//! Template format:
//! ```toml
//! [if_expr]
//! template = "if {cond} {{ {then} }} else {{ {else} }}"
//!
//! [some_expr]
//! template = "Some({inner})"
//!
//! [some_expr]
//! when_attr = "option_erased"
//! template = "{inner}"
//! ```
//!
//! Templates can have conditional variants selected by:
//! - `when_type`: type of the expression (Int, Float, String, List, etc.)
//! - `when_attr`: TargetAttrs flag (option_erased, needs_try, etc.)
//! - Default: the unguarded template

use std::collections::HashMap;

/// A single template rule with optional guard conditions.
#[derive(Debug, Clone)]
pub struct TemplateRule {
    /// The template string with `{placeholder}` holes
    pub template: String,
    /// Only apply when this type matches (e.g., "Int", "Float", "String")
    pub when_type: Option<String>,
    /// Only apply when this TargetAttrs flag is set
    pub when_attr: Option<String>,
}

/// All template rules for a single construct (e.g., `if_expr`, `some_expr`).
/// Rules are checked in order; first matching rule wins.
#[derive(Debug, Clone)]
pub struct TemplateEntry {
    pub rules: Vec<TemplateRule>,
}

impl TemplateEntry {
    /// Select the first matching rule given type and attrs.
    pub fn select(&self, ty: Option<&str>, attrs: &[&str]) -> Option<&TemplateRule> {
        for rule in &self.rules {
            // Check when_type guard
            if let Some(ref guard_ty) = rule.when_type {
                if ty != Some(guard_ty.as_str()) {
                    continue;
                }
            }
            // Check when_attr guard
            if let Some(ref guard_attr) = rule.when_attr {
                if !attrs.contains(&guard_attr.as_str()) {
                    continue;
                }
            }
            return Some(rule);
        }
        None
    }
}

/// The full template set for a target language.
#[derive(Debug, Clone)]
pub struct TemplateSet {
    pub target_name: String,
    pub entries: HashMap<String, TemplateEntry>,
}

impl TemplateSet {
    pub fn new(target_name: &str) -> Self {
        Self {
            target_name: target_name.to_string(),
            entries: HashMap::new(),
        }
    }

    /// Get the template entry for a construct (e.g., "if_expr").
    pub fn get(&self, construct: &str) -> Option<&TemplateEntry> {
        self.entries.get(construct)
    }

    /// Render a construct by selecting the right template and filling placeholders.
    pub fn render(
        &self,
        construct: &str,
        ty: Option<&str>,
        attrs: &[&str],
        bindings: &HashMap<&str, String>,
    ) -> Option<String> {
        let entry = self.get(construct)?;
        let rule = entry.select(ty, attrs)?;
        // Convert HashMap to slice for fill_template
        let pairs: Vec<(&str, &str)> = bindings.iter().map(|(&k, v)| (k, v.as_str())).collect();
        Some(fill_template(&rule.template, &pairs))
    }

    /// Render with slice bindings (avoids HashMap allocation)
    pub fn render_with(
        &self,
        construct: &str,
        ty: Option<&str>,
        attrs: &[&str],
        bindings: &[(&str, &str)],
    ) -> Option<String> {
        let entry = self.get(construct)?;
        let rule = entry.select(ty, attrs)?;
        Some(fill_template(&rule.template, bindings))
    }
}

/// Fill `{placeholder}` holes in a template string.
/// `{{` and `}}` are escape sequences for literal `{` and `}`.
fn fill_template(template: &str, bindings: &[(&str, &str)]) -> String {
    let mut result = String::with_capacity(template.len() * 2);
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            // Check for `{{` escape → literal `{`
            if chars.peek() == Some(&'{') {
                chars.next();
                result.push('{');
            } else {
                // Collect placeholder name until '}'
                let mut name = String::new();
                for inner in chars.by_ref() {
                    if inner == '}' {
                        break;
                    }
                    name.push(inner);
                }
                // Look up binding; if not found, keep placeholder as-is
                if let Some(value) = bindings.iter().find(|(k, _)| *k == name.as_str()).map(|(_, v)| *v) {
                    result.push_str(value);
                } else {
                    result.push('{');
                    result.push_str(&name);
                    result.push('}');
                }
            }
        } else if ch == '}' {
            // Check for `}}` escape → literal `}`
            if chars.peek() == Some(&'}') {
                chars.next();
            }
            result.push('}');
        } else {
            result.push(ch);
        }
    }

    result
}

// ── TOML Loading ──

/// Load templates from a TOML string.
/// Handles both single `[construct]` and array `[[construct]]` forms.
pub fn load_from_toml(target_name: &str, toml_str: &str) -> TemplateSet {
    let mut ts = TemplateSet::new(target_name);
    let table: toml::Table = toml_str.parse().expect("invalid template TOML");

    for (key, value) in &table {
        let mut rules = Vec::new();

        match value {
            // Single rule: [construct_name]
            toml::Value::Table(t) => {
                if let Some(rule) = parse_rule(t) {
                    rules.push(rule);
                }
            }
            // Array of rules: [[construct_name]]
            toml::Value::Array(arr) => {
                for item in arr {
                    if let toml::Value::Table(t) = item {
                        if let Some(rule) = parse_rule(t) {
                            rules.push(rule);
                        }
                    }
                }
            }
            _ => {}
        }

        if !rules.is_empty() {
            // Merge with existing entry (allows [[construct]] to extend [construct])
            let entry = ts.entries.entry(key.clone()).or_insert_with(|| TemplateEntry {
                rules: Vec::new(),
            });
            // Guarded rules first, default rules last
            let mut guarded: Vec<TemplateRule> = Vec::new();
            let mut defaults: Vec<TemplateRule> = Vec::new();
            for r in rules {
                if r.when_type.is_some() || r.when_attr.is_some() {
                    guarded.push(r);
                } else {
                    defaults.push(r);
                }
            }
            // Prepend guarded rules (checked first), then append defaults
            let mut merged = guarded;
            merged.append(&mut defaults);
            merged.append(&mut entry.rules);
            entry.rules = merged;
        }
    }

    ts
}

fn parse_rule(t: &toml::Table) -> Option<TemplateRule> {
    let template = t.get("template")?.as_str()?.to_string();
    let when_type = t.get("when_type").and_then(|v| v.as_str()).map(String::from);
    let when_attr = t.get("when_attr").and_then(|v| v.as_str()).map(String::from);
    Some(TemplateRule {
        template,
        when_type,
        when_attr,
    })
}

/// Load Rust templates from embedded TOML
pub fn rust_templates() -> TemplateSet {
    load_from_toml("rust", include_str!("../../codegen/templates/rust.toml"))
}

/// Load TypeScript templates from embedded TOML
pub fn typescript_templates() -> TemplateSet {
    load_from_toml("typescript", include_str!("../../codegen/templates/typescript.toml"))
}

/// Load built-in Rust templates (inline fallback — kept for reference)
pub fn rust_templates_inline() -> TemplateSet {
    let mut ts = TemplateSet::new("rust");

    // if/else
    ts.entries.insert("if_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "if {cond} {{ {then} }} else {{ {else} }}".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // let binding
    ts.entries.insert("let_binding".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "let {name}: {type} = {value};".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // var binding
    ts.entries.insert("var_binding".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "let mut {name}: {type} = {value};".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // some(x)
    ts.entries.insert("some_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "Some({inner})".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // none
    ts.entries.insert("none_expr".into(), TemplateEntry {
        rules: vec![
            // With type hint (when inference needs help)
            TemplateRule {
                template: "None::<{type_hint}>".into(),
                when_type: None,
                when_attr: Some("none_type_hint".into()),
            },
            // Default
            TemplateRule {
                template: "None".into(),
                when_type: None,
                when_attr: None,
            },
        ],
    });

    // ok(x)
    ts.entries.insert("ok_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "Ok({inner})".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // err(x)
    ts.entries.insert("err_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "Err({inner}.to_string())".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // binary op: concat
    ts.entries.insert("concat_expr".into(), TemplateEntry {
        rules: vec![
            TemplateRule {
                template: "format!(\"{{}}{{}}\", {left}, {right})".into(),
                when_type: Some("String".into()),
                when_attr: None,
            },
            TemplateRule {
                template: "AlmideConcat::concat({left}, {right})".into(),
                when_type: Some("List".into()),
                when_attr: None,
            },
        ],
    });

    // function call with try
    ts.entries.insert("call_expr".into(), TemplateEntry {
        rules: vec![
            TemplateRule {
                template: "{callee}({args})?".into(),
                when_type: None,
                when_attr: Some("needs_try".into()),
            },
            TemplateRule {
                template: "{callee}({args})".into(),
                when_type: None,
                when_attr: None,
            },
        ],
    });

    ts
}

/// Load built-in TypeScript templates (inline fallback — kept for reference)
pub fn typescript_templates_inline() -> TemplateSet {
    let mut ts = TemplateSet::new("typescript");

    // if/else
    ts.entries.insert("if_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "if ({cond}) {{ {then} }} else {{ {else} }}".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // let binding
    ts.entries.insert("let_binding".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "const {name}: {type} = {value};".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // var binding
    ts.entries.insert("var_binding".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "let {name}: {type} = {value};".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // some(x) — erased in TS
    ts.entries.insert("some_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "{inner}".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // none — null in TS
    ts.entries.insert("none_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "null".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // ok(x)
    ts.entries.insert("ok_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "{{ ok: true, value: {inner} }}".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // err(x)
    ts.entries.insert("err_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "{{ ok: false, error: {inner} }}".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    // concat
    ts.entries.insert("concat_expr".into(), TemplateEntry {
        rules: vec![
            TemplateRule {
                template: "{left} + {right}".into(),
                when_type: Some("String".into()),
                when_attr: None,
            },
            TemplateRule {
                template: "[...{left}, ...{right}]".into(),
                when_type: Some("List".into()),
                when_attr: None,
            },
        ],
    });

    // function call (no try in TS)
    ts.entries.insert("call_expr".into(), TemplateEntry {
        rules: vec![TemplateRule {
            template: "{callee}({args})".into(),
            when_type: None,
            when_attr: None,
        }],
    });

    ts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fill_template() {
        let mut bindings = HashMap::new();
        bindings.insert("name", "x".to_string());
        bindings.insert("value", "42".to_string());
        bindings.insert("type", "i64".to_string());

        let result = fill_template("let {name}: {type} = {value};", &bindings);
        assert_eq!(result, "let x: i64 = 42;");
    }

    #[test]
    fn test_rust_some() {
        let ts = rust_templates();
        let mut bindings = HashMap::new();
        bindings.insert("inner", "x".to_string());

        let result = ts.render("some_expr", None, &[], &bindings);
        assert_eq!(result, Some("Some(x)".to_string()));
    }

    #[test]
    fn test_ts_some_erased() {
        let ts = typescript_templates();
        let mut bindings = HashMap::new();
        bindings.insert("inner", "x".to_string());

        let result = ts.render("some_expr", None, &[], &bindings);
        assert_eq!(result, Some("x".to_string()));
    }

    #[test]
    fn test_concat_type_dispatch() {
        let ts = rust_templates();
        let mut bindings = HashMap::new();
        bindings.insert("left", "a".to_string());
        bindings.insert("right", "b".to_string());

        let str_result = ts.render("concat_expr", Some("String"), &[], &bindings);
        assert_eq!(str_result, Some("format!(\"{}{}\", a, b)".to_string()));

        let list_result = ts.render("concat_expr", Some("List"), &[], &bindings);
        assert_eq!(list_result, Some("AlmideConcat::concat(a, b)".to_string()));
    }

    #[test]
    fn test_call_with_try() {
        let ts = rust_templates();
        let mut bindings = HashMap::new();
        bindings.insert("callee", "fs_read".to_string());
        bindings.insert("args", "path".to_string());

        // Without try
        let normal = ts.render("call_expr", None, &[], &bindings);
        assert_eq!(normal, Some("fs_read(path)".to_string()));

        // With try
        let with_try = ts.render("call_expr", None, &["needs_try"], &bindings);
        assert_eq!(with_try, Some("fs_read(path)?".to_string()));
    }

    #[test]
    fn test_toml_loader_basic() {
        let toml = r#"
[if_expr]
template = "if ({cond}) {{ {then} }} else {{ {else} }}"

[some_expr]
template = "{inner}"
"#;
        let ts = load_from_toml("test", toml);
        let mut b = HashMap::new();
        b.insert("inner", "42".to_string());
        assert_eq!(ts.render("some_expr", None, &[], &b), Some("42".to_string()));
    }

    #[test]
    fn test_toml_loader_array_rules() {
        let toml = r#"
[[concat_expr]]
when_type = "String"
template = "{left} + {right}"

[[concat_expr]]
when_type = "List"
template = "[...{left}, ...{right}]"
"#;
        let ts = load_from_toml("test", toml);
        let mut b = HashMap::new();
        b.insert("left", "a".to_string());
        b.insert("right", "b".to_string());

        assert_eq!(ts.render("concat_expr", Some("String"), &[], &b), Some("a + b".to_string()));
        assert_eq!(ts.render("concat_expr", Some("List"), &[], &b), Some("[...a, ...b]".to_string()));
    }

    #[test]
    fn test_toml_loader_attr_guard() {
        let toml = r#"
[[call_expr]]
when_attr = "needs_try"
template = "{callee}({args})?"

[[call_expr]]
template = "{callee}({args})"
"#;
        let ts = load_from_toml("test", toml);
        let mut b = HashMap::new();
        b.insert("callee", "read".to_string());
        b.insert("args", "f".to_string());

        assert_eq!(ts.render("call_expr", None, &[], &b), Some("read(f)".to_string()));
        assert_eq!(ts.render("call_expr", None, &["needs_try"], &b), Some("read(f)?".to_string()));
    }

    #[test]
    fn test_rust_toml_loads() {
        // Verify the actual rust.toml loads without panicking
        let ts = rust_templates();
        assert!(ts.get("if_expr").is_some());
        assert!(ts.get("some_expr").is_some());
        assert!(ts.get("fn_decl").is_some());
        assert!(ts.get("type_int").is_some());
    }

    #[test]
    fn test_ts_toml_loads() {
        // Verify the actual typescript.toml loads without panicking
        let ts = typescript_templates();
        assert!(ts.get("if_expr").is_some());
        assert!(ts.get("some_expr").is_some());
        assert!(ts.get("fn_decl").is_some());
        assert!(ts.get("type_int").is_some());
    }
}
