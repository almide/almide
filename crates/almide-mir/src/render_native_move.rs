//! Last-use moves on the native render (#4006, no-perf-loss rule).
//!
//! The MIR copies a heap value with `Dup` and rebinds a slot with `SetLocal`;
//! the native render realizes both as `.clone()`, which keeps every alias its
//! own value. When the source is never read again that copy is pure cost — a
//! swap-carried list (`cur = merged` at a loop's end) copied its whole buffer
//! twice per round. Here such a copy becomes `std::mem::take(&mut src)`: the
//! destination gets the buffer and the source is left empty, which nothing
//! observes, because:
//!
//! * no op after the copy reads the source (a drop is not a read: drops erase
//!   to scope end on this render), nor is it the function's result;
//! * the source is defined in the same loop as the copy, so a later iteration
//!   re-defines it before any read — a value from outside the loop is never
//!   emptied for the next round;
//! * no borrow join (`crate::borrow_join`) holds a reference to it;
//! * its type has a `Default` to leave behind.
//!
//! Any doubt keeps the clone.

use crate::render_native::NTy;
use crate::{MirFunction, Op, ValueId};

/// Whether `op` only does ownership bookkeeping on this render (no code reads
/// its operand).
pub(crate) fn bookkeeping(op: &Op) -> bool {
    matches!(
        op,
        Op::Drop { .. }
            | Op::DropListStr { .. }
            // The typed releases erase to scope end too (render_native's drop arms).
            | Op::DropValue { .. }
            | Op::DropListValue { .. }
            | Op::DropListStrValue { .. }
            | Op::DropResultValue { .. }
            | Op::DropResultListValue { .. }
            | Op::DropResultValueInt { .. }
            | Op::DropResultListValueInt { .. }
            | Op::DropListStrStr { .. }
            | Op::DropListIntStr { .. }
            | Op::DropListStrInt { .. }
            | Op::DropResultStrInt { .. }
            | Op::DropResultListStrInt { .. }
            | Op::DropResultListStr { .. }
            | Op::DropListListStr { .. }
            | Op::DropVariant { .. }
            | Op::DropWrapperRec { .. }
            | Op::Consume { .. }
            | Op::Borrow { .. }
            | Op::MakeUnique { .. }
    )
}

/// The negative control of scripts/check-native-v1-perf.sh: with
/// `ALMIDE_NATIVE_DRILL_NO_MOVES=1` the render keeps every copy (and the
/// one-element-list push), so the gate must read the cost.
pub(crate) fn drill_no_moves() -> bool {
    almide_base::env::flag("ALMIDE_NATIVE_DRILL_NO_MOVES")
}

/// The innermost open `LoopStart` index at each op (`None` = no loop).
fn loop_of(func: &MirFunction) -> Vec<Option<usize>> {
    let mut stack: Vec<usize> = Vec::new();
    func.ops
        .iter()
        .enumerate()
        .map(|(i, op)| {
            let here = stack.last().copied();
            match op {
                Op::LoopStart => stack.push(i),
                Op::LoopEnd => {
                    stack.pop();
                }
                _ => {}
            }
            here
        })
        .collect()
}

/// Whether the copy of `src` at `ops[at]` may move it out instead.
pub(crate) fn last_use(
    func: &MirFunction,
    at: usize,
    src: ValueId,
    t: NTy,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) -> bool {
    if drill_no_moves() || !matches!(t, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_)) || !crate::render_native_rec::has_default(t) {
        return false;
    }
    if func.ret == Some(src) || func.params.iter().any(|p| p.value == src) {
        return false;
    }
    let mut reads = Vec::new();
    for op in &func.ops[at + 1..] {
        if bookkeeping(op) {
            continue;
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if reads.contains(&src) {
            return false;
        }
        if matches!(op, Op::SetLocal { local, .. } if *local == src) {
            return false;
        }
    }
    let joined = func
        .ops
        .iter()
        .any(|op| matches!(op, Op::Dup { dst, src: s } if *s == src && borrow_arms.contains(dst)));
    if joined {
        return false;
    }
    let loops = loop_of(func);
    let defs: Vec<usize> = func
        .ops
        .iter()
        .enumerate()
        .filter(|(_, op)| {
            crate::mir_ops::defined_value(op) == Some(src) || matches!(op, Op::SetLocal { local, .. } if *local == src)
        })
        .map(|(i, _)| i)
        .collect();
    // Every definition precedes the copy, inside its loop (or a loop nested in
    // it), and one is in that loop itself: each round re-defines the value.
    let within = |d: usize| {
        let mut l = loops[d];
        loop {
            if l == loops[at] {
                return true;
            }
            match l {
                Some(start) => l = loops[start],
                None => return false,
            }
        }
    };
    defs.iter().any(|&d| d < at && loops[d] == loops[at]) && defs.iter().all(|&d| d < at && within(d))
}

/// Whether `src`, read at `ops[at]`, is rebound (`SetLocal`) later in the
/// block `at` sits in (not inside a nested `if` or loop) before any further
/// read: the rebind dominates everything after, the loop's back edge
/// included, so the read is the old value's last (`m = map.set(m, k, v)`).
fn rebound_before_read(func: &MirFunction, at: usize, src: ValueId) -> bool {
    let mut depth = 0usize;
    let mut reads = Vec::new();
    for op in &func.ops[at + 1..] {
        match op {
            Op::IfThen { .. } | Op::LoopStart => depth += 1,
            Op::EndIf { .. } | Op::LoopEnd | Op::Else { .. } if depth == 0 => return false,
            Op::EndIf { .. } | Op::LoopEnd => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            if let Op::SetLocal { local, src: s } = op {
                if *local == src {
                    return *s != src;
                }
            }
        }
        if bookkeeping(op) {
            continue;
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if reads.contains(&src) {
            return false;
        }
    }
    false
}

/// Whether a call's arg `src` at `ops[at]` may be moved into a by-value
/// runtime param: the last-use rule above, or a read the value's own rebind
/// follows before any other read. Same eligibility (owned kind with a
/// `Default`, not a param, not the result, no borrow join).
pub(crate) fn moves_at_call(
    func: &MirFunction,
    at: usize,
    src: ValueId,
    t: NTy,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) -> bool {
    if last_use(func, at, src, t, borrow_arms) {
        return true;
    }
    if drill_no_moves() || !matches!(t, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Rec(_)) || !crate::render_native_rec::has_default(t) {
        return false;
    }
    // (The function's result is fine here: what it returns is the rebound
    // value, read after the rebind.)
    if func.params.iter().any(|p| p.value == src) {
        return false;
    }
    let joined = func
        .ops
        .iter()
        .any(|op| matches!(op, Op::Dup { dst, src: s } if *s == src && borrow_arms.contains(dst)));
    !joined && rebound_before_read(func, at, src)
}

thread_local! {
    static MOVING: std::cell::RefCell<std::collections::BTreeSet<ValueId>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

thread_local! {
    static STATIC_STRS: std::cell::RefCell<std::collections::BTreeSet<ValueId>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// The string literals of the function being rendered: values an
/// `Alloc { init: Str }` defines, rendered as immortal static blocks, whose
/// buffer is never unique (an edit copies it first).
pub(crate) fn set_static_strs(func: &MirFunction) {
    let lits = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Alloc { dst, init: crate::Init::Str(_), .. } => Some(*dst),
            _ => None,
        })
        .collect();
    STATIC_STRS.with(|m| *m.borrow_mut() = lits);
}

pub(crate) fn is_static_str(v: ValueId) -> bool {
    STATIC_STRS.with(|m| m.borrow().contains(&v))
}

/// Record which of `ops[at]`'s call args may move (read by the floor render).
pub(crate) fn set_moving_args(
    func: &MirFunction,
    at: usize,
    tys: &std::collections::BTreeMap<ValueId, NTy>,
    borrow_arms: &std::collections::BTreeSet<ValueId>,
) {
    let moving = match &func.ops[at] {
        Op::CallFn { args, .. } => args
            .iter()
            .filter_map(|a| match a {
                crate::CallArg::Handle(v) => Some(*v),
                _ => None,
            })
            .filter(|v| tys.get(v).is_some_and(|t| moves_at_call(func, at, *v, *t, borrow_arms)))
            .collect(),
        _ => Default::default(),
    };
    MOVING.with(|m| *m.borrow_mut() = moving);
}

/// `v` is an arg of the call being rendered that may be moved.
pub(crate) fn moving_arg(v: ValueId) -> bool {
    MOVING.with(|m| m.borrow().contains(&v))
}
