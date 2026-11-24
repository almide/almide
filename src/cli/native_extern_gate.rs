//! C-461 (#4006, owner ruling 2026-11-24 W1): an `@extern` fn with no binding
//! for the native target and no Almide body has nothing a native binary could
//! run. Declared only for another target (`@extern(wasm, "js", ...)`, the JS
//! host's hook), it lowered to a typed hole the native build compiled into an
//! abort, and that build succeeded through the unverified codegen. The native
//! routes (`build`, `run`, `bench`, `verify --mir`, and `check --target
//! native`, which reports their verdict) refuse such a fn at check time
//! instead, with E081, before any render, when the program's native roots reach it
//! (`reachability::reachable_native_program`: `main`, `@export(c)` fns,
//! top-level `let` initializers). A fn that also declares `@extern(rust, ...)`
//! or `@extern(c, ...)` binds through that; one with an Almide body runs the
//! body natively. Plain `almide check` stays the language check every target
//! shares, so a program written for the JS host checks clean; `--target wasm`
//! / `--target wasi` judge their own routes.

use almide::ir::{IrExprKind, IrFunction, IrProgram};

/// The fns `ir`'s native program reaches that have no native implementation,
/// as `(name, declared attrs, line in the entry file)`, in declaration order
/// (a module's fn names its module instead of a line of the entry).
fn unbound(ir: &IrProgram) -> Vec<(String, String, Option<usize>)> {
    let reachable = almide::codegen::reachability::reachable_native_program(ir);
    let is_reachable = |module: Option<&str>, name: &str| {
        almide::codegen::reachability::registered_keys(module, name).iter().any(|k| reachable.contains(k))
    };
    let has_no_native_impl = |f: &IrFunction| {
        !f.extern_attrs.is_empty()
            && !f.extern_attrs.iter().any(|a| almide::types::extern_abi::native_extern_kind(a.target.as_str()).is_some())
            && matches!(f.body.kind, IrExprKind::Hole)
    };
    let attrs = |f: &IrFunction| {
        f.extern_attrs
            .iter()
            .map(|a| format!("@extern({}, \"{}\", \"{}\")", a.target.as_str(), a.module.as_str(), a.function.as_str()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let top = ir.functions.iter().filter(|f| has_no_native_impl(f) && is_reachable(None, f.name.as_str())).map(|f| (f.name.to_string(), attrs(f), f.body.span.map(|s| s.line)));
    let modules = ir.modules.iter().flat_map(|m| {
        let mname = m.name.to_string();
        m.functions
            .iter()
            .filter(move |f| has_no_native_impl(f) && is_reachable(Some(&mname), f.name.as_str()))
            .map(move |f| (format!("{}.{}", m.name.as_str(), f.name.as_str()), attrs(f), None))
    });
    top.chain(modules).collect()
}

/// The refusals for `ir` on the native target, one `error[E081]` text per fn.
pub(crate) fn refusals(ir: &IrProgram, file: &str) -> Vec<(&'static str, String)> {
    unbound(ir)
        .into_iter()
        .map(|(name, attrs, line)| {
            let at = line.map(|l| format!("\n  --> {file}:{l}")).unwrap_or_default();
            ("E081", format!(
                "error[E081]: `{name}` has no implementation on the native target{at}\n  \
                 reason: it is declared only {attrs} and has no Almide body, so a native binary has nothing to run when it is called\n  \
                 try: build it for its host (`almide build {file} --target wasm --host js`), add an `@extern(rust, ...)` or `@extern(c, ...)` binding, or give the fn an Almide body the native build runs\n  \
                 note: C-461; the wasm route (`almide check --target wasm`) binds the import"
            ))
        })
        .collect()
}

/// Print `ir`'s native refusals; `Err` when there was one.
pub(crate) fn check(ir: &IrProgram, file: &str) -> Result<(), ()> {
    let rows = refusals(ir, file);
    for (_, text) in &rows {
        crate::err(text);
    }
    if rows.is_empty() { Ok(()) } else { Err(()) }
}
