//! A `!` inside a `fan { … }` arm ends that ARM with its Err (#3462).
//!
//! The reading (C-199, C-005, ADR-0024 D1): a fan block is the sequential,
//! run-every-element evaluation of its arms. An arm's `!` is the arm's own
//! propagation, the same channel a `fan.map` callback's `!` rides, so every
//! arm still runs, output appears in arm order, the lowest-index Err is the
//! block's, and in `main` it surfaces as `Error: <msg>` + exit 1.
//!
//! Before: native stripped the inner `!` (FanLowering), so the generated Rust
//! failed rustc E0308; the wasm leg read it as an exit from the enclosing fn,
//! so an arm-0 failure never ran arm 1.
//!
//! The pin, per cell of the matrix: stdout, stderr and the exit code, equal
//! to the expected values on both legs; and for the threaded form, the
//! emitted Rust compiles warning-free and still spawns its arms.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn tool_available(tool: &str) -> bool {
    let found = Command::new(tool).arg("--version").output().is_ok();
    if !found && std::env::var("ALMIDE_EXPECT_TOOLS").is_ok() {
        panic!("{tool} is required (ALMIDE_EXPECT_TOOLS is set) but was not found");
    }
    found
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-3462-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// (stdout, stderr, exit code) of `almide run` on one target.
fn run(almd: &Path, target: &str) -> (String, String, i32) {
    let out = Command::new(almide_bin())
        .arg("run")
        .arg(almd)
        .args(["--target", target])
        .output()
        .expect("spawn almide run");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

const BOOM: &str = "\
effect fn boom(n: Int) -> Int = {
  if n > 1 then err(\"boom ${n}\")!
  else n
}
";

/// Runs `main` on both legs and asserts the expected observation on each.
fn check(name: &str, main: &str, stdout: &str, stderr: &str, code: i32) {
    if !tool_available("rustc") || !tool_available("wasmtime") {
        return;
    }
    let dir = scratch(name);
    let almd = dir.join("main.almd");
    std::fs::write(&almd, format!("{BOOM}\n{main}")).expect("write source");
    for target in ["rust", "wasm"] {
        let (out, err, got) = run(&almd, target);
        assert_eq!(out, stdout, "{name} [{target}]: stdout (stderr: {err})");
        assert_eq!(err, stderr, "{name} [{target}]: stderr");
        assert_eq!(got, code, "{name} [{target}]: exit code");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn arm_zero_fails_while_arm_one_prints() {
    check("a0", "\
effect fn main() -> Unit = {
  let (a, b) = fan {
    { let x = boom(5)!; println(\"arm0 after\"); x + 1 },
    { println(\"arm1 runs\"); let y = boom(1)!; y + 1 },
  }
  println(int.to_string(a + b))
}
", "arm1 runs\n", "Error: boom 5\n", 1);
}

#[test]
fn arm_one_fails_while_arm_zero_prints() {
    check("a1", "\
effect fn main() -> Unit = {
  let (a, b) = fan {
    { println(\"arm0 runs\"); let x = boom(1)!; x + 1 },
    { let y = boom(5)!; println(\"arm1 after\"); y + 1 },
  }
  println(int.to_string(a + b))
}
", "arm0 runs\n", "Error: boom 5\n", 1);
}

#[test]
fn both_fail_and_the_lowest_index_err_wins() {
    check("both", "\
effect fn main() -> Unit = {
  let (a, b) = fan {
    { println(\"arm0 runs\"); let x = boom(7)!; x + 1 },
    { println(\"arm1 runs\"); let y = boom(5)!; y + 1 },
  }
  println(int.to_string(a + b))
}
", "arm0 runs\narm1 runs\n", "Error: boom 7\n", 1);
}

#[test]
fn a_bang_nested_in_a_match_within_an_arm() {
    check("nested", "\
effect fn main() -> Unit = {
  let (a, b) = fan {
    { match 3 { 3 => boom(9)!, _ => 0 } },
    { println(\"arm1 runs\"); 2 },
  }
  println(int.to_string(a + b))
}
", "arm1 runs\n", "Error: boom 9\n", 1);
}

#[test]
fn an_option_bang_in_an_arm_fails_with_none() {
    check("option", "\
effect fn main() -> Unit = {
  let xs = [10, 20]
  let (a, b) = fan {
    { let v = list.get(xs, 5)!; v + 1 },
    { println(\"arm1 runs\"); let w = list.get(xs, 1)!; w },
  }
  println(int.to_string(a + b))
}
", "arm1 runs\n", "Error: none\n", 1);
}

#[test]
fn every_arm_succeeds() {
    check("ok", "\
effect fn main() -> Unit = {
  let (a, b) = fan {
    { println(\"arm0 runs\"); let x = boom(1)!; x + 1 },
    { println(\"arm1 runs\"); let y = boom(1)!; y + 1 },
  }
  println(int.to_string(a + b))
}
", "arm0 runs\narm1 runs\n4\n", "", 0);
}

#[test]
fn an_inline_fan_arm_calling_a_closure() {
    check("inline", "\
effect fn main() -> Unit = {
  let f = (n: Int) => n * 2
  let (a, b) = fan {
    { let x = boom(f(3))!; println(\"arm0 after\"); x },
    { println(\"arm1 runs ${f(1)}\"); let y = boom(1)!; y },
  }
  println(int.to_string(a + b))
}
", "arm1 runs 2\n", "Error: boom 6\n", 1);
}

#[test]
fn a_one_arm_fan() {
    check("single", "\
effect fn main() -> Unit = {
  let a = fan { { println(\"only arm\"); let x = boom(4)!; x + 1 } }
  println(int.to_string(a))
}
", "only arm\n", "Error: boom 4\n", 1);
}

#[test]
fn a_fan_in_a_callee_propagated_with_bang() {
    check("callee", "\
effect fn pair(p: Int, q: Int) -> Int = {
  let (a, b) = fan {
    { let x = boom(p)!; x * 10 },
    { println(\"pair arm1 ${q}\"); let y = boom(q)!; y },
  }
  a + b
}

effect fn main() -> Unit = {
  println(\"ok ${pair(1, 1)!}\")
  println(\"unreachable ${pair(3, 1)!}\")
}
", "pair arm1 1\nok 11\npair arm1 1\n", "Error: boom 3\n", 1);
}

#[test]
fn fan_map_callback_bang_runs_every_element() {
    check("map", "\
effect fn main() -> Unit = {
  let r = fan.map([3, 1, 4], (n) => { println(\"el ${n}\"); let v = boom(n)!; ok(v * 10) })!
  println(int.to_string(list.len(r)))
}
", "el 3\nel 1\nel 4\n", "Error: boom 3\n", 1);
}

#[test]
fn fan_settle_callback_bang_is_captured_per_element() {
    check("settle", "\
effect fn main() -> Unit = {
  let rs = fan.settle([3, 1], (n) => { println(\"el ${n}\"); let v = boom(n)!; ok(v * 10) })
  for r in rs {
    println(match r { ok(v) => \"ok ${v}\", err(e) => \"err ${e}\" })
  }
}
", "el 3\nel 1\nerr boom 3\nok 10\n", "", 0);
}

/// The threaded form survives: the arms still spawn, the emitted Rust
/// compiles warning-free, and the arm's `!` is the closure's own `?`.
#[test]
fn the_threaded_form_keeps_its_threads_and_compiles_warning_free() {
    if !tool_available("rustc") {
        return;
    }
    let dir = scratch("threaded");
    let almd = dir.join("main.almd");
    std::fs::write(&almd, format!("{BOOM}\n\
effect fn main() -> Unit = {{
  let (a, b) = fan {{
    {{ println(\"arm0 runs\"); let x = boom(1)!; x + 1 }},
    {{ println(\"arm1 runs\"); let y = boom(2)!; y + 1 }},
  }}
  println(int.to_string(a + b))
}}
")).expect("write source");
    let emitted = Command::new(almide_bin()).arg(&almd).args(["--target", "rust"]).output().expect("spawn almide");
    assert!(emitted.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&emitted.stderr));
    let rust = String::from_utf8_lossy(&emitted.stdout).into_owned();
    let user = rust.rsplit("//__ALMIDE_RT_BOUNDARY__").next().unwrap();
    assert!(user.contains("__almide_s.spawn"), "a fan whose arms capture no Rc value keeps its threads:\n{user}");
    assert!(user.contains("(boom(1i64))?"), "the arm's `!` is the spawn closure's own `?`:\n{user}");
    let rs = dir.join("main.rs");
    std::fs::write(&rs, &rust).expect("write rust");
    let bin = dir.join("main");
    let rustc = Command::new("rustc")
        .args(["--edition", "2021", "-D", "warnings", "-A", "non_snake_case", "-A", "unused_macros", "-o"])
        .arg(&bin)
        .arg(&rs)
        .output()
        .expect("spawn rustc");
    assert!(rustc.status.success(), "the generated Rust does not compile warning-free:\n{}", String::from_utf8_lossy(&rustc.stderr));
    let out = Command::new(&bin).output().expect("run binary");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "arm0 runs\narm1 runs\n");
    assert_eq!(String::from_utf8_lossy(&out.stderr), "Error: boom 2\n");
    assert_eq!(out.status.code(), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}
