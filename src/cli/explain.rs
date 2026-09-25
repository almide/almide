//! `almide explain --list [--json]` (#2149): every diagnostic code as one row
//! — `code | mnemonic | severity | since | verdict` — so an agent can orient
//! without knowing a code first.
//!
//! `mnemonic` and `verdict` are read from the embedded `docs/diagnostics/<CODE>.md`
//! (its title and its `## Fix-it verdict`); `severity` and `since` from
//! `docs/diagnostics/codes.toml`, embedded here. Rows are the documented codes;
//! `tests/explain_list_test.rs` keeps that set equal to the set the compiler
//! emits and checks every `severity` against the binary.

use crate::out;

const CODES_TOML: &str = include_str!("../../docs/diagnostics/codes.toml");

/// One listing row. Printed through `serde_json::json!`, not a derive: it is
/// rendered once per invocation and never read back.
pub struct CodeRow {
    pub code: String,
    pub mnemonic: String,
    pub severity: String,
    pub since: String,
    pub verdict: String,
}

/// The title after `# EXXX — `, minus a trailing `(warning)` (severity has its
/// own column).
fn mnemonic(doc: &str) -> String {
    let first = doc.lines().next().unwrap_or("");
    let title = first.split_once(" — ").map_or(first, |(_, t)| t).trim();
    title.strip_suffix("(warning)").unwrap_or(title).trim().to_string()
}

/// The first bold word of the `## Fix-it verdict` section: `mechanical`,
/// `conditional` or `not-fixable`; `unstated` when the section is missing.
fn verdict(doc: &str) -> String {
    doc.split("## Fix-it verdict")
        .nth(1)
        .and_then(|v| v.split("**").nth(1))
        .map_or_else(|| "unstated".to_string(), |w| w.trim().to_string())
}

pub fn rows(docs: &[(&str, &str)]) -> Vec<CodeRow> {
    let registry: toml::Table = CODES_TOML.parse().expect("docs/diagnostics/codes.toml parses");
    docs.iter()
        .map(|(code, doc)| {
            let entry = registry.get(*code).and_then(|v| v.as_table());
            let field = |k: &str| {
                entry
                    .and_then(|t| t.get(k))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string()
            };
            CodeRow {
                code: code.to_string(),
                mnemonic: mnemonic(doc),
                severity: field("severity"),
                since: field("since"),
                verdict: verdict(doc),
            }
        })
        .collect()
}

pub fn print_list(docs: &[(&str, &str)], json: bool) {
    let rows = rows(docs);
    if json {
        let arr: Vec<serde_json::Value> = rows.iter().map(|r| serde_json::json!({
            "code": r.code, "mnemonic": r.mnemonic, "severity": r.severity,
            "since": r.since, "verdict": r.verdict,
        })).collect();
        out(&serde_json::Value::Array(arr).to_string());
        return;
    }
    for r in &rows {
        out(&format!("{:<5} {:<13} {:<8} {:<12} {}", r.code, r.severity, r.since, r.verdict, r.mnemonic));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_verdict_are_read_from_the_doc() {
        let doc = "# E060 — import hygiene (warning)\n\ntext\n\n## Fix-it verdict\n\n**mechanical** — deleting.\n";
        assert_eq!(mnemonic(doc), "import hygiene");
        assert_eq!(verdict(doc), "mechanical");
        assert_eq!(verdict("# E1 — x\n"), "unstated");
    }

    #[test]
    fn every_registry_row_parses() {
        let registry: toml::Table = CODES_TOML.parse().expect("codes.toml parses");
        assert!(registry.len() >= 80);
    }
}
