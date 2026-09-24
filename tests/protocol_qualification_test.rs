//! #1589: protocol names are qualified by module the way types are.
//!
//! A protocol declared in another module is named `ports.Store`; the bare
//! `Store` still resolves (it always has — protocol names resolved globally)
//! but is a deprecation WARNING whose machine-applicable fix is exactly the
//! qualified spelling. The qualified spelling marks the import used on its
//! own, so the E060 unused-import fix never deletes the import a qualified
//! bound or conformance depends on.
//!
//! Multi-file, so it cannot live in tests/diagnostics (a fixture there must
//! produce an ERROR; this is a warning on a program that still compiles).

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const PORTS: &str = "protocol Store {\n  fn read(self) -> Int\n}\n";

fn package(tag: &str, main: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-proto-qual-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"pq\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(dir.join("src/ports.almd"), PORTS).unwrap();
    std::fs::write(dir.join("src/main.almd"), main).unwrap();
    dir
}

fn check_json(dir: &Path) -> (i32, Vec<serde_json::Value>) {
    let out = Command::new(almide())
        .args(["check", "src/main.almd", "--json"])
        .current_dir(dir)
        .output()
        .expect("almide check");
    let diags = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("one JSON diagnostic per line"))
        .collect();
    (out.status.code().unwrap_or(-1), diags)
}

const BARE: &str = "import self.ports\n\ntype Mem: Store = { n: Int }\n\nfn Mem.read(self) -> Int = self.n\n\nfn go[S: Store](s: S) -> Int = s.read()\n\nfn main() -> Unit = println(int.to_string(go(Mem { n: 7 })))\n";

#[test]
fn a_bare_cross_module_protocol_name_warns_with_the_qualified_spelling_as_its_fix() {
    let dir = package("bare", BARE);
    let (code, diags) = check_json(&dir);
    assert_eq!(code, 0, "the bare spelling must keep compiling: {diags:?}");
    let warns: Vec<&serde_json::Value> = diags
        .iter()
        .filter(|d| d["message"].as_str().is_some_and(|m| m.contains("referenced by its bare name")))
        .collect();
    assert_eq!(warns.len(), 2, "one warning per bare reference (conformance + bound): {diags:?}");
    for w in &warns {
        assert_eq!(w["level"], "warning");
        assert_eq!(w["applicability"], "machine-applicable");
        assert_eq!(w["suggestions"][0]["replacement"], "ports.Store");
    }
    // The fix replaces exactly the name: `type Mem: Store` col 11..16,
    // `[S: Store]` col 10..15.
    let spans: Vec<(i64, i64, i64)> = warns
        .iter()
        .map(|w| {
            let s = &w["suggestions"][0];
            (s["line"].as_i64().unwrap(), s["col"].as_i64().unwrap(), s["end_col"].as_i64().unwrap())
        })
        .collect();
    assert_eq!(spans, vec![(3, 11, 16), (7, 10, 15)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_qualified_spelling_is_clean_and_keeps_its_import_through_almide_fix() {
    let dir = package("qualified", &BARE.replace(": Store", ": ports.Store"));
    let (code, diags) = check_json(&dir);
    assert_eq!(code, 0, "{diags:?}");
    assert!(diags.is_empty(), "the qualified spelling has nothing to report: {diags:?}");
    // `almide fix` judges imports syntactically; the qualifier is the use.
    let fix = Command::new(almide())
        .args(["fix", "src/main.almd"])
        .current_dir(&dir)
        .output()
        .expect("almide fix");
    assert!(fix.status.success(), "{}", String::from_utf8_lossy(&fix.stderr));
    let after = std::fs::read_to_string(dir.join("src/main.almd")).unwrap();
    assert!(after.starts_with("import self.ports\n"), "fix must not delete the import:\n{after}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_qualifier_that_names_the_wrong_module_is_an_error() {
    let dir = package("wrong", &BARE.replace("import self.ports\n", "import self.ports\nimport self.other\n").replace(": Store", ": other.Store"));
    std::fs::write(dir.join("src/other.almd"), "pub fn noop() -> Int = 0\n").unwrap();
    let (code, diags) = check_json(&dir);
    assert_ne!(code, 0);
    assert!(
        diags.iter().any(|d| d["message"].as_str().is_some_and(|m| m.contains("protocol 'Store' is not declared in module 'other'"))
            && d["hint"].as_str().is_some_and(|h| h.contains("Write `ports.Store`"))),
        "{diags:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
