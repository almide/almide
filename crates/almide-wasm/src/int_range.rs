//! #4611: RANGE facts for `Int` locals, so that a signed division (or
//! remainder) by a power of two whose dividend is provably non-negative
//! lowers to one unsigned shift (or mask) instead of the four-instruction
//! signed sequence (binop.rs `emit_const_div`): for `x >= 0`, `x / 2^k` is
//! `x >>u k` and `x % 2^k` is `x & (2^k - 1)`, bit for bit.
//!
//! A FACT `v ∈ [lo, hi]` says: every value the local `v` can hold where the
//! code being lowered runs lies in `[lo, hi]`. Facts come from two places:
//!
//! - **A straight-line binding** in a statement list (`let v = e`, `var v =
//!   e`, `v = e`) whose value's range is known — literals, variables with
//!   facts, and `+` `-` `*` (and `/` `%` by a positive literal) whose exact
//!   result interval fits in `i64`. An interval that leaves `i64` is no fact:
//!   the wrapped value could be anything, so `(i + j) * (i + j + 1)` is only
//!   known non-negative when its operands are small enough not to wrap.
//! - **A loop's induction variable** ([`Emitter::induction_ranges`]): a
//!   `while` whose body assigns `v` exactly once, as a top-level `v = v + c`
//!   (`0 <= c <= 2^20`), with `v`'s range on entry known. `v` never
//!   decreases, so the entry's lower bound holds at every iteration. The
//!   upper bound comes from what must hold whenever the increment runs:
//!   (a) the condition is `v < X` / `v <= X` with `X`'s range known (and `X`
//!   not assigned in the loop), or (b) an earlier top-level statement of the
//!   body evaluates `xs[v]` (a read or `xs[v] = e`) on every path that
//!   completes it, with `xs` a list local — a list access that completes
//!   normally had `0 <= v < count(xs)`, and a count is at most
//!   `(2^32 - 1) / stride` (the length word is a `u32` byte count).
//!   Then after the increment `v <= bound + c`, and at the loop head
//!   `v ∈ [lo0, max(hi0, bound + c)]`.
//!
//! WHY A FACT STAYS TRUE: facts are only made for locals that are not cells
//! (a cell is memory a closure can write), and only ever by the two rules
//! above. Every other way a local changes is an IR `Bind` / `Assign` /
//! pattern binder, and the statement-list hook ([`Emitter::ranges_enter_stmt`]
//! / [`Emitter::ranges_leave_stmt`]) forgets, BEFORE a statement is lowered,
//! every variable the statement assigns anywhere below its own top level, and
//! after it, every variable it assigns at all — so a fact never survives an
//! assignment it did not see, and a fact made inside a nested list (an `if`
//! arm's block) is forgotten when that list ends ([`Emitter::ranges_leave_list`]),
//! so nothing flows across a join. A loop forgets every variable it assigns
//! before any of its code is lowered (the unrolled lane and the pre-judge
//! included), and its induction facts live only inside it.

use std::collections::{HashMap, HashSet};

use almide_ir::visit::{walk_expr, walk_pattern, walk_stmt, IrVisitor};
use almide_ir::{BinOp, IrExpr, IrExprKind, IrPattern, IrStmt, IrStmtKind, VarId};
use almide_types::types::Ty;

use crate::emitter::Emitter;
use crate::SliceTy;

/// An inclusive interval.
pub(crate) type Range = (i64, i64);

/// The largest increment an induction variable is recognised with.
const MAX_STEP: i64 = 1 << 20;

/// Every variable bound or assigned anywhere inside the visited nodes.
#[derive(Default)]
struct Assigned(HashSet<VarId>);

impl IrVisitor for Assigned {
    fn visit_stmt(&mut self, s: &IrStmt) {
        if let IrStmtKind::Bind { var, .. } | IrStmtKind::Assign { var, .. } = &s.kind {
            self.0.insert(*var);
        }
        walk_stmt(self, s);
    }

    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.0.insert(*var);
                self.0.extend(var_tuple.iter().flatten().copied());
            }
            IrExprKind::Lambda { params, .. } => self.0.extend(params.iter().map(|(v, _)| *v)),
            _ => {}
        }
        walk_expr(self, e);
    }

    fn visit_pattern(&mut self, p: &IrPattern) {
        if let IrPattern::Bind { var, .. } | IrPattern::As { var, .. } = p {
            self.0.insert(*var);
        }
        walk_pattern(self, p);
    }
}

fn assigned_in(stmts: &[IrStmt]) -> HashSet<VarId> {
    let mut a = Assigned::default();
    for s in stmts {
        a.visit_stmt(s);
    }
    a.0
}

/// The variables `s` assigns below its own top level: all of them, except a
/// top-level `Bind` / `Assign`'s own target when its value does not assign
/// it again.
fn nested_assigned(s: &IrStmt) -> HashSet<VarId> {
    match &s.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
            let mut a = Assigned::default();
            a.visit_expr(value);
            a.0
        }
        _ => assigned_in(std::slice::from_ref(s)),
    }
}

fn fits(lo: i128, hi: i128) -> Option<(i128, i128)> {
    (lo >= i128::from(i64::MIN) && hi <= i128::from(i64::MAX) && lo <= hi).then_some((lo, hi))
}

fn lit(e: &IrExpr) -> Option<i64> {
    match e.kind {
        IrExprKind::LitInt { value } => Some(value),
        _ => None,
    }
}

/// `v = v + c` / `v = c + v` with `0 <= c <= MAX_STEP`: `(v, c)`.
fn increment(s: &IrStmt) -> Option<(VarId, i64)> {
    let IrStmtKind::Assign { var, value } = &s.kind else { return None };
    let IrExprKind::BinOp { op: BinOp::AddInt, left, right } = &value.kind else { return None };
    let is_v = |e: &IrExpr| matches!(e.kind, IrExprKind::Var { id } if id == *var);
    let c = if is_v(left) { lit(right)? } else if is_v(right) { lit(left)? } else { return None };
    (0..=MAX_STEP).contains(&c).then_some((*var, c))
}

impl Emitter<'_> {
    /// `v` can carry a fact: an `Int` local that is not a cell.
    fn rangeable(&self, v: VarId) -> bool {
        self.locals.get(&v).is_some_and(|&(_, t)| t == crate::INT) && !self.cells.contains(&v)
    }

    /// The exact interval of `e`'s value, when the facts decide it and it
    /// cannot wrap (module header).
    pub(crate) fn int_range(&self, e: &IrExpr) -> Option<Range> {
        self.range128(e).map(|(lo, hi)| (lo as i64, hi as i64))
    }

    fn range128(&self, e: &IrExpr) -> Option<(i128, i128)> {
        if e.ty != Ty::Int {
            return None;
        }
        match &e.kind {
            IrExprKind::LitInt { value } => Some((i128::from(*value), i128::from(*value))),
            IrExprKind::Var { id } => self.int_ranges.get(id).map(|&(lo, hi)| (i128::from(lo), i128::from(hi))),
            IrExprKind::BinOp { op, left, right } => {
                let (a, b) = self.range128(left)?;
                match op {
                    BinOp::DivInt | BinOp::ModInt => {
                        let c = i128::from(lit(right)?);
                        if c <= 0 {
                            return None;
                        }
                        if matches!(op, BinOp::DivInt) {
                            // Truncating division by a positive constant is
                            // monotone.
                            fits(a / c, b / c)
                        } else if a >= 0 {
                            fits(0, b.min(c - 1))
                        } else {
                            fits(-(c - 1), c - 1)
                        }
                    }
                    BinOp::AddInt | BinOp::SubInt | BinOp::MulInt => {
                        let (c, d) = self.range128(right)?;
                        match op {
                            BinOp::AddInt => fits(a + c, b + d),
                            BinOp::SubInt => fits(a - d, b - c),
                            _ => {
                                // |operands| < 2^63: every product fits i128.
                                let p = [a * c, a * d, b * c, b * d];
                                fits(*p.iter().min()?, *p.iter().max()?)
                            }
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// `e` is provably `>= 0`.
    pub(crate) fn int_nonneg(&self, e: &IrExpr) -> bool {
        self.int_range(e).is_some_and(|(lo, _)| lo >= 0)
    }

    /// Before statement `s` of a statement list is lowered (module header).
    /// A `while` statement first hands its loop the entry ranges of the
    /// variables the loop assigns (`lower_while` takes them).
    pub(crate) fn ranges_enter_stmt(&mut self, s: &IrStmt) {
        if let IrStmtKind::Expr { expr } = &s.kind
            && let IrExprKind::While { cond, body } = &expr.kind
        {
            let mut a = Assigned::default();
            a.visit_expr(cond);
            for st in body {
                a.visit_stmt(st);
            }
            let entry: HashMap<VarId, Range> =
                a.0.iter().filter_map(|v| self.int_ranges.get(v).map(|&r| (*v, r))).collect();
            self.loop_entry_ranges = Some((std::ptr::from_ref::<IrExpr>(&**cond) as usize, entry));
        }
        for v in nested_assigned(s) {
            self.int_ranges.remove(&v);
        }
    }

    /// After statement `s` was lowered: forget what it assigned, then learn
    /// a top-level binding's value range.
    pub(crate) fn ranges_leave_stmt(&mut self, s: &IrStmt) {
        let learned = match &s.kind {
            IrStmtKind::Bind { var, value, .. } | IrStmtKind::Assign { var, value } if self.rangeable(*var) => {
                self.int_range(value).map(|r| (*var, r))
            }
            _ => None,
        };
        for v in assigned_in(std::slice::from_ref(s)) {
            self.int_ranges.remove(&v);
        }
        if let Some((v, r)) = learned {
            self.int_ranges.insert(v, r);
        }
    }

    /// A statement list ended: what it assigned is forgotten, so no fact
    /// made inside it reaches past its join.
    pub(crate) fn ranges_leave_list(&mut self, stmts: &[IrStmt]) {
        for v in assigned_in(stmts) {
            self.int_ranges.remove(&v);
        }
    }

    /// `lower_while`'s first act: take the entry ranges the statement hook
    /// left for THIS loop, then forget every variable the loop assigns.
    pub(crate) fn ranges_enter_loop(&mut self, cond: &IrExpr, body: &[IrStmt]) -> HashMap<VarId, Range> {
        let key = std::ptr::from_ref::<IrExpr>(cond) as usize;
        let entry = match self.loop_entry_ranges.take() {
            Some((k, e)) if k == key => e,
            _ => HashMap::new(),
        };
        let mut a = Assigned::default();
        a.visit_expr(cond);
        for st in body {
            a.visit_stmt(st);
        }
        for v in &a.0 {
            self.int_ranges.remove(v);
        }
        entry
    }

    /// After the loop: forget every variable it assigns again (its own
    /// induction facts and anything its body learned).
    pub(crate) fn ranges_leave_loop(&mut self, cond: &IrExpr, body: &[IrStmt]) {
        let mut a = Assigned::default();
        a.visit_expr(cond);
        for st in body {
            a.visit_stmt(st);
        }
        for v in a.0 {
            self.int_ranges.remove(&v);
        }
    }

    /// The loop's induction variables (module header): `(v, range at the
    /// loop head, range in the body before its increment)`.
    pub(crate) fn induction_ranges(
        &self,
        cond: &IrExpr,
        body: &[IrStmt],
        entry: &HashMap<VarId, Range>,
    ) -> Vec<(VarId, Range, Range)> {
        let mut out = Vec::new();
        for (k, st) in body.iter().enumerate() {
            let Some((v, c)) = increment(st) else { continue };
            let Some(&(lo0, hi0)) = entry.get(&v) else { continue };
            if !self.rangeable(v) || Self::assignments_of(cond, body, v) != 1 {
                continue;
            }
            let c = i128::from(c);
            // (a) the condition bounds v in the body.
            let by_cond = match &cond.kind {
                IrExprKind::BinOp { op: op @ (BinOp::Lt | BinOp::Lte), left, right }
                    if matches!(left.kind, IrExprKind::Var { id } if id == v) =>
                {
                    self.range128(right).map(|(_, x)| if matches!(op, BinOp::Lt) { x - 1 } else { x })
                }
                _ => None,
            };
            // (b) a completed list access before the increment.
            let by_access = body[..k]
                .iter()
                .filter_map(|s| self.access_stride(s, v))
                .max()
                .map(|stride| i128::from(u32::MAX) / i128::from(stride) - 1);
            let Some(ub) = [by_cond, by_access].into_iter().flatten().min() else { continue };
            let head_hi = i128::from(hi0).max(ub + c);
            let Some((lo, hi)) = fits(i128::from(lo0), head_hi) else { continue };
            let body_hi = by_cond.map_or(hi, |b| hi.min(b));
            let Some(body_r) = fits(lo, body_hi) else { continue };
            out.push((v, (lo as i64, hi as i64), (body_r.0 as i64, body_r.1 as i64)));
        }
        out
    }

    /// How many statements in the loop assign `v`.
    fn assignments_of(cond: &IrExpr, body: &[IrStmt], v: VarId) -> usize {
        struct Count(VarId, usize);
        impl IrVisitor for Count {
            fn visit_stmt(&mut self, s: &IrStmt) {
                if let IrStmtKind::Bind { var, .. } | IrStmtKind::Assign { var, .. } = &s.kind
                    && *var == self.0
                {
                    self.1 += 1;
                }
                walk_stmt(self, s);
            }
            fn visit_expr(&mut self, e: &IrExpr) {
                match &e.kind {
                    IrExprKind::ForIn { var, var_tuple, .. }
                        if *var == self.0 || var_tuple.iter().flatten().any(|x| *x == self.0) =>
                    {
                        self.1 += 2;
                    }
                    IrExprKind::Lambda { .. } => self.1 += 2,
                    _ => {}
                }
                walk_expr(self, e);
            }
            fn visit_pattern(&mut self, p: &IrPattern) {
                if let IrPattern::Bind { var, .. } | IrPattern::As { var, .. } = p
                    && *var == self.0
                {
                    self.1 += 2;
                }
                walk_pattern(self, p);
            }
        }
        let mut c = Count(v, 0);
        c.visit_expr(cond);
        for s in body {
            c.visit_stmt(s);
        }
        c.1
    }

    /// The slot stride of a list `xs` whose `xs[v]` statement `s` evaluates
    /// on every path that completes it.
    fn access_stride(&self, s: &IrStmt, v: VarId) -> Option<u32> {
        match &s.kind {
            IrStmtKind::IndexAssign { target, index, value } => {
                let own = matches!(index.kind, IrExprKind::Var { id } if id == v).then(|| self.list_stride(*target)).flatten();
                own.or_else(|| self.completed_access(index, v)).or_else(|| self.completed_access(value, v))
            }
            IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => self.completed_access(value, v),
            IrStmtKind::Expr { expr } => self.completed_access(expr, v),
            _ => None,
        }
    }

    /// A list local's slot stride (none for a cell, a global or a zero-size
    /// slot).
    fn list_stride(&self, xs: VarId) -> Option<u32> {
        if self.cells.contains(&xs) {
            return None;
        }
        let &(_, SliceTy::List(h)) = self.locals.get(&xs)? else { return None };
        let s = self.types.el(h).slot_size();
        (s > 0).then_some(s)
    }

    /// `e` evaluates `xs[v]` (a list local `xs`) whenever it completes:
    /// that list's stride.
    fn completed_access(&self, e: &IrExpr, v: VarId) -> Option<u32> {
        let sub = |x: &IrExpr| self.completed_access(x, v);
        match &e.kind {
            IrExprKind::IndexAccess { object, index } => {
                let own = match (&object.kind, &index.kind) {
                    (IrExprKind::Var { id: xs }, IrExprKind::Var { id }) if *id == v => self.list_stride(*xs),
                    _ => None,
                };
                own.or_else(|| sub(object)).or_else(|| sub(index))
            }
            IrExprKind::BinOp { op: BinOp::And | BinOp::Or, left, .. } => sub(left),
            IrExprKind::BinOp { left, right, .. } => sub(left).or_else(|| sub(right)),
            IrExprKind::UnOp { operand, .. } => sub(operand),
            IrExprKind::If { cond, .. } => sub(cond),
            IrExprKind::Call { args, .. } => args.iter().find_map(sub),
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => sub(object),
            IrExprKind::Block { stmts, expr } => {
                stmts.iter().find_map(|s| self.access_stride(s, v)).or_else(|| expr.as_deref().and_then(sub))
            }
            _ => None,
        }
    }
}
