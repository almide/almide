//! `fan.map` on threads on the certified native render (#4006 family H).
//!
//! The MIR keeps `fan.map(xs, f)` as the certified SEQUENTIAL call the
//! lowering routes it to: the self-host body the registry declares for the
//! call (stdlib/fan_map.almd), lowered, verified and witnessed like any
//! other. The render emits that call as the standard codegen's parallel twin
//! instead — `almide_rt_fan_map_par` and the dealer it runs on, the shipped
//! runtime text verbatim (`almide_codegen::fan_map_parallel_runtime_text`) —
//! with each element calling the certified lambda through its typed dispatch
//! table. The TRUSTED step is proofs/TRUSTED_BASE.md's "fan arms on threads"
//! row: that the parallel twin is observation-equal to the certified
//! sequential body (ADR-0024 D1/D5/D6).
//!
//! Which calls take the twin comes from the declarations, never from a list
//! of names: the lift ([`admit`]) reads a `fan` module call's argument and
//! result types; a call that maps a function over a list (a `List[A]`
//! argument, a function `A -> Result[B, String]`, the result
//! `Result[List[B], String]`) has the route the lowering computes for those
//! types, and the self-host registry's declared row for that route names the
//! body the twin stands for ([`registry_impl`]).
//!
//! The PRECONDITIONS, each checked by [`admit`] (a call that fails one
//! declines the program, and the standard codegen builds it):
//! - the function is a lambda literal whose captures are immutable locals of
//!   a scalar type (`Int`, `Float`, `Bool`): the closure block the elements
//!   share is a slab of plain words;
//! - the element and result types may cross a thread
//!   ([`crate::fan_native::thread_crossable`]);
//! - the lowering routes the call to a body the registry declares.

use almide_ir::{IrExpr, IrExprKind, IrProgram, Mutability, VarId};
use almide_lang::types::constructor::TypeConstructorId as TC;
use almide_lang::types::Ty;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::MirFunction;

thread_local! {
    static TWINS: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
}

pub(crate) fn reset() {
    TWINS.with(|t| t.borrow_mut().clear());
}

/// Is `name` a self-host body the last lift admitted a parallel twin for?
pub(crate) fn is_twin(name: &str) -> bool {
    TWINS.with(|t| t.borrow().contains(name))
}

/// The self-host body the registry declares for the route the lowering takes
/// for a `fan.map` call of these types, or `None` (an unregistered pairing).
pub(crate) fn registry_impl(arg_tys: &[Ty], result_ty: &Ty) -> Option<&'static str> {
    let route = crate::lower::fan_map_call_name(arg_tys, result_ty);
    crate::mir_ops::self_host_runtime()
        .iter()
        .flat_map(|(_, pairs)| pairs.iter())
        .find(|(_, call)| *call == route)
        .map(|(impl_fn, _)| *impl_fn)
}

/// `Result[T, String]`'s `T`.
fn ok_of(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Applied(TC::Result, a) if a.len() == 2 && a[1] == Ty::String => Some(&a[0]),
        _ => None,
    }
}

fn list_elem(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Applied(TC::List, a) if a.len() == 1 => Some(&a[0]),
        _ => None,
    }
}

/// Is the `fan` module call `args -> result_ty` a map of a function over a
/// list? `Some(why)` when it is one this render cannot thread; `Ok` admits
/// its twin. `Err(None)`: not a map (the caller keeps its own rule).
pub(crate) fn admit(
    args: &[IrExpr],
    result_ty: &Ty,
    ir: &IrProgram,
    globals: &HashSet<VarId>,
) -> Result<(), Option<String>> {
    let [xs, f] = args else { return Err(None) };
    let Some(a) = list_elem(&xs.ty) else { return Err(None) };
    let Ty::Fn { params, ret, .. } = &f.ty else { return Err(None) };
    let Some(b) = ok_of(ret) else { return Err(None) };
    if params.len() != 1 || params[0] != *a {
        return Err(None);
    }
    // The map's own shape: `Result[List[B], String]`.
    if ok_of(result_ty).and_then(list_elem) != Some(b) {
        return Err(None);
    }
    let decls = &ir.type_decls;
    if !crate::fan_native::thread_crossable(a, decls, 0) || !crate::fan_native::thread_crossable(b, decls, 0) {
        return Err(Some(format!("a fan.map over `{a:?}` to `{b:?}`, which cannot cross a thread")));
    }
    let IrExprKind::Lambda { params: lp, body, .. } = &f.kind else {
        return Err(Some("a fan.map whose function is not a lambda literal".into()));
    };
    let bound: HashSet<VarId> = lp.iter().map(|(v, _)| *v).collect();
    for id in almide_ir::free_vars::free_vars(body, &bound) {
        let info = ir.var_table.get(id);
        if info.mutability == Mutability::Var {
            return Err(Some(format!("a fan.map lambda that captures the `var` `{}`", info.name)));
        }
        if globals.contains(&id) {
            return Err(Some(format!("a fan.map lambda that reads the top-level `{}`", info.name)));
        }
        if !matches!(info.ty, Ty::Int | Ty::Float | Ty::Bool) {
            return Err(Some(format!("a fan.map lambda that captures `{}` of a non-scalar type", info.name)));
        }
    }
    let Some(impl_fn) = registry_impl(&[xs.ty.clone(), f.ty.clone()], result_ty) else {
        return Err(Some(format!("a fan.map from `{a:?}` to `{b:?}` with no registered body")));
    };
    TWINS.with(|t| t.borrow_mut().insert(impl_fn.to_string()));
    Ok(())
}

/// The bodies only a twinned call reaches: dead in the render (the twin runs
/// in their place) and still lowered, verified and witnessed. A body any
/// other function calls stays live.
pub(crate) fn twin_only<'a>(fns: impl IntoIterator<Item = &'a MirFunction>) -> BTreeSet<String> {
    let fns: Vec<&MirFunction> = fns.into_iter().collect();
    let calls: BTreeMap<&str, Vec<&str>> = fns
        .iter()
        .map(|f| {
            let callees = f
                .ops
                .iter()
                .filter_map(|op| match op {
                    crate::Op::CallFn { name, .. } | crate::Op::FuncRef { name, .. } => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            (f.name.as_str(), callees)
        })
        .collect();
    let reach = |from: Vec<&str>, stop: &dyn Fn(&str) -> bool| {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut todo = from;
        while let Some(n) = todo.pop() {
            if !seen.insert(n.to_string()) {
                continue;
            }
            for c in calls.get(n).into_iter().flatten() {
                if !stop(c) {
                    todo.push(c);
                }
            }
        }
        seen
    };
    let twins: Vec<&str> = fns.iter().map(|f| f.name.as_str()).filter(|n| is_twin(n)).collect();
    let under = reach(twins, &|_| false);
    let roots: Vec<&str> = fns.iter().map(|f| f.name.as_str()).filter(|n| !under.contains(*n)).collect();
    // From every other function, a call of a twin is the twin itself.
    let live = reach(roots, &|c| is_twin(c));
    under.into_iter().filter(|n| !live.contains(n)).collect()
}


#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = "effect fn step(x: Int, k: Int) -> Result[Int, String] = ok(x * k)\n\
                       effect fn main() -> Unit = {\n  let k = 3\n  let ys = fan.map([1, 2, 3], (x) => step(x, k))!\n  println(int.to_string(list.len(ys)))\n}\n";

    fn lift(src: &str) -> Result<IrProgram, crate::lower::LowerError> {
        crate::fan_native::lift_fan_arms(crate::pipeline::source_to_ir_for_tests(src).expect("ir"))
    }

    #[test]
    fn a_map_over_scalar_captures_takes_the_registered_twin() {
        lift(MAP).expect("admitted");
        let twins: Vec<String> = TWINS.with(|t| t.borrow().iter().cloned().collect());
        assert_eq!(twins.len(), 1, "{twins:?}");
        let declared = crate::mir_ops::self_host_runtime()
            .iter()
            .flat_map(|(_, pairs)| pairs.iter())
            .any(|(impl_fn, call)| *impl_fn == twins[0] && call.starts_with("fan."));
        assert!(declared, "{} is a body the registry declares for a fan call", twins[0]);
    }

    // The precondition (TRUSTED_BASE "fan arms on threads"): the closure block
    // the elements share is a slab of plain words, so a capture of a heap
    // value declines the program.
    #[test]
    fn a_map_capturing_a_heap_value_is_refused() {
        let src = "effect fn step(x: Int, s: String) -> Result[Int, String] = ok(x + string.len(s))\n\
                   effect fn main() -> Unit = {\n  let s = \"ab\"\n  let ys = fan.map([1, 2], (x) => step(x, s))!\n  println(int.to_string(list.len(ys)))\n}\n";
        let e = lift(src).expect_err("a heap capture is refused");
        assert!(e.to_string().contains("of a non-scalar type"), "{e}");
    }

    #[test]
    fn a_map_capturing_a_var_is_refused() {
        let ir = crate::pipeline::source_to_ir_for_tests(MAP).expect("ir");
        let mut ir = ir;
        for v in ir.var_table.entries.iter_mut().filter(|v| v.name.as_str() == "k") {
            v.mutability = Mutability::Var;
        }
        let e = crate::fan_native::lift_fan_arms(ir).expect_err("a var capture is refused");
        assert!(e.to_string().contains("captures the `var`"), "{e}");
    }

    // The twin's bodies are dead in the render and still JUDGED: every one
    // is in the certificate set proofs/native-wall.sh checks, and none is in
    // the Rust source.
    #[test]
    fn the_bodies_a_twin_replaces_are_certified_and_not_rendered() {
        let (rust, certs) = crate::pipeline::try_render_rust_source_certified(MAP).expect("renders");
        assert!(rust.contains("almide_v1_fan_map("), "the call renders as the twin");
        let names: Vec<&str> = certs.iter().map(|(n, _)| n.as_str()).collect();
        for body in ["fan_map", "__fan_map_go"] {
            assert!(names.contains(&body), "`{body}` is in the certificate set: {names:?}");
            assert!(!rust.contains(&format!("fn {}(", crate::render_native::mangle(body))), "`{body}` is not rendered");
        }
    }
}
