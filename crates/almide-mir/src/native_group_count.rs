//! `map.map(list.group_by(xs, key), (g) => list.len(g))` as ONE counting pass
//! on the native render: `list.__group_count(xs, key)`. Spelled literally the
//! chain copies every element for its key, pushes it into its group, then
//! copies each group again only to measure it. The standard render applies
//! the same rule (`pass_peephole.rs`), the wasm render its own
//! (`almide-wasm/src/group_count.rs`).
//!
//! Equivalence with the two-step form. `key` runs once per element, in order,
//! either way, and both forms keep the keys in first-seen order. The callback
//! that disappears is exactly `list.len` of its one parameter: it reads
//! nothing else, has no effect and cannot abort. The group map is a temporary
//! of the outer call, so nothing else reads the groups. A key type other than
//! `String` / `Int`, a key that is not a lambda literal, and a second callback
//! of any other body keep the two-step form.
use almide_ir::{visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::{intern::sym, types::{Ty, TypeConstructorId as C}};

pub(crate) fn expand(ir: &mut IrProgram) {
    // A metered region charges what its plan names, and the plan names the
    // two calls of the source: such a program keeps them.
    if almide_ir::meter_plan::program_has_regions(ir) { return; }
    for f in &mut ir.functions { Count.visit_expr_mut(&mut f.body); }
}

struct Count;

impl IrMutVisitor for Count {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let Some(args) = counted_groups(e) else { return; };
        let target = CallTarget::Module { module: sym("list"), func: sym("__group_count"), def_id: None };
        e.kind = IrExprKind::Call { target, args, type_args: Vec::new() };
    }
}

/// The `[xs, key]` of the `list.group_by` whose groups `e` only counts.
fn counted_groups(e: &IrExpr) -> Option<Vec<IrExpr>> {
    let [groups, count] = module_call(e, "map", "map")? else { return None; };
    let IrExprKind::Lambda { params, body, .. } = &count.kind else { return None; };
    let [(group, _)] = params.as_slice() else { return None; };
    let [counted] = module_call(body, "list", "len")? else { return None; };
    if !matches!(counted.kind, IrExprKind::Var { id } if id == *group) { return None; }
    let grouped = module_call(groups, "list", "group_by")?;
    let [_, key] = grouped else { return None; };
    let admitted = matches!(key.kind, IrExprKind::Lambda { .. })
        && matches!(&e.ty, Ty::Applied(C::Map, kv) if matches!(kv.as_slice(), [Ty::String | Ty::Int, Ty::Int]));
    admitted.then(|| grouped.to_vec())
}

/// The arguments of `e` when it is the call `module.func(..)`.
fn module_call<'e>(e: &'e IrExpr, module: &str, func: &str) -> Option<&'e [IrExpr]> {
    let IrExprKind::Call { target: CallTarget::Module { module: m, func: f, .. }, args, .. } = &e.kind else { return None; };
    (m.as_str() == module && f.as_str() == func).then_some(args.as_slice())
}
