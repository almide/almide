//! A recursive fn over a variant that hands a PART of its param back to
//! itself and reads that part again afterwards borrows the param (#3434).
//!
//! `infer(t)` below matches `t`; one arm builds its result from parts of `t`
//! (`Wrap(a, b) => ok(Wrap(a, b))`), another recurses on a part and reads it
//! again (`infer(f)!` then `describe(f)`). With `t` owned, the recursive call
//! site deep-copied `f` at every level and the copies stayed alive down the
//! recursion: time, allocations and peak memory were quadratic in the depth
//! (8000 levels: 2.2 s). Borrowed, the recursive call passes `&f` and only
//! the `Wrap` arm clones what it moves, once, when it runs.
//!
//! The negative cells say where the owned param is still right: a param
//! returned whole on a path (`whole`), and a param handed to an owned slot
//! unconditionally (`forward`). If a positive assertion fails, the rule in
//! `recursion_rereads_a_part` (pass_borrow_inference_ownership.rs) is gone —
//! fix the pass, don't relax the assertion.
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in the
//! build step; locally run `cargo build --release` first).

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

const TYPES: &str = r#"type T =
  | Leaf
  | Wrap(T, T)
  | Node(T, T)

fn build(n: Int) -> T = {
  var t = Leaf
  for _ in 0..<n {
    t = Node(t, Leaf)
  }
  t
}

fn describe(t: T) -> String = match t {
  Leaf => "leaf",
  Wrap(_, _) => "wrap",
  _ => "node",
}

fn infer(t: T) -> T! = match t {
  Leaf => ok(Leaf),
  Wrap(a, b) => ok(Wrap(a, b)),
  Node(f, a) => {
    let ty = infer(f)!
    guard describe(f) != "?" else err("bad")
    ok(ty)
  },
}

fn keep(t: T) -> T = t

fn whole(t: T) -> T = match t {
  Node(f, _) => {
    let r = whole(f)
    if describe(f) == "?" then r else t
  },
  _ => t,
}

fn forward(t: T) -> T = match t {
  Node(f, _) => {
    let r = forward(f)
    if describe(f) == "?" then r else keep(t)
  },
  _ => keep(t),
}
"#;

fn program(main: &str) -> String {
    format!("{TYPES}\n{main}\n")
}

fn write(name: &str, src: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(name);
    std::fs::write(&file, src).unwrap();
    (dir, file)
}

fn emitted() -> String {
    let src = program("fn main() -> Unit = println(describe(infer(build(3)) ?? Leaf))");
    let (_dir, file) = write("prog.almd", &src);
    let out = Command::new(almide_bin()).arg(&file).args(["--target", "rust"]).output().expect("spawn almide");
    assert!(out.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The body of `pub fn <name>(` up to the first column-zero `}`.
fn body<'a>(rust: &'a str, name: &str) -> &'a str {
    let needle = format!("pub fn {name}(");
    let start = rust.find(&needle).unwrap_or_else(|| panic!("emitted Rust has no `{needle}`"));
    let rest = &rust[start..];
    let end = rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn the_recursive_call_borrows_the_part_and_only_the_moving_arm_clones() {
    if !tool_available() { return; }
    let rust = emitted();
    let infer = body(&rust, "infer");
    assert!(infer.starts_with("pub fn infer(t: &T)"), "the param must be borrowed:\n{infer}");
    assert!(infer.contains("infer(&(*f))"), "the recursive call must pass a borrow of the part:\n{infer}");
    assert!(!infer.contains("f.clone()"), "no copy of the part at the recursive call:\n{infer}");
    let clones: Vec<&str> = infer.lines().filter(|l| l.contains(".clone()")).collect();
    assert!(
        !clones.is_empty() && clones.iter().all(|l| l.trim_start().starts_with("T::Wrap(a, b) =>")),
        "the only copies are the parts the `Wrap` arm moves:\n{infer}"
    );
}

#[test]
fn the_param_stays_owned_where_it_is_consumed_whole() {
    if !tool_available() { return; }
    let rust = emitted();
    let whole = body(&rust, "whole");
    assert!(whole.starts_with("pub fn whole(t: T)"), "returned whole on a path: owned:\n{whole}");
    let forward = body(&rust, "forward");
    assert!(forward.starts_with("pub fn forward(t: T)"), "handed to an owned slot on every path: owned:\n{forward}");
}

#[test]
fn both_legs_print_what_value_semantics_say() {
    if !tool_available() { return; }
    let src = program(
        "fn main() -> Unit = {\n  let t = Node(Node(Wrap(Leaf, Node(Leaf, Leaf)), Leaf), Leaf)\n  println(describe(infer(t) ?? Leaf))\n  println(describe(infer(Node(Leaf, Leaf)) ?? Leaf))\n  println(describe(whole(t)))\n  println(describe(forward(t)))\n  println(describe(t))\n}",
    );
    let (_dir, file) = write("run.almd", &src);
    for target in ["rust", "wasm"] {
        let out = Command::new(almide_bin()).args(["run", file.to_str().unwrap(), "--target", target]).output().expect("spawn almide run");
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "wrap\nleaf\nnode\nnode\nnode\n", "{target}");
    }
}

/// `(allocs, peak)` of one counted native run of `infer(build(depth))`.
fn counted(depth: u32) -> (u64, u64) {
    let src = program(&format!("fn main() -> Unit = println(describe(infer(build({depth})) ?? Leaf))"));
    let (_dir, file) = write("count.almd", &src);
    let out = Command::new(almide_bin()).arg("run").arg(&file).env("ALMIDE_ALLOC_COUNT", "1").output().expect("spawn almide run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "counted run failed:\n{stderr}");
    let line = stderr.lines().find(|l| l.starts_with("__ALMD_ALLOC ")).unwrap_or_else(|| panic!("no __ALMD_ALLOC report:\n{stderr}"));
    let field = |key: &str| -> u64 {
        line.split_whitespace()
            .find_map(|kv| kv.strip_prefix(&format!("{key}=")))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("malformed report `{line}`"))
    };
    (field("allocs"), field("peak"))
}

#[test]
fn doubling_the_depth_does_not_quadruple_allocations() {
    if !tool_available() { return; }
    let (a1, p1) = counted(500);
    let (a2, p2) = counted(1000);
    // Linear work doubles; the quadratic copy quadrupled (501 505 → 2 003 005
    // allocations, 6.0 MB → 24.0 MB peak on 0.64.0).
    assert!(a2 < a1 * 3, "allocations grew {a1} → {a2} when the depth doubled: the per-level copy is back");
    assert!(p2 < p1 * 3, "peak memory grew {p1} → {p2} when the depth doubled: the per-level copy is back");
}
