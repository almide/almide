//! SharedCellBorrowPass (#1143): borrow a captured cell's value in place for
//! reads a statement proves safe, instead of `.get()`-cloning it per read.
//!
//! A closure-captured mutable heap local (`var stats: Map[...]` used from a
//! lambda) lowers to a `SharedMut` cell (`Rc<RefCell<T>>`). A READ of it in
//! borrow position emitted `&cap.get()` — a deep clone of the whole value
//! per read, which made every `map.get` through a closure clone the Map
//! (~5x on the onebrc aggregation loop). A mutating access already writes
//! in place (`&mut *cap.borrow_mut()`); this pass lets the safe reads do
//! the same (`&*cap.borrow()`).
//!
//! MARKER, not a schema change: a qualifying read `Borrow { Var v }` is
//! rewritten to `Borrow { Deref { Var v } }` — a shape that cannot
//! otherwise occur on a `SharedMut` var (the cell type has no `Deref`
//! impl, so the generic `&*v` render would not compile) — and the walker's
//! borrow renderer turns exactly that shape into `&*v.borrow()`. Unmarked
//! reads keep the owned `.get()` snapshot.
//!
//! Safety argument — a marked statement cannot RefCell-panic:
//! - marking requires EVERY use of the var in the statement's subtree to be
//!   a shared, non-mutable `Borrow` in direct call-argument position; the
//!   resulting guards are all shared borrows of the same cell, `RefCell`
//!   admits any number of overlapping shared borrows, and every guard dies
//!   at the statement's semicolon.
//! - the statement's subtree must contain no lambda literal, no
//!   computed-callee call, no `for`/`while`, no `match` with a non-Var
//!   subject, and no pre-rendered code — the shapes under which a closure
//!   aliasing the same cell could run (and take a mut borrow) while a
//!   guard is live, or whose temporary-lifetime extension keeps a guard
//!   alive into arm/body code. Named user fns cannot hold the cell:
//!   `SharedMut` exists only as a closure-capture representation, and a
//!   nested statement (its own semicolon scope) is still visited and
//!   marked independently.
//! - a `match f(read) { .. }` whose SUBJECT holds the reads is first
//!   hoisted to `{ let s = f(read); match s { .. } }`: the bind ends the
//!   guard before the arms run, and the bind statement then qualifies on
//!   its own.
//!
//! v0/Rust-target only: the v1 MIR path has its own ownership model, and
//! the interpreter consumes the pre-codegen IR — neither sees this marker.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use almide_base::intern::sym;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct SharedCellBorrowPass;

impl NanoPass for SharedCellBorrowPass {
    fn name(&self) -> &str { "SharedCellBorrow" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let cells: HashSet<VarId> = program
            .codegen_annotations
            .shared_mut_vars
            .iter()
            .filter(|v| {
                let ty = &program.var_table.get(**v).ty;
                // Copy cells stay on the cheap `Cell.get()` path; String cells
                // stay on `.get()` because their borrow sites may need `&str`
                // (`as_str`), which `&*cell.borrow()` does not produce.
                !top_let_storage::capture_copy_cell(ty)
                    && !matches!(ty, almide_lang::types::Ty::String)
            })
            .copied()
            .collect();
        if cells.is_empty() {
            return PassResult { program, changed: false };
        }
        let mut changed = false;
        let mut fns = std::mem::take(&mut program.functions);
        for f in fns.iter_mut() {
            // Fn-local truth: `optimize/branch_lift.rs` helpers KEEP the
            // captured free vars' ids as their params, where the binding is
            // a plain snapshot, not a cell — never mark those.
            let fn_cells: HashSet<VarId> = cells
                .iter()
                .filter(|v| !f.params.iter().any(|p| p.var == **v))
                .copied()
                .collect();
            if fn_cells.is_empty() {
                continue;
            }
            let body = std::mem::take(&mut f.body);
            f.body = hoist_match_subjects(body, &fn_cells, &mut program.var_table, &mut changed);
            let mut scopes = ScopeWalker { cells: &fn_cells, changed: &mut changed };
            scopes.visit_expr_mut(&mut f.body);
        }
        program.functions = fns;
        PassResult { program, changed }
    }
}

// ── Step 1: hoist match subjects that hold cell reads ────────────────────

/// `match f(&cell) { arms }` extends the subject's temporaries across the
/// arms, so a borrow guard in the subject would overlap the arms'
/// `borrow_mut`. Rewrite to `{ let __scb = f(&cell); match __scb { arms } }`
/// — the guard ends at the bind, and the bind statement can then qualify
/// for the borrow marking on its own.
fn hoist_match_subjects(
    expr: IrExpr,
    cells: &HashSet<VarId>,
    vt: &mut VarTable,
    changed: &mut bool,
) -> IrExpr {
    let rebuilt = expr.map_children(&mut |c| hoist_match_subjects(c, cells, vt, changed));
    let needs_hoist = matches!(
        &rebuilt.kind,
        IrExprKind::Match { subject, .. }
            if !matches!(subject.kind, IrExprKind::Var { .. })
                && touches_any(subject, cells)
    );
    if !needs_hoist {
        return rebuilt;
    }
    let IrExpr { kind: IrExprKind::Match { subject, arms }, ty, span, def_id } = rebuilt else {
        unreachable!("needs_hoist checked the Match shape")
    };
    let subj_ty = subject.ty.clone();
    let v = vt.alloc(sym("__scb_subj"), subj_ty.clone(), Mutability::Let, span);
    let bind = IrStmt {
        kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty: subj_ty.clone(), value: *subject },
        span,
    };
    let subj_var = IrExpr { kind: IrExprKind::Var { id: v }, ty: subj_ty, span, def_id: None };
    let hoisted = IrExpr {
        kind: IrExprKind::Match { subject: Box::new(subj_var), arms },
        ty: ty.clone(),
        span,
        def_id,
    };
    *changed = true;
    IrExpr {
        kind: IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(hoisted)) },
        ty,
        span,
        def_id: None,
    }
}

/// Does the subtree contain a `Var` naming any of the cell vars?
fn touches_any(expr: &IrExpr, cells: &HashSet<VarId>) -> bool {
    struct Touch<'a> {
        cells: &'a HashSet<VarId>,
        found: bool,
    }
    impl IrVisitor for Touch<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.found {
                return;
            }
            if let IrExprKind::Var { id } = &e.kind {
                if self.cells.contains(id) {
                    self.found = true;
                    return;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut t = Touch { cells, found: false };
    t.visit_expr(expr);
    t.found
}

// ── Step 2: per-statement analysis + marking ─────────────────────────────

/// Visits every statement in the body (any nesting level — lambda bodies,
/// loop bodies, match arms) and tries to mark its cell reads.
struct ScopeWalker<'a> {
    cells: &'a HashSet<VarId>,
    changed: &'a mut bool,
}

impl IrMutVisitor for ScopeWalker<'_> {
    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        try_mark_stmt(stmt, self.cells, self.changed);
        walk_stmt_mut(self, stmt);
    }
}

/// The per-statement decision: reject on any hard-to-reason shape in the
/// subtree, then, per cell var, require every use to be a shared borrow in
/// direct call-arg position before rewriting those reads to the marker.
fn try_mark_stmt(stmt: &mut IrStmt, cells: &HashSet<VarId>, changed: &mut bool) {
    if stmt_has_disqualifying_shape(stmt) {
        return;
    }
    for v in cells {
        if stmt_writes_var(stmt, *v) {
            continue;
        }
        let uses = count_uses(stmt, *v);
        if uses.total > 0 && uses.good == uses.total {
            let mut m = MarkReads { v: *v, changed };
            m.visit_stmt_mut(stmt);
        }
    }
}

/// Shapes this pass refuses to reason about within one statement scope.
fn stmt_has_disqualifying_shape(stmt: &IrStmt) -> bool {
    struct Scan {
        disq: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.disq {
                return;
            }
            if matches!(
                &e.kind,
                IrExprKind::Lambda { .. }
                    | IrExprKind::ForIn { .. }
                    | IrExprKind::While { .. }
                    | IrExprKind::RenderedCall { .. }
            ) {
                self.disq = true;
                return;
            }
            if let IrExprKind::Call { target: CallTarget::Computed { .. }, .. } = &e.kind {
                self.disq = true;
                return;
            }
            if let IrExprKind::Match { subject, .. } = &e.kind {
                if !matches!(subject.kind, IrExprKind::Var { .. }) {
                    self.disq = true;
                    return;
                }
            }
            walk_expr(self, e);
        }
    }
    let mut s = Scan { disq: false };
    s.visit_stmt(stmt);
    s.disq
}

/// Statement-kind write targets are VarIds, invisible to the expr walk.
fn stmt_writes_var(stmt: &IrStmt, v: VarId) -> bool {
    if let IrStmtKind::Assign { var, .. } = &stmt.kind {
        return *var == v;
    }
    if let IrStmtKind::Bind { var, .. } = &stmt.kind {
        return *var == v;
    }
    if let IrStmtKind::IndexAssign { target, .. } = &stmt.kind {
        return *target == v;
    }
    if let IrStmtKind::MapInsert { target, .. } = &stmt.kind {
        return *target == v;
    }
    if let IrStmtKind::FieldAssign { target, .. } = &stmt.kind {
        return *target == v;
    }
    false
}

struct Uses {
    total: usize,
    good: usize,
}

/// `total` counts every `Var v` node; `good` counts the ones sitting in the
/// exact safe shape (a shared `Borrow` that is a direct call argument).
/// Each good argument contains exactly one `Var v` node, so
/// `good == total` means no use escapes the safe shape.
fn count_uses(stmt: &IrStmt, v: VarId) -> Uses {
    struct Count {
        v: VarId,
        total: usize,
        good: usize,
    }
    impl Count {
        fn scan_args(&mut self, args: &[IrExpr]) {
            for a in args {
                if is_shared_cell_read(a, self.v) {
                    self.good += 1;
                }
            }
        }
    }
    impl IrVisitor for Count {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                if *id == self.v {
                    self.total += 1;
                }
            }
            if let IrExprKind::Call { args, .. } = &e.kind {
                self.scan_args(args);
            } else if let IrExprKind::RuntimeCall { args, .. } = &e.kind {
                self.scan_args(args);
            } else if let IrExprKind::TailCall { args, .. } = &e.kind {
                self.scan_args(args);
            }
            walk_expr(self, e);
        }
    }
    let mut c = Count { v, total: 0, good: 0 };
    c.visit_stmt(stmt);
    Uses { total: c.total, good: c.good }
}

/// `&v` (shared, not `&mut`) directly on the cell var — or the already
/// marked `&*v` form from an enclosing scope's earlier visit.
fn is_shared_cell_read(arg: &IrExpr, v: VarId) -> bool {
    let IrExprKind::Borrow { expr: inner, mutable: false, .. } = &arg.kind else {
        return false;
    };
    if matches!(&inner.kind, IrExprKind::Var { id } if *id == v) {
        return true;
    }
    if let IrExprKind::Deref { expr: dinner } = &inner.kind {
        return matches!(&dinner.kind, IrExprKind::Var { id } if *id == v);
    }
    false
}

/// Rewrite every shared `Borrow { Var v }` in the statement into the
/// `Borrow { Deref { Var v } }` marker. Only run when the statement
/// qualified, so every such site is a shared call-arg read.
struct MarkReads<'a> {
    v: VarId,
    changed: &'a mut bool,
}

impl IrMutVisitor for MarkReads<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        if let IrExprKind::Borrow { expr: inner, mutable: false, .. } = &mut expr.kind {
            if matches!(&inner.kind, IrExprKind::Var { id } if *id == self.v) {
                let var_expr = std::mem::take(inner.as_mut());
                **inner = IrExpr {
                    ty: var_expr.ty.clone(),
                    span: var_expr.span,
                    def_id: None,
                    kind: IrExprKind::Deref { expr: Box::new(var_expr) },
                };
                *self.changed = true;
                return;
            }
        }
        walk_expr_mut(self, expr);
    }
}
