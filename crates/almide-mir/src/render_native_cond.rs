//! Comparisons read only as a branch condition render as a Rust `bool`.
//!
//! MIR carries a comparison's result as an i64 0/1, and the render spelled
//! every one `let mut v: i64 = (a <= b) as i64;` with its branch as
//! `if v != 0 {`. At opt-level 1 that zero-extend-then-test pair reaches
//! LLVM's loop passes in a shape they optimise less well than a plain `i1`
//! branch: the one-level self-inlined `depth` of
//! `spec/wasm_cross/recursion_depth_within_limits.almd` (#4468) kept two
//! tests and a compare per iteration where the standard codegen's
//! `if n <= 0 {` compiled to one (1.31x its in-main work on the small perf
//! gate). A comparison whose ONLY readers are `IfThen` / `LoopBreakUnless`
//! conditions is now rendered as `let v: bool = a <= b;` and tested as
//! `if v {` / `if !v { break; }` — the shape the standard codegen emits.
//!
//! Any other reader (a print, an arithmetic use, a call argument, a rebind,
//! a join yield) keeps the i64 spelling, so no consumer ever sees a `bool`.
//! The value is the same bit either way: Rust's `bool as i64` is 0/1.

use crate::{IntOp, MirFunction, Op, PrimKind, ValueId};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    static COND_CMPS: RefCell<BTreeSet<ValueId>> = const { RefCell::new(BTreeSet::new()) };
}

fn is_cmp_int_op(op: &IntOp) -> bool {
    matches!(
        op,
        IntOp::Eq
            | IntOp::Ne
            | IntOp::Lt
            | IntOp::Le
            | IntOp::Gt
            | IntOp::Ge
    )
}

/// The comparison an op defines, if it is one.
fn cmp_def(op: &Op) -> Option<ValueId> {
    match op {
        Op::IntBinOp { dst, op, .. } if is_cmp_int_op(op) => Some(*dst),
        Op::Prim { kind: PrimKind::FloatCmp(_), dst: Some(d), args } if args.len() == 2 => Some(*d),
        _ => None,
    }
}

/// Record `func`'s branch-only comparisons (read by the render below).
///
/// `ALMIDE_NATIVE_DRILL_COND_I64=1` records none (the negative control: every
/// comparison back to the i64 spelling, the small perf gate must read the loss).
pub(crate) fn set_cond_cmps(func: &MirFunction) {
    let drill = almide_base::env::flag("ALMIDE_NATIVE_DRILL_COND_I64");
    // Per value: (defining comparisons, condition reads, any other mention).
    let mut seen: BTreeMap<ValueId, (u32, u32, bool)> = BTreeMap::new();
    let mut vals = Vec::new();
    for op in &func.ops {
        if crate::render_native_move::bookkeeping(op) {
            continue;
        }
        let def = cmp_def(op);
        let cond = match op {
            Op::IfThen { cond, .. } | Op::LoopBreakUnless { cond } => Some(*cond),
            _ => None,
        };
        vals.clear();
        crate::mir_ops::op_values(op, &mut vals);
        for v in &vals {
            let e = seen.entry(*v).or_default();
            if Some(*v) == def {
                e.0 += 1;
            } else if Some(*v) == cond {
                e.1 += 1;
            } else {
                e.2 = true;
            }
        }
    }
    let cmps: BTreeSet<ValueId> = seen
        .into_iter()
        .filter(|(v, (defs, conds, other))| !drill && *defs == 1 && *conds >= 1 && !*other && func.ret != Some(*v))
        .map(|(v, _)| v)
        .collect();
    COND_CMPS.with(|m| *m.borrow_mut() = cmps);
}

/// True iff `v` is a comparison read only as a branch condition.
pub(crate) fn is_cond_cmp(v: ValueId) -> bool {
    COND_CMPS.with(|m| m.borrow().contains(&v))
}

/// The `let` that binds comparison `d` to the Rust `bool` expression `expr`.
pub(crate) fn bind(d: ValueId, expr: &str) -> String {
    if is_cond_cmp(d) {
        format!("let {}: bool = {expr};", crate::render_native::var(d))
    } else {
        format!("let mut {}: i64 = ({expr}) as i64;", crate::render_native::var(d))
    }
}

/// The Rust condition that is true when `cond` is non-zero.
pub(crate) fn truthy(cond: ValueId) -> String {
    if is_cond_cmp(cond) {
        crate::render_native::var(cond)
    } else {
        format!("{} != 0", crate::render_native::var(cond))
    }
}

/// The Rust condition that is true when `cond` is zero.
pub(crate) fn falsy(cond: ValueId) -> String {
    if is_cond_cmp(cond) {
        format!("!{}", crate::render_native::var(cond))
    } else {
        format!("{} == 0", crate::render_native::var(cond))
    }
}
