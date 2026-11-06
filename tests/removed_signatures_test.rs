//! `almide fix` applies a dialect-epoch rename unattended only when the new
//! fn has the old fn's type (#4525).
//!
//! A ledger rename (`` `string.to_bytes(s)` → rewrite as `bytes.from_string(s)` ``)
//! keeps the call's argument text; whether it keeps the TYPE is read from the
//! stdlib signature index, recorded for the removed fns in
//! `crates/almide-frontend/src/generated/removed-signatures.tsv` by
//! `tools/gen-removed-signatures.py`. This file holds:
//!
//! - the record whole and current: one row per ledger rename, no stale row,
//!   and every row's new line equal to the index line the stdlib has now;
//! - the verdicts: the type-changing epoch-15 renames are suggestion-only,
//!   the type-preserving ones stay machine-applicable;
//! - `almide fix` end to end on the tests/diagnostics/e002-epoch15-* fixtures:
//!   a type-changing call is left as written and reported as manual work,
//!   a type-preserving one is rewritten to fixed.almd.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use almide_frontend::{removed_signatures, removed_spellings};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The signature index lines of docs/stdlib/*.md now, by `module.fn`.
fn current_index() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(repo().join("docs/stdlib"))
        .expect("docs/stdlib")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        let mut on = false;
        for line in text.lines() {
            if line.contains("BEGIN GENERATED SIGNATURE INDEX") {
                on = true;
            } else if line.contains("END GENERATED SIGNATURE INDEX") {
                on = false;
            } else if on && !line.starts_with("//") && line.contains(") -> ") {
                let decl = line.strip_prefix("effect ").unwrap_or(line);
                let Some(name) = decl.split('(').next().filter(|n| n.contains('.') && !n.contains(' ')) else { continue };
                let line = line.split("   (deprecated — ").next().unwrap_or(line).trim_end();
                out.entry(name.to_string()).or_insert_with(|| line.to_string());
            }
        }
    }
    out
}

#[test]
fn the_record_has_one_current_row_per_ledger_rename() {
    let renames = removed_spellings::renames();
    assert!(renames.len() >= 300, "the ledger's renames did not parse: {}", renames.len());
    let index = current_index();
    let mut problems = Vec::new();
    for (old, new) in &renames {
        let Some(rec) = removed_signatures::recorded(old) else {
            problems.push(format!("{old} -> {new}: no row"));
            continue;
        };
        if rec.new != index.get(new).map(String::as_str) {
            problems.push(format!("{old} -> {new}: recorded {:?}, the index has {:?}", rec.new, index.get(new)));
        }
        if !rec.old.contains(&format!("{old}(")) {
            problems.push(format!("{old}: the recorded old line names another fn: {}", rec.old));
        }
    }
    let olds: BTreeSet<&str> = renames.iter().map(|(o, _)| o.as_str()).collect();
    for name in removed_signatures::recorded_names() {
        if !olds.contains(name) {
            problems.push(format!("{name}: a row for no ledger rename"));
        }
    }
    assert!(
        problems.is_empty(),
        "crates/almide-frontend/src/generated/removed-signatures.tsv is stale — run \
         `python3 tools/gen-removed-signatures.py`:\n{}",
        problems.join("\n")
    );
}

/// The renames the issue audited: each keeps the argument text and changes
/// the type, so each is a suggestion whose hint names the change.
const TYPE_CHANGING: &[(&str, &str)] = &[
    ("string.to_bytes", "returns Bytes, not List[Int]"),
    ("value.field", "returns Option[Value], not Result[Value, String]"),
    ("value.keys", "returns Result[List[(String, Value)], String], not List[String]"),
    ("url.query_pairs", "returns Result[List[(String, String)], String], not List[(String, String)]"),
    ("bytes.read_string_be", "returns Option[Int], not String"),
    ("matrix.zeros_f32", "returns Matrix, not Matrix[Float32]"),
    ("matrix.ones_f32", "returns Matrix, not Matrix[Float32]"),
    ("bytes.data_ptr", "returns Region, not Int"),
];

#[test]
fn a_rename_that_changes_the_type_is_suggestion_only() {
    for (name, change) in TYPE_CHANGING {
        let r = removed_spellings::same_shape_rename(name).unwrap_or_else(|| panic!("{name}: no ledger rename"));
        assert!(!r.is_machine_applicable(), "{name} -> {} is machine-applicable", r.to);
        let said = r.type_change.unwrap();
        assert!(said.contains(change), "{name}: the hint says {said:?}, not {change:?}");
    }
    // Every verdict, for the record.
    let suggested: Vec<String> = removed_spellings::renames()
        .iter()
        .filter_map(|(o, _)| removed_spellings::same_shape_rename(o))
        .filter(|r| !r.is_machine_applicable())
        .map(|r| r.type_change.unwrap())
        .collect();
    eprintln!("suggestion-only renames ({}):\n{}", suggested.len(), suggested.join("\n"));
}

#[test]
fn a_rename_that_keeps_the_type_stays_machine_applicable() {
    // Plain equality, a bounded generic standing for the old primitive
    // (`int8.from_wrapping[S: Integer](x: S)` at `S = Int`), and one standing
    // for a stdlib type that declares the bound (`SafeHtml: Body`).
    for (name, to) in [("bytes.cmp", "bytes.compare"), ("int.to_int8", "int8.from_wrapping"), ("float.to_int", "int.from_saturating"), ("http.html", "http.response")] {
        let r = removed_spellings::same_shape_rename(name).unwrap_or_else(|| panic!("{name}: no ledger rename"));
        assert_eq!(r.to, to);
        assert!(r.is_machine_applicable(), "{name} -> {to} became a suggestion: {:?}", r.type_change);
    }
    let machine = removed_spellings::renames()
        .iter()
        .filter(|(o, _)| removed_spellings::same_shape_rename(o).is_some_and(|r| r.is_machine_applicable()))
        .count();
    assert!(machine >= 250, "only {machine} ledger renames stay machine-applicable");
}

fn fixture(name: &str) -> PathBuf {
    repo().join("tests/diagnostics").join(name)
}

/// `almide fix broken.almd` on a scratch copy: (exit ok, stderr, the file after).
fn fix(case: &Path) -> (bool, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("broken.almd");
    std::fs::copy(case.join("broken.almd"), &file).expect("copy broken.almd");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["fix", "broken.almd"])
        .current_dir(dir.path())
        .output()
        .expect("almide fix");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string(), std::fs::read_to_string(&file).unwrap())
}

#[test]
fn almide_fix_leaves_a_type_changing_rename_and_reports_it() {
    for case in [
        "e002-epoch15-string-to-bytes-type-changed",
        "e002-epoch15-value-field-type-changed",
        "e002-epoch15-value-keys-type-changed",
        "e002-epoch15-url-query-pairs-type-changed",
        "e002-epoch15-bytes-read-string-be-type-changed",
        "e002-epoch15-matrix-zeros-f32-type-changed",
        "e002-epoch15-bytes-data-ptr-type-changed",
    ] {
        let dir = fixture(case);
        let before = std::fs::read_to_string(dir.join("broken.almd")).unwrap();
        let (ok, stderr, after) = fix(&dir);
        assert_eq!(after, before, "{case}: `almide fix` rewrote the call:\n{after}");
        assert!(!ok, "{case}: `almide fix` reported the file clean");
        assert!(
            stderr.contains("need manual application") && stderr.contains("[E002]"),
            "{case}: `almide fix` does not report the call as manual work:\n{stderr}"
        );
    }
}

#[test]
fn almide_fix_applies_a_type_preserving_rename() {
    for case in ["e002-epoch15-int-to-int8-renamed", "e002-epoch15-bytes-cmp-renamed"] {
        let dir = fixture(case);
        let (ok, stderr, after) = fix(&dir);
        let fixed = std::fs::read_to_string(dir.join("fixed.almd")).unwrap();
        assert!(ok, "{case}: `almide fix` left work:\n{stderr}");
        assert_eq!(after.trim_end(), fixed.trim_end(), "{case}: `almide fix` did not reach fixed.almd");
    }
}
