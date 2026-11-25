//! C-461 (#4006): an `@extern` fn with no native binding and no Almide body
//! (`@extern(wasm, "js", ...)` alone) has nothing a native binary could run.
//! The native routes refuse a program whose native roots reach one before the
//! render (src/cli/native_extern_gate.rs). One its roots never reach — the
//! helper of a wasm export the native `main` does not call — is dropped here
//! with every fn that can reach it, so the render neither walls on its import
//! nor ships an abort for it. A reached one is left in place: its import
//! walls the render, so a caller that skipped the refusal still gets no
//! binary. A test build drops nothing: its tests are its roots.
use almide_ir::{IrExprKind, IrFunction, IrProgram};
use std::collections::HashSet;

fn unbound(f: &IrFunction) -> bool {
    !f.extern_attrs.is_empty()
        && crate::native_foreign::selected(f).is_none()
        && matches!(f.body.kind, IrExprKind::Hole)
}

/// Drop the program's own unbound externs the native program does not reach,
/// and every unreached fn of the program that calls one (transitively). Runs
/// before the link, so the library bodies only those fns called are never
/// linked.
pub(crate) fn prune_unreached(ir: &mut IrProgram) {
    // A test build's roots are its tests, which keep their typed holes
    // (`native_test_panic`): nothing is dropped there.
    if crate::native_test_harness::active() { return; }
    let mut tainted: HashSet<String> = ir.functions.iter().filter(|f| unbound(f)).map(|f| f.name.to_string()).collect();
    if tainted.is_empty() { return; }
    let calls: Vec<(String, HashSet<String>)> = ir.functions.iter()
        .map(|f| (f.name.to_string(), almide_codegen::reachability::called_names(&f.body)))
        .collect();
    loop {
        let before = tainted.len();
        for (name, called) in &calls {
            if !tainted.contains(name) && called.iter().any(|c| tainted.contains(c)) {
                tainted.insert(name.clone());
            }
        }
        if tainted.len() == before { break; }
    }
    let reached = almide_codegen::reachability::reachable_native_program(ir);
    ir.functions.retain(|f| !tainted.contains(f.name.as_str()) || reached.contains(f.name.as_str()));
}
