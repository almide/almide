//! A map over a list it consumes hands each element's credit to the
//! callback (#4612).
//!
//! `string.split(s, ",") |> list.map((p) => string.to_upper(string.trim(p)))`
//! lent every piece to its callback: the piece stayed in the split list, so
//! `string.trim` met it at rc 2 and copied it out, though nothing would ever
//! read the list again. Native moves out of a dying source in the same place
//! (`render_native_loops::moves_out_of_dying_source`). Here, when the map's
//! source is an owned temporary — the arm releases it after the loop — and
//! its block is a heap block at rc 1, each element is taken out of its slot
//! (the slot is nulled, so the release after the loop walks a null there)
//! and the callback's one read of its parameter is lowered as a MOVE. A
//! shared source keeps its elements: there the callback runs as it always
//! did, on a lent element. The callback body is lowered once per arm, so
//! each arm's RC instructions are exactly the ones its witness records — no
//! arm holds a call (a +1 on the element) the ownership witness never saw.
//!
//! The read must be the callback's only one, and the body itself or an
//! argument of a call reached from the body only through call arguments: it
//! runs exactly once per element, where an argument's ownership is judged
//! (`lower_arg`, `lower_conv_arg`). An owning callee takes the credit; a
//! borrowing one parks the value and releases it after the call.

use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::{IrExpr, IrExprKind, VarId};

use crate::emitter::Emitter;
use crate::*;

/// The one read of `p` in `body`, when `body` reads `p` exactly once and that
/// read is `body` itself or an argument of a call reached from `body` only
/// through call arguments.
pub(crate) fn sole_call_arg_read(body: &IrExpr, p: VarId) -> Option<&IrExpr> {
    struct Count(VarId, usize);
    impl IrVisitor for Count {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::Var { id } if id == self.0) {
                self.1 += 1;
            }
            walk_expr(self, e);
        }
    }
    let mut n = Count(p, 0);
    n.visit_expr(body);
    if n.1 != 1 {
        return None;
    }
    let mut at = body;
    loop {
        match &at.kind {
            IrExprKind::Var { id } if *id == p => return Some(at),
            IrExprKind::Call { args, .. } => {
                at = args.iter().find(|a| {
                    let mut c = Count(p, 0);
                    c.visit_expr(a);
                    c.1 == 1
                })?;
            }
            _ => return None,
        }
    }
}

impl Emitter<'_> {
    /// Before the loop: whether the source block in `bh` gives its elements
    /// up — a heap block (at or above the heap floor: never a pool static)
    /// at rc 1. Returns the i32 local holding the answer.
    pub(crate) fn list_take_open(&mut self, bh: u32) -> Result<u32, EmitError> {
        let take = self.hold_i32()?;
        let rc = wasm_encoder::MemArg { offset: u64::from(almide_layout::RC.offset), align: 2, target: wasm_encoder::MemArgTarget::Memory(0), ordering: None };
        self.f
            .instructions()
            .local_get(bh)
            .global_get(G_LINE_END)
            .i32_ge_u()
            .local_get(bh)
            .i32_load(rc)
            .i32_const(1)
            .i32_eq()
            .i32_and()
            .local_set(take);
        Ok(take)
    }

    /// The element just loaded leaves its slot (`bh`'s element `ih`): the
    /// slot is nulled, so the source's release after the loop skips it.
    pub(crate) fn list_take_slot(&mut self, elem: SliceTy, (bh, ih): (u32, u32)) {
        self.f
            .instructions()
            .local_get(bh)
            .local_get(ih)
            .i32_const(elem.slot_size() as i32)
            .i32_mul()
            .i32_add()
            .i32_const(0);
        self.store_ty_slot(elem, 0);
    }
}
