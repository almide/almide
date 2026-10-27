//! A `value.Path` built step by step is linear on the wasm leg.
//!
//! `value.Path` is the transparent newtype `mod type Path = List[(Bool,
//! String, Int)]` and `value.key` / `value.index` are `match p { Path(steps)
//! => Path(steps + [step]) }`. Erased (newtype.rs), the body is `{ let steps =
//! p; steps + [step] }`: the bind is `p`'s last read and the push is the
//! frame's tail, so with `p` owned the credit moves `p` → `steps` → the push,
//! which grows the block in place at rc 1 (#3530). With `p` borrowed the
//! caller kept its credit on the block and `$cow` copied the whole path on
//! every step: a 50,000-step chain ran out of memory on wasm while native took
//! milliseconds. param_borrow.rs now owns a param whose credit reaches a
//! consuming op at a result leaf, through such binds.
//!
//! Measured through the product route (`almide run --target wasm`) at n and
//! 4n: the bytes requested must stay well under the quadratic 16x (linear
//! lands near 4x), the wall time likewise, nothing may leak, and the answers
//! — including a path kept and extended twice, which must see value
//! semantics — agree with native.
use std::process::Command;
use std::time::{Duration, Instant};

fn program(step: &str, steps: u32) -> String {
    format!(
        r#"fn build(n: Int) -> value.Path = list.range(0, n) |> list.fold(
  value.root(),
  (acc, _i) => {step},
)

effect fn main() -> Unit = {{
  let doc = value.object([("a", value.object([("b", value.int(1)), ("c", value.int(2))]))])
  let a = value.key(value.root(), "a")
  let ab = value.key(a, "b")
  let ac = value.key(a, "c")
  let ab_v = value.get_path(doc, ab) ?? value.null()
  let ac_v = value.get_path(doc, ac) ?? value.null()
  let a_some = option.is_some(value.get_path(doc, a))
  println("${{value.as_int(ab_v) ?? -1}} ${{value.as_int(ac_v) ?? -1}} ${{a_some}}")
  let p = build({steps})
  println("${{option.is_some(value.get_path(doc, p))}}")
}}
"#
    )
}

/// stdout, the bytes `$alloc` was asked for, the live count, and the wall
/// time of a warm run (the first run fills the build cache).
fn run_wasm(src: &str) -> (String, u64, u64, Duration) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("path.almd");
    std::fs::write(&file, src).expect("source");
    let go = || {
        let t = Instant::now();
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("run")
            .arg(&file)
            .args(["--target", "wasm"])
            .env("ALMIDE_WASM_ALLOC_COUNT", "1")
            .output()
            .expect("wasm run");
        (out, t.elapsed())
    };
    let _ = go();
    let (out, took) = go();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{stderr}");
    let field = |k: &str| -> u64 {
        stderr
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {stderr}"))
    };
    (String::from_utf8_lossy(&out.stdout).to_string(), field("bytes="), field("live="), took)
}

fn run_native(src: &str) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("path.almd");
    std::fs::write(&file, src).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).output().expect("native run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn a_50000_step_path_chain_is_linear_on_wasm() {
    const N: u32 = 12_500;
    for step in ["value.key(acc, \"k\")", "value.index(acc, 0)"] {
        let (small_out, small, small_live, small_t) = run_wasm(&program(step, N));
        let (large_out, large, large_live, large_t) = run_wasm(&program(step, 4 * N));
        let native = run_native(&program(step, 4 * N));
        assert_eq!(small_out, "1 2 true\nfalse\n", "{step}: wasm at n");
        assert_eq!(large_out, native, "{step}: native and wasm differ at 4n");
        assert_eq!((small_live, large_live), (0, 0), "{step}: the path leaks");
        let bytes = large as f64 / small as f64;
        eprintln!("{step}: n={N} {small} B {small_t:?}; 4n {large} B {large_t:?} ({bytes:.2}x bytes)");
        assert!(bytes < 6.0, "{step}: 4x the steps requested {large} B against {small} B ({bytes:.1}x) — each step copies the path");
        assert!(
            large_t < small_t * 8 + Duration::from_millis(500),
            "{step}: 4x the steps took {large_t:?} against {small_t:?} — quadratic again?"
        );
    }
}
