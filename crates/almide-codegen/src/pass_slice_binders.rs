//! SliceBindersPass: a `match` over `string.split_once` / `strip_prefix` /
//! `strip_suffix` whose payload binders are only READ binds them as `&str`
//! slices of the subject instead of fresh `String`s.
//!
//! Target: Rust only. Runs after BorrowLowering, on the final IR.
//!
//! `string.split_once(line, ";")` returns `(String, String)?`, and the native
//! runtime builds both halves as owned copies — two allocations and two frees
//! per call, and a parse loop that splits every line (`station;12.3` then
//! `12` / `3`) pays four or five of them where hand-written Rust pays none:
//! `str::split_once` hands back two slices of the line. Nothing in Almide can
//! observe the difference between a `String` and a `&str` with the same bytes
//! — what a binder is used FOR decides whether the copy was ever needed.
//!
//! The match switches to the runtime's `_ref` twin (`Option<(&str, &str)>`)
//! when every occurrence of every payload binder is one of:
//!   - a `&str` read (`Borrow { as_str }`, rendered `&*b`) — unchanged;
//!   - a consumed value (a call argument, a constructor field or element, a
//!     `let` initializer, a branch result) or an explicit `Clone` — rewritten
//!     to `b.to_string()`, so the one allocation the consumer needs happens
//!     where it is consumed (the `String` the runtime built was that same
//!     allocation, made earlier);
//! and the subject borrows a variable (or a literal) that the arms never
//! write or hand on — the slices point into it for as long as the arms run.
//! Any other use (a comparison, an interpolation part, a method receiver, a
//! capture) keeps the owned twin: those render against a `String` today and
//! are not re-proved here. The pass never adds an allocation: a binder with no
//! consumed use saves its copy, one with `k` consumed uses costs the `k` it
//! already cost (the move was the runtime's copy, the clones were the rest).

use std::collections::HashSet;
use almide_base::intern::sym;
use almide_ir::*;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct SliceBindersPass;

impl NanoPass for SliceBindersPass {
    fn name(&self) -> &str { "SliceBinders" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    /// Reads the FINAL spelling of every borrow and clone.
    fn depends_on(&self) -> Vec<&'static str> { vec!["BorrowLowering"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut v = Rewriter { changed: false };
        for func in &mut program.functions {
            v.visit_expr_mut(&mut func.body);
        }
        for tl in &mut program.top_lets {
            v.visit_expr_mut(&mut tl.value);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                v.visit_expr_mut(&mut func.body);
            }
            for tl in &mut module.top_lets {
                v.visit_expr_mut(&mut tl.value);
            }
        }
        PassResult { program, changed: v.changed }
    }
}

/// The borrowing twin of a substring-returning runtime fn.
fn slice_twin(symbol: &str) -> Option<&'static str> {
    match symbol {
        "almide_rt_string_split_once" => Some("almide_rt_string_split_once_ref"),
        "almide_rt_string_strip_prefix" => Some("almide_rt_string_strip_prefix_ref"),
        "almide_rt_string_strip_suffix" => Some("almide_rt_string_strip_suffix_ref"),
        _ => None,
    }
}

fn var_id(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        _ => None,
    }
}

/// A subject argument the slices may point into: a borrowed variable (the
/// variable is returned, to be checked against the arms) or a literal.
fn borrowed_source(arg: &IrExpr) -> Result<Option<VarId>, ()> {
    match &arg.kind {
        IrExprKind::LitStr { .. } => Ok(None),
        IrExprKind::Borrow { expr, as_str: true, mutable: false } => match &expr.kind {
            IrExprKind::LitStr { .. } => Ok(None),
            IrExprKind::Var { id } => Ok(Some(*id)),
            _ => Err(()),
        },
        _ => Err(()),
    }
}

/// The payload binders of `some(b)` / `some((a, b))` arms, or `None` when a
/// pattern has any other shape (a literal, a nested constructor).
fn payload_binders(arms: &[IrMatchArm]) -> Option<HashSet<VarId>> {
    let mut out = HashSet::new();
    let leaf = |p: &IrPattern, out: &mut HashSet<VarId>| match p {
        IrPattern::Bind { var, .. } => { out.insert(*var); true }
        IrPattern::Wildcard => true,
        _ => false,
    };
    for arm in arms {
        let ok = match &arm.pattern {
            IrPattern::None | IrPattern::Wildcard => true,
            IrPattern::Some { inner } => match inner.as_ref() {
                IrPattern::Tuple { elements } => elements.iter().all(|e| leaf(e, &mut out)),
                p => leaf(p, &mut out),
            },
            _ => false,
        };
        if !ok { return None; }
    }
    Some(out)
}

/// Does every occurrence of `binders` and `sources` in the arms keep to the
/// shapes the module doc lists?
struct Audit<'a> {
    binders: &'a HashSet<VarId>,
    sources: &'a HashSet<VarId>,
    ok: bool,
}

impl Audit<'_> {
    fn is_binder(&self, e: &IrExpr) -> bool {
        var_id(e).is_some_and(|id| self.binders.contains(&id))
    }

    /// `e` sits where its value is consumed: a bare binder there is an owned use.
    fn value(&mut self, e: &IrExpr) {
        if !self.is_binder(e) {
            self.visit_expr(e);
        }
    }

    fn arm(&mut self, arm: &IrMatchArm) {
        if let Some(g) = &arm.guard { self.visit_expr(g); }
        self.value(&arm.body);
    }

    /// The kinds whose children are consumed values; `false` = not one.
    fn value_children(&mut self, e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
                match target {
                    CallTarget::Method { object, .. } => self.visit_expr(object),
                    CallTarget::Computed { callee } => self.visit_expr(callee),
                    CallTarget::Named { .. } | CallTarget::Module { .. } => {}
                }
                args.iter().for_each(|a| self.value(a));
            }
            IrExprKind::RuntimeCall { symbol, args } if !symbol.as_str().ends_with('!') => {
                args.iter().for_each(|a| self.value(a));
            }
            IrExprKind::Record { fields, .. } => fields.iter().for_each(|(_, f)| self.value(f)),
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
                elements.iter().for_each(|x| self.value(x));
            }
            IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => {
                self.value(expr);
            }
            IrExprKind::If { cond, then, else_ } => {
                self.visit_expr(cond);
                self.value(then);
                self.value(else_);
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                arms.iter().for_each(|a| self.arm(a));
            }
            IrExprKind::Block { stmts, expr } => {
                stmts.iter().for_each(|s| self.visit_stmt(s));
                if let Some(t) = expr { self.value(t); }
            }
            _ => return false,
        }
        true
    }
}

impl IrVisitor for Audit<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        if !self.ok { return; }
        match &e.kind {
            // A read through a shared borrow: `&*b` is a `&str` either way,
            // and `&*src` / `&src` leave the source in place.
            IrExprKind::Borrow { expr, mutable: false, as_str } => match var_id(expr) {
                Some(id) if self.sources.contains(&id) => {}
                Some(id) if self.binders.contains(&id) => self.ok = *as_str,
                _ => self.visit_expr(expr),
            },
            IrExprKind::Clone { expr } if self.is_binder(expr) => {}
            // Anything else reaching a binder or the source bare: a use the
            // `&str` spelling is not proved for (a move of the source, a
            // comparison, an interpolation part, a receiver).
            IrExprKind::Var { id } if self.binders.contains(id) || self.sources.contains(id) => self.ok = false,
            // A closure would capture the slice past the match.
            IrExprKind::Lambda { .. } => {
                let free = almide_ir::free_vars::free_vars(e, &HashSet::new());
                if free.iter().any(|id| self.binders.contains(id) || self.sources.contains(id)) {
                    self.ok = false;
                }
            }
            _ => {
                if !self.value_children(e) {
                    walk_expr(self, e);
                }
            }
        }
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        if !self.ok { return; }
        let writes = match &s.kind {
            IrStmtKind::Assign { var, .. } => Some(*var),
            IrStmtKind::IndexAssign { target, .. } | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => Some(*target),
            _ => None,
        };
        if writes.is_some_and(|id| self.sources.contains(&id) || self.binders.contains(&id)) {
            self.ok = false;
            return;
        }
        match &s.kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => self.value(value),
            _ => walk_stmt(self, s),
        }
    }
}

/// Rewrites every remaining owned read of a binder to `b.to_string()`.
struct OwnReads<'a> {
    binders: &'a HashSet<VarId>,
}

impl OwnReads<'_> {
    fn owned(&self, e: &mut IrExpr) -> bool {
        let read = match &e.kind {
            IrExprKind::Var { id } if self.binders.contains(id) => e.clone(),
            IrExprKind::Clone { expr } if var_id(expr).is_some_and(|id| self.binders.contains(&id)) => (**expr).clone(),
            _ => return false,
        };
        let span = read.span;
        *e = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Method { object: Box::new(read), method: sym("to_string") },
                args: vec![],
                type_args: vec![],
            },
            ty: Ty::String,
            span,
            def_id: None,
        };
        true
    }
}

impl IrMutVisitor for OwnReads<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if let IrExprKind::Borrow { expr, as_str: true, .. } = &e.kind
            && var_id(expr).is_some_and(|id| self.binders.contains(&id))
        {
            return;
        }
        if !self.owned(e) {
            walk_expr_mut(self, e);
        }
    }
}

struct Rewriter {
    changed: bool,
}

impl Rewriter {
    fn try_slice(&mut self, e: &mut IrExpr) {
        let IrExprKind::Match { subject, arms } = &mut e.kind else { return };
        let IrExprKind::RuntimeCall { symbol, args } = &mut subject.kind else { return };
        let Some(twin) = slice_twin(symbol.as_str()) else { return };
        let Ok(sources) = args.iter().map(borrowed_source).collect::<Result<Vec<_>, ()>>() else { return };
        let sources: HashSet<VarId> = sources.into_iter().flatten().collect();
        let Some(binders) = payload_binders(arms) else { return };
        let mut audit = Audit { binders: &binders, sources: &sources, ok: true };
        arms.iter().for_each(|a| audit.arm(a));
        if !audit.ok { return; }
        *symbol = sym(twin);
        let mut own = OwnReads { binders: &binders };
        for arm in arms.iter_mut() {
            if let Some(g) = &mut arm.guard { own.visit_expr_mut(g); }
            own.visit_expr_mut(&mut arm.body);
        }
        self.changed = true;
    }
}

impl IrMutVisitor for Rewriter {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        self.try_slice(e);
    }
}
