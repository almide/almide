//! E062 — `almide check` warns on an `env` shebang that lacks `-S` (#2159).
//!
//! The spelling runs on macOS and dies on Linux, and the failure never
//! reproduces where the script was written. What is asserted: the warning
//! fires with its code and the exact `-S` rewrite, the `-S` form stays
//! quiet, `--json` carries it, and `--deny-warnings` turns it into an exit 1.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(shebang: &str, args: &[&str]) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("script.almd");
    std::fs::write(&file, format!("{shebang}\n\nfn main() -> Unit = println(\"ok\")\n")).expect("write");
    let out = Command::new(almide()).arg("check").args(args).arg(&file).output().expect("run");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

#[test]
fn env_without_split_string_warns_with_the_rewrite() {
    let (ok, text) = check("#!/usr/bin/env almide run", &[]);
    assert!(ok, "a warning alone must not fail the check:\n{text}");
    assert!(text.contains("warning[E062]"), "code missing:\n{text}");
    assert!(text.contains("does not run on Linux"), "message missing:\n{text}");
    assert!(text.contains("#!/usr/bin/env -S almide run"), "try line missing:\n{text}");
    assert!(text.contains("No errors found"), "verdict missing:\n{text}");
}

#[test]
fn the_split_form_is_quiet() {
    let (ok, text) = check("#!/usr/bin/env -S almide run", &[]);
    assert!(ok);
    assert!(!text.contains("E062"), "-S form must not warn:\n{text}");
}

#[test]
fn json_carries_the_warning() {
    let (ok, text) = check("#!/usr/bin/env almide run", &["--json"]);
    assert!(ok, "{text}");
    let line = text.lines().find(|l| l.contains("E062")).unwrap_or_else(|| panic!("no E062 JSON line:\n{text}"));
    assert!(line.trim_start().starts_with('{'), "not JSON: {line}");
    assert!(line.contains("\"warning\""), "level missing: {line}");
}

#[test]
fn deny_warnings_turns_it_into_an_error() {
    let (ok, text) = check("#!/usr/bin/env almide run", &["--deny-warnings"]);
    assert!(!ok, "--deny-warnings must fail on E062:\n{text}");
    assert!(text.contains("treated as errors"), "{text}");
}
