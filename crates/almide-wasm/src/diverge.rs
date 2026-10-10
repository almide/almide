//! Diverging expressions in a value slot (#2769, #3144).
//!
//! Control never comes back from a diverging expression, so the wasm stack is
//! polymorphic past it and the slot it fills types as whatever the slot
//! expects. The IR's `diverge` cut has already reduced every strict operand
//! position to a block whose tail is the diverging expression; what reaches
//! here is that tail, under the expectation of the slot it ended up in.

use almide_ir::{CallTarget, IrExpr, IrExprKind};
use almide_types::types::Ty;

use crate::emitter::Emitter;
use crate::*;

/// The builtin calls whose own lowering ends in `unreachable`: `panic(msg)`
/// and `process.exit(code)`.
pub(crate) fn is_diverging_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Named { name } => name.as_str() == "panic",
        CallTarget::Module { module, func, .. } => module.as_str() == "process" && func.as_str() == "exit",
        _ => false,
    }
}

impl Emitter<'_> {
    /// The value of a diverging call that produced none: a builtin already
    /// ended in `unreachable`; a user fn declared `-> Never` has no result,
    /// and `unreachable` states that control does not come back from it.
    pub(crate) fn diverged_call_value(
        &mut self,
        target: &CallTarget,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        let Some(t) = want else { return unsup("diverging-call-untyped") };
        if !is_diverging_call(target) {
            self.f.instructions().unreachable();
        }
        Ok(t)
    }

    /// [`Emitter::lower_node`], except for `bail(c)!` on an
    /// `effect fn … -> Never` in a value slot: its unwrap yields the carrier's
    /// Unit ok payload, which the callee never returns. It is lowered without
    /// an expectation, the payload dropped, and `unreachable` leaves the stack
    /// polymorphic, so the slot types as `want`.
    /// The bytes it emits carry `e`'s line (#1315, debug_lines.rs).
    pub(crate) fn lower_node_or_never(&mut self, e: &IrExpr, want: Option<SliceTy>) -> Result<SliceTy, EmitError> {
        crate::debug_lines::enter(self.f.byte_len(), e.span);
        let r = self.lower_node_or_never_unspanned(e, want);
        crate::debug_lines::leave(self.f.byte_len());
        r
    }

    fn lower_node_or_never_unspanned(&mut self, e: &IrExpr, want: Option<SliceTy>) -> Result<SliceTy, EmitError> {
        let never_unwrap =
            e.ty == Ty::Never && matches!(e.kind, IrExprKind::Try { .. } | IrExprKind::Unwrap { .. });
        match want {
            Some(w) if never_unwrap => {
                self.in_tail = false;
                if self.lower_node(e, None)? != w {
                    self.f.instructions().drop().unreachable();
                }
                Ok(w)
            }
            _ => match self.try_concat_dying(e)? {
                Some(t) => Ok(t),
                None => self.lower_node(e, want),
            },
        }
    }
}
