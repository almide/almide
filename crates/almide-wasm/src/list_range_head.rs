//! The source of a fold's loop (#2045): a list block walked by index, or a
//! `list.range(a, b)` head COUNTED in two i64 locals and never built. Native
//! renders that head as `(a..b)` and walks it once; this leg built the whole
//! `List[Int]` first, so a span past wasm32's address space died `Error: out
//! of memory` where native printed the answer. The counter reads the same
//! sequence the list held — `a, a+1, …, b-1`, nothing when `b <= a` — and it
//! steps only while `cur < end`, so `cur + 1 <= end` never overflows, at
//! i64::MAX included.
//!
//! Every fold-shaped arm evaluates its source BEFORE its init, left to right
//! as native does. A source held as a bare borrow would not survive an init
//! that rebinds the variable it names (`xs |> list.fold({ xs = [0]; 0 }, f)`
//! released the block the loop then read), so when the init may run code the
//! source is KEPT: it takes a credit here and the arm releases it after the
//! loop ([`Emitter::lower_fold_source`]).
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind};

use crate::emitter::Emitter;
use crate::*;

/// Where a fold's elements come from.
#[derive(Clone, Copy)]
pub(crate) enum LoopSrc {
    /// `hof_loop_open`'s (base, count, idx) over a built list; `kept` is the
    /// block's type when the arm holds a credit on it to release.
    Block { elem: SliceTy, base: u32, count: u32, idx: u32, kept: Option<SliceTy> },
    /// `list.range(a, b)`: the next element and the exclusive end.
    Count { cur: u32, end: u32 },
}

impl LoopSrc {
    pub(crate) fn elem(self) -> SliceTy {
        match self {
            LoopSrc::Block { elem, .. } => elem,
            LoopSrc::Count { .. } => INT,
        }
    }
}

/// `list.range(a, b)` or the range literal `a..<b` as its two bound
/// expressions (#3810: the literal is the same list, so it counts the same
/// way). An inclusive `a..=b` keeps the built list: its end is not exclusive.
pub(crate) fn range_head(xs: &IrExpr) -> Option<(&IrExpr, &IrExpr)> {
    if let IrExprKind::Range { start, end, inclusive: false } = &xs.kind {
        return Some((start, end));
    }
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &xs.kind else {
        return None;
    };
    match args.as_slice() {
        [a, b] if module.as_str() == "list" && func.as_str() == "range" => Some((a, b)),
        _ => None,
    }
}

/// Whether evaluating `e` runs no code that could write a variable: no
/// call, no fan, and no assignment statement. Only then may a fold's source
/// stay a bare borrow while its init is evaluated.
pub(crate) fn runs_no_code(e: &IrExpr) -> bool {
    struct Scan(bool);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::Call { .. } | IrExprKind::RuntimeCall { .. } | IrExprKind::Fan { .. }) {
                self.0 = false;
            }
            if self.0 {
                walk_expr(self, e);
            }
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if matches!(
                s.kind,
                IrStmtKind::Assign { .. }
                    | IrStmtKind::IndexAssign { .. }
                    | IrStmtKind::FieldAssign { .. }
                    | IrStmtKind::MapInsert { .. }
            ) {
                self.0 = false;
            }
            if self.0 {
                walk_stmt(self, s);
            }
        }
    }
    let mut s = Scan(true);
    s.visit_expr(e);
    s.0
}

impl Emitter<'_> {
    /// Lower a fold's collection SOURCE ahead of its `init`, leaving the
    /// block on the stack. Returns its type and whether the arm KEEPS it —
    /// holds one credit it must give back with [`Self::release_fold_source`]
    /// after the loop: a borrowed block shares here, an owned temporary's
    /// own credit is the arm's. An init that runs no code takes the plain
    /// borrow, unchanged.
    pub(crate) fn lower_fold_source(&mut self, xs: &IrExpr, init: &IrExpr) -> Result<(SliceTy, bool), EmitError> {
        if runs_no_code(init) {
            return Ok((self.lower_arg(xs, None, ArgMode::Borrow)?, false));
        }
        self.witness_inline_arg(xs);
        let got = self.lower(xs, None)?;
        if !self.rc_droppable(got) {
            return Ok((got, false));
        }
        if !self.rc_owned_result(crate::rc_ownership::rc_tail(xs)) {
            self.rc_inc_top();
        }
        // #2755: shared when borrowed, released after the walk either way.
        self.witness_shared_released(xs, got);
        Ok((got, true))
    }

    /// Give back the credit [`Self::lower_fold_source`] kept on the block in
    /// `local`. Stack-neutral.
    pub(crate) fn release_fold_source(&mut self, local: u32, kept: Option<SliceTy>) {
        if let Some(t) = kept {
            let dec = self.dec_fn_of(t);
            self.f.instructions().local_get(local).call(dec);
        }
    }

    /// Evaluate the source — a range's two bounds, in order, or the list —
    /// and hold what the loop reads.
    pub(crate) fn loop_src_open(&mut self, xs: &IrExpr, init: &IrExpr) -> Result<LoopSrc, EmitError> {
        let Some((a, b)) = range_head(xs) else {
            return self.loop_src_open_block(xs, init);
        };
        // #2755: the range call is consumed here, never lowered as a value.
        self.witness_inline_arg(xs);
        self.lower(a, Some(INT))?;
        let cur = self.hold_i64()?;
        self.f.instructions().local_set(cur);
        self.lower(b, Some(INT))?;
        let end = self.hold_i64()?;
        self.f.instructions().local_set(end);
        Ok(LoopSrc::Count { cur, end })
    }

    /// [`Self::loop_src_open`] over a built list, a range included.
    pub(crate) fn loop_src_open_block(&mut self, xs: &IrExpr, init: &IrExpr) -> Result<LoopSrc, EmitError> {
        let (got, keep) = self.lower_fold_source(xs, init)?;
        let (elem, base, count, idx) = self.hof_loop_header(got)?;
        Ok(LoopSrc::Block { elem, base, count, idx, kept: keep.then_some(got) })
    }

    /// Inside `block { loop {`: leave the loop once the source is exhausted.
    pub(crate) fn loop_src_exit(&mut self, s: LoopSrc) {
        match s {
            LoopSrc::Block { count, idx, .. } => {
                self.f.instructions().local_get(idx).local_get(count).i32_ge_u().br_if(1);
            }
            LoopSrc::Count { cur, end } => {
                self.f.instructions().local_get(cur).local_get(end).i64_ge_s().br_if(1);
            }
        }
    }

    /// Push the current element.
    pub(crate) fn loop_src_load(&mut self, s: LoopSrc) {
        match s {
            LoopSrc::Block { elem, base, idx, .. } => {
                self.f
                    .instructions()
                    .local_get(base)
                    .local_get(idx)
                    .i32_const(elem.slot_size() as i32)
                    .i32_mul()
                    .i32_add();
                self.load_ty_slot(elem, 0);
            }
            LoopSrc::Count { cur, .. } => {
                self.f.instructions().local_get(cur);
            }
        }
    }

    /// Advance, branch back, and close the `block { loop {`.
    pub(crate) fn loop_src_step(&mut self, s: LoopSrc) {
        match s {
            LoopSrc::Block { idx, .. } => self.hof_step(idx),
            LoopSrc::Count { cur, .. } => {
                self.f.instructions().local_get(cur).i64_const(1).i64_add().local_set(cur).br(0).end().end();
            }
        }
    }

    /// Release a kept source and the holds [`Self::loop_src_open`] took.
    pub(crate) fn loop_src_close(&mut self, s: LoopSrc) {
        match s {
            LoopSrc::Block { base, kept, .. } => {
                self.release_fold_source(base, kept);
                self.release_i32();
                self.release_i32();
                self.release_i32();
            }
            LoopSrc::Count { .. } => {
                self.release_i64();
                self.release_i64();
            }
        }
    }
}
