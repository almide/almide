//! ADR-0013: a `String` a parallel `fan.map` callback captures is marked
//! thread-shared before the threads start.
//!
//! `fan.map`'s parallel route (`fan.map_par`, #2044) reaches the walker as a
//! `Call { Module }`, not a `RuntimeCall`, and the thread-share marks used to
//! cover only the latter. A captured `prefix` then kept a non-atomic count
//! while every worker cloned and dropped it, and `fan_heap_parallel_test`
//! failed about one run in four (a garbage string, a 532564459379244062-byte
//! allocation, a bus error). The marks are now derived for every spelling of
//! a call to a runtime helper whose signature bounds `Send`
//! (`ThreadShareMarksPass`).
//!
//! This test pins the marks in the generated Rust and runs the race shape 50
//! times against the sequential run: built by the compiler before the fix,
//! 37 of 50 runs of this program diverged.
use std::process::Command;

const SOURCE: &str = r#"fn spin(seed: Int) -> Int = list.range(0, 20000) |> list.fold(seed, (a, i) => (a * 31 + i) % 1000003)

effect fn label(prefix: String, xs: List[String]) -> List[String] =
  fan.map(xs, (x) => ok(prefix + x + int.to_string(spin(string.len(x)))))!

effect fn main() -> Unit = {
  let words = ["alpha", "be", "gamma", "", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa", "lambda"]
  let tag = "t:"
  let a = fan.map(words, (w) => ok(tag + string.to_upper(w)))!
  println("${a}")
  println("${label("p-", words)!}")
  println("${label("q-", a)!}")
}
"#;

fn almide(args: &[&str]) -> std::process::Output {
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).args(args).output().expect("almide");
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    out
}

fn run(bin: &std::path::Path, sequential: bool) -> String {
    let mut c = Command::new(bin);
    if sequential {
        c.env("ALMIDE_FAN_SEQUENTIAL", "1");
    }
    let out = c.output().expect("run");
    assert!(out.status.success(), "run failed (status {:?}): {}", out.status, String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn every_parallel_fan_map_marks_its_captures_shared_first() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("fan_share.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let rust = String::from_utf8(almide(&[file.to_str().unwrap(), "--target", "rust"]).stdout).expect("utf8");
    let user = rust.split("pub fn label").nth(1).expect("label is emitted");
    let user = &user[..user.find("\nfn main() {").expect("main follows")];
    assert_eq!(user.matches("almide_rt_fan_map_par(").count(), 2, "both fan.map sites route parallel:\n{user}");
    assert_eq!(
        user.matches(".almide_share(); almide_rt_fan_map_par(").count(), 2,
        "every parallel fan.map marks what it captures before the threads start:\n{user}"
    );
    let label = &user[..user.find("\n}\n").expect("label ends")];
    assert!(label.contains("(prefix).almide_share();"), "the captured prefix is marked:\n{label}");
}

#[test]
fn a_captured_string_survives_fifty_parallel_runs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("fan_share.almd");
    std::fs::write(&file, SOURCE).expect("source");
    let bin = dir.path().join("fan_share");
    almide(&["build", file.to_str().unwrap(), "-o", bin.to_str().unwrap()]);
    let expected = run(&bin, true);
    assert!(expected.contains("p-alpha"), "sequential output:\n{expected}");
    for i in 0..50 {
        assert_eq!(run(&bin, false), expected, "parallel run {i} diverged from the sequential run");
    }
}
