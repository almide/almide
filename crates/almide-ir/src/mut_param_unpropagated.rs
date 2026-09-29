//! C-132 call sites of a NEVER-ERR effect fn that no `!` reaches (#2945).
//!
//! An effect fn's call is typed with its Result carrier. Under `!` the
//! move-mode rewrite rotates the unwrap onto the bound call
//! (`let __mp_buf = f(r)!; r = __mp_buf; ()`), and the carrier never
//! surfaces. Without `!` — `let _ = f(r)`, `let res = f(r)`, a `match` over
//! the call — the rewritten block yields `()` / `__mp_res` where the
//! consumer expects the Result, and the wasm leg walls on the mismatch.
//!
//! When the callee can NEVER take its err exit, the Result is always
//! `ok(<payload>)`, and the block can say so:
//!
//!   { let __mp_buf = f(r)!; r = __mp_buf; ok(()) }                  was-Unit
//!   { let (__mp_res, __mp_buf) = f(r)!; r = __mp_buf; ok(__mp_res) } value
//!
//! The `!` cannot fire, so the write-back runs exactly as native's `&mut`
//! write does, and the value is the one the Result consumer reads.
//!
//! A callee that CAN err is left alone: when it writes and then errs,
//! native keeps the partial write, and the C-132 carrier returns no buffer
//! on the err path. Which of those the language promises is the open ruling
//! in #2917 / #1871, so those sites stay walled.
//!
//! "Never-err" is a local scan of the ORIGINAL body (before the rewrite
//! strips a declared-Result fn's ok layer): no `err(..)` construction and
//! no `!` / `?` anywhere in it. A call it makes without `!` cannot raise
//! through it. Anything else counts as can-err, so the scan can wall a
//! site that was safe, but it never admits one that was not.

use crate::visit::IrVisitor;
use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use crate::*;
use almide_lang::types::Ty;
use std::collections::HashSet;

/// Does `body` have no exit through its err channel: no `err(..)` and no
/// propagating `!` / `?`?
fn never_errs(body: &IrExpr) -> bool {
    struct Raises(bool);
    impl IrVisitor for Raises {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::ResultErr { .. } | IrExprKind::Unwrap { .. } | IrExprKind::Try { .. }) {
                self.0 = true;
            }
            if !self.0 {
                crate::visit::walk_expr(self, e);
            }
        }
    }
    let mut r = Raises(false);
    r.visit_expr(body);
    !r.0
}

/// Every `MutFns` key that names a never-err EFFECT fn, over the keys
/// `collect_mut_fns` gives each fn (main-scope bare; module bare, dotted,
/// mangled, and a unique method spelling).
pub(crate) fn never_err_effect_keys(program: &IrProgram, is_mut_key: impl Fn(&str) -> bool) -> HashSet<String> {
    let admit = |f: &IrFunction| f.is_effect && never_errs(&f.body);
    let mut keys = HashSet::new();
    for f in program.functions.iter().filter(|f| admit(f)) {
        keys.insert(crate::mut_param::scope_key("", f.name.as_str()));
    }
    for m in &program.modules {
        for f in m.functions.iter().filter(|f| admit(f)) {
            let (mname, fname) = (m.name.as_str(), f.name.as_str());
            keys.insert(format!("almide_rt_{}_{}", mname.replace('.', "_"), fname.replace('.', "_")));
            keys.insert(format!("{mname}.{fname}"));
            keys.insert(fname.to_string());
            keys.insert(crate::mut_param::scope_key(mname, fname));
        }
    }
    keys.retain(|k| is_mut_key(k));
    keys
}

/// Rewrite every unpropagated move-mode block whose callee is in `keys`.
pub(crate) fn settle(program: &mut IrProgram, keys: &HashSet<String>) {
    if keys.is_empty() {
        return;
    }
    let mut s = Settler { keys, scope: String::new() };
    for f in program.functions.iter_mut() {
        s.visit_expr_mut(&mut f.body);
    }
    for tl in &mut program.top_lets {
        s.visit_expr_mut(&mut tl.value);
    }
    for m in &mut program.modules {
        s.scope = m.name.to_string();
        for f in m.functions.iter_mut() {
            s.visit_expr_mut(&mut f.body);
        }
        for tl in &mut m.top_lets {
            s.visit_expr_mut(&mut tl.value);
        }
    }
}

struct Settler<'a> {
    keys: &'a HashSet<String>,
    scope: String,
}

impl Settler<'_> {
    fn names_never_err_callee(&self, value: &IrExpr) -> bool {
        let IrExprKind::Call { target, .. } = &value.kind else { return false };
        let Some(name) = crate::mut_param::call_spelling(target) else { return false };
        self.keys.contains(&name) || self.keys.contains(&crate::mut_param::scope_key(&self.scope, &name))
    }
}

impl IrMutVisitor for Settler<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        let IrExprKind::Block { stmts, expr: Some(tail) } = &mut expr.kind else { return };
        // The rewriter's own block: its first statement binds a BARE call to
        // a collected fn (a user-written bind of such a call was itself
        // rewritten into a block, and a `!` site was rotated to bind
        // `Unwrap{call}`, so neither matches).
        let value = match stmts.first_mut().map(|s| &mut s.kind) {
            Some(IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }) => value,
            _ => return,
        };
        if !self.names_never_err_callee(value) {
            return;
        }
        let span = value.span;
        let call = std::mem::replace(value, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None });
        let bound_ty = call.ty.clone();
        *value = IrExpr { kind: IrExprKind::Unwrap { expr: Box::new(call) }, ty: bound_ty, span, def_id: None };
        let payload = std::mem::replace(
            tail.as_mut(),
            IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None },
        );
        let carrier = Ty::result(payload.ty.clone(), Ty::String);
        **tail = IrExpr { kind: IrExprKind::ResultOk { expr: Box::new(payload) }, ty: carrier.clone(), span, def_id: None };
        expr.ty = carrier;
    }
}

