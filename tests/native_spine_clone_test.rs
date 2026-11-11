//! #4661: a recursive variant value is copied along its chain in a loop
//! (`render_native_spine_clone`), and a payload is not moved out of a value
//! while a sibling payload of it is still lent (`render_native_addr`).
//!
//! - A function that takes a long chain by value copies it per call. The
//!   derived copy recursed once per level; the loop copy builds the same
//!   value, one block per level, in a constant number of frames.
//! - `Link(name, xs, rest) => total(rest, k + list.len(xs))` moved `rest` out
//!   of the block `xs` was still a slice of: the render was refused by the
//!   Rust compiler.
//!
//! Each program prints the same text on the native render (the ownership
//! certificate required, rendered by the certified render itself) and on
//! wasm; the shape of the rendered copy and the blocks a copy allocates are
//! asserted, never a wall-clock time.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-spine-clone-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, the certificate required.
fn run(src: &Path, target: Option<&str>, alloc: bool) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    if alloc {
        cmd.env("ALMIDE_ALLOC_COUNT", "1");
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The counting allocator's `allocs` for `source`.
fn allocs(tag: &str, source: &str) -> u64 {
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    let (_, stderr) = run(&src, None, true);
    let _ = std::fs::remove_dir_all(&dir);
    let line = stderr.lines().find(|l| l.starts_with("__ALMD_ALLOC ")).unwrap_or_else(|| panic!("no allocation report:\n{stderr}"));
    line.split_whitespace().find_map(|kv| kv.strip_prefix("allocs=")).and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("malformed report `{line}`"))
}

/// `source` prints `expected` on the native render and on wasm, and the
/// native build is the certified render's own.
fn prints(tag: &str, source: &str, expected: &str) {
    if let Err(e) = almide_mir::pipeline::try_render_rust_source(source) {
        panic!("{tag} left the certified render: {e:?}");
    }
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target, false);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A chain with a branching and a string leaf, a search tree, an expression
/// type with no case without fields, and a list whose links hold a string
/// and a list beside the rest of the chain.
const SHAPES: &str = r#"type Level = | LZero | LSucc(Level) | LMax(Level, Level) | LParam(String)
type Tree = | Leaf | Node(Tree, Int, Tree)
type Expr = | Num(Int) | Add(Expr, Expr) | Neg(Expr)
type Chain = | End | Link(String, List[Int], Chain)

fn mk(n: Int, acc: Level) -> Level = if n == 0 then acc else mk(n - 1, LSucc(acc))

fn offset(l: Level, k: Int) -> (Level, Int) = match l {
  LSucc(a) => offset(a, k + 1),
  other => (other, k),
}

fn show(l: Level) -> String = match l {
  LZero => "0",
  LSucc(a) => "S" + show(a),
  LMax(a, b) => "M(" + show(a) + "," + show(b) + ")",
  LParam(s) => s,
}

fn grow(t: Tree, v: Int) -> Tree = match t {
  Leaf => Node(Leaf, v, Leaf),
  Node(l, x, r) => if v < x then Node(grow(l, v), x, r) else Node(l, x, grow(r, v)),
}

fn walk(t: Tree) -> List[Int] = match t {
  Leaf => [],
  Node(l, x, r) => walk(l) + [x] + walk(r),
}

fn eval(e: Expr) -> Int = match e {
  Num(n) => n,
  Add(a, b) => eval(a) + eval(b),
  Neg(a) => 0 - eval(a),
}

fn link(n: Int, acc: Chain) -> Chain = if n == 0 then acc else link(n - 1, Link("n" + int.to_string(n), [n, n * 2], acc))

fn total(c: Chain, k: Int) -> Int = match c {
  End => k,
  Link(name, xs, rest) => total(rest, k + string.len(name) + list.len(xs) + list.fold(xs, 0, (a, x) => a + x)),
}

effect fn main() -> Unit = {
  let deep = mk(DEPTH, LMax(LSucc(LZero), LParam("u")))
  var sum = 0
  for i in 0..<3 {
    let (base, k) = offset(deep, i)
    sum = sum + k + string.len(show(base))
  }
  println("${sum} ${string.len(show(deep))}")
  let small = mk(3, LMax(LMax(LZero, LParam("p")), LSucc(LParam("q"))))
  let (b1, k1) = offset(small, 0)
  println("${show(small)} ${show(b1)} ${k1} ${small == mk(3, LMax(LMax(LZero, LParam("p")), LSucc(LParam("q"))))}")
  let t = list.fold([5, 2, 8, 1, 9, 3, 7], Leaf, (acc, v) => grow(acc, v))
  let t2 = grow(t, 4)
  println("${walk(t)} ${walk(t2)} ${t == t2}")
  let e = Add(Neg(Num(3)), Add(Num(10), Neg(Neg(Num(4)))))
  let e2 = Neg(e)
  println("${eval(e)} ${eval(e2)} ${eval(e)}")
  let c = link(40, End)
  println("${total(c, 0)} ${total(c, 1)}")
}
"#;

fn shapes(depth: usize) -> String {
    SHAPES.replace("DEPTH", &depth.to_string())
}

#[test]
fn copies_of_recursive_values_print_the_same_on_both_targets() {
    // Each value is read again after a call that took a copy of it.
    prints(
        "shapes",
        &shapes(5000),
        "15024 5007\nSSSM(M(0,p),Sq) M(M(0,p),Sq) 3 true\n[1, 2, 3, 5, 7, 8, 9] [1, 2, 3, 4, 5, 7, 8, 9] false\n11 -11 11\n2651 2652\n",
    );
}

#[test]
fn a_type_with_a_fieldless_case_and_an_own_last_field_gets_the_loop_copy() {
    let rust = almide_mir::pipeline::try_render_rust_source(&shapes(10)).expect("the certified render");
    // `Level`, `Tree` and `Chain` descend their last field in a loop.
    assert_eq!(rust.matches("impl Clone for AlmdRec").count(), 3, "{rust}");
    assert_eq!(rust.matches("dst = match dst {").count(), 4, "{rust}");
    for name in ["Level", "Tree", "Chain"] {
        assert!(rust.contains(&format!("/// variant `{name}`\n#[derive(PartialEq)]\nenum")), "{name}: {rust}");
    }
    // `Expr` has no case without fields to stand in the hole: derived.
    assert!(rust.contains("/// variant `Expr`\n#[derive(Clone, PartialEq)]\nenum"), "{rust}");
}

#[test]
fn a_copy_of_a_chain_allocates_one_block_per_level() {
    // Three calls copy the chain: twice the depth is twice the blocks, with
    // nothing added per copy but the levels themselves.
    let (small, large) = (allocs("chain-2000", &shapes(2000)), allocs("chain-4000", &shapes(4000)));
    assert!(large > small && large - small < 2 * small, "{small} blocks at depth 2000, {large} at 4000");
}
