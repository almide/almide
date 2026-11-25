//! C-462 (ALS-I5, owner ruling 2026-11-24 on decision-site row
//! `verify.gate-build-incomplete-accepted`): every route that makes a wasm
//! module judges the module's own certificate, and INCOMPLETE — every
//! witness accepted, some function the compiler declared uncertified — is
//! refused by default: exit 1, no artifact written, nothing run, each
//! uncertified function named with its reason. `--allow-incomplete-
//! verification` lets it through with a `warning[verification-incomplete]`,
//! and `almide build` records the pass on its `Built` line.
//!
//! The program declines on purpose: an Option `?` at a fn's tail over an
//! owned temporary is a frame shape the structural wasm leg's ownership
//! witness does not certify yet (`tail:view-result`, witness_hooks.rs; the
//! guarded list rest and then the match-arm select this test used certify
//! since the 29th and 31st engineers).
//! When that shape is certified,
//! this test needs another declining shape — it asserts the program IS
//! declined before it asserts anything about the refusal.

use std::path::Path;
use std::process::{Command, Output};

fn almide(dir: &Path, args: &[&str]) -> Output {
    let bin = std::env::var("ALMIDE_CONTROL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    Command::new(bin).args(args).current_dir(dir).output().expect("almide runs")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

const FLAG: &str = "--allow-incomplete-verification";

const PROGRAM: &str = r#"fn try_first(xs: List[Int]) -> Int? = list.first(xs)?

effect fn main() -> Unit = {
  println("a+${try_first([2]) ?? 0}")
}

test "try_first" {
  assert_eq(try_first([1, 2]), some(1))
}
"#;

#[test]
fn an_incomplete_certificate_is_refused_unless_the_flag_allows_it() {
    let td = tempfile::TempDir::new().unwrap();
    let dir = td.path();
    std::fs::write(dir.join("main.almd"), PROGRAM).unwrap();

    // The program is INCOMPLETE, not REJECTED: the refusal below is about
    // the declined frame, and names it.
    let routes: [(&str, Vec<&str>); 4] = [
        ("build", vec!["build", "main.almd", "--target", "wasm", "-o", "app.wasm"]),
        ("run", vec!["run", "main.almd", "--target", "wasm"]),
        ("test", vec!["test", "main.almd", "--target", "wasm"]),
        ("bench", vec!["bench", "main.almd", "--target", "wasm", "--runs", "1"]),
    ];
    for (route, args) in &routes {
        let o = almide(dir, args);
        let t = text(&o);
        assert_eq!(o.status.code(), Some(1), "`almide {route}` did not refuse an INCOMPLETE certificate:\n{t}");
        assert!(t.contains("error[certificate-incomplete]"), "`almide {route}` did not name the refusal:\n{t}");
        assert!(t.contains("UNCERTIFIED  try_first: tail:view-result"), "`almide {route}` did not name the uncertified function:\n{t}");
        assert!(t.contains(FLAG), "`almide {route}` did not name the flag:\n{t}");
        assert!(!String::from_utf8_lossy(&o.stdout).contains("a+2"), "`almide {route}` ran the refused module:\n{t}");
    }
    assert!(!dir.join("app.wasm").exists(), "the refused build wrote a module");

    // With the flag every route passes, warns, and the build records it.
    for (route, args) in &routes {
        let mut args = args.clone();
        args.push(FLAG);
        let o = almide(dir, &args);
        let t = text(&o);
        assert!(o.status.success(), "`almide {route} {FLAG}` refused:\n{t}");
        assert!(t.contains("warning[verification-incomplete]"), "`almide {route} {FLAG}` did not warn:\n{t}");
        assert!(t.contains("UNCERTIFIED  try_first: tail:view-result"), "`almide {route} {FLAG}` did not name the function:\n{t}");
        if *route == "build" {
            assert!(dir.join("app.wasm").exists(), "the allowed build wrote no module");
            assert!(
                t.lines().any(|l| l.starts_with("Built app.wasm") && l.contains(&format!("certificate INCOMPLETE, passed under {FLAG}"))),
                "the Built line does not record the pass:\n{t}"
            );
        }
        if *route == "run" {
            assert!(String::from_utf8_lossy(&o.stdout).contains("a+2"), "the allowed run did not run:\n{t}");
        }
    }
}
