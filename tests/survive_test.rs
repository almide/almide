//! `almide survive` / `almide apply --if-survives` end-to-end (#2147): drive
//! the REAL binary on the three golden-delta fixtures under `tests/survive/`.
//!
//! Each fixture directory is a tiny project, one proposed edit (`edit.patch`
//! or `edit.txt` — never `.almd`, so no sweep mistakes it for source) and the
//! `expected.json` survival delta. The three edits are the three shapes a
//! delta can take:
//!
//! - `breaks_test`      — a patch that turns a passing test red;
//! - `fixes_diagnostic` — a full-text edit that fixes one error and inserts a
//!   line above another, which must stay the SAME diagnostic (line 5 → 6);
//! - `neutral`          — a patch that only inserts a comment: every
//!   diagnostic and test is unchanged, the warning renumbered 2 → 4.
//!
//! Every run happens in a scratch copy, so a regression that writes cannot
//! dirty the repository — and the no-write test proves it does not.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// (fixture dir, edited file, edit file)
const CASES: [(&str, &str, &str); 3] = [
    ("breaks_test", "calc.almd", "edit.patch"),
    ("fixes_diagnostic", "shapes.almd", "edit.txt"),
    ("neutral", "greet.almd", "edit.patch"),
];

/// A fresh copy of `tests/survive/<case>` in the temp dir.
fn scratch_copy(case: &str, tag: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/survive").join(case);
    let dst = std::env::temp_dir().join(format!("almide-survive-test-{}-{}-{}", case, tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dst);
    std::fs::create_dir_all(&dst).expect("create scratch");
    for e in std::fs::read_dir(&src).expect("fixture dir") {
        let p = e.expect("entry").path();
        std::fs::copy(&p, dst.join(p.file_name().unwrap())).expect("copy fixture file");
    }
    dst
}

fn run(dir: &Path, args: &[&str]) -> (i32, Value, String) {
    let out = Command::new(almide())
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .expect("spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let json = serde_json::from_str(stdout.trim()).unwrap_or(Value::Null);
    (out.status.code().unwrap_or(-1), json, format!("stdout:\n{stdout}\nstderr:\n{stderr}"))
}

/// Every file's bytes and mtime — the whole directory, so a stray temp file
/// or a rewritten neighbour is caught along with the edited file.
fn snapshot(dir: &Path) -> Vec<(String, Vec<u8>, std::time::SystemTime)> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let meta = std::fs::metadata(&p).unwrap();
            (p.file_name().unwrap().to_string_lossy().to_string(), std::fs::read(&p).unwrap(), meta.modified().unwrap())
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn the_three_golden_deltas_match() {
    for (case, file, edit) in CASES {
        let dir = scratch_copy(case, "golden");
        let (code, got, raw) = run(&dir, &["survive", file, "--with", edit, "--json"]);
        let expected: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
        assert_eq!(
            got,
            expected,
            "{case}: the survival delta drifted from tests/survive/{case}/expected.json\nactual:\n{}\n{raw}",
            serde_json::to_string_pretty(&got).unwrap()
        );
        assert_eq!(got["schema_version"], 1, "{case}");
        let want_code = if expected["survives"] == true { 0 } else { 1 };
        assert_eq!(code, want_code, "{case}: exit code follows the verdict\n{raw}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn survive_never_writes() {
    // The breaks_test case runs every leg — check, a compiled test binary on
    // both sides — which is the most any evaluation does to the tree.
    let dir = scratch_copy("breaks_test", "nowrite");
    let before = snapshot(&dir);
    // A coarse mtime clock must not hide a rewrite of identical length.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let (code, got, raw) = run(&dir, &["survive", "calc.almd", "--with", "edit.patch", "--json"]);
    assert_eq!(code, 1, "{raw}");
    assert_eq!(got["written"], false, "{raw}");
    assert_eq!(snapshot(&dir), before, "survive changed the directory it judged\n{raw}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apply_refuses_an_edit_that_does_not_survive_and_force_overrides() {
    let dir = scratch_copy("breaks_test", "refuse");
    let original = std::fs::read(dir.join("calc.almd")).unwrap();
    let (code, got, raw) = run(&dir, &["apply", "calc.almd", "--with", "edit.patch", "--if-survives", "--json"]);
    assert_eq!(code, 1, "{raw}");
    assert_eq!((got["survives"].clone(), got["written"].clone()), (Value::Bool(false), Value::Bool(false)), "{raw}");
    assert_eq!(std::fs::read(dir.join("calc.almd")).unwrap(), original, "a refused apply wrote\n{raw}");

    let (code, got, raw) = run(&dir, &["apply", "calc.almd", "--with", "edit.patch", "--force", "--json"]);
    assert_eq!(code, 0, "{raw}");
    assert_eq!((got["written"].clone(), got["forced"].clone()), (Value::Bool(true), Value::Bool(true)), "{raw}");
    let now = std::fs::read_to_string(dir.join("calc.almd")).unwrap();
    assert!(now.contains("a - b"), "--force did not write the edit:\n{now}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apply_writes_an_edit_that_survives() {
    let dir = scratch_copy("neutral", "accept");
    let (code, got, raw) = run(&dir, &["apply", "greet.almd", "--with", "edit.patch", "--if-survives=true", "--json"]);
    assert_eq!(code, 0, "{raw}");
    assert_eq!((got["survives"].clone(), got["written"].clone()), (Value::Bool(true), Value::Bool(true)), "{raw}");
    let now = std::fs::read_to_string(dir.join("greet.almd")).unwrap();
    assert!(now.starts_with("// Greetings, in two directions.\n\nfn greet"), "{now}");
    // Atomic write: the sibling temp file is gone.
    let stray: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("almide-apply"))
        .collect();
    assert!(stray.is_empty(), "temp file left behind: {:?}", stray.iter().map(|e| e.file_name()).collect::<Vec<_>>());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apply_fails_closed_on_a_non_boolean_gate_or_no_gate() {
    let dir = scratch_copy("neutral", "closed");
    let before = snapshot(&dir);
    for args in [
        vec!["apply", "greet.almd", "--with", "edit.patch", "--if-survives=maybe", "--json"],
        vec!["apply", "greet.almd", "--with", "edit.patch", "--if-survives=1", "--json"],
        vec!["apply", "greet.almd", "--with", "edit.patch", "--if-survives=false", "--json"],
        vec!["apply", "greet.almd", "--with", "edit.patch", "--json"],
    ] {
        let (code, got, raw) = run(&dir, &args);
        assert_eq!(code, 2, "{args:?} must refuse before judging\n{raw}");
        assert!(got["error"].is_string(), "{args:?}: the refusal is JSON under --json\n{raw}");
    }
    // `--force` is a plain flag: a value on it is a usage error, not a yes.
    let (code, _, raw) = run(&dir, &["apply", "greet.almd", "--with", "edit.patch", "--force=yes"]);
    assert_ne!(code, 0, "{raw}");
    assert_eq!(snapshot(&dir), before, "a refused apply touched the directory");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_patch_that_does_not_apply_is_an_error_not_a_verdict() {
    let dir = scratch_copy("neutral", "badpatch");
    std::fs::write(dir.join("bad.patch"), "@@ -1,1 +1,1 @@\n-no such line\n+x\n").unwrap();
    let (code, got, raw) = run(&dir, &["survive", "greet.almd", "--with", "bad.patch", "--json"]);
    assert_eq!(code, 2, "{raw}");
    assert!(got["error"].as_str().unwrap_or("").contains("does not apply"), "{raw}");
    let _ = std::fs::remove_dir_all(&dir);
}
