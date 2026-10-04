//! Cross-module top-let references (#3058).
//!
//! The entry file reads `lib.TITLE` through a VarId the frontend synthesizes
//! in the ENTRY var table (`module_top_let_var`: the source spelling,
//! `module_origin` = the module's mangled ident). This resolver folds case
//! on both sides, so case twins (`buf` / `BUF`, #3316) are a name defined
//! twice and stay unbound. The module's own top-let
//! lives in the MODULE's var table under another id, so no globals map keyed
//! by the module's ids binds the reference, and the function walled on
//! "use of unbound var" or an unresolvable condition.
//!
//! [`cross_module_toplet_refs`] is a pure function of the program: each such
//! reference whose `(module, NAME)` names exactly one IMMUTABLE top-let of
//! that module, with an initializer that reads no variable and calls no
//! function (both resolve in the module's own scope), resolves to that top-let's declared type and initializer.
//! Anything else stays unbound (an honest wall): a `var` (its init would
//! const-fold reads across writes), a name defined twice, an init that would
//! carry a module-scoped id or call across regions, or a type that disagrees
//! (an `Unknown` hole in the entry's synthesized type is not a disagreement).

use std::collections::HashMap;

use almide_ir::{IrExpr, IrExprKind, IrModule, IrProgram, Mutability, VarId};
use almide_lang::types::Ty;

/// The `module_origin` spelling of a module: its versioned name when it has
/// one, else its name, as its `module_ident` (#3338) — byte for byte what the
/// frontend writes into a synthesized reference.
fn origin_key(m: &IrModule) -> String {
    almide_base::names::module_ident(m.versioned_name.unwrap_or(m.name).as_str())
}

/// Does `e` read a variable or call a function? Either is resolved in the
/// MODULE's scope (its var region, its bare fn names), so neither may be
/// re-evaluated in the entry file's.
fn scope_bound(e: &IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct V(bool);
    impl IrVisitor for V {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(
                e.kind,
                IrExprKind::Var { .. }
                    | IrExprKind::Call { .. }
                    | IrExprKind::RuntimeCall { .. }
                    | IrExprKind::Lambda { .. }
            ) {
                self.0 = true;
            }
            walk_expr(self, e);
        }
    }
    let mut v = V(false);
    v.visit_expr(e);
    v.0
}

/// One module top-let as a reference target: its type and initializer after
/// chasing an in-module alias chain (`let white = _white`, bounded), or `None`
/// when it cannot cross regions.
fn target(m: &IrModule, var: VarId, ty: &Ty, value: &IrExpr) -> Option<(Ty, IrExpr)> {
    let info = m.var_table.entries.get(var.0 as usize)?;
    if matches!(info.mutability, Mutability::Var) {
        return None;
    }
    let local: HashMap<u32, (&Ty, &IrExpr)> = m.top_lets.iter().map(|t| (t.var.0, (&t.ty, &t.value))).collect();
    let (mut ty, mut init) = (ty, value);
    for _ in 0..4 {
        let IrExprKind::Var { id } = &init.kind else { break };
        let Some((t2, i2)) = local.get(&id.0) else { break };
        if matches!(ty, Ty::Unknown) {
            ty = t2;
        }
        init = i2;
    }
    if matches!(ty, Ty::Unknown) {
        ty = &init.ty;
    }
    if matches!(ty, Ty::Unknown) || scope_bound(init) {
        return None;
    }
    Some((ty.clone(), init.clone()))
}

/// Does the entry file's synthesized type agree with the top-let's declared
/// type? The frontend leaves the part of an un-annotated top-let it could not
/// see from the entry file `Unknown` (`List[Unknown]` for a list of records):
/// an `Unknown` there is a hole the declared type fills, anything else must
/// match constructor for constructor.
fn agrees(entry: &Ty, decl: &Ty) -> bool {
    match (entry, decl) {
        (Ty::Unknown, _) => true,
        (Ty::Applied(c1, a1), Ty::Applied(c2, a2)) => c1 == c2 && args_agree(a1, a2),
        (Ty::Tuple(a1), Ty::Tuple(a2)) => args_agree(a1, a2),
        _ => entry == decl,
    }
}

fn args_agree(a: &[Ty], b: &[Ty]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| agrees(x, y))
}

/// Every entry-file reference to another module's top-let that resolves (see
/// the module doc): VarId → (declared type, initializer).
pub fn cross_module_toplet_refs(ir: &IrProgram) -> HashMap<VarId, (Ty, IrExpr)> {
    let mut by_name: HashMap<(String, String), Option<(Ty, IrExpr)>> = HashMap::new();
    for m in &ir.modules {
        let origin = origin_key(m);
        for tl in &m.top_lets {
            let Some(info) = m.var_table.entries.get(tl.var.0 as usize) else { continue };
            let key = (origin.clone(), info.name.as_str().to_uppercase());
            let entry = target(m, tl.var, &tl.ty, &tl.value);
            // A second definition of the name makes it ambiguous.
            by_name.entry(key).and_modify(|e| *e = None).or_insert(entry);
        }
    }
    let mut out = HashMap::new();
    for (i, info) in ir.var_table.entries.iter().enumerate() {
        let Some(origin) = info.module_origin.as_deref() else { continue };
        let Some(Some((ty, init))) = by_name.get(&(origin.to_string(), info.name.as_str().to_uppercase())) else {
            continue;
        };
        if agrees(&info.ty, ty) {
            out.insert(VarId(i as u32), (ty.clone(), init.clone()));
        }
    }
    out
}

/// Add the resolved cross-module references to a globals pair, never
/// overriding an id the caller already bound.
pub fn bind_cross_module_toplets(
    ir: &IrProgram,
    globals: &mut HashMap<VarId, Ty>,
    global_inits: &mut HashMap<VarId, IrExpr>,
) {
    for (id, (ty, init)) in cross_module_toplet_refs(ir) {
        if globals.contains_key(&id) {
            continue;
        }
        globals.insert(id, ty);
        global_inits.insert(id, init);
    }
}
