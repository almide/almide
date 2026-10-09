//! A string LITERAL on the left of `+` prepends into the right operand's own
//! buffer instead of becoming a fresh one-literal String that is then grown.
//! `"k" + int.to_string(i)` was three allocator calls per evaluation on the
//! native leg — the literal's String, its realloc to the joined length, and
//! the right operand's free — against one on wasm; the mapbuild perf row read
//! wasm FASTER than native (0.67x) partly on that. `int.to_string` reserves 19
//! bytes, so the prepend now fits in place: one allocation per key.
use std::process::Command;

const PROGRAM: &str = r#"
effect fn main() -> Unit = {
  var total = 0
  var last = ""
  for i in 0..<1000 {
    let k = "k" + int.to_string(i)
    total = total + string.len(k)
    last = k
  }
  let wide = "héllo, " + int.to_string(42) + "!"
  println("${int.to_string(total)} ${last} ${wide}")
}
"#;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write_program() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("literal_prefix.almd");
    std::fs::write(&source, PROGRAM).unwrap();
    (directory, source)
}

#[test]
fn a_literal_left_operand_is_emitted_as_a_bare_str() {
    let (_dir, source) = write_program();
    let out = Command::new(almide()).arg(&source).args(["--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "emit failed: {}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        rust.contains("AlmideConcat::concat(\"k\", "),
        "the literal left operand should stay a `&str`:\n{}",
        rust.lines().filter(|l| l.contains("AlmideConcat::concat(")).collect::<Vec<_>>().join("\n")
    );
    assert!(!rust.contains("AlmideConcat::concat(\"k\".to_string()"), "the literal was materialized as a String");
}

#[test]
fn the_literal_prefix_loop_allocates_once_per_key() {
    let (_dir, source) = write_program();
    let native = Command::new(almide()).arg("run").arg(&source).env("ALMIDE_ALLOC_COUNT", "1").output().unwrap();
    let stderr = String::from_utf8_lossy(&native.stderr).to_string();
    assert!(native.status.success(), "native run failed:\n{stderr}");
    assert_eq!(String::from_utf8_lossy(&native.stdout), "3890 k999 héllo, 42!\n");
    let line = stderr
        .lines()
        .find(|l| l.starts_with("__ALMD_ALLOC"))
        .unwrap_or_else(|| panic!("no `__ALMD_ALLOC` line — the allocation lane did not arm:\n{stderr}"));
    let field = |name: &str| -> u64 {
        line.split_whitespace()
            .find_map(|f| f.strip_prefix(name))
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("unparsable allocation report: {line}"))
    };
    // One `int.to_string` buffer per key (the prepend fits its 19-byte
    // reservation); the old shape made ~2,000 allocations and ~1,000 reallocs.
    assert!(field("allocs=") < 1_100, "native made too many allocations: {line}");
    assert!(field("reallocs=") < 10, "the prepend reallocated per key: {line}");
}
