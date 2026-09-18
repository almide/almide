//! #2227: every function the `almide init` CLAUDE.md template names in its
//! "Stdlib Quick Reference" block exists in the stdlib. The template is what a
//! model reads before writing its first line, so a name that is not there
//! (`env.get_or` was advertised for months) is a promise the compiler breaks
//! on the first call. Hand-maintained lists drift; this gate does not.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf() }

/// `(module, function)` pairs named in the quick-reference block.
fn advertised() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(repo().join("docs/project/CLAUDE_TEMPLATE.md")).unwrap();
    let block = text.split("## Stdlib Quick Reference").nth(1).expect("quick reference heading");
    let block = block.split("```").nth(1).expect("fenced block under the heading");
    let mut out = Vec::new();
    let mut module: Option<String> = None;
    for line in block.lines() {
        let l = line.trim();
        if l.is_empty() || l.ends_with(':') || l.contains("also auto-imported") { module = None; continue; }
        // `import fs      — read_text write …`
        if let Some(rest) = l.strip_prefix("import ") {
            let (m, names) = rest.split_once('—').expect("import line uses an em dash");
            module = Some(m.trim().to_string());
            out.extend(names.split_whitespace().map(|n| (m.trim().to_string(), n.to_string())));
            continue;
        }
        // `string: len trim …` (continuation lines carry no colon)
        if let Some((m, names)) = l.split_once(':') {
            if !m.contains(' ') {
                module = Some(m.to_string());
                out.extend(names.split_whitespace().map(|n| (m.to_string(), n.to_string())));
                continue;
            }
        }
        if let Some(m) = &module {
            out.extend(l.split_whitespace().map(|n| (m.clone(), n.to_string())));
        }
    }
    assert!(out.len() > 50, "the block parsed to only {} names — layout changed?", out.len());
    out
}

/// Function names a module defines: `fn f(`, `pub fn f(`, `pub effect fn f(`,
/// across `stdlib/<mod>.almd` and `stdlib/<mod>_*.almd`.
fn defined(module: &str) -> BTreeSet<String> {
    let dir = repo().join("stdlib");
    let mut names = BTreeSet::new();
    let mut files = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let p = entry.unwrap().path();
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let ext_ok = p.extension().is_some_and(|e| e == "almd");
        if !ext_ok || !(stem == module || stem.starts_with(&format!("{module}_"))) { continue; }
        files += 1;
        for line in std::fs::read_to_string(&p).unwrap().lines() {
            let l = line.trim_start();
            let l = l.strip_prefix("pub ").unwrap_or(l);
            let l = l.strip_prefix("effect ").unwrap_or(l);
            if let Some(rest) = l.strip_prefix("fn ") {
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !name.is_empty() { names.insert(name); }
            }
        }
    }
    assert!(files > 0, "no stdlib source for module `{module}` under stdlib/");
    names
}

#[test]
fn every_function_the_init_template_advertises_exists_in_the_stdlib() {
    let mut missing = Vec::new();
    let mut cache: std::collections::BTreeMap<String, BTreeSet<String>> = Default::default();
    for (m, f) in advertised() {
        let defs = cache.entry(m.clone()).or_insert_with(|| defined(&m));
        if !defs.contains(&f) { missing.push(format!("{m}.{f}")); }
    }
    assert!(missing.is_empty(), "the CLAUDE.md template names functions the stdlib does not define:\n  {}", missing.join("\n  "));
}
