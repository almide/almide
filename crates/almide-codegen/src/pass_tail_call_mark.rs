//! Tail Call Mark pass: marks tail-position calls as TailCall for WASM return_call emission.
//!
//! Unlike TailCallOptPass (which converts to loops for Rust), this pass preserves
//! the original call structure and simply changes `Call` → `TailCall` for any call
//! in tail position. The WASM emitter then emits `return_call` / `return_call_indirect`
//! for TailCall nodes.
//!
//! This applies to ALL tail-position calls, not just self-recursive ones.
//! WASM native tail calls eliminate stack growth for any call in tail position.

use almide_ir::*;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct TailCallMarkPass;

impl NanoPass for TailCallMarkPass {
    fn name(&self) -> &str { "TailCallMark" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Wasm])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut changed = false;
        for func in &mut program.functions {
            if mark_tail_calls(&mut func.body) { changed = true; }
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                if mark_tail_calls(&mut func.body) { changed = true; }
            }
        }
        PassResult { program, changed }
    }
}

/// Mark tail-position calls in an expression. Returns true if any changes were made.
fn mark_tail_calls(expr: &mut IrExpr) -> bool {
    match &mut expr.kind {
        // A Call in tail position → convert to TailCall
        IrExprKind::Call { .. } => {
            // Take ownership of the Call fields and rebuild as TailCall
            let old = std::mem::replace(&mut expr.kind, IrExprKind::Unit);
            if let IrExprKind::Call { target, args, .. } = old {
                expr.kind = IrExprKind::TailCall { target, args };
                true
            } else {
                unreachable!()
            }
        }

        // Block: tail position is the trailing expression
        IrExprKind::Block { stmts: _, expr: Some(tail) } => {
            mark_tail_calls(tail)
        }

        // If: both branches are in tail position
        IrExprKind::If { cond: _, then, else_ } => {
            let a = mark_tail_calls(then);
            let b = mark_tail_calls(else_);
            a || b
        }

        // Match: each arm body is in tail position
        IrExprKind::Match { subject: _, arms } => {
            let mut changed = false;
            for arm in arms {
                if mark_tail_calls(&mut arm.body) { changed = true; }
            }
            changed
        }

        // ResultOk/Try are NOT tail positions: the wrapper changes the return type.
        // `Ok(f(x))` wraps f's result in Result — return_call(f) would skip the wrapping.
        // `Try(f(x))` unwraps f's Result — return_call(f) would return Result, not the inner T.

        // Everything else is NOT a tail position
        _ => false,
    }
}
