//! #3435: the embedded wasm host (`almide run --target wasm`, the wasm leg
//! of `almide test`) budgets an 8 MiB wasm call stack (`EMBEDDED_WASM_STACK`
//! in almide-wasm-run), the size of native's main-thread stack. On wasmtime's
//! 512 KiB default these recursions trapped `call stack exhausted` near depth
//! 5,400 while native answered; with the budget they reach ~87,000 (native
//! ~43,000 and ~32,500; measured 2026-10-06, macOS aarch64). Depth 15,000
//! sits ~3x past the old trap and ~2x inside native's limit, so neither leg
//! is near its edge on Linux or macOS. Recursion past a leg's budget stays
//! C-196's contracted resource limit; this pins where the embedded lane's
//! limit is, not a cross-target promise.

use std::process::Command;

const DEPTH: &str = "15000";

/// #3434's shape: a non-tail tree walk returning `T!`.
const INFER: &str = r#"import env

type T =
  | Leaf
  | Wrap(T, T)
  | Node(T, T)

fn build(n: Int) -> T = {
  var t = Leaf
  var i = 0
  while i < n {
    t = Node(t, Leaf)
    i = i + 1
  }
  t
}

fn describe(t: T) -> String = match t {
  Leaf => "leaf",
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

effect fn main() -> Unit = {
  let n = int.parse(list.last(env.args()) ?? "10") ?? 10
  let r = infer(build(n)) ?? Leaf
  println("${int.to_string(n)}: ${describe(r)}")
}
"#;

/// A recursion with several heap locals live across the call.
const LOCALS: &str = r#"import env

fn walk(n: Int) -> Int = {
  if n == 0 then 0
  else {
    let s = "frame-${int.to_string(n)}"
    let xs = [n, n + 1, n + 2]
    let t = s + "!"
    let ys = xs + [string.len(t)]
    let r = walk(n - 1)
    r + string.len(s) + list.len(ys) + string.len(t)
  }
}

effect fn main() -> Unit = {
  let n = int.parse(list.last(env.args()) ?? "10") ?? 10
  println("${int.to_string(n)}: ${int.to_string(walk(n))}")
}
"#;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let release = root.join("target/release/almide");
    if release.exists() {
        return release.to_str().unwrap().to_string();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

fn run(src: &std::path::Path, target: &str) -> (Option<i32>, String, String) {
    let out = Command::new(almide_bin())
        .args(["run", "--target", target])
        .arg(src)
        .args(["--", DEPTH])
        .output()
        .expect("spawn almide run");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_deep_recursion_matches_native(name: &str, program: &str, expected: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join(format!("{name}.almd"));
    std::fs::write(&src, program).expect("write probe");
    let native = run(&src, "rust");
    let wasm = run(&src, "wasm");
    assert_eq!(native, (Some(0), expected.to_string(), String::new()), "native leg of {name}");
    assert_eq!(
        wasm, native,
        "{name} at depth {DEPTH}: the embedded wasm host must answer as native does (#3435)"
    );
}

#[test]
fn non_tail_tree_walk_fits_the_embedded_stack() {
    assert_deep_recursion_matches_native("infer", INFER, "15000: leaf\n");
}

#[test]
fn recursion_with_heap_locals_fits_the_embedded_stack() {
    assert_deep_recursion_matches_native("locals", LOCALS, "15000: 382788\n");
}
