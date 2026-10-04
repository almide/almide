//! A module-level `var` written from inside a literal callback that a native
//! arm inlines (`list.map`, `filter`, `fold`, `any`) keeps every field it
//! did not write, on both legs (#3317, C-187).
//!
//! The structural wasm leg classifies C-319 cells (vars a lambda captures and
//! the frame mutates) by scanning the body. An inlined callback is a lambda,
//! and the scan also saw the GLOBAL it writes, so the global's id joined the
//! frame's cell set. A field write then read the global "through its cell":
//! the record's first slot was copied as if it were the record, and the
//! global was rebound to that copy. The output was a silently wrong record
//! (`6 0` for `6 7`) or allocator words, depending on the route. Only a
//! frame's LOCALS are cells now.
//!
//! The family is every write route a callback can take on a global: a field,
//! a nested field, a mut receiver on a field (`list.push(buf.xs, …)`), a whole
//! record assign, an index assign on a global list. It also covers a field
//! write that reads the global it writes (`buf.v = buf.v + k`). Each route is
//! exercised in a single file and from a sibling module. The expected output
//! is pinned exactly, and both legs must print it.

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

const DECLS: &str = r#"type In = { a: Int, b: Int }
type Box = { v: Int, w: Int, xs: List[Int], inner: In }
var buf: Box = { v: 0, w: 7, xs: [1], inner: { a: 1, b: 2 } }
var arr: List[Int] = [10, 20, 30]

fn show(tag: String) -> Unit =
  println("${tag}: v=${buf.v} w=${buf.w} xs=${list.len(buf.xs)} a=${buf.inner.a} b=${buf.inner.b} arr=${arr[0]},${arr[1]},${arr[2]}")

fn field_map(k: Int) -> Unit = {
  let _ = list.map([1], (i) => {
    buf.v = k * 2
    i
  })
}

fn nested_filter(k: Int) -> Unit = {
  let _ = list.filter([1, 2], (i) => {
    buf.inner.a = k + i
    true
  })
}

fn push_fold(k: Int) -> Unit = {
  let _ = list.fold([1, 2], 0, (acc, i) => {
    list.push(buf.xs, k + i)
    acc + i
  })
}

fn whole_map(k: Int) -> Unit = {
  let _ = list.map([1], (i) => {
    buf = { ...buf, w: k }
    i
  })
}

fn index_map(k: Int) -> Unit = {
  let _ = list.map([1], (i) => {
    arr[1] = k
    i
  })
}

fn field_any(k: Int) -> Unit = {
  let _ = list.any([1], (i) => {
    buf.v = buf.v + k
    false
  })
}
"#;

/// The calls, in order; `{p}` is the module prefix (`` or `m.`).
const CALLS: &[(&str, &str)] = &[
    ("field_map(3)", "field"),
    ("nested_filter(5)", "nested"),
    ("push_fold(9)", "push"),
    ("whole_map(4)", "whole"),
    ("index_map(8)", "index"),
    ("field_any(100)", "any"),
];

const WANT: &str = "field: v=6 w=7 xs=1 a=1 b=2 arr=10,20,30
nested: v=6 w=7 xs=1 a=7 b=2 arr=10,20,30
push: v=6 w=7 xs=3 a=7 b=2 arr=10,20,30
whole: v=6 w=4 xs=3 a=7 b=2 arr=10,20,30
index: v=6 w=4 xs=3 a=7 b=2 arr=10,8,30
any: v=106 w=4 xs=3 a=7 b=2 arr=10,8,30
";

fn main_body(prefix: &str) -> String {
    let mut s = String::from("effect fn main() -> Unit = {\n");
    for (call, tag) in CALLS {
        s.push_str(&format!("  {prefix}{call}\n  {prefix}show(\"{tag}\")\n"));
    }
    s.push_str("}\n");
    s
}

/// The sibling module: every fn public (types are public already).
fn module_source() -> String {
    DECLS
        .lines()
        .map(|l| match l.strip_prefix("fn ") {
            Some(rest) => format!("pub fn {rest}"),
            None => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3317-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::write(root.join("almide.toml"), "[package]\nname = \"gw\"\nversion = \"0.1.0\"\n").expect("toml");
    root
}

fn run(root: &Path, wasm: bool) -> String {
    let mut cmd = Command::new(almide_bin());
    cmd.current_dir(root).arg("run").arg("src/main.almd");
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().expect("spawn almide");
    assert!(
        out.status.success(),
        "{} leg failed in {}: {}",
        if wasm { "wasm" } else { "native" },
        root.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_global_written_from_an_inlined_callback_keeps_its_other_fields() {
    if Command::new("wasmtime").arg("--version").output().is_err() {
        if std::env::var("CI").is_ok() {
            panic!("wasmtime is required under CI");
        }
        eprintln!("skip: wasmtime not on PATH");
        return;
    }
    let single = scratch("single");
    std::fs::write(single.join("src/main.almd"), format!("{DECLS}\n{}", main_body(""))).expect("write");
    let sibling = scratch("sibling");
    std::fs::write(sibling.join("src/m.almd"), module_source()).expect("write");
    std::fs::write(sibling.join("src/main.almd"), format!("import self.m\n{}", main_body("m."))).expect("write");
    for (name, root) in [("single", &single), ("sibling", &sibling)] {
        assert_eq!(run(root, false), WANT, "{name}: native leg");
        assert_eq!(run(root, true), WANT, "{name}: wasm leg");
    }
    let _ = std::fs::remove_dir_all(&single);
    let _ = std::fs::remove_dir_all(&sibling);
}
