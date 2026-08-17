//! Phase 4 of `docs/roadmap/active/diagnostics-here-try-hint.md`:
//! coverage report across the 3-layer diagnostic surface.
//!
//! For every `with_code("E###")` site under `crates/`, verify:
//!
//! 1. **Fixture**: `tests/diagnostics/<case>/meta.toml` declares
//!    `expects_code = "E###"` somewhere.
//! 2. **Doc**: `docs/diagnostics/E###.md` exists (Phase 5 registry).
//!
//! Missing coverage is surfaced via a printed report; the test
//! **does not currently fail** on gaps (soft gate). The intent is to
//! make the gap visible on every CI run so adding a new `with_code`
//! call without its fixture + doc file becomes an obvious
//! regression rather than silent drift.
//!
//! Promote to a hard gate by flipping the `SOFT_GATE` constant once
//! backfill completes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// When `true`, the test prints coverage gaps but passes. Flipped
/// to `false` once fixture backfill reached the allowlist-only
/// residual — the coverage gate now enforces at CI time.
const SOFT_GATE: bool = false;

/// Codes that intentionally skip the fixture requirement. Usually
/// because the diagnostic needs a multi-file setup the single-file
/// fixture harness can't express (E420 is cross-module visibility,
/// which requires an `import` graph). Keep this list short — each
/// entry is a gap the harness can't cover today.
const FIXTURE_ALLOWLIST: &[&str] = &[
    "E420",
    // E033 (opaque-type construction outside its defining module) needs a
    // two-module import graph the single-file harness can't express.
    "E033",
    // E054 (fmt verification failed) fires on an INTERNAL formatter defect —
    // no committed source file can (or should) trigger it deliberately, and
    // the harness runs `almide check`, which never formats. The verifier
    // logic is pinned by crates/almide-tools/tests/fmt_corpus_test.rs, which
    // feeds corrupted outputs straight into verify_format (#1464).
    "E054",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scan_diagnostic_codes() -> BTreeSet<String> {
    // Scan every `crates/**/*.rs` for `with_code("E###")`; collect
    // unique codes. Deliberately doesn't use a regex crate — the
    // test crate stays dependency-light.
    let root = repo_root().join("crates");
    let mut codes = BTreeSet::new();
    fn walk(dir: &Path, codes: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().map_or(false, |n| n == "target") { continue; }
                walk(&path, codes);
            } else if path.extension().map_or(false, |e| e == "rs") {
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let mut rest = text.as_str();
                while let Some(pos) = rest.find("with_code(\"E") {
                    rest = &rest[pos + "with_code(\"E".len()..];
                    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if !digits.is_empty() && rest[digits.len()..].starts_with('"') {
                        codes.insert(format!("E{:03}", digits.parse::<u32>().unwrap_or(0)));
                    }
                }
            }
        }
    }
    walk(&root, &mut codes);
    codes
}

fn scan_fixture_codes() -> BTreeMap<String, Vec<String>> {
    // Returns code → list of fixture names that declare it.
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let root = repo_root().join("tests/diagnostics");
    let Ok(entries) = std::fs::read_dir(&root) else { return out; };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let meta = path.join("meta.toml");
        let Ok(text) = std::fs::read_to_string(&meta) else { continue };
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("expects_code") {
                let value = rest
                    .trim_start_matches(|c: char| c == '=' || c.is_whitespace())
                    .trim_matches('"');
                let case = path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.entry(value.to_string()).or_default().push(case);
            }
        }
    }
    out
}

fn scan_doc_codes() -> BTreeSet<String> {
    let root = repo_root().join("docs/diagnostics");
    let Ok(entries) = std::fs::read_dir(&root) else { return BTreeSet::new(); };
    entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_suffix(".md").map(|s| s.to_string()))
        .filter(|n| n.starts_with('E') && n.len() >= 4)
        .collect()
}

#[test]
fn diagnostic_fixture_and_doc_coverage_report() {
    let codes = scan_diagnostic_codes();
    assert!(!codes.is_empty(), "no diagnostic codes found under crates/");

    let fixtures = scan_fixture_codes();
    let docs = scan_doc_codes();

    let mut missing_fixture: Vec<&String> = Vec::new();
    let mut allowlisted_fixture: Vec<&String> = Vec::new();
    let mut missing_doc: Vec<&String> = Vec::new();
    for code in &codes {
        if !fixtures.contains_key(code) {
            if FIXTURE_ALLOWLIST.contains(&code.as_str()) {
                allowlisted_fixture.push(code);
            } else {
                missing_fixture.push(code);
            }
        }
        if !docs.contains(code)         { missing_doc.push(code); }
    }

    eprintln!();
    eprintln!("── Diagnostic coverage report ───────────────────────────");
    eprintln!("  Codes in source    : {}", codes.len());
    eprintln!("  Fixture-covered    : {} ({})",
        codes.len() - missing_fixture.len() - allowlisted_fixture.len(),
        fixtures.values().map(|v| v.len()).sum::<usize>());
    eprintln!("  Doc-covered        : {}", codes.len() - missing_doc.len());
    if !missing_fixture.is_empty() {
        eprintln!("  Missing fixtures    : {}", missing_fixture.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "));
    }
    if !allowlisted_fixture.is_empty() {
        eprintln!("  Allowlisted         : {}", allowlisted_fixture.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "));
    }
    if !missing_doc.is_empty() {
        eprintln!("  Missing docs        : {}", missing_doc.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(", "));
    }
    eprintln!("─────────────────────────────────────────────────────────");
    eprintln!();

    if !SOFT_GATE && (!missing_fixture.is_empty() || !missing_doc.is_empty()) {
        panic!(
            "diagnostic coverage incomplete:\n  missing fixtures: {:?}\n  missing docs: {:?}",
            missing_fixture, missing_doc
        );
    }
}

#[test]
fn every_fixture_meta_declares_known_code() {
    // Reverse gate: every `meta.toml` with `expects_code` must name a
    // code that actually exists in source. Catches typos like
    // `E02` → fixture orphaned.
    let source_codes = scan_diagnostic_codes();
    let fixtures = scan_fixture_codes();
    let mut orphans: Vec<String> = Vec::new();
    for (code, cases) in &fixtures {
        if !source_codes.contains(code) {
            for case in cases {
                orphans.push(format!("{} declares {}", case, code));
            }
        }
    }
    if !orphans.is_empty() {
        panic!("fixtures declare unknown codes:\n  {}", orphans.join("\n  "));
    }
}


/// Ratchet: the number of `super::err(` construction sites WITHOUT a
/// `.with_code(...)` attached. Every user-facing error should carry a
/// stable E-code (the `--explain` / dojo-feedback surface); this count
/// may only go DOWN. When you add a code to an existing site, lower the
/// constant. Adding a NEW uncoded error site fails this gate — attach a
/// code (and its fixture + doc, enforced above) instead.
/// Baseline measured 2026-08-06 (#1113); lowered to 39 when the eq
/// "compares" site gained E037 (#1116).
const UNCODED_ERR_BASELINE: usize = 39;

#[test]
fn uncoded_error_sites_ratchet() {
    let root = repo_root().join("crates");
    let mut uncoded = 0usize;
    let mut by_file: BTreeMap<String, usize> = BTreeMap::new();
    fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().map_or(false, |n| n == "target") { continue; }
                walk(&path, f);
            } else if path.extension().map_or(false, |e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) { f(&path, &text); }
            }
        }
    }
    walk(&root, &mut |path, text| {
        let mut i = 0usize;
        while let Some(j) = text[i..].find("super::err(") {
            let j = i + j;
            // The 600-byte window must not split a multi-byte char (an
            // em-dash in a diagnostic message sat exactly on the boundary).
            let mut end = (j + 600).min(text.len());
            while !text.is_char_boundary(end) { end -= 1; }
            let window = &text[j..end];
            let seg = match window.find("));") {
                Some(end) => &window[..end + 3],
                None => window,
            };
            if !seg.contains(".with_code(") {
                uncoded += 1;
                *by_file.entry(path.display().to_string()).or_default() += 1;
            }
            i = j + "super::err(".len();
        }
    });
    for (file, n) in &by_file {
        eprintln!("uncoded err sites: {:3}  {}", n, file);
    }
    assert!(
        uncoded <= UNCODED_ERR_BASELINE,
        "uncoded diagnostic sites grew: {} > baseline {} — attach .with_code (+ fixture + doc) to new errors",
        uncoded, UNCODED_ERR_BASELINE
    );
    if uncoded < UNCODED_ERR_BASELINE {
        eprintln!(
            "NOTE: uncoded sites = {} < baseline {} — lower UNCODED_ERR_BASELINE to lock in the progress",
            uncoded, UNCODED_ERR_BASELINE
        );
    }
}
