//! `almide test --update-snapshots` — the accept step of
//! `testing.assert_snapshot` (#1314), end to end on the built binary.
//!
//! The fixture pair under tests/fixtures/snapshot/ is the contract: a file
//! with one NEW snapshot (`""`), one DRIFTED heredoc and one matching call
//! must become `drift_test.accepted.almd` byte-for-byte after one accept run
//! — the one-line value escaped, the multi-line value written as a heredoc
//! that reads back exactly, the matching call and everything else untouched.
//! The negatives pin the refusals: a plain run fails with the accept hint and
//! writes nothing; CI mode (`CI=true`) writes nothing even with the flag.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn fixture(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/snapshot").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// A scratch dir holding a copy of the drifting fixture as `<tag>_test.almd`.
/// The file name is per test on purpose: the harness keys its scratch wasm
/// module and native worker dir on the RELATIVE path, so two parallel tests
/// running a same-named file from different directories would race on them.
fn scratch(tag: &str) -> (PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("almd_1314_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let name = format!("{tag}_test.almd");
    std::fs::write(dir.join(&name), fixture("drift_test.almd")).unwrap();
    (dir, name)
}

/// `almide test <name> <args>` in `dir`, with the CI switch pinned either
/// way so the developer's own environment cannot flip the verdict.
fn run_test(dir: &Path, name: &str, args: &[&str], ci: Option<&str>) -> (bool, String) {
    let mut cmd = Command::new(almide_bin());
    cmd.arg("test").arg(name).args(args).current_dir(dir);
    cmd.env_remove("ALMIDE_UPDATE_SNAPSHOTS");
    match ci {
        Some(v) => { cmd.env("CI", v); }
        None => { cmd.env_remove("CI"); }
    }
    let out = cmd.output().expect("failed to spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

#[test]
fn the_accept_step_rewrites_every_drifted_literal_in_place() {
    let (dir, name) = scratch("accept");
    let (ok, text) = run_test(&dir, &name, &["--update-snapshots"], None);
    let after = std::fs::read_to_string(dir.join(&name)).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(ok, "the accept run must end green:\n{text}");
    assert!(text.contains("accept_test.almd:6: new snapshot written"), "{text}");
    assert!(text.contains("accept_test.almd:10: snapshot rewritten"), "{text}");
    assert!(text.contains("2 snapshot(s) updated"), "{text}");
    assert_eq!(after, fixture("drift_test.accepted.almd"), "the rewritten source:\n{after}");
}

#[test]
fn the_accepted_file_is_green_on_both_lanes_and_stable_under_a_second_accept() {
    let (dir, name) = scratch("stable");
    std::fs::write(dir.join(&name), fixture("drift_test.accepted.almd")).unwrap();
    let (ok_native, t1) = run_test(&dir, &name, &["--target", "rust"], None);
    let (ok_wasm, t2) = run_test(&dir, &name, &["--target", "wasm"], None);
    let (ok_again, t3) = run_test(&dir, &name, &["--update-snapshots"], None);
    let after = std::fs::read_to_string(dir.join(&name)).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(ok_native, "native:\n{t1}");
    // wasmtime may be absent locally; a SKIP-only run is still a green exit.
    assert!(ok_wasm, "wasm:\n{t2}");
    assert!(ok_again && t3.contains("0 snapshot(s) updated"), "{t3}");
    assert_eq!(after, fixture("drift_test.accepted.almd"), "a second accept must not move the file");
}

#[test]
fn a_plain_run_fails_with_the_accept_hint_and_writes_nothing() {
    let (dir, name) = scratch("plain");
    let (ok, text) = run_test(&dir, &name, &[], None);
    let after = std::fs::read_to_string(dir.join(&name)).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(!ok, "a drifted snapshot must fail the plain run:\n{text}");
    assert!(text.contains("FAILED: plain_test.almd"), "{text}");
    assert!(text.contains("accept:") && text.contains("almide test --update-snapshots plain_test.almd"), "{text}");
    assert_eq!(after, fixture("drift_test.almd"), "a plain run must never write");
}

#[test]
fn ci_mode_refuses_to_write_even_with_the_flag() {
    for (tag, args, ci) in [("cienv", &["--update-snapshots"][..], Some("true")), ("ciflag", &["--update-snapshots", "--ci"][..], None)] {
        let (dir, name) = scratch(tag);
        let (ok, text) = run_test(&dir, &name, args, ci);
        let after = std::fs::read_to_string(dir.join(&name)).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert!(!ok, "CI mode must fail on a new snapshot ({args:?}, CI={ci:?}):\n{text}");
        assert!(text.contains("CI mode") && text.contains("writes nothing"), "{text}");
        assert!(text.contains(&format!("FAILED: {name}")), "{text}");
        assert_eq!(after, fixture("drift_test.almd"), "CI mode must never write ({args:?}, CI={ci:?})");
    }
}

#[test]
fn a_non_literal_expectation_is_refused_not_rewritten() {
    let (dir, name) = scratch("nonlit");
    let src = "import testing\n\ntest \"t\" {\n  let e = \"\"\n  testing.assert_snapshot(\"x\", e)\n}\n";
    std::fs::write(dir.join(&name), src).unwrap();
    let (ok, text) = run_test(&dir, &name, &["--update-snapshots"], None);
    let after = std::fs::read_to_string(dir.join(&name)).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(!ok, "{text}");
    assert!(text.contains("could not be updated") && text.contains("not a string literal"), "{text}");
    assert_eq!(after, src);
}
