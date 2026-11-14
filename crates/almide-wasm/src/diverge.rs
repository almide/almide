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

/// The program fns control never comes back from: a body whose every path
/// ends in a diverging call — `panic(msg)`, `process.exit(code)`, or a call
/// to a fn of this set (`fn out_of_range(..) -> Unit = panic(..)`). A fixed
/// point over named and qualified calls, resolved the way the call site
/// resolves them (module first). Only a call in TAIL position counts: the
/// tail of a block (or its last statement when it has none) and both arms
/// of an `if`; anything else — a loop, a match, a lambda — is not claimed.
/// The witness reads it (`witness_diverged`): such a call ends its path in
/// the checker's abort terminal, as `panic` itself does.
pub(crate) fn diverging_fns(program_fns: &[(&almide_ir::IrFunction, Option<String>, u32)], table: &crate::FnTable) -> Vec<bool> {
    let mut out = vec![false; program_fns.len()];
    loop {
        let mut changed = false;
        for (i, (f, qual, _)) in program_fns.iter().enumerate() {
            if out[i] {
                continue;
            }
            let module = crate::emit::fn_module(qual.as_deref(), f);
            let hit = |t: &CallTarget| {
                let j = match t {
                    CallTarget::Named { name } => module
                        .and_then(|m| table.by_name.get(&format!("{m}.{name}")))
                        .or_else(|| table.by_name.get(name.as_str())),
                    CallTarget::Module { module, func, .. } => table.by_name.get(&format!("{module}.{func}")),
                    _ => None,
                };
                is_diverging_call(t) || j.is_some_and(|&j| out[j])
            };
            if tail_diverges(&f.body, &hit) {
                out[i] = true;
                changed = true;
            }
        }
        if !changed {
            return out;
        }
    }
}

fn tail_diverges(e: &IrExpr, hit: &impl Fn(&CallTarget) -> bool) -> bool {
    match &e.kind {
        IrExprKind::Call { target, .. } => hit(target),
        IrExprKind::If { then, else_, .. } => tail_diverges(then, hit) && tail_diverges(else_, hit),
        IrExprKind::Block { expr: Some(t), .. } => tail_diverges(t, hit),
        IrExprKind::Block { stmts, expr: None } => matches!(
            stmts.last().map(|s| &s.kind),
            Some(almide_ir::IrStmtKind::Expr { expr }) if tail_diverges(expr, hit)
        ),
        _ => false,
    }
}

impl Emitter<'_> {
    /// A call to a fn of [`diverging_fns`] returned to no one: the path
    /// ends in the checker's abort terminal, as after `panic` (#2758).
    pub(crate) fn witness_diverged(&mut self, callee: usize) {
        if self.table.infos[callee].diverges
            && let Some(w) = self.witness.as_mut()
        {
            w.abort_end();
        }
    }

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
            _ => match self.routine(|r: &Option<_>| r.is_some(), |em| em.try_concat_dying(e))? {
                Some(t) => Ok(t),
                None => self.lower_node(e, want),
            },
        }
    }
}
