//! The effect-slot sweep (#4545): every place a PURE fn value fills an
//! `effect (A) -> B` slot gets the ok(...) adapter, once, over the lowered IR.
//!
//! The adapter itself is `adapt_fn_value_to_effect_slot` (#1148). Lowering
//! calls it at two sites, a direct call's argument and an annotated `let`.
//! Every other route a value takes into a slot skipped it: the argument of a
//! piped call (`v |> on_click(f)`), a computed call (`h(f)` with `h` a fn
//! value), a record field and a spread override (`{ ...v, on_click: f }`), a
//! fn or lambda return, a list element, an assignment. The IR then handed a
//! `() -> Unit` value to a `() -> Result[Unit, String]` slot, which the wasm
//! leg refused (E082 `ty-mismatch:Fn`) and the native leg could not type.
//!
//! The sweep reads each slot's type from the IR before the carrier
//! normalization (`normalize_effect_fn_types`) erases the effect marker, and
//! runs before `finalize_ir_program`, so use counts and auto-try see the
//! adapted values. It is idempotent: a value that already returns the
//! carrier — an adapted argument, a lambda the checker typed against the
//! slot, a named effect fn's value — is left as it is, so the two lowering
//! sites keep their output byte for byte and the sweep only reaches the
//! values they never saw.

use std::collections::HashMap;
use almide_base::intern::{Sym, sym};
use almide_ir::*;
use almide_ir::visit_mut::IrMutVisitor;
use crate::types::Ty;
use super::LowerCtx;

/// Adapt every pure fn value that fills an effect slot in `functions` and
/// `top_lets` (the program being lowered).
pub(super) fn adapt_effect_slots(ctx: &mut LowerCtx, functions: &mut [IrFunction], top_lets: &mut [IrTopLet]) {
    let own: HashMap<Sym, Vec<Ty>> = functions.iter()
        .map(|f| (f.name, f.params.iter().map(|p| p.ty.clone()).collect()))
        .collect();
    let mut sweep = Sweep { ctx, own };
    for f in functions.iter_mut() {
        sweep.visit_expr_mut(&mut f.body);
        let ret = f.ret_ty.clone();
        sweep.adapt_in_place(&mut f.body, &ret);
    }
    for tl in top_lets.iter_mut() {
        sweep.visit_expr_mut(&mut tl.value);
        let ty = tl.ty.clone();
        sweep.adapt_in_place(&mut tl.value, &ty);
    }
}

struct Sweep<'c, 'a> {
    ctx: &'c mut LowerCtx<'a>,
    /// The declared param types of the fns this program lowers, by IR name.
    own: HashMap<Sym, Vec<Ty>>,
}

impl Sweep<'_, '_> {
    /// Whether `slot` is an effect fn type (read through an alias).
    fn is_effect_slot(&self, slot: &Ty) -> bool {
        matches!(self.ctx.env.resolve_named(slot), Ty::Fn { is_effect: true, .. })
    }

    /// Adapt `val` to `slot` when `slot` is an effect fn type — or a list of
    /// them filled by a list literal (`-> List[effect (A) -> B] = [f, g]`:
    /// the checker types the literal by its elements, so the literal itself
    /// never names the effect slot).
    fn adapt_in_place(&mut self, val: &mut IrExpr, slot: &Ty) {
        if let Ty::Applied(crate::types::TypeConstructorId::List, a) = self.ctx.env.resolve_named(slot)
            && let Some(elem) = a.first()
            && self.is_effect_slot(elem)
            && let IrExprKind::List { elements } = &mut val.kind
            && !elements.is_empty()
        {
            for e in elements.iter_mut() {
                self.adapt_in_place(e, elem);
            }
            val.ty = Ty::Applied(crate::types::TypeConstructorId::List, vec![elements[0].ty.clone()].into());
            return;
        }
        if !self.is_effect_slot(slot) {
            return;
        }
        let owned = std::mem::replace(val, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None });
        *val = super::expressions::adapt_fn_value_to_effect_slot(self.ctx, owned, slot);
    }

    /// The declared param types of a call's callee, when it is known.
    fn callee_params(&self, target: &CallTarget) -> Option<Vec<Ty>> {
        let env = self.ctx.env;
        let sig_params = |s: &crate::types::FnSig| s.params.iter().map(|(_, t)| t.clone()).collect::<Vec<_>>();
        match target {
            CallTarget::Named { name } => {
                if let Some(ps) = self.own.get(name) {
                    return Some(ps.clone());
                }
                let own_mod = self.ctx.current_module
                    .and_then(|m| env.functions.get(&sym(&format!("{}.{}", m, name))));
                if let Some(s) = env.functions.get(name).or(own_mod) {
                    return Some(sig_params(s));
                }
                if let Some((_, case)) = env.lookup_ctor_in(name, self.ctx.current_module.map(|s| s.as_str())) {
                    return match case.payload {
                        crate::types::VariantPayload::Tuple(tys) => Some(tys),
                        _ => None,
                    };
                }
                let (module, func) = name.as_str().split_once('.')?;
                crate::stdlib::lookup_sig(module, func).map(|s| sig_params(&s))
            }
            CallTarget::Module { module, func, .. } => crate::stdlib::lookup_sig(module.as_str(), func.as_str())
                .map(|s| sig_params(&s))
                .or_else(|| env.functions.get(&sym(&format!("{}.{}", module, func))).map(sig_params)),
            CallTarget::Computed { callee } => match env.resolve_named(&callee.ty) {
                Ty::Fn { params, .. } => Some(params),
                _ => None,
            },
            CallTarget::Method { .. } => None,
        }
    }

    /// The declared field types of a record literal of type `ty`.
    fn record_fields(&self, name: Option<Sym>, ty: &Ty) -> Option<Vec<(Sym, Ty)>> {
        let cur_mod = self.ctx.current_module.map(|m| m.as_str());
        let decl = name.and_then(|n| super::statements::declared_record_ty(self.ctx.env, n, cur_mod))
            .unwrap_or_else(|| self.ctx.env.resolve_named(ty));
        match decl {
            Ty::Record { fields } | Ty::OpenRecord { fields } => Some(fields),
            _ => None,
        }
    }

    fn adapt_fields(&mut self, fields: &mut [(Sym, IrExpr)], decl: &[(Sym, Ty)]) {
        for (fname, fval) in fields.iter_mut() {
            if let Some((_, fty)) = decl.iter().find(|(n, _)| n == fname) {
                self.adapt_in_place(fval, fty);
            }
        }
    }
}

impl IrMutVisitor for Sweep<'_, '_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        visit_mut::walk_expr_mut(self, expr);
        let ty = expr.ty.clone();
        match &mut expr.kind {
            IrExprKind::Call { target, args, .. } => {
                if let Some(params) = self.callee_params(target) {
                    for (arg, pty) in args.iter_mut().zip(params.iter()) {
                        self.adapt_in_place(arg, pty);
                    }
                }
            }
            IrExprKind::Record { name, fields } => {
                if let Some(decl) = self.record_fields(*name, &ty) {
                    self.adapt_fields(fields, &decl);
                }
            }
            IrExprKind::SpreadRecord { fields, .. } => {
                if let Some(decl) = self.record_fields(None, &ty) {
                    self.adapt_fields(fields, &decl);
                }
            }
            IrExprKind::List { elements } => {
                if let Ty::Applied(crate::types::TypeConstructorId::List, a) = &ty
                    && let Some(elem) = a.first()
                {
                    let elem = elem.clone();
                    for e in elements.iter_mut() {
                        self.adapt_in_place(e, &elem);
                    }
                }
            }
            IrExprKind::Lambda { body, .. } => {
                if let Ty::Fn { ret, .. } = &ty {
                    let ret = ret.as_ref().clone();
                    self.adapt_in_place(body, &ret);
                }
            }
            _ => {}
        }
    }

    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        visit_mut::walk_stmt_mut(self, stmt);
        match &mut stmt.kind {
            IrStmtKind::Bind { ty, value, .. } => {
                let ty = ty.clone();
                self.adapt_in_place(value, &ty);
            }
            IrStmtKind::Assign { var, value } => {
                let ty = self.ctx.var_table.get(*var).ty.clone();
                self.adapt_in_place(value, &ty);
            }
            _ => {}
        }
    }
}
