//! #4385: the certified native render runs the shared accumulator
//! tail-recursion rewrite (`almide_ir::accum_tre`, #2577) like the standard
//! codegen and the wasm leg, so a binary `Int` recursion makes ONE recursive
//! call per non-leaf. Before the fix the render kept both calls and ran
//! `tri(n - 1) + tri(n - 3)` 1.24x slower than the standard codegen.
//!
//! Negative control (GCE, 2026-11-01): with the rewrite call removed from
//! `pipeline_native_rungs.rs`, `rendered_tri_recurses_once_per_node` fails
//! (two `almd_tri(` calls in the body) and the shape bench
//! research/benchmark/perf/v1-shapes/binary_recursion.almd reads higher in
//! instruction count against the standard codegen.

fn rendered_fn(src: &str, name: &str) -> String {
    let rust = almide_mir::pipeline::try_render_rust_source(src).expect("renders on the certified path");
    let head = format!("fn {name}(");
    let at = rust.find(&head).unwrap_or_else(|| panic!("no `{head}` in the render"));
    let body = &rust[at..];
    let end = body.find("\n}\n").expect("fn body ends");
    body[..end].to_string()
}

/// Since #4456 a recursion small enough for the one-level self-inlining
/// (`almide_ir::self_inline`) takes that rewrite instead; a body over its
/// node budget — the base case here is a 30-factor product — still gets the
/// accumulator loop, which is what this test pins.
#[test]
fn rendered_tri_recurses_once_per_node() {
    let base = std::iter::repeat_n("n", 30).collect::<Vec<_>>().join(" * ");
    let src = format!("fn tri(n: Int) -> Int = if n <= 1 then {base} - 3 else tri(n - 1) + tri(n - 3)\n\nfn main() -> Unit = println(int.to_string(tri(20)))\n");
    let src = src.as_str();
    let body = rendered_fn(src, "almd_tri");
    let calls = body.matches("almd_tri(").count() - 1; // minus the signature
    assert_eq!(calls, 1, "one recursive call per node, the other folded into a loop:\n{body}");
    assert!(body.contains("loop {"), "the accumulator loop:\n{body}");
}

#[test]
fn a_subtraction_recursion_keeps_both_calls() {
    // The rewrite reassociates; `-` is neither associative nor commutative, so
    // it must not fire (the preconditions admit only Int `+` and `*`). The
    // base is over the self-inlining budget, as above, so the accumulator
    // rewrite is the only one that could take it.
    let base = std::iter::repeat_n("n", 30).collect::<Vec<_>>().join(" * ");
    let src = format!("fn g(n: Int) -> Int = if n < 2 then {base} else g(n - 1) - g(n - 2)\n\nfn main() -> Unit = println(int.to_string(g(20)))\n");
    let body = rendered_fn(&src, "almd_g");
    assert_eq!(body.matches("almd_g(").count() - 1, 2, "{body}");
}
