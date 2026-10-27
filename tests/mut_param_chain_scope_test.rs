//! #3891: a `mut` param a fused chain's lambda writes is a unique reborrow of
//! the caller's place, not a write-back cell. The chain's step / collector
//! lambdas render as non-`move` scopes the chain borrows for its own
//! duration, so a fold over `marks` that calls `draw_glyph(r, …)` needs no
//! copy of `r` in and out per call — that copy was most of a frame's time in
//! a 40-field renderer. A chain that would hold two borrows of the param at
//! once (two lambdas, or the source reading it) keeps the cell. Every shape
//! prints the same bytes on native and wasm.

use std::path::Path;
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

const SRC: &str = r#"type Big = { m: Map[String, Int], xs: List[Int], n: Int }

fn bump(mut r: Big, x: Int) -> Int = {
  r.n = r.n + x
  r.n
}

fn draw(mut r: Big, marks: List[Int]) -> Int =
  list.fold(marks, bump(r, 1), (acc, x) => acc + bump(r, x))

fn any_big(mut r: Big, xs: List[Int]) -> Bool = list.any(xs, (x) => bump(r, x) > 10)

fn bumped(mut r: Big, xs: List[Int]) -> List[Int] = xs |> list.map((x) => bump(r, x))

fn from_own(mut r: Big) -> Int = list.fold(r.xs, 0, (a, x) => a + bump(r, x))

fn show(r: Big) -> String = int.to_string(r.n)

effect fn main() -> Unit = {
  var b = { m: map.from_list([("a", 1)]), xs: [1, 2, 3], n: 0 }
  println(int.to_string(draw(b, [2, 3])) + " " + show(b))
  println(int.to_string(draw(b, [])) + " " + show(b))
  println(bool.to_string(any_big(b, [1, 5, 2])) + " " + show(b))
  println((bumped(b, [1, 1]) |> list.map((x) => int.to_string(x)) |> list.join(",")) + " " + show(b))
  println(int.to_string(from_own(b)) + " " + show(b))
}
"#;

const WANT: &str = "10 6\n7 7\ntrue 13\n14,15 15\n55 21\n";

fn run(dir: &Path, wasm: bool) -> (i32, String, String) {
    let mut args = vec!["run", "p.almd"];
    if wasm {
        args.extend(["--target", "wasm"]);
    }
    let o = Command::new(almide()).current_dir(dir).args(&args).output().expect("almide run");
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

/// The emitted Rust of `pub fn <name>(` up to the next top-level fn.
fn rust_fn<'a>(rust: &'a str, name: &str) -> &'a str {
    let head = format!("pub fn {name}(");
    let start = rust.find(&head).unwrap_or_else(|| panic!("no `{head}` in the emitted Rust"));
    let rest = &rust[start + head.len()..];
    let end = rest.find("\npub fn ").map(|i| start + head.len() + i).unwrap_or(rust.len());
    &rust[start..end]
}

#[test]
fn a_chain_lambda_writing_a_mut_param_agrees_on_both_legs() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("p.almd"), SRC).unwrap();
    let native = run(dir.path(), false);
    assert_eq!((native.0, native.1.as_str()), (0, WANT), "native: {native:?}");
    if wasmtime_available() {
        let w = run(dir.path(), true);
        assert_eq!(w, native, "wasm differs from native");
    } else {
        eprintln!("NOTE mut-param chain scope: no `wasmtime` on PATH — the wasm leg is skipped here");
    }
}

#[test]
fn a_chain_lambda_writing_a_mut_param_copies_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("p.almd"), SRC).unwrap();
    let o = Command::new(almide()).current_dir(dir.path()).args(["p.almd", "--target", "rust"]).output().expect("emit");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let rust = String::from_utf8_lossy(&o.stdout).into_owned();
    for name in ["draw", "any_big", "bumped"] {
        let f = rust_fn(&rust, name);
        assert!(!f.contains("AlmideWriteBack(") && !f.contains("AlmideSharedMut::new("),
            "`{name}`'s chain lambda is a scope; its `mut` param needs no cell:\n{f}");
    }
    // The source reads the param while the lambda writes it: two borrows at
    // once, so this one keeps its cell.
    assert!(rust_fn(&rust, "from_own").contains("AlmideWriteBack("), "from_own lost its cell");
}
