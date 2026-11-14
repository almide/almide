//! An Option's payload read inside its `some` arm, used only by reading,
//! renders on native v1 as a reference into the Option — no copy.
//!
//! `fn width(o: Bytes?) -> Int = match o { some(b) => bytes.len(b), none => 1 }`
//! copied the whole buffer out of the Option on every call
//! (`match &v0 { Some(v) => v.clone(), .. }`): one allocation and a memcpy
//! per call where the standard codegen bumps a refcount. The Result's in-arm
//! read already took a reference; the Option's now does too, for a record
//! payload, a String and a list.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-option-view-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of the native `almide run <src>`, the certificate required.
fn run(src: &Path, alloc: bool) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if alloc {
        cmd.env("ALMIDE_ALLOC_COUNT", "1");
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Three readers of an Option's payload, each called `n` times on the same
/// `some` value.
fn readers(n: usize) -> String {
    format!(
        r#"fn width(o: Bytes?) -> Int = match o {{
  some(b) => bytes.len(b),
  none => 1,
}}

fn chars(o: String?) -> Int = match o {{
  some(s) => string.len(s),
  none => 0,
}}

fn count(o: List[Int]?) -> Int = match o {{
  some(xs) => list.len(xs),
  none => 0,
}}

effect fn main() -> Unit = {{
  let b = some(bytes.zeros(64))
  let s = some("payload")
  let xs = some([1, 2, 3])
  var total = 0
  for i in 0..<{n} {{
    total = total + width(b) + chars(s) + count(xs)
  }}
  println("${{total}} ${{width(none)}} ${{chars(none)}} ${{count(none)}}")
}}
"#
    )
}

/// The text of `fn <name>(` in the certified render of `source`.
fn rendered_fn(rust: &str, name: &str) -> String {
    let at = rust.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("no fn {name}"));
    let end = rust[at..].find("\n}\n").expect("the end of the fn");
    rust[at..at + end].to_string()
}

#[test]
fn a_payload_read_only_borrowed_is_a_reference_into_the_option() {
    let rust = almide_mir::pipeline::try_render_rust_source(&readers(4)).unwrap_or_else(|e| panic!("left the certified render: {e:?}"));
    for f in ["almd_width", "almd_chars", "almd_count"] {
        let body = rendered_fn(&rust, f);
        assert!(body.contains("Some(v) =>"), "{f} reads no payload:\n{body}");
        assert!(!body.contains(".clone()"), "{f} copies its payload out of the Option:\n{body}");
    }
}

/// The counting allocator's `allocs` for `source`, after checking its output.
fn allocs(tag: &str, source: &str, expect: &str) -> u64 {
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    let (stdout, stderr) = run(&src, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(stdout, expect, "{stderr}");
    let line = stderr.lines().find(|l| l.starts_with("__ALMD_ALLOC ")).unwrap_or_else(|| panic!("no allocation report:\n{stderr}"));
    line.split_whitespace().find_map(|kv| kv.strip_prefix("allocs=")).and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("malformed report `{line}`"))
}

#[test]
fn reading_a_payload_allocates_nothing_per_call() {
    let small = allocs("small", &readers(100), "7400 1 0 0\n");
    let large = allocs("large", &readers(2000), "148000 1 0 0\n");
    // 1,900 more calls of each reader: no more blocks, where a copy per read
    // allocated at least one per call.
    assert!(large < small + 100, "{large} blocks at 2,000 calls against {small} at 100 — a copy per payload read?");
}
