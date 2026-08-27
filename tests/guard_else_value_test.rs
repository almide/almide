//! #1593: a VALUE else on `guard` is a FUNCTION return — never loop control.
//!
//! The Rust walker's `stmt_guard_is_loop_control` classified `ok(())` (and
//! any hoisted `Result[Unit,_]` else) as a loop `break`: at fn level that
//! emitted `break` outside a loop (rustc E0268), and inside a loop it
//! silently diverged from the interp/wasm legs, which RETURN from the fn
//! (the interp's `exec_stmt_guard`: `Flow::Value → Flow::Return` — the
//! normative reading). Only a literal `break`/`continue` else is loop
//! control. Lives as a native-leg test (not a spec fixture) because the
//! effect-fn guard shape sits on the v1 wasm leg's linearization frontier —
//! a spec file would land in the fallback set the coverage ratchet freezes.

use std::path::Path;
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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

const SRC: &str = r#"import int

effect fn check(n: Int) -> Unit = {
  guard n > 0 else ok(())
  println("positive ${int.to_string(n)}")
  ok(())
}

effect fn scan_until_gap(xs: List[Int]) -> Unit = {
  for x in xs {
    guard x > 0 else ok(())
    println("saw ${int.to_string(x)}")
  }
  println("no gap")
  ok(())
}

fn keep_positive(xs: List[Int]) -> Int = {
  var total = 0
  for x in xs {
    guard x > 0 else continue
    total = total + x
  }
  total
}

effect fn main() -> Unit = {
  check(1)!
  check(-1)!
  scan_until_gap([1, 2])!
  scan_until_gap([3, -1, 9])!
  println(int.to_string(keep_positive([1, -2, 3])))
}
"#;

#[test]
fn guard_else_value_returns_from_the_fn_on_the_native_leg() {
    if !tools_available() {
        return;
    }
    let dir = std::env::temp_dir().join("almide-issue1593");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let src = dir.join("main.almd");
    std::fs::write(&src, SRC).expect("write");
    let out = Command::new(almide_bin())
        .args(["run", src.to_str().unwrap()])
        .current_dir(&dir)
        .output()
        .expect("spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("E0268"),
        "guard's value else rendered as a loop break again (#1593):\n{stderr}"
    );
    assert!(out.status.success(), "run failed:\nstdout: {stdout}\nstderr: {stderr}");
    // check(-1) returns silently; scan_until_gap([3,-1,9]) exits the FN at
    // -1 — no "saw 9", no "no gap".
    assert_eq!(
        stdout,
        "positive 1\nsaw 1\nsaw 2\nno gap\nsaw 3\n4\n",
        "wrong output: {stdout}"
    );
}
