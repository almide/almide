//! Runs of field writes to a module-level `var` on the native render (#4621).
//!
//! The native IR pass takes the var out of its slot once around a run (or a
//! loop) of its field writes and writes a local instead. A heap-field write
//! is a `FieldAssign` whose target is the var itself, not an expression: when
//! the pass substituted only expressions, the grouped run still targeted the
//! global, so the pass grouped its own output again until the compiler's
//! stack overflowed (two consecutive heap-field writes were enough). Each
//! program here must build natively and print what the wasm leg prints.
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

fn run_native(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide_global_field_runs_{}_{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("{name}.almd"));
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide_bin()).arg("run").arg(&file).output().expect("almide runs");
    assert!(
        out.status.success(),
        "{name}: native run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn consecutive_heap_field_writes_build_and_run() {
    let src = r#"type C = { n: Int, xs: List[Int], s: String }
var g: C = { n: 0, xs: [], s: "" }
fn main() -> Unit = {
  g.s = "a"
  g.xs = [1, 2]
  println("${g.s} ${list.len(g.xs)}")
}
"#;
    assert_eq!(run_native("heap_fields", src), "a 2\n");
}

#[test]
fn a_loop_of_mixed_field_writes_keeps_every_write() {
    let src = r#"type C = { n: Int, xs: List[Int], m: Map[String, Int] }
var g: C = { n: 0, xs: [], m: [:] }
fn main() -> Unit = {
  for i in 0..<50 {
    g.n = g.n + 1
    list.push(g.xs, i)
    g.m = map.set(g.m, "k${i % 5}", i)
  }
  let before = g
  g.xs = []
  println("${g.n} ${list.len(g.xs)} ${map.len(g.m)} ${list.len(before.xs)}")
}
"#;
    assert_eq!(run_native("mixed_loop", src), "50 0 5 50\n");
}
