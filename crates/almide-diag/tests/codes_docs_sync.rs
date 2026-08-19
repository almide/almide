//! Bidirectional sync gate: the code registry (`codes::CODES`) and the prose
//! pages (`docs/diagnostics/EXXX.md`) must agree exactly — every code has a
//! page, every page has a row, and the row's title is byte-identical to the
//! page's `# EXXX — <title>` heading. Same doctrine as the incumbent's
//! contract ledger: no evidence may exist on one side only.

use std::collections::BTreeMap;
use std::path::PathBuf;

fn docs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/diagnostics")
}

fn doc_headings() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(docs_dir()).expect("docs/diagnostics must exist (ported at unit 0/1)") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.starts_with('E') || !name.ends_with(".md") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let first = text.lines().next().unwrap_or("");
        let rest = first
            .strip_prefix("# ")
            .unwrap_or_else(|| panic!("{name}: heading must start with '# '"));
        let (code, title) = rest
            .split_once(" — ")
            .unwrap_or_else(|| panic!("{name}: heading must be '# EXXX — <title>'"));
        assert_eq!(format!("{code}.md"), name, "{name}: heading code disagrees with filename");
        out.insert(code.to_string(), title.to_string());
    }
    out
}

#[test]
fn registry_and_docs_are_bidirectionally_in_sync() {
    let docs = doc_headings();
    let registry: BTreeMap<&str, &str> =
        almide_diag::codes::CODES.iter().map(|c| (c.code, c.title)).collect();

    for (code, title) in &docs {
        let row = registry
            .get(code.as_str())
            .unwrap_or_else(|| panic!("{code} has a doc page but no registry row"));
        assert_eq!(row, title, "{code}: registry title drifted from the doc heading");
    }
    for code in registry.keys() {
        assert!(docs.contains_key(*code), "{code} has a registry row but no doc page");
    }
    assert_eq!(docs.len(), registry.len());
}
