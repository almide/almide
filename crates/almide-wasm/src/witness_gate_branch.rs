//! The witness gate's block and value-position branch rules (split from
//! witness_gate.rs for the file budget): one arm group of `value_subset`.

use almide_ir::{IrExpr, IrExprKind};

use super::{match_head_subset, stmts_subset, value_subset, Why};

/// `value_subset` over a `Block`, `If` or `Match` value.
pub(super) fn branch_value_subset(e: &IrExpr) -> Option<Why> {
    match &e.kind {
        // `{ let t = f(x); op(t) }` — arg_temps.rs's shape: the binds are the
        // Bind hook's (the frame's exit plan releases them), the value is
        // the tail's (`rc_owned_result` and the hooks read through blocks).
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            stmts_subset(stmts).map(Why::Deep).or_else(|| value_subset(tail))
        }
        // A block with no tail is the same shape with a Unit tail (no block,
        // no site): its statements are the frame's, judged as any body's
        // (an inlined callback's `(line) => { count = count + 1 }`).
        IrExprKind::Block { stmts, expr: None } => stmts_subset(stmts).map(Why::Deep),
        // #2756: a value-position branch — one site, each arm hands the
        // join one credit (`witness_arm_value`); the condition runs first.
        IrExprKind::If { cond, then, else_ } => value_subset(cond)
            .map(|w| w.inside("if-cond"))
            .or_else(|| value_subset(then))
            .or_else(|| value_subset(else_)),
        IrExprKind::Match { subject, arms } => match_head_subset(subject, arms)
            .map(Why::Deep)
            .or_else(|| arms.iter().find_map(|a| value_subset(&a.body))),
        _ => None,
    }
}
