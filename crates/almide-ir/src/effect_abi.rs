//! The compiled ABI of every user fn, derived from the typed IR (#3058).
//!
//! An `effect fn f(..) -> T` is called through `Result[T, String]`, but what the
//! body returns depends on whether it can ever take its err exit:
//!
//! - **never-err lifted**: effect, declared return not Result, and no exit
//!   through the err channel (the greatest fixpoint in
//!   [`crate::mut_param_unpropagated::never_err_effect_fn_keys`], shared with
//!   the C-132 move-mode rewrite). The body returns the raw `T` — a declared
//!   `Option[T]` included, whose Option is then the whole return.
//! - **auto-wrap**: effect, not `main`, declared return neither Result nor
//!   Unit, and CAN err. The body's true carrier is `Result[T, String]` on every
//!   exit (a declared Option: `Result[Option[T], String]`).
//! - **mut params**: which positions a PURE fn writes back into (an effect
//!   callee's argument already holds its own credit).
//!
//! [`effect_abi_facts`] is one pure function of the program, so every consumer
//! — the MIR lowering on each product path and the corpus classifier — reads
//! the same facts. [`settle_never_err_calls`] then makes every call site of a
//! never-err lifted fn agree with its raw return: a `!` / `?` over it is the
//! identity, a `match` over it binds the Ok payload, and any other use that
//! still wants the carrier gets `ok(<raw call>)`, exact because it never errs.
//! Together they replace the registries the deleted incumbent passes filled
//! (#2950, #3000).

use std::collections::{HashMap, HashSet};

use almide_base::intern::sym;
use almide_lang::types::constructor::TypeConstructorId;
use almide_lang::types::Ty;

use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use crate::*;

/// The ABI facts of one program, keyed by every call-site spelling of a fn.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EffectAbiFacts {
    /// Never-err effect fns whose declared return is not Result (raw ABI).
    pub never_err_lifted: HashSet<String>,
    /// The members of `never_err_lifted` that declare `Option[..]`.
    pub declared_option_effect: HashSet<String>,
    /// Pure fns declaring `Option[..]`, and the never-err effect ones.
    pub declared_option: HashSet<String>,
    /// Can-err effect fns whose body's true carrier is `Result[T, String]`.
    pub auto_wrap: HashSet<String>,
    /// Pure fns' written-back parameter positions.
    pub mut_params: HashMap<String, Vec<bool>>,
}

fn declares(f: &IrFunction, ctor: TypeConstructorId) -> bool {
    matches!(&f.ret_ty, Ty::Applied(c, _) if *c == ctor)
}

/// Every fn of the program with the spellings a call site may use for it: a
/// main-scope fn by its bare name and its scope key; a module fn by its
/// mangled, dotted and scoped keys, and its bare name only when it is a
/// uniquely spelled `Type.method` (the `never_err_effect_fn_keys` rule — an
/// ambiguous bare name must not make a raising call look quiet).
fn fns_with_spellings(program: &IrProgram) -> Vec<(Option<&str>, &IrFunction, Vec<String>)> {
    let mut spelled: HashMap<&str, usize> = HashMap::new();
    for f in program.functions.iter().chain(program.modules.iter().flat_map(|m| m.functions.iter())) {
        *spelled.entry(f.name.as_str()).or_insert(0) += 1;
    }
    let mut out = Vec::new();
    for f in &program.functions {
        let mut keys = crate::mut_param_unpropagated::fn_spellings(None, f.name.as_str());
        keys.push(f.name.as_str().to_string());
        out.push((None, f, keys));
    }
    for m in &program.modules {
        for f in &m.functions {
            let mut keys = crate::mut_param_unpropagated::fn_spellings(Some(m.name.as_str()), f.name.as_str());
            if !(f.name.as_str().contains('.') && spelled.get(f.name.as_str()) == Some(&1)) {
                keys.retain(|k| k != f.name.as_str());
            }
            out.push((Some(m.name.as_str()), f, keys));
        }
    }
    out
}

/// Derive [`EffectAbiFacts`] from `program`.
pub fn effect_abi_facts(program: &IrProgram) -> EffectAbiFacts {
    let never_err = crate::mut_param_unpropagated::never_err_effect_fn_keys(program);
    let mut facts = EffectAbiFacts::default();
    for (module, f, keys) in fns_with_spellings(program) {
        let quiet = keys.iter().any(|k| never_err.contains(k));
        let result = declares(f, TypeConstructorId::Result);
        let option = declares(f, TypeConstructorId::Option);
        let add = |set: &mut HashSet<String>| set.extend(keys.iter().cloned());
        if f.is_effect && !result && quiet {
            add(&mut facts.never_err_lifted);
            if option {
                add(&mut facts.declared_option_effect);
            }
        }
        if option && (!f.is_effect || quiet) {
            add(&mut facts.declared_option);
        }
        let is_main = module.is_none() && f.name.as_str() == "main";
        if f.is_effect && !quiet && !result && !matches!(f.ret_ty, Ty::Unit) && !is_main {
            add(&mut facts.auto_wrap);
        }
        if !f.is_effect && f.params.iter().any(|p| p.is_mut) {
            let positions: Vec<bool> = f.params.iter().map(|p| p.is_mut).collect();
            for k in &keys {
                facts.mut_params.insert(k.clone(), positions.clone());
            }
        }
    }
    facts
}

/// The raw return `T` of a call to a never-err lifted fn still typed with its
/// lifted `Result[T, String]` carrier, if `e` is one.
fn raw_never_err_call(e: &IrExpr, facts: &EffectAbiFacts, scope: &str) -> Option<Ty> {
    let IrExprKind::Call { target, .. } = &e.kind else { return None };
    let name = crate::mut_param::call_spelling(target)?;
    let quiet = facts.never_err_lifted.contains(&name)
        || facts.never_err_lifted.contains(&crate::mut_param::scope_key(scope, &name));
    if !quiet {
        return None;
    }
    match &e.ty {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && matches!(a[1], Ty::String | Ty::Unknown) => {
            Some(a[0].clone())
        }
        _ => None,
    }
}

struct Settler<'a> {
    facts: &'a EffectAbiFacts,
    scope: String,
    vt: &'a mut VarTable,
}

impl Settler<'_> {
    fn raw(&self, e: &IrExpr) -> Option<Ty> {
        raw_never_err_call(e, self.facts, &self.scope)
    }

    /// `match <never-err call> { ok(x) => A, .. }` (no guard on the Ok arm)
    /// → `{ let x = <raw call>; A }`: the err arm is dead.
    fn settle_match(&mut self, expr: &mut IrExpr) -> bool {
        let IrExprKind::Match { subject, arms } = &expr.kind else { return false };
        let Some(raw_ty) = self.raw(subject) else { return false };
        let Some(ok_arm) = arms.iter().find(|a| matches!(a.pattern, IrPattern::Ok { .. })) else {
            return false;
        };
        if ok_arm.guard.is_some() {
            return false;
        }
        let IrPattern::Ok { inner } = &ok_arm.pattern else { return false };
        let var = match &**inner {
            IrPattern::Bind { var, .. } => *var,
            IrPattern::Wildcard => self.vt.alloc(sym("__never_err_ok"), raw_ty.clone(), Mutability::Let, None),
            _ => return false,
        };
        let call = IrExpr { ty: raw_ty.clone(), ..(**subject).clone() };
        let bind = IrStmt {
            kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: raw_ty, value: call },
            span: subject.span,
        };
        let body = ok_arm.body.clone();
        expr.kind = IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(body)) };
        true
    }

    /// A match over a never-err call it could not settle keeps that subject:
    /// the lowering refuses the raw-value match cleanly. Its parts still settle.
    fn visit_kept_match(&mut self, expr: &mut IrExpr) -> bool {
        let IrExprKind::Match { subject, arms } = &mut expr.kind else { return false };
        if self.raw(subject).is_none() {
            return false;
        }
        if let IrExprKind::Call { args, .. } = &mut subject.kind {
            args.iter_mut().for_each(|a| self.visit_expr_mut(a));
        }
        for arm in arms.iter_mut() {
            if let Some(g) = &mut arm.guard {
                self.visit_expr_mut(g);
            }
            self.visit_expr_mut(&mut arm.body);
        }
        true
    }

    /// `f(..)!` / `f(..)?` over a never-err call is the identity; so is
    /// `f(..) ?? d` with a fallback that has no effect of its own.
    fn strip_identity(&mut self, expr: &mut IrExpr) -> bool {
        let strip = match &expr.kind {
            IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner } => {
                self.raw(inner).is_some_and(|t| t == expr.ty)
            }
            IrExprKind::UnwrapOr { expr: inner, fallback } => {
                self.raw(inner).is_some_and(|t| t == expr.ty) && effect_free(fallback)
            }
            _ => false,
        };
        if !strip {
            return false;
        }
        let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner } | IrExprKind::UnwrapOr { expr: inner, .. }) =
            &mut expr.kind
        else {
            return false;
        };
        let mut call = std::mem::take(&mut **inner);
        call.ty = expr.ty.clone();
        *expr = call;
        walk_expr_mut(self, expr);
        true
    }
}

impl IrMutVisitor for Settler<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        if self.settle_match(expr) {
            self.visit_expr_mut(expr);
            return;
        }
        if self.visit_kept_match(expr) || self.strip_identity(expr) {
            return;
        }
        walk_expr_mut(self, expr);
        // Any other use reads the carrier: `ok(<raw call>)`.
        if let Some(raw_ty) = self.raw(expr) {
            let carrier = expr.ty.clone();
            let call = IrExpr { ty: raw_ty, ..std::mem::take(expr) };
            *expr = IrExpr {
                span: call.span,
                kind: IrExprKind::ResultOk { expr: Box::new(call) },
                ty: carrier,
                def_id: None,
            };
        }
    }
}

/// A `??` fallback evaluating which has no observable effect.
fn effect_free(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::Unit
        | IrExprKind::OptionNone
        | IrExprKind::Var { .. } => true,
        IrExprKind::UnOp { operand, .. } => effect_free(operand),
        _ => false,
    }
}

/// The value exits of a never-err lifted fn's own body return its raw `T`, so
/// a never-err call there passes its raw value through instead of a carrier.
fn retype_raw_tails(e: &mut IrExpr, facts: &EffectAbiFacts, scope: &str) {
    match &mut e.kind {
        IrExprKind::Block { expr: Some(tail), .. } => retype_raw_tails(tail, facts, scope),
        IrExprKind::If { then, else_, .. } => {
            retype_raw_tails(then, facts, scope);
            retype_raw_tails(else_, facts, scope);
        }
        IrExprKind::Match { arms, .. } => {
            arms.iter_mut().for_each(|a| retype_raw_tails(&mut a.body, facts, scope));
        }
        _ => {
            if let Some(raw_ty) = raw_never_err_call(e, facts, scope) {
                e.ty = raw_ty;
            }
        }
    }
}

fn is_never_err_fn(f: &IrFunction, facts: &EffectAbiFacts, scope: &str) -> bool {
    let key = if scope.is_empty() {
        f.name.as_str().to_string()
    } else {
        crate::mut_param::scope_key(scope, f.name.as_str())
    };
    facts.never_err_lifted.contains(&key)
}

/// Make every call site of a never-err lifted fn agree with its raw return
/// (see the module doc). Idempotent: a settled site is no longer typed with
/// the carrier.
pub fn settle_never_err_calls(program: &mut IrProgram, facts: &EffectAbiFacts) {
    if facts.never_err_lifted.is_empty() {
        return;
    }
    let IrProgram { functions, top_lets, modules, var_table, .. } = program;
    let mut s = Settler { facts, scope: String::new(), vt: var_table };
    let settle_fn = |s: &mut Settler<'_>, f: &mut IrFunction| {
        if is_never_err_fn(f, s.facts, &s.scope) {
            retype_raw_tails(&mut f.body, s.facts, &s.scope);
        }
        s.visit_expr_mut(&mut f.body);
    };
    for f in functions.iter_mut() {
        settle_fn(&mut s, f);
    }
    for tl in top_lets.iter_mut() {
        s.visit_expr_mut(&mut tl.value);
    }
    for m in modules.iter_mut() {
        s.scope = m.name.as_str().to_string();
        for f in m.functions.iter_mut() {
            settle_fn(&mut s, f);
        }
        for tl in m.top_lets.iter_mut() {
            s.visit_expr_mut(&mut tl.value);
        }
    }
}
