//! A `fan` arm and a parallel worker get the main thread's stack (#4617, C-196).
//!
//! Arm threads and the `list.par` pool took Rust's spawned-thread default of
//! 2 MiB while the sequential evaluation runs on the main thread (8 MiB by
//! default), so a recursion depth of 200000 finished sequentially and ended in
//! `Error: stack overflow`, exit 1, inside a `fan { }` arm on every machine and
//! inside a parallel `fan.map` on two or more CPUs: the exit code depended on
//! the core count. The arms and the workers now take the main thread's size.

use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

const DEEP: &str = r#"fn deep(n: Int) -> Int = if n <= 0 then 1 else {
  let r = deep(n - 1)
  (r * 31 + n) % 1000003
}

effect fn arm(id: Int, n: Int) -> Result[Int, String] = {
  let v = deep(n)
  println("arm ${id} ${v}")
  ok(v)
}
"#;

/// The fan program: a two-arm block, then a parallel map over four elements.
const FAN_MAIN: &str = r#"
effect fn main() -> Unit = {
  let n = 200000
  let (a, b) = fan {
    arm(0, n)!
    arm(1, n)!
  }
  println("block ${a} ${b}")
  let ys = fan.map(list.range(0, 4), (i) => ok(deep(n + i)))!
  println("map ${list.join(list.map(ys, (y) => int.to_string(y)), ",")}")
}
"#;

/// Its sequential reading: the arms one after another, the map in list order.
const SEQ_MAIN: &str = r#"
effect fn main() -> Unit = {
  let n = 200000
  let a = arm(0, n)!
  let b = arm(1, n)!
  println("block ${a} ${b}")
  let ys = list.map(list.range(0, 4), (i) => deep(n + i))
  println("map ${list.join(list.map(ys, (y) => int.to_string(y)), ",")}")
}
"#;

fn build_and_run(dir: &std::path::Path, name: &str, main: &str) -> (Option<i32>, String, String) {
    let src = dir.join(format!("{name}.almd"));
    std::fs::write(&src, format!("{DEEP}{main}")).expect("write");
    let bin = dir.join(name);
    let out = Command::new(almide())
        .args(["build", &format!("{name}.almd"), "-o"])
        .arg(&bin)
        .current_dir(dir)
        .output()
        .expect("almide build");
    assert!(out.status.success(), "build {name} failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let run = Command::new(&bin).env_remove("RUST_MIN_STACK").output().expect("run");
    (
        run.status.code(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
    )
}

#[test]
fn a_depth_the_sequential_run_reaches_is_reached_in_fan_arms_and_parallel_workers() {
    let dir = tempfile::Builder::new().prefix("t4617").tempdir().expect("tempdir");
    let seq = build_and_run(dir.path(), "seq", SEQ_MAIN);
    assert_eq!(seq.0, Some(0), "the sequential run must reach the depth:\n{}", seq.2);
    let fan = build_and_run(dir.path(), "fan", FAN_MAIN);
    assert_eq!(fan, seq, "the fan run must answer what the sequential run answers (exit, stdout, stderr)");
}
