//! A stdlib spelling the dialect ledger records as removed stays removed.
//!
//! `proofs/dialect-epochs.toml` is the one place a removal is written: each
//! removed spelling is a `breaks` entry, and `almide_frontend::removed_spellings`
//! parses it for the check-time hint. This gate reads the same entries and
//! holds them against the DECLARED public surface (`almide compile <module>
//! --json`): a spelling the ledger says is gone must not be public again. So
//! the set is never a hand list — adding a removal to the ledger extends the
//! gate, and re-adding a removed function (security table row 20: `hash.md5`,
//! `md5_hex`, `sha1`, `sha1_hex`, removed at epoch 15, C-424) fails it.
//!
//! The rule is a pure function over the surface, so the negative control
//! below runs it on a forged surface that brings one back.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use almide_frontend::removed_spellings;

/// The public fn names of `module`, or `None` when it is no module (a module
/// the ledger removed whole, `mem`).
fn declared(module: &str) -> Option<BTreeSet<String>> {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["compile", module, "--json"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("almide compile");
    if !o.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).expect("interface json");
    Some(v["functions"].as_array().into_iter().flatten().filter_map(|f| f["name"].as_str().map(String::from)).collect())
}

/// Every module a removal names, with its declared surface.
fn surfaces() -> BTreeMap<String, Option<BTreeSet<String>>> {
    let mut out = BTreeMap::new();
    for r in removed_spellings::all() {
        let module = r.from.split('.').next().unwrap_or("").to_string();
        if !out.contains_key(&module) {
            let s = declared(&module);
            out.insert(module, s);
        }
    }
    out
}

/// The removals that are public again.
fn returned(surfaces: &BTreeMap<String, Option<BTreeSet<String>>>) -> Vec<String> {
    removed_spellings::all()
        .iter()
        .filter(|r| {
            let (m, f) = r.from.split_once('.').unwrap_or(("", ""));
            surfaces.get(m).and_then(|s| s.as_ref()).is_some_and(|s| s.contains(f))
        })
        .map(|r| format!("`{}` was removed at dialect epoch {} and is public again", r.from, r.epoch))
        .collect()
}

#[test]
fn no_removed_spelling_is_public_again() {
    assert!(removed_spellings::all().len() >= 34, "the ledger's removals did not parse");
    let back = returned(&surfaces());
    assert!(back.is_empty(), "proofs/dialect-epochs.toml records these as removed:\n{}", back.join("\n"));
}

/// The ledger is append-only, so row 20's removal stays recorded; this
/// anchors that the four digests are among the removals the gate holds.
#[test]
fn the_broken_digests_are_recorded_as_removed() {
    for name in ["hash.md5", "hash.md5_hex", "hash.sha1", "hash.sha1_hex"] {
        let r = removed_spellings::lookup(name).unwrap_or_else(|| panic!("{name}: no removal in the ledger"));
        assert_eq!(r.epoch, 15, "{name}");
    }
}

/// Negative control: a surface that brings `hash.md5` back must fail.
#[test]
fn control_a_returned_digest_fails_the_gate() {
    let mut s = surfaces();
    assert!(returned(&s).is_empty(), "control baseline must be clean");
    s.get_mut("hash").and_then(|h| h.as_mut()).expect("hash surface").insert("md5".to_string());
    assert!(returned(&s).iter().any(|p| p.contains("`hash.md5`")), "control: a returned md5 must fail the gate");
}
