//! SharedCellBorrowPass (#1143): borrow a captured cell's value in place for
//! reads a statement proves safe, instead of `.get()`-cloning it per read.
//!
//! A closure-captured mutable heap local (`var stats: Map[...]` used from a
//! lambda) lowers to a `AlmideSharedMut` cell (`Rc<RefCell<T>>`). A READ of it in
//! borrow position emitted `&cap.get()` — a deep clone of the whole value
//! per read, which made every `map.get` through a closure clone the Map
//! (~5x on the onebrc aggregation loop). A mutating access already writes
//! in place (`&mut *cap.borrow_mut()`); this pass lets the safe reads do
//! the same (`&*cap.borrow()`).
//!
//! MARKER, not a schema change: a qualifying read `Borrow { Var v }` is
//! rewritten to `Borrow { Deref { Var v } }` — a shape that cannot
//! otherwise occur on a `AlmideSharedMut` var (the cell type has no `Deref`
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
//!   `AlmideSharedMut` exists only as a closure-capture representation, and a
//!   nested statement (its own semicolon scope) is still visited and
//!   marked independently.
//! - a `match f(read) { .. }` whose SUBJECT holds the reads is first
//!   hoisted to `{ let s = f(read); match s { .. } }`: the bind ends the
//!   guard before the arms run, and the bind statement then qualifies on
//!   its own.
//!
//! v0/Rust-target only: the v1 MIR path has its own ownership model, and
//! the interpreter consumes the pre-codegen IR — neither sees this marker.

use std::collections::{HashMap, HashSet};
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
            let fn_cells: Vec<VarId> = cells
                .iter()
                .filter(|v| !f.params.iter().any(|p| p.var == **v))
                .copied()
                .collect();
            if fn_cells.is_empty() {
                continue;
            }
            let body = std::mem::take(&mut f.body);
            f.body = hoist_match_subjects(body, &fn_cells, &mut program.var_table, &mut changed);
            let qualifying = StmtSummaries::of(&f.body, &fn_cells);
            let mut marks = MarkActive { cells: &fn_cells, qualifying: &qualifying, active: vec![0; fn_cells.len()], changed: &mut changed };
            marks.visit_expr_mut(&mut f.body);
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
    cells: &[VarId],
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
fn touches_any(expr: &IrExpr, cells: &[VarId]) -> bool {
    struct Touch<'a> {
        cells: &'a [VarId],
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

// ── Step 2: per-statement summaries, then marking ────────────────────────
//
// The per-statement decision (reject on any hard-to-reason shape in the
// subtree, then, per cell var, require every use to be a shared borrow in
// direct call-arg position) used to be three subtree scans per statement
// per cell — and a statement nested k levels deep was rescanned by each of
// its k enclosing statements. Every quantity the decision reads is a SUM
// over the subtree's nodes (disqualifying nodes, `Var v` nodes, good
// call-arg reads of `v`), so one walk keeping running totals gives each
// statement its summary as "counters after minus counters before" (#1232).
// The marking is then a second single walk: a read is rewritten iff some
// enclosing statement qualified for its var — which is what visiting every
// statement with a fresh rescan computed, since the marker is idempotent
// and invisible to the counts (`shared_cell_read_of` accepts both forms).

/// Which cells each statement qualifies for, keyed by the statement's heap
/// address — the same keying as `pass_clone.rs`'s `BranchCounts` (#1230):
/// the marking walk mutates borrow nodes in place and never moves a
/// statement, so every key still names its original statement.
struct StmtSummaries {
    qualifying: HashMap<*const IrStmt, Vec<VarId>>,
}

impl StmtSummaries {
    fn of(body: &IrExpr, cells: &[VarId]) -> StmtSummaries {
        let mut s = Summarize {
            cells,
            disq: 0,
            total: vec![0; cells.len()],
            good: vec![0; cells.len()],
            stack: Vec::new(),
            qualifying: HashMap::new(),
        };
        s.visit_expr(body);
        StmtSummaries { qualifying: s.qualifying }
    }
}

/// The running-total walk behind [`StmtSummaries`].
struct Summarize<'a> {
    cells: &'a [VarId],
    /// Disqualifying nodes seen so far (see [`is_disqualifying_shape`]).
    disq: u32,
    /// Per cell slot: `Var v` nodes seen so far.
    total: Vec<u32>,
    /// Per cell slot: good call-arg reads seen so far (see `count_good`).
    good: Vec<u32>,
    /// Snapshots of the three counters at each open statement, flat:
    /// `disq`, then `total`, then `good` — no allocation per statement.
    stack: Vec<u32>,
    qualifying: HashMap<*const IrStmt, Vec<VarId>>,
}

impl Summarize<'_> {
    fn slot(&self, id: VarId) -> Option<usize> {
        self.cells.iter().position(|c| *c == id)
    }

    /// The `good` contribution of one call node: each argument sitting in
    /// the exact safe shape (a shared `Borrow` directly on a cell var, or
    /// the already-marked `&*v` form) counts once for that var. Each such
    /// argument contains exactly one `Var v` node, so `good == total` over a
    /// statement means no use of `v` escapes the safe shape.
    fn count_good(&mut self, args: &[IrExpr]) {
        for a in args {
            if let Some(k) = shared_cell_read_of(a).and_then(|id| self.slot(id)) {
                self.good[k] += 1;
            }
        }
    }

    /// The cells `stmt` qualifies for, given the counters snapshotted at
    /// `base` when the statement was entered.
    fn qualifying_cells(&self, stmt: &IrStmt, base: usize) -> Vec<VarId> {
        let n = self.cells.len();
        if self.disq != self.stack[base] {
            return Vec::new();
        }
        let (total0, good0) = (&self.stack[base + 1..base + 1 + n], &self.stack[base + 1 + n..base + 1 + 2 * n]);
        (0..n)
            .filter(|&k| {
                let total = self.total[k] - total0[k];
                let good = self.good[k] - good0[k];
                total > 0 && good == total && !stmt_writes_var(stmt, self.cells[k])
            })
            .map(|k| self.cells[k])
            .collect()
    }
}

impl IrVisitor for Summarize<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        self.disq += u32::from(is_disqualifying_shape(e));
        if let IrExprKind::Var { id } = &e.kind {
            if let Some(k) = self.slot(*id) {
                self.total[k] += 1;
            }
        }
        if let IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } | IrExprKind::TailCall { args, .. } = &e.kind {
            self.count_good(args);
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        let base = self.stack.len();
        self.stack.push(self.disq);
        self.stack.extend_from_slice(&self.total);
        self.stack.extend_from_slice(&self.good);
        walk_stmt(self, stmt);
        let cells = self.qualifying_cells(stmt, base);
        if !cells.is_empty() {
            self.qualifying.insert(std::ptr::from_ref(stmt), cells);
        }
        self.stack.truncate(base);
    }
}

/// Shapes this pass refuses to reason about within one statement scope.
fn is_disqualifying_shape(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Lambda { .. }
        | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. }
        | IrExprKind::RenderedCall { .. } => true,
        IrExprKind::Call { target: CallTarget::Computed { .. }, .. } => true,
        IrExprKind::Match { subject, .. } => !matches!(subject.kind, IrExprKind::Var { .. }),
        _ => false,
    }
}

/// Statement-kind write targets are VarIds, invisible to the expr walk.
fn stmt_writes_var(stmt: &IrStmt, v: VarId) -> bool {
    match &stmt.kind {
        IrStmtKind::Assign { var, .. } | IrStmtKind::Bind { var, .. } => *var == v,
        IrStmtKind::IndexAssign { target, .. }
        | IrStmtKind::MapInsert { target, .. }
        | IrStmtKind::FieldAssign { target, .. } => *target == v,
        _ => false,
    }
}

/// `&v` (shared, not `&mut`) directly on a var — or the already marked `&*v`
/// form from an earlier marking — returns that var.
fn shared_cell_read_of(arg: &IrExpr) -> Option<VarId> {
    let IrExprKind::Borrow { expr: inner, mutable: false, .. } = &arg.kind else {
        return None;
    };
    match &inner.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Deref { expr: dinner } => match &dinner.kind {
            IrExprKind::Var { id } => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

/// The marking walk: a shared `Borrow { Var v }` becomes the
/// `Borrow { Deref { Var v } }` marker iff `v` is ACTIVE — some enclosing
/// statement qualified for it. `active[k]` counts the open statements that
/// qualified for cell slot `k`, so nesting composes by increment/decrement.
struct MarkActive<'a> {
    cells: &'a [VarId],
    qualifying: &'a StmtSummaries,
    active: Vec<u32>,
    changed: &'a mut bool,
}

impl MarkActive<'_> {
    fn slot(&self, id: VarId) -> Option<usize> {
        self.cells.iter().position(|c| *c == id)
    }

    fn is_active(&self, id: VarId) -> bool {
        self.slot(id).is_some_and(|k| self.active[k] > 0)
    }
}

impl IrMutVisitor for MarkActive<'_> {
    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        let key = std::ptr::from_ref(&*stmt);
        let opened: Vec<usize> = self
            .qualifying
            .qualifying
            .get(&key)
            .map(|cells| cells.iter().filter_map(|v| self.slot(*v)).collect())
            .unwrap_or_default();
        for &k in &opened {
            self.active[k] += 1;
        }
        walk_stmt_mut(self, stmt);
        for &k in &opened {
            self.active[k] -= 1;
        }
    }

    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        if let IrExprKind::Borrow { expr: inner, mutable: false, .. } = &mut expr.kind {
            if matches!(&inner.kind, IrExprKind::Var { id } if self.is_active(*id)) {
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
