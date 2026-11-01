//! #4006: which call args a native render may MOVE instead of clone.
//!
//! The MIR keeps a value owned by its caller across a call that only borrows
//! it (`map.set(m, k, v)` builds a new map and the caller drops `m` later), so
//! the wasm leg copies. A Rust runtime body that takes the value by value
//! (`almide_rt_map_set(mut m: AlmideMap<K, V>, …)`) can be handed the caller's
//! own value when nothing reads it afterwards: the copy would be observably
//! the same, and only slower. That is v0's move, read off the MIR instead of
//! guessed: `v` dies at op `i` when op `i` is its last read other than a
//! release, and op `i` is not inside a loop that will read `v` again (a loop
//! entered after `v` was defined, unless the loop rebinds it (`SetLocal`)
//! after the read).

use crate::{MirFunction, Op, ValueId};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    static DYING: RefCell<BTreeSet<ValueId>> = const { RefCell::new(BTreeSet::new()) };
    static DECLARED: RefCell<BTreeMap<ValueId, u16>> = const { RefCell::new(BTreeMap::new()) };
}

/// The generated map type each value of the function being rendered is
/// declared as (its signature's, its callees', through joins and rebinds).
pub(crate) fn set_declared(m: BTreeMap<ValueId, u16>) {
    DECLARED.with(|d| *d.borrow_mut() = m);
}

/// The declared generated map type of `v`.
pub(crate) fn declared_map(v: ValueId) -> Option<u16> {
    DECLARED.with(|d| d.borrow().get(&v).copied())
}

fn is_release(op: &Op) -> bool {
    use Op::*;
    matches!(
        op,
        Drop { .. }
            | DropListStr { .. }
            | DropValue { .. }
            | DropListValue { .. }
            | DropListStrValue { .. }
            | DropListStrStr { .. }
            | DropListIntStr { .. }
            | DropListStrInt { .. }
            | DropResultListValue { .. }
            | DropResultValue { .. }
            | DropResultStrInt { .. }
            | DropResultValueInt { .. }
            | DropResultListValueInt { .. }
            | DropResultListStrInt { .. }
            | DropResultListStr { .. }
            | DropListListStr { .. }
            | DropVariant { .. }
            | DropWrapperRec { .. }
            | Consume { .. }
    )
}

/// Op index → the values that die there.
pub(crate) fn dying_at(func: &MirFunction) -> BTreeMap<usize, BTreeSet<ValueId>> {
    let mut defined_at: BTreeMap<ValueId, usize> = BTreeMap::new();
    let mut last_read: BTreeMap<ValueId, usize> = BTreeMap::new();
    // Each loop: (start, end) op indices.
    let mut loops: Vec<(usize, usize)> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::LoopStart => open.push(i),
            Op::LoopEnd => {
                if let Some(s) = open.pop() {
                    loops.push((s, i));
                }
            }
            _ => {}
        }
        if !is_release(op) {
            let mut reads = Vec::new();
            crate::mir_ops::op_reads(op, &mut reads);
            for v in reads {
                last_read.insert(v, i);
            }
        }
        if let Some(d) = crate::mir_ops::defined_value(op) {
            defined_at.entry(d).or_insert(i);
        }
    }
    let mut out: BTreeMap<usize, BTreeSet<ValueId>> = BTreeMap::new();
    // A read whose value is rebound later in its own block, with no read of
    // it in between, is that value's last read on every path: the rebind
    // dominates whatever follows, the loop's back edge included.
    for (i, op) in func.ops.iter().enumerate() {
        if is_release(op) {
            continue;
        }
        let mut reads = Vec::new();
        crate::mir_ops::op_reads(op, &mut reads);
        for v in reads {
            if rebound_before_read(func, i, v) {
                out.entry(i).or_default().insert(v);
            }
        }
    }
    for (v, i) in last_read {
        let def = defined_at.get(&v).copied();
        let carried = loops.iter().any(|&(s, e)| s < i && i < e && def.is_none_or(|d| d < s));
        if !carried {
            out.entry(i).or_default().insert(v);
        }
    }
    out
}

/// `v`, read at op `i`, is rebound (`SetLocal`) later in the block `i` sits
/// in (not inside a nested `if` or loop) before any further read of it.
fn rebound_before_read(func: &MirFunction, i: usize, v: ValueId) -> bool {
    let mut depth = 0usize;
    for op in &func.ops[i + 1..] {
        match op {
            Op::IfThen { .. } | Op::LoopStart => depth += 1,
            Op::EndIf { .. } | Op::LoopEnd if depth == 0 => return false,
            Op::Else { .. } if depth == 0 => return false,
            Op::EndIf { .. } | Op::LoopEnd => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            if let Op::SetLocal { local, src } = op {
                if *local == v {
                    return *src != v;
                }
            }
        }
        // (A release of the old value before its rebind is no read.)
        if is_release(op) {
            continue;
        }
        let mut reads = Vec::new();
        crate::mir_ops::op_reads(op, &mut reads);
        if reads.contains(&v) {
            return false;
        }
    }
    false
}

/// Set the values dying at the op about to render.
pub(crate) fn set_dying(vs: BTreeSet<ValueId>) {
    DYING.with(|d| *d.borrow_mut() = vs);
}

/// `v` is not read again after the op being rendered.
pub(crate) fn dies_here(v: ValueId) -> bool {
    DYING.with(|d| d.borrow().contains(&v))
}
