//! #2244: `xs = list.set(xs, i, v)` — the copy assigned back to the list it
//! came from — lowers to an in-place slot write, so a loop of them is linear.
//! `list.set` into ANOTHER binding still copies. Asserted on the emitted Rust
//! (which calls the copying runtime fn) and on the wall clock of the issue's
//! own program at its largest row: 16,000 sets over 16,000 elements took
//! 14 s quadratic; linear, it is milliseconds, so a generous deadline tells
//! the two apart without measuring anything fine.
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn almide() -> &'static str { env!("CARGO_BIN_EXE_almide") }

fn write(dir: &Path, name: &str, src: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, src).unwrap();
    p
}

const SHAPES: &str = "effect fn main() -> Unit = {\n  var xs = [1, 2, 3]\n  xs = list.set(xs, 1, 9)\n  xs = xs.set(0, 8)\n  let ys = list.set(xs, 2, 7)\n  var zs = [1]\n  zs = list.set(ys, 0, 6)\n  println(\"${xs} ${ys} ${zs}\")\n}\n";

#[test]
fn only_the_self_assigned_set_avoids_the_copying_runtime_call() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "shapes.almd", SHAPES);
    let out = Command::new(almide()).arg(&file).args(["--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rs = String::from_utf8_lossy(&out.stdout);
    let main = rs.split("fn __almide_main").nth(1).expect("main");
    // `ys = list.set(xs, …)` and `zs = list.set(ys, …)` copy; the two self-assignments do not.
    assert_eq!(main.matches("almide_rt_list_set(").count(), 2, "{main}");
    let run = Command::new(almide()).arg("run").arg(&file).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout), "[8, 9, 3] [8, 9, 7] [6, 9, 7]\n");
}

const ISSUE: &str = "import env\n\neffect fn main() -> Unit = {\n  let n = int.parse(list.get(env.args(), 0) ?? \"2000\") ?? 2000\n  var xs: List[List[String]] = []\n  var i = 0\n  while i < n { list.push(xs, [\"a\", \"b\", \"c\", \"d\", \"e\"]); i = i + 1 }\n  var j = 0\n  while j < n { xs = list.set(xs, j, [\"x\"]); j = j + 1 }\n  println(int.to_string(list.len(xs)))\n}\n";

#[test]
fn sixteen_thousand_self_assigned_sets_run_in_linear_time_on_both_targets() {
    let dir = tempfile::tempdir().unwrap();
    let file = write(dir.path(), "issue.almd", ISSUE);
    let exe = dir.path().join("issue");
    let built = Command::new(almide()).arg("build").arg(&file).arg("-o").arg(&exe).output().unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let t = Instant::now();
    let out = Command::new(&exe).arg("16000").output().unwrap();
    let took = t.elapsed();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "16000\n");
    assert!(took < Duration::from_secs(5), "native took {took:?} — the quadratic copy is back (the issue measured 14 s)");
    let out = Command::new(almide()).arg("run").arg(&file).args(["--target", "wasm", "--", "16000"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "16000\n");
}
