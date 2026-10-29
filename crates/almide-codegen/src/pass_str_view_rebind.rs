//! StrViewRebindPass: which binding arms of a str-view match own their binder
//! (#4296).
//!
//! Target: Rust only. Runs last, on the final IR the walker renders.
//!
//! A match over a `&str` view of its `String` subject (`MatchSubject`:
//! `match &*x { "a" => .., other => .. }`) binds `other` as a `&str` into the
//! subject. The subject stays borrowed (`Site::StrView`); a binding arm whose
//! binder is wanted as a value — returned, stored, handed on, cloned — takes
//! its owned copy at the head of THAT arm (`let other: AlmideString =
//! other.to_almide();`), so the copy is paid only where the binder is owned.
//! A binder only read as `&*other` keeps the view, unless the arm also
//! touches the subject's variable (a move of it there would outlive the
//! borrow the view holds).
//!
//! The verdict is published as `str_view_owned_binders`; the walker spells
//! the rebind for exactly those binders.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct StrViewRebindPass;

impl NanoPass for StrViewRebindPass {
    fn name(&self) -> &str { "StrViewRebind" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    /// Reads the final spellings of the arm bodies — every Borrow / Clone any
    /// earlier pass places — so every pass declared before it stays before
    /// it, the shuffle included.
    fn barrier(&self) -> bool { true }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut v = Matches::default();
        for func in &program.functions {
            v.visit_expr(&func.body);
        }
        for tl in &program.top_lets {
            v.visit_expr(&tl.value);
        }
        for module in &program.modules {
            for func in &module.functions {
                v.visit_expr(&func.body);
            }
            for tl in &module.top_lets {
                v.visit_expr(&tl.value);
            }
        }
        let ann = &program.codegen_annotations;
        for e in ann.default_fields.values().chain(ann.case_default_fields.values()) {
            v.visit_expr(e);
        }
        program.codegen_annotations.str_view_owned_binders = v.owned;
        PassResult { program, changed: false }
    }
}

/// Every str-view match in a body, nested ones included.
#[derive(Default)]
struct Matches {
    owned: HashSet<VarId>,
}

impl IrVisitor for Matches {
    fn visit_expr(&mut self, e: &IrExpr) {
        if let IrExprKind::Match { subject, arms } = &e.kind
            && crate::pass_match_subject::matches_through_str_view(&subject.ty, arms)
        {
            let root = str_view_subject_root(subject);
            self.owned.extend(arms.iter().filter_map(|arm| owned_binder(arm, root)));
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) { walk_stmt(self, s); }
}

/// The variable a str-view match subject reads (`&*x`, `x.as_str()`,
/// `&*t.name`), through the borrow / clone / projection a pass wrapped it in.
fn str_view_subject_root(subject: &IrExpr) -> Option<VarId> {
    match &subject.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Borrow { expr, .. } | IrExprKind::Clone { expr } | IrExprKind::Deref { expr } => str_view_subject_root(expr),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => str_view_subject_root(object),
        IrExprKind::Call { target: CallTarget::Method { object, .. }, args, .. } if args.is_empty() => str_view_subject_root(object),
        _ => None,
    }
}

/// The arm's binder, when its body wants it owned: read as a value anywhere,
/// or read at all in an arm that also touches the subject's variable.
fn owned_binder(arm: &IrMatchArm, subject_root: Option<VarId>) -> Option<VarId> {
    let var = match &arm.pattern {
        IrPattern::Bind { var, .. } | IrPattern::As { var, .. } => *var,
        _ => return None,
    };
    let mut scan = Scan { var, root: subject_root, used: false, owned: false, root_seen: false };
    scan.visit_expr(&arm.body);
    (scan.used && (scan.owned || scan.root_seen)).then_some(var)
}

struct Scan { var: VarId, root: Option<VarId>, used: bool, owned: bool, root_seen: bool }

impl IrVisitor for Scan {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Borrow { expr, as_str: true, mutable: false }
                if matches!(expr.kind, IrExprKind::Var { id } if id == self.var) => self.used = true,
            IrExprKind::Var { id } if *id == self.var => { self.used = true; self.owned = true; }
            IrExprKind::Var { id } if Some(*id) == self.root => self.root_seen = true,
            _ => walk_expr(self, e),
        }
    }
    fn visit_stmt(&mut self, s: &IrStmt) { walk_stmt(self, s); }
}
