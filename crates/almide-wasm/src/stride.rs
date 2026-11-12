//! Strided counting loops: a `for i in a..b` whose body skips every value
//! of one parity runs over the other parity only, two values per trip.
//!
//! Three steps, each keyed on the IR's shape and nothing else:
//!
//! 1. **Parity through an odd multiplier** ([`parity`]). Two's-complement
//!    `i64` arithmetic wraps, and wrapping keeps the low bit: `x * k` has
//!    the parity of `x` when `k` is odd, `x ± k` flips it when `k` is odd,
//!    `-x` keeps it. So a test `e % 2 == 0` (or `!= 0`, under `not`) of an
//!    expression built that way from the loop variable — directly or
//!    through the immutable `Int` lets that open the body — is a test of
//!    the loop variable's own low bit. `% 2` against any other remainder is
//!    sign-dependent (`-3 % 2 == -1`) and not claimed.
//! 2. **Strided loop** ([`Emitter::try_strided_range`]). When the statement
//!    after those lets skips the iteration (`guard c else continue`, or
//!    `if c then continue`) on such a test, the kept values are exactly
//!    the range's members of one parity: the loop starts at the first of
//!    them, steps by 2, and the skip test is gone. The opening lets are
//!    pure wrapping arithmetic (`+`, `-`, `*`, negation over `Int`), so not
//!    evaluating them for a skipped value changes nothing observable.
//! 3. **Unrolled ×2.** The trip count is computed once in unsigned
//!    arithmetic (exact for every pair of bounds, the ends of `i64`
//!    included); a fast lane runs two bodies per trip while two remain,
//!    then at most one more runs. The variable's last `+ 2` may wrap past
//!    the bound: it is never read again (the loop variable is scoped to
//!    the loop, and a captured one is not claimed).
//!
//! Not claimed: a metered program (the per-iteration charge is normative),
//! a body with a `break`, any `continue` but the skip, a lambda or a nested
//! loop (their labels and activations would need the per-copy bookkeeping
//! this pass does not buy), a write to the loop variable, or a body longer
//! than [`MAX_BODY_STMTS`] statements (the copy doubles its bytes).

use std::collections::HashMap;

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{BinOp, IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, UnOp, VarId};
use almide_types::types::Ty;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::EmitError;

/// Strided loops emitted, process-wide (tests/strided_range.rs reads it
/// to tell the shapes that take the lane from the ones that must not).
static FIRED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The strided loops emitted so far, process-wide.
pub(crate) fn fired() -> u64 {
    FIRED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Bodies above this statement count keep the rolled form.
const MAX_BODY_STMTS: usize = 8;

fn lit(e: &IrExpr) -> Option<i64> {
    match e.kind {
        IrExprKind::LitInt { value } => Some(value),
        _ => None,
    }
}

/// `Some(c)`: for every value of `ivar`, the low bit of `e` is the low bit
/// of `ivar` xor `c`. `defs` are the immutable `Int` lets in scope.
pub(crate) fn parity(e: &IrExpr, ivar: VarId, defs: &HashMap<VarId, &IrExpr>) -> Option<bool> {
    match &e.kind {
        IrExprKind::Var { id } if *id == ivar => Some(false),
        IrExprKind::Var { id } => parity(defs.get(id)?, ivar, defs),
        IrExprKind::UnOp { op: UnOp::NegInt, operand } => parity(operand, ivar, defs),
        IrExprKind::BinOp { op: BinOp::MulInt, left, right } => match (lit(left), lit(right)) {
            (_, Some(k)) if k & 1 == 1 => parity(left, ivar, defs),
            (Some(k), _) if k & 1 == 1 => parity(right, ivar, defs),
            _ => None,
        },
        IrExprKind::BinOp { op: BinOp::AddInt | BinOp::SubInt, left, right } => match (lit(left), lit(right)) {
            (_, Some(k)) => Some(parity(left, ivar, defs)? ^ (k & 1 == 1)),
            (Some(k), _) => Some(parity(right, ivar, defs)? ^ (k & 1 == 1)),
            _ => None,
        },
        _ => None,
    }
}

/// `Some((k, c))`: `e` is `k * p + c` in wrapping `i64` arithmetic for
/// every value of `p` — what an induction value stepping `k` per unit of
/// `p` reproduces exactly (list_fuse.rs, the strided lane's first map).
pub(crate) fn affine(e: &IrExpr, p: VarId) -> Option<(i64, i64)> {
    match &e.kind {
        IrExprKind::Var { id } if *id == p => Some((1, 0)),
        IrExprKind::LitInt { value } => Some((0, *value)),
        IrExprKind::UnOp { op: UnOp::NegInt, operand } => {
            let (k, c) = affine(operand, p)?;
            Some((k.wrapping_neg(), c.wrapping_neg()))
        }
        IrExprKind::BinOp { op: BinOp::AddInt, left, right } => {
            let ((a, b), (c, d)) = (affine(left, p)?, affine(right, p)?);
            Some((a.wrapping_add(c), b.wrapping_add(d)))
        }
        IrExprKind::BinOp { op: BinOp::SubInt, left, right } => {
            let ((a, b), (c, d)) = (affine(left, p)?, affine(right, p)?);
            Some((a.wrapping_sub(c), b.wrapping_sub(d)))
        }
        IrExprKind::BinOp { op: BinOp::MulInt, left, right } => match (lit(left), lit(right)) {
            (_, Some(m)) => affine(left, p).map(|(k, c)| (k.wrapping_mul(m), c.wrapping_mul(m))),
            (Some(m), _) => affine(right, p).map(|(k, c)| (k.wrapping_mul(m), c.wrapping_mul(m))),
            _ => None,
        },
        _ => None,
    }
}

/// `Some(q)`: `cond` holds exactly when the low bit of `ivar` is `q`.
pub(crate) fn parity_test(cond: &IrExpr, ivar: VarId, defs: &HashMap<VarId, &IrExpr>) -> Option<bool> {
    match &cond.kind {
        IrExprKind::UnOp { op: UnOp::Not, operand } => parity_test(operand, ivar, defs).map(|q| !q),
        IrExprKind::BinOp { op: op @ (BinOp::Eq | BinOp::Neq), left, right } => {
            let m = match (lit(left), lit(right)) {
                (_, Some(0)) => left,
                (Some(0), _) => right,
                _ => return None,
            };
            let IrExprKind::BinOp { op: BinOp::ModInt, left: e, right: two } = &m.kind else { return None };
            if lit(two) != Some(2) {
                return None;
            }
            // `e % 2 == 0` holds iff low(e) == 0 iff low(ivar) == c.
            let c = parity(e, ivar, defs)?;
            Some(if *op == BinOp::Eq { c } else { !c })
        }
        _ => None,
    }
}

/// Pure wrapping `Int` arithmetic over variables and literals: what a
/// skipped iteration may leave unevaluated.
pub(crate) fn pure_int(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::LitInt { .. } | IrExprKind::Var { .. } => e.ty == Ty::Int,
        IrExprKind::UnOp { op: UnOp::NegInt, operand } => pure_int(operand),
        IrExprKind::BinOp { op: BinOp::AddInt | BinOp::SubInt | BinOp::MulInt, left, right } => {
            pure_int(left) && pure_int(right)
        }
        _ => false,
    }
}

fn is_continue(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Continue => true,
        IrExprKind::Block { stmts, expr: Some(t) } if stmts.is_empty() => is_continue(t),
        IrExprKind::Block { stmts, expr: None } => {
            matches!(stmts.as_slice(), [IrStmt { kind: IrStmtKind::Expr { expr }, .. }] if is_continue(expr))
        }
        _ => false,
    }
}

fn is_unit(e: &IrExpr) -> bool {
    matches!(&e.kind, IrExprKind::Unit) || matches!(&e.kind, IrExprKind::Block { stmts, expr: None } if stmts.is_empty())
}

/// A body statement this pass cannot copy: a jump, a lambda, a nested loop,
/// or a write to the loop variable.
struct Refuse {
    ivar: VarId,
    hit: bool,
}

impl IrVisitor for Refuse {
    fn visit_expr(&mut self, e: &IrExpr) {
        if self.hit {
            return;
        }
        if matches!(
            e.kind,
            IrExprKind::Break | IrExprKind::Continue | IrExprKind::Lambda { .. } | IrExprKind::While { .. } | IrExprKind::ForIn { .. }
        ) {
            self.hit = true;
            return;
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        if self.hit {
            return;
        }
        if matches!(&s.kind, IrStmtKind::Assign { var, .. } if *var == self.ivar) {
            self.hit = true;
            return;
        }
        walk_stmt(self, s);
    }
}

/// Where a range loop's bounds come from: its `a..<b` operands, or the
/// pair of locals a deferred range variable was bound to (ranges.rs).
pub(crate) enum Bounds<'e> {
    Exprs(&'e IrExpr, &'e IrExpr),
    Locals(u32, u32),
}

/// The plan for a strided loop: the parity of the values the body keeps,
/// and the body without its skip.
pub(crate) struct Stride {
    keep: bool,
    body: Vec<IrStmt>,
}

/// The strided plan of a range loop's body over `ivar`, if its shape has one.
pub(crate) fn stride_plan(ivar: VarId, body: &[IrStmt]) -> Option<Stride> {
    if body.len() > MAX_BODY_STMTS + 1 {
        return None;
    }
    let mut defs: HashMap<VarId, &IrExpr> = HashMap::new();
    let mut at = 0;
    while let Some(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: Ty::Int, value }, .. }) = body.get(at) {
        if !pure_int(value) {
            break;
        }
        defs.insert(*var, value);
        at += 1;
    }
    let keep = match &body.get(at)?.kind {
        IrStmtKind::Guard { cond, else_ } if is_continue(else_) => parity_test(cond, ivar, &defs)?,
        IrStmtKind::Expr { expr: IrExpr { kind: IrExprKind::If { cond, then, else_ }, .. } }
            if is_continue(then) && is_unit(else_) =>
        {
            !parity_test(cond, ivar, &defs)?
        }
        _ => return None,
    };
    let rest: Vec<IrStmt> = body[..at].iter().chain(&body[at + 1..]).cloned().collect();
    let mut scan = Refuse { ivar, hit: false };
    rest.iter().for_each(|s| scan.visit_stmt(s));
    (!scan.hit).then_some(Stride { keep, body: rest })
}

impl Emitter<'_> {
    /// `for var in start..end` (or `..=`) through the strided ×2 lane when
    /// [`stride_plan`] finds the shape; `false`: emit the ordinary loop.
    pub(crate) fn try_strided_range(
        &mut self,
        var: VarId,
        bounds: Bounds<'_>,
        inclusive: bool,
        body: &[IrStmt],
    ) -> Result<bool, EmitError> {
        if self.metered || self.cells.contains(&var) {
            return Ok(false);
        }
        let Some(&(vi, ty)) = self.locals.get(&var) else { return Ok(false) };
        if ty != crate::INT {
            return Ok(false);
        }
        let Some(plan) = stride_plan(var, body) else { return Ok(false) };
        match bounds {
            Bounds::Exprs(start, end) => {
                self.lower(start, Some(crate::INT))?;
                self.f.instructions().local_set(vi);
                self.lower(end, Some(crate::INT))?;
            }
            Bounds::Locals(sl, el) => {
                self.f.instructions().local_get(sl).local_set(vi).local_get(el);
            }
        }
        let end = self.hold_i64()?;
        self.f.instructions().local_set(end);
        let n = self.strided_header(vi, end, inclusive, plan.keep)?;
        // The trip's base steps once per trip (a one-add chain); each copy
        // sets the loop variable from it.
        let base = self.hold_i64()?;
        self.f.instructions().local_get(vi).local_set(base);
        self.strided_lane_open(n);
        self.witness_loop_open();
        self.f.instructions().local_get(base).local_set(vi);
        self.lower_loop_body(&plan.body, false)?;
        self.f.instructions().local_get(base).i64_const(2).i64_add().local_set(vi);
        self.lower_loop_body(&plan.body, false)?;
        self.witness_loop_close();
        self.f.instructions().local_get(base).i64_const(4).i64_add().local_set(base);
        self.strided_middle(n);
        self.f.instructions().local_get(base).local_set(vi);
        self.release_i64();
        self.witness_loop_open();
        self.lower_loop_body(&plan.body, false)?;
        self.witness_loop_close();
        self.strided_footer();
        self.release_i64();
        Ok(true)
    }

    /// The strided lane's head over the counter `vi` (already at the
    /// range's start) and the bound in `end`: an empty range or one with no
    /// value of parity `keep` skips everything; otherwise `vi` moves to the
    /// first kept value, the trip count goes to the returned local
    /// (unsigned: exact for every pair of bounds). Opens the skip-all
    /// block; [`Self::strided_lane_open`] opens the fast lane's two.
    pub(crate) fn strided_header(&mut self, vi: u32, end: u32, inclusive: bool, keep: bool) -> Result<u32, EmitError> {
        let d = self.hold_i64()?;
        let n = self.hold_i64()?;
        let mut i = self.f.instructions();
        i.block(BlockType::Empty);
        i.local_get(vi).local_get(end);
        if inclusive { i.i64_gt_s() } else { i.i64_ge_s() };
        i.br_if(0);
        // d = last - start >= 0, exact as u64.
        i.local_get(end).local_get(vi).i64_sub();
        if !inclusive {
            i.i64_const(1).i64_sub();
        }
        i.local_set(d);
        // n = the step to the first kept value (0 or 1); none fits: skip.
        i.local_get(vi).i64_const(i64::from(keep)).i64_xor().i64_const(1).i64_and().local_set(n);
        i.local_get(n).local_get(d).i64_gt_u().br_if(0);
        i.local_get(vi).local_get(n).i64_add().local_set(vi);
        // n = trips = (d - step) / 2 + 1, unsigned.
        i.local_get(d).local_get(n).i64_sub().i64_const(1).i64_shr_u().i64_const(1).i64_add().local_set(n);
        Ok(n)
    }

    /// Open the fast lane: left while fewer than two trips remain.
    pub(crate) fn strided_lane_open(&mut self, n: u32) {
        let mut i = self.f.instructions();
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(n).i64_const(2).i64_lt_u().br_if(1);
    }

    /// After the fast lane's two copies: count them, loop, and fall into
    /// the remainder only when one trip is left.
    pub(crate) fn strided_middle(&mut self, n: u32) {
        let mut i = self.f.instructions();
        i.local_get(n).i64_const(2).i64_sub().local_set(n).br(0).end().end();
        i.local_get(n).i64_eqz().br_if(0);
    }

    /// Close the skip-all block and the two holds of [`Self::strided_header`].
    pub(crate) fn strided_footer(&mut self) {
        self.f.instructions().end();
        self.release_i64();
        self.release_i64();
        FIRED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// A later copy's activation of a fused chain (list_fuse.rs): the
    /// chain's arguments were noted by the first.
    pub(crate) fn witness_fused_reopen(&mut self, acc: u32, t: crate::SliceTy) {
        let carried = self.rc_droppable(t);
        let Some(w) = self.witness.as_mut() else { return };
        w.loop_open();
        w.fused_open();
        if carried {
            w.carried_owned(acc);
        }
    }
}
