//! A tuple sort key walls the incumbent wasm leg instead of trapping (#2154).
//!
//! `list.sort_by(xs, ((w, c)) => (0 - c, w))` over `map.entries` built on the
//! incumbent as "verified" and died under wasmtime with `indirect call type
//! mismatch` — `sort_by_rc` takes a scalar key fn, and a heap-returning key
//! does not fit its `call_indirect` type. Native printed the answer. Now the
//! shape takes the `_x` twin (unlinked, honest wall), so the build refuses,
//! `check --target wasm` says so, and no artifact claims to be verified.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const SRC: &str = "fn main() -> Unit = {\n\
  let words = [\"b\", \"a\", \"c\", \"a\", \"b\", \"a\"]\n\
  var counts: Map[String, Int] = map.new()\n\
  for w in words { counts = map.set(counts, w, (map.get(counts, w) ?? 0) + 1) }\n\
  let top = map.entries(counts)\n\
    |> list.sort_by(((w, c)) => (0 - c, w))\n\
    |> list.take(10)\n\
  for (w, c) in top { println(\"${w} ${int.to_string(c)}\") }\n\
}\n";

fn run(args: &[&str], file: &std::path::Path) -> (bool, String) {
    let out = Command::new(almide()).args(args).arg(file).output().expect("run almide");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn native_still_answers() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("p.almd");
    std::fs::write(&file, SRC).unwrap();
    let (ok, text) = run(&["run"], &file);
    assert!(ok, "{text}");
    assert!(text.starts_with("a 3\nb 2\nc 1"), "{text}");
}

#[test]
fn the_wasm_build_walls_honestly_instead_of_shipping_a_trap() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("p.almd");
    std::fs::write(&file, SRC).unwrap();
    let out_wasm = dir.path().join("p.wasm");
    let out = Command::new(almide()).args(["build", "--target", "wasm", "-o"]).arg(&out_wasm).arg(&file).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!out.status.success(), "a tuple sort key must not build as verified:\n{text}");
    assert!(text.contains("sort_by"), "the wall names the call:\n{text}");
    assert!(!text.contains("Built "), "no artifact line on a wall:\n{text}");
}

#[test]
fn check_target_wasm_reports_the_wall_at_check_time() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("p.almd");
    std::fs::write(&file, SRC).unwrap();
    let (ok, text) = run(&["check", "--target", "wasm"], &file);
    assert!(!ok, "check --target wasm must not say No errors found for a shape the build refuses:\n{text}");
    assert!(text.contains("E082") || text.contains("wall"), "{text}");
}
