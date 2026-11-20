//! `almide check --effects` runs the value gate `almide check` runs (#4847):
//! every E109 and E110 fixture (E110 hand decodes are the value
//! flow's refusal) is refused there too, and its `fixed.almd` passes.
//! The fixtures are read from `tests/diagnostics/`, so a new one is covered
//! without editing this file. `ALMIDE_CONTROL_BIN` runs an older binary.

use std::path::Path;
use std::process::Command;

fn almide(args: &[&str], dir: &Path) -> std::process::Output {
    let bin = std::env::var("ALMIDE_CONTROL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    Command::new(bin).args(args).current_dir(dir).output().expect("almide runs")
}

#[test]
fn check_effects_refuses_what_the_value_gate_refuses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/diagnostics");
    let mut cases: Vec<(std::path::PathBuf, &str)> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            let code = if name.starts_with("e109-") { "E109" } else if name.starts_with("e110-") { "E110" } else { return None };
            Some((p, code))
        })
        .collect();
    cases.sort();
    assert!(cases.len() >= 5, "the E109 / E110 hand-decode fixtures are missing: {cases:?}");
    for (dir, code) in cases {
        let o = almide(&["check", "--effects", "broken.almd"], &dir);
        let t = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        assert!(!o.status.success(), "{}: `check --effects` accepted it:\n{t}", dir.display());
        assert!(t.contains(code), "{}: `check --effects` did not name {code}:\n{t}", dir.display());
        if dir.join("fixed.almd").exists() {
            let o = almide(&["check", "--effects", "fixed.almd"], &dir);
            let t = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            assert!(o.status.success(), "{}: `check --effects` refused the fixed program:\n{t}", dir.display());
        }
    }
}
