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
//! A callee that CAN err is not settled here: it carries its buffer on the
//! err arm too (#2917, `mut_param_err_carry`).
//!
//! "Never-err" is a scan of the ORIGINAL body (before the rewrite strips a
//! declared-Result fn's ok layer): no `err(..)` construction, and every `!` /
//! `?` is over a call to another never-err effect fn (a greatest fixpoint, so
//! a recursive walk over its own call qualifies). A call it makes without `!`
//! cannot raise through it. Anything else counts as can-err and takes the
//! err-carrying form (`mut_param_err_carry`).

use crate::visit::IrVisitor;
use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use crate::*;
use almide_lang::types::Ty;
use std::collections::HashSet;

/// Does `body` have no exit through its err channel: no `err(..)`, and every
/// propagating `!` / `?` is over a call to a fn `quiet` admits (a never-err
/// effect fn, so the propagation cannot fire)?
fn never_errs(body: &IrExpr, quiet: &dyn Fn(&str) -> bool) -> bool {
    struct Raises<'q>(bool, &'q dyn Fn(&str) -> bool);
    impl IrVisitor for Raises<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::ResultErr { .. } => self.0 = true,
                IrExprKind::Unwrap { expr } | IrExprKind::Try { expr } => {
                    let quiet_call = match &expr.kind {
                        IrExprKind::Call { target, .. } => {
                            crate::mut_param::call_spelling(target).is_some_and(|n| (self.1)(&n))
                        }
                        _ => false,
                    };
                    if !quiet_call {
                        self.0 = true;
                    }
                }
                _ => {}
            }
            if !self.0 {
                crate::visit::walk_expr(self, e);
            }
        }
    }
    let mut r = Raises(false, quiet);
    r.visit_expr(body);
    !r.0 && !passes_carrier_through(body, quiet)
}

/// Does a value exit of `body` pass a Result carrier through (#3058): a tail
/// typed `Result[..]` that is neither an `ok(..)` nor a call to a fn `quiet`
/// admits — `effect fn helper(p) -> String = fs.read_text(p)`, whose callee's
/// err flows out verbatim with no `err(..)` and no `!` in the body?
fn passes_carrier_through(e: &IrExpr, quiet: &dyn Fn(&str) -> bool) -> bool {
    match &e.kind {
        IrExprKind::Block { expr: Some(t), .. } => passes_carrier_through(t, quiet),
        IrExprKind::If { then, else_, .. } => {
            passes_carrier_through(then, quiet) || passes_carrier_through(else_, quiet)
        }
        IrExprKind::Match { arms, .. } => arms.iter().any(|a| passes_carrier_through(&a.body, quiet)),
        IrExprKind::ResultOk { .. } => false,
        IrExprKind::Call { target, .. }
            if crate::mut_param::call_spelling(target).is_some_and(|n| quiet(&n)) =>
        {
            false
        }
        _ => e.ty.is_result(),
    }
}

/// Every call-site spelling of a fn: main-scope bare, or a module fn's
/// mangled, dotted, bare (a unique method) and scoped keys — the keys
/// `collect_mut_fns` gives it.
pub(crate) fn fn_spellings(module: Option<&str>, fname: &str) -> Vec<String> {
    match module {
        None => vec![crate::mut_param::scope_key("", fname)],
        Some(mname) => vec![
            format!("almide_rt_{}_{}", mname.replace('.', "_"), fname.replace('.', "_")),
            format!("{mname}.{fname}"),
            fname.to_string(),
            crate::mut_param::scope_key(mname, fname),
        ],
    }
}

/// Every EFFECT fn that can never take its err exit, as the keys of
/// [`fn_spellings`]. A greatest fixpoint: a fn stays in the set while every
/// `!` / `?` in its body is over a call to a fn still in the set, so a
/// recursive walk whose only propagation is its own call (`walk(buf, n - 1)!`)
/// is never-err, and one `err(..)` anywhere in a cycle takes the cycle out.
pub(crate) fn never_err_effect_fn_keys(program: &IrProgram) -> HashSet<String> {
    never_err_effect_fn_keys_among(program, &|_| true)
}

/// [`never_err_effect_fn_keys`] over the effect fns `admit` accepts: a fn it
/// refuses is never a candidate, so a `!` over a call to it counts as a raise.
pub(crate) fn never_err_effect_fn_keys_among(
    program: &IrProgram,
    admit: &dyn Fn(&IrFunction) -> bool,
) -> HashSet<String> {
    let mut cands: Vec<(Option<&str>, &IrFunction)> = Vec::new();
    cands.extend(program.functions.iter().filter(|f| f.is_effect && admit(f)).map(|f| (None, f)));
    for m in &program.modules {
        cands.extend(
            m.functions.iter().filter(|f| f.is_effect && admit(f)).map(|f| (Some(m.name.as_str()), f)),
        );
    }
    // A bare `Type.method` spelling names a module fn only when exactly one
    // fn in the program spells it (the `collect_mut_fns` rule); an ambiguous
    // one must not make a raising call look quiet.
    let mut spelled: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for f in program.functions.iter().chain(program.modules.iter().flat_map(|m| m.functions.iter())) {
        *spelled.entry(f.name.as_str()).or_insert(0) += 1;
    }
    let spellings = |m: Option<&str>, f: &IrFunction| {
        let mut ks = fn_spellings(m, f.name.as_str());
        if m.is_some() && !(f.name.as_str().contains('.') && spelled.get(f.name.as_str()) == Some(&1)) {
            ks.retain(|k| k != f.name.as_str());
        }
        ks
    };
    loop {
        let keys: HashSet<String> = cands.iter().flat_map(|(m, f)| spellings(*m, f)).collect();
        let before = cands.len();
        cands.retain(|(m, f)| {
            let scope = m.unwrap_or("");
            let quiet = |n: &str| keys.contains(n) || keys.contains(&crate::mut_param::scope_key(scope, n));
            never_errs(&f.body, &quiet)
        });
        if cands.len() == before {
            return keys;
        }
    }
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

