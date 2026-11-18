//! An ordering comparison of a cell-held `String` against a literal builds on
//! the Rust target (#4780).
//!
//! A `var` captured by a callback after an in-place call is lowered to a
//! shared cell, and its read at a `&str` slot rendered `&a.get()` — a
//! `&AlmideStr` — while the literal side stayed `"q"`. `AlmideStr` orders only
//! against itself, so rustc refused `(&a.get() > "q")` (E0308) on a program
//! `almide check` accepts and the wasm leg runs. The cell read at a `&str`
//! slot now derefs through to `str`, as the plain var's `&*a` always did.
//! Found by the 2026-11 stdlib re-inspection through `testing.assert_gt`.

use std::io::Write;
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

fn run_prints(name: &str, src: &str, expected: &str) {
    let dir = std::env::temp_dir().join(format!("almd_4780_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.almd");
    std::fs::File::create(&file).unwrap().write_all(src.as_bytes()).unwrap();
    let out = Command::new(almide_bin()).args(["run", file.to_str().unwrap()]).output().expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.status.success(), "[{name}] almide run failed:\n{stderr}");
    assert_eq!(stdout.trim_end(), expected, "[{name}] wrong output");
}

const PROGRAM: &str = r#"effect fn main() -> Unit = {
  var a: String = "zz"
  string.clear(a)
  let t = a > "q"
  let u = "q" < a
  let w = a <= "q"
  let s = list.fold([1, 2], set.new(), (acc, b) => set.from_list([a]))
  println("${t} ${u} ${w} ${set.len(s)}")
}
"#;

#[test]
fn a_cell_string_orders_against_a_literal() {
    run_prints("cell_lit", PROGRAM, "false false true 1");
}

#[test]
fn a_cell_string_still_orders_against_a_string() {
    // The other side owned: the comparison keeps building and agrees.
    let src = PROGRAM.replace("a > \"q\"", "a > string.repeat(\"q\", 1)");
    run_prints("cell_owned", &src, "false false true 1");
}
