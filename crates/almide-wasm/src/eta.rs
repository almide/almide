//! #2750 — a function VALUE as a stdlib higher-order callback
//! (`xs |> list.filter(pred)` with `pred` a parameter, a let-bound closure,
//! a fn reference) is eta-expanded into the literal lambda the HOF arms
//! already lower: `list.filter(xs, pred)` becomes
//! `list.filter(xs, (a) => pred(a))`.
//!
//! The arms inline a literal lambda's body per element (`hof_lambda`), and
//! a call through a value is the closure call every other site uses
//! (`CallTarget::Computed`, calls.rs). So the rewrite adds no second
//! lowering of any HOF: the one fused/staged loop sees a lambda whose body
//! is an ordinary call (the #2397 lesson — a second copy of a HOF's
//! lowering is a second place to remember every rule).
//!
//! The frontend already eta-expands a bare fn NAME in value position
//! (lower_expr_ident); what reaches here is a `Var` of fn type (a param or
//! a let-bound closure) or a leftover `FnRef`. Anything else — a call that
//! PRODUCES a closure — is left alone: expanding it would re-evaluate the
//! producer per element.
//!
//! An effect fn value is skipped: its runtime shape is the
//! carrier-returning closure, which the value paths
//! (`lower_list_map_fnvalue`, the fan closure route) call as is.

use almide_base::intern::sym;
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrProgram, Mutability, VarTable};
use almide_types::types::Ty;

/// The stdlib families whose arms inline a callback (`hof_lambda`).
const HOF_MODULES: &[&str] = &["list", "map", "set", "option", "result", "fan"];

/// The eta-expanded program, or `None` when no callback needed it (the
/// common case: no clone).
pub(crate) fn eta_expand_callbacks(ir: &IrProgram) -> Option<IrProgram> {
    let mut out = ir.clone();
    let mut changed = false;
    {
        let mut v = Eta { vars: &mut out.var_table, changed: &mut changed };
        for f in out.functions.iter_mut() {
            v.visit_expr_mut(&mut f.body);
        }
        for tl in out.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    for m in out.modules.iter_mut() {
        let mut v = Eta { vars: &mut m.var_table, changed: &mut changed };
        for f in m.functions.iter_mut() {
            v.visit_expr_mut(&mut f.body);
        }
        for tl in m.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    changed.then_some(out)
}

struct Eta<'a> {
    vars: &'a mut VarTable,
    changed: &'a mut bool,
}

impl Eta<'_> {
    /// `(a0..an) => callee(a0..an)` for a fn value `v`.
    fn expand(&mut self, v: IrExpr) -> IrExpr {
        let Ty::Fn { params, ret, .. } = v.ty.clone() else { return v };
        let span = v.span;
        let ps: Vec<_> = params
            .iter()
            .enumerate()
            .map(|(i, pt)| {
                let id = self.vars.alloc(sym(&format!("__eta_cb{i}")), pt.clone(), Mutability::Let, span);
                (id, pt.clone())
            })
            .collect();
        let args = ps
            .iter()
            .map(|(id, pt)| IrExpr { kind: IrExprKind::Var { id: *id }, ty: pt.clone(), span, def_id: None })
            .collect();
        let lambda_ty = v.ty.clone();
        let target = match &v.kind {
            IrExprKind::FnRef { name } => CallTarget::Named { name: *name },
            _ => CallTarget::Computed { callee: Box::new(v) },
        };
        let body = IrExpr {
            kind: IrExprKind::Call { target, args, type_args: vec![] },
            ty: *ret,
            span,
            def_id: None,
        };
        IrExpr { kind: IrExprKind::Lambda { params: ps, body: Box::new(body), lambda_id: None }, ty: lambda_ty, span, def_id: None }
    }
}

impl IrMutVisitor for Eta<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::Call { target: CallTarget::Module { module, .. }, args, .. } = &mut e.kind else {
            return;
        };
        if !HOF_MODULES.contains(&module.as_str()) {
            return;
        }
        for a in args.iter_mut() {
            let value_shape = matches!(a.kind, IrExprKind::Var { .. } | IrExprKind::FnRef { .. });
            if value_shape && matches!(a.ty, Ty::Fn { is_effect: false, .. }) {
                let v = std::mem::take(a);
                *a = self.expand(v);
                *self.changed = true;
            }
        }
    }
}
