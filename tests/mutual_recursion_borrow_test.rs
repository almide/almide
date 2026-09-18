//! A read-only heap parameter of a MUTUALLY recursive group is borrowed
//! (#2040). Self-recursive fns already took `xs: &[T]` (#647); a group
//! `rec_a ↔ rec_b` fell to `Vec<T>` + `.clone()` at every call because the
//! first fixed-point round met the partner before its signature existed
//! and seeded the slot as Own, which no later round could undo — a
//! recursive-descent parser threading its grammar and token list through
//! five helpers spent 140× its runtime deep-copying them.
//!
//! Emit-shape tests in the mold of `codec_decode_borrows_input_test.rs`.
//! Skips cleanly when the `almide` binary is unavailable.

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

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn emitted(source: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide-mutual-borrow-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    let output = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("failed to spawn almide");
    let rust = String::from_utf8_lossy(&output.stdout).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&output.stderr));
    rust
}

/// The issue's repro: a self-recursive reader, a mutually recursive pair
/// of readers, and a pair whose second member CONSUMES the list.
const SRC: &str = "type T = { k: String, v: Int }\n\
    fn rec_self(xs: List[T], i: Int) -> Int =\n\
      if i >= 100 then 0 else { let a = rec_self(xs, i + 1); a + (option.map(list.get(xs, i), (t) => t.v) ?? 0) }\n\
    fn rec_a(xs: List[T], i: Int) -> Int =\n\
      if i >= 100 then 0 else { let a = rec_b(xs, i + 1); a + (option.map(list.get(xs, i), (t) => t.v) ?? 0) }\n\
    fn rec_b(xs: List[T], i: Int) -> Int =\n\
      if i >= 100 then 0 else { let a = rec_a(xs, i + 1); a + 1 }\n\
    fn keep_a(xs: List[T], i: Int) -> List[T] =\n\
      if i >= 3 then xs else keep_b(xs, i + 1)\n\
    fn keep_b(xs: List[T], i: Int) -> List[T] =\n\
      if i >= 3 then xs + [T { k: \"z\", v: i }] else keep_a(xs, i + 1)\n\
    effect fn main() -> Unit = {\n\
      let xs = [T { k: \"a\", v: 1 }]\n\
      println(int.to_string(rec_self(xs, 0) + rec_a(xs, 0) + list.len(keep_a(xs, 0))))\n\
    }\n";

#[test]
fn a_mutually_recursive_reader_group_borrows_its_list() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(SRC, "readers");
    assert!(rust.contains("pub fn rec_self(xs: &[T]"), "the self-recursive reader keeps its borrow:\n{}", grep(&rust, "pub fn rec_"));
    assert!(rust.contains("pub fn rec_a(xs: &[T]"), "rec_a only reads xs and hands it to rec_b: borrowed:\n{}", grep(&rust, "pub fn rec_"));
    assert!(rust.contains("pub fn rec_b(xs: &[T]"), "rec_b only hands xs back to rec_a: borrowed:\n{}", grep(&rust, "pub fn rec_"));
    let group = rust
        .lines()
        .skip_while(|l| !l.starts_with("pub fn rec_a("))
        .take_while(|l| !l.starts_with("pub fn keep_a("))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!group.contains("xs.clone()"), "no per-call clone survives in the reader group:\n{group}");
}

#[test]
fn a_group_that_consumes_the_list_stays_owned() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let rust = emitted(SRC, "consumers");
    // keep_b returns `xs + [..]` (consumes) and keep_a returns xs itself:
    // the optimistic first round must be corrected by the next one.
    assert!(rust.contains("pub fn keep_b(xs: Vec<T>"), "a member that consumes the list owns it:\n{}", grep(&rust, "pub fn keep_"));
    assert!(rust.contains("pub fn keep_a(xs: Vec<T>"), "its partner, which returns the list, owns it too:\n{}", grep(&rust, "pub fn keep_"));
}

fn grep(rust: &str, needle: &str) -> String {
    rust.lines().filter(|l| l.contains(needle)).collect::<Vec<_>>().join("\n")
}
