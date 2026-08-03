//! The `return_call_indirect` instruction gate (C-178's indirect twin).
//!
//! A function-tail CLOSURE call must transfer its frame exactly like the
//! named `return_call` (#864): the classification is shared
//! (`tail_call_indexes` admits `Op::CallIndirect` in the same
//! reaches-ret-unmodified shape), and this gate pins the ENCODING — the
//! C-178 fixture's `step` renders its `next(k - 1)` as
//! `return_call_indirect`, and the lifted lambda's `spin(j)` as
//! `return_call`. Depth-based proof is not available cross-target (the
//! creator hop legitimately keeps one frame per round — see the fixture
//! header), so the instruction assertion is the machine check.

#[test]
fn function_tail_closure_calls_render_as_return_call_indirect() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("spec/wasm_cross/closure_tail_recursion.almd"))
        .expect("read the C-178 indirect fixture");
    let modules = almide_mir::pipeline::bundled_self_modules(&source);
    let wat = almide_mir::pipeline::try_render_wasm_source(&source, &modules, false)
        .expect("the fixture must render on the wasm leg");
    let code: String = wat
        .lines()
        .map(|l| l.split(";;").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        code.contains("return_call_indirect"),
        "step's function-tail closure call must render as return_call_indirect"
    );
    assert!(
        code.contains("return_call $spin"),
        "the lifted lambda's named tail call must render as return_call"
    );
}
