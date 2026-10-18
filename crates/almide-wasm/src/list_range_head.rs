//! The source of a fold's loop (#2045): a list block walked by index, or a
//! `list.range(a, b)` head COUNTED in two i64 locals and never built. Native
//! renders that head as `(a..b)` and walks it once; this leg built the whole
//! `List[Int]` first, so a span past wasm32's address space died `Error: out
//! of memory` where native printed the answer. The counter reads the same
//! sequence the list held — `a, a+1, …, b-1`, nothing when `b <= a` — and it
//! steps only while `cur < end`, so `cur + 1 <= end` never overflows, at
//! i64::MAX included.
use almide_ir::{CallTarget, IrExpr, IrExprKind};

use crate::emitter::Emitter;
use crate::*;

/// Where a fold's elements come from.
#[derive(Clone, Copy)]
pub(crate) enum LoopSrc {
    /// `hof_loop_open`'s (base, count, idx) over a built list.
    Block { elem: SliceTy, base: u32, count: u32, idx: u32 },
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

/// `list.range(a, b)` as its two bound expressions.
pub(crate) fn range_head(xs: &IrExpr) -> Option<(&IrExpr, &IrExpr)> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &xs.kind else {
        return None;
    };
    match args.as_slice() {
        [a, b] if module.as_str() == "list" && func.as_str() == "range" => Some((a, b)),
        _ => None,
    }
}

impl Emitter<'_> {
    /// Evaluate the source — a range's two bounds, in order, or the list —
    /// and hold what the loop reads.
    pub(crate) fn loop_src_open(&mut self, xs: &IrExpr) -> Result<LoopSrc, EmitError> {
        let Some((a, b)) = range_head(xs) else {
            let (elem, base, count, idx) = self.hof_loop_open(xs)?;
            return Ok(LoopSrc::Block { elem, base, count, idx });
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

    /// Release the holds [`Self::loop_src_open`] took.
    pub(crate) fn loop_src_close(&mut self, s: LoopSrc) {
        match s {
            LoopSrc::Block { .. } => {
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
