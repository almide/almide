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
        let slot = match self.carrier_slot(slot, &val.ty) {
            Some(eff) => eff,
            None if !self.is_effect_slot(slot) => {
                self.adapt_through(val, slot);
                return;
            }
            None => slot.clone(),
        };
        let owned = std::mem::replace(val, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None });
        *val = super::expressions::adapt_fn_value_to_effect_slot(self.ctx, owned, &slot);
    }

    /// A slot already in the adapted shape — a pure fn returning the
    /// `Result` carrier, the type an adapted sibling (an `if` arm) has — met
    /// by a pure fn value of the same arity that returns the bare value: the
    /// value takes the same adapter, as at the effect slot it stands for.
    /// The checker never lets a bare-return fn fill a pure carrier-return
    /// slot, so this shape only arises from an effect slot.
    fn carrier_slot(&self, slot: &Ty, val_ty: &Ty) -> Option<Ty> {
        let Ty::Fn { params, ret, is_effect: false } = self.ctx.env.resolve_named(slot) else { return None };
        let Ty::Fn { params: vp, ret: vr, is_effect: false } = val_ty else { return None };
        (ret.is_result() && !vr.is_result() && vp.len() == params.len())
            .then(|| Ty::Fn { params: vp.clone(), ret: vr.clone(), is_effect: true })
    }

    /// The arms of an `if` / `match` agree on one type again after the sweep
    /// adapted some of them (#4768: `if c then unwrap_or_else(v, …) else
    /// secret.new(f)`, where only the first arm met the handler slot).
    fn reconcile_arms(&mut self, expr: &mut IrExpr) {
        let split = match &expr.kind {
            IrExprKind::If { then, else_, .. } => then.ty != else_.ty,
            IrExprKind::Match { arms, .. } => arms.windows(2).any(|w| w[0].body.ty != w[1].body.ty),
            _ => false,
        };
        if !split || !holds_fn_value(expr) {
            return;
        }
        match &mut expr.kind {
            IrExprKind::If { then, else_, .. } if then.ty != else_.ty => {
                let (t, e) = (then.ty.clone(), else_.ty.clone());
                self.adapt_in_place(else_, &t);
                self.adapt_in_place(then, &e);
                if then.ty == else_.ty {
                    expr.ty = then.ty.clone();
                }
            }
            IrExprKind::Match { arms, .. } if arms.windows(2).any(|w| w[0].body.ty != w[1].body.ty) => {
                let tys: Vec<Ty> = arms.iter().map(|a| a.body.ty.clone()).collect();
                for arm in arms.iter_mut() {
                    for t in &tys {
                        if arm.body.ty != *t {
                            self.adapt_in_place(&mut arm.body, t);
                        }
                    }
                }
                if arms.windows(2).all(|w| w[0].body.ty == w[1].body.ty) && let Some(a) = arms.first() {
                    expr.ty = a.body.ty.clone();
                }
            }
            _ => {}
        }
    }

    /// #4768: a slot that is not itself an effect fn type can still hold one
    /// inside it (`Secret[HttpHandler]`, `Result[Secret[HttpHandler], E]`).
    /// A value that reaches such a slot through `ok(..)` / `some(..)` or
    /// through a generic call (`secret.new(f)`, whose `T` the slot names) is
    /// adapted at the position the slot gives it, and its type is re-derived
    /// from the adapted parts. A value whose parts need no adapter keeps the
    /// types it had.
    fn adapt_through(&mut self, val: &mut IrExpr, slot: &Ty) {
        use crate::types::TypeConstructorId as C;
        // Only an expression that holds a pure fn value can need the
        // adapter. Asking the expression keeps the sweep linear in the
        // program: the types it would otherwise unify are shared trees whose
        // unfolded size can double per line (#4114, ADR-0030 D5).
        if !holds_fn_value(val) {
            return;
        }
        let resolved = self.ctx.env.resolve_named(slot);
        match (&mut val.kind, &resolved) {
            (IrExprKind::ResultOk { expr }, Ty::Applied(C::Result, a)) if a.len() == 2 => {
                self.adapt_in_place(expr, &a[0]);
                if let Ty::Applied(C::Result, va) = &val.ty && va.len() == 2 {
                    val.ty = Ty::result(expr.ty.clone(), va[1].clone());
                }
            }
            (IrExprKind::OptionSome { expr }, Ty::Applied(C::Option, a)) if a.len() == 1 => {
                self.adapt_in_place(expr, &a[0]);
                if matches!(&val.ty, Ty::Applied(C::Option, _)) {
                    val.ty = Ty::Applied(C::Option, vec![expr.ty.clone()].into());
                }
            }
            (IrExprKind::Lambda { body, .. }, Ty::Fn { ret, is_effect: false, .. }) => {
                let before = body.ty.clone();
                self.adapt_in_place(body, ret);
                if body.ty != before
                    && let Ty::Fn { params, is_effect, .. } = &val.ty
                {
                    val.ty = Ty::Fn { params: params.clone(), ret: Box::new(body.ty.clone()), is_effect: *is_effect };
                }
            }
            (IrExprKind::Call { target, args, type_args }, _) => {
                let Some(sig) = self.generic_sig(target) else { return };
                let mut want = HashMap::new();
                if !crate::types::unify(&sig.ret, slot, &mut want) {
                    return;
                }
                self.adapt_generic_args(&sig, args, &want, &mut val.ty, type_args, None);
            }
            _ => {}
        }
    }

    /// Adapt each argument of a generic call to its param with the type
    /// variables bound by `seed` (what the call's slot names) and by the
    /// OTHER arguments (`result.unwrap_or(v, secret.new(f))` with `v:
    /// Result[Secret[HttpHandler], E]` gives `f` the handler slot). When an
    /// argument's type changes, the call's type and type args are re-derived
    /// from the adapted arguments.
    /// `before` is the argument types to compare against: the visitor passes
    /// them as they were before it walked the arguments, so a call whose
    /// argument an inner step adapted is re-derived too.
    fn adapt_generic_args(&mut self, sig: &crate::types::FnSig, args: &mut [IrExpr], seed: &HashMap<Sym, Ty>, ty: &mut Ty, type_args: &mut Vec<Ty>, before: Option<Vec<Ty>>) {
        let before: Vec<Ty> = before.unwrap_or_else(|| args.iter().map(|a| a.ty.clone()).collect());
        for i in 0..args.len().min(sig.params.len()) {
            let mut b = seed.clone();
            for (j, (arg, (_, pty))) in args.iter().zip(sig.params.iter()).enumerate() {
                if j != i {
                    crate::types::unify(pty, &arg.ty, &mut b);
                }
            }
            let pslot = crate::types::substitute(&sig.params[i].1, &b);
            if !pslot.has_typevar() {
                self.adapt_in_place(&mut args[i], &pslot);
            }
        }
        if args.iter().zip(before.iter()).all(|(a, b)| a.ty == *b) {
            return;
        }
        let mut got = HashMap::new();
        for (arg, (_, pty)) in args.iter().zip(sig.params.iter()) {
            crate::types::unify(pty, &arg.ty, &mut got);
        }
        if sig.generics.iter().all(|g| got.contains_key(g)) {
            *ty = crate::types::substitute(&sig.ret, &got);
            if type_args.len() == sig.generics.len() {
                *type_args = sig.generics.iter().map(|g| got[g].clone()).collect();
            }
        }
    }

    /// The signature of a generic stdlib or program fn a call names.
    fn generic_sig(&self, target: &CallTarget) -> Option<crate::types::FnSig> {
        let env = self.ctx.env;
        let sig = match target {
            CallTarget::Named { name } => env.functions.get(name).cloned().or_else(|| {
                let (module, func) = name.as_str().split_once('.')?;
                crate::stdlib::lookup_sig(module, func)
            }),
            CallTarget::Module { module, func, .. } => crate::stdlib::lookup_sig(module.as_str(), func.as_str())
                .or_else(|| env.functions.get(&sym(&format!("{}.{}", module, func))).cloned()),
            _ => None,
        }?;
        (!sig.generics.is_empty() && sig.ret.has_typevar()).then_some(sig)
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
        let pre: Option<Vec<Ty>> = match &expr.kind {
            IrExprKind::Call { args, .. } => Some(args.iter().map(|a| a.ty.clone()).collect()),
            _ => None,
        };
        visit_mut::walk_expr_mut(self, expr);
        self.reconcile_arms(expr);
        let ty = expr.ty.clone();
        match &mut expr.kind {
            IrExprKind::Call { target, args, type_args } => {
                if let Some(params) = self.callee_params(target) {
                    for (arg, pty) in args.iter_mut().zip(params.iter()) {
                        self.adapt_in_place(arg, pty);
                    }
                }
                if args.iter().any(holds_fn_value)
                    && let Some(sig) = self.generic_sig(target)
                {
                    self.adapt_generic_args(&sig, args, &HashMap::new(), &mut expr.ty, type_args, pre);
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

/// Whether `e` holds a pure fn value an effect slot could need to adapt: a
/// lambda, or a variable / fn name / call result whose own type is a pure
/// fn (read at the top of each node's type only, never through it).
fn holds_fn_value(e: &IrExpr) -> bool {
    struct Find(bool);
    impl almide_ir::visit::IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.0 {
                return;
            }
            if matches!(e.kind, IrExprKind::Lambda { .. }) || matches!(e.ty, Ty::Fn { is_effect: false, .. }) {
                self.0 = true;
                return;
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut f = Find(false);
    almide_ir::visit::IrVisitor::visit_expr(&mut f, e);
    f.0
}
