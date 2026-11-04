//! An empty list literal joined into a list of strings (#4006, native only).
//!
//! The lowering folds `[]` to `Alloc IntList([])` whatever its element type:
//! on the wasm leg an empty list is one header with length 0, whichever
//! element it would hold. The native render spells that alloc `Vec<i64>`, so
//! an if-value whose other arm yields a `Vec<AlmideString>` (`Pair(a, c) =>
//! [a, c], _ => []`) walled on the type disagreement.
//!
//! An empty list is INERT when nothing but the ownership bookkeeping
//! (`Consume`, `Borrow`, a drop) and the arm-result markers (`Else`, `EndIf`)
//! reads it: no op adds an element to it, indexes it or hands it to a call.
//! Its value is then the empty list at every point, and the join takes the
//! empty list of the join's own type, `Vec::new()`. The MIR is unchanged, so
//! the ownership, call-mode and bounds witnesses are the ones taken from it.

use crate::{MirFunction, Op, ValueId};
use std::cell::RefCell;
use std::collections::BTreeSet;

thread_local! {
    /// The inert empty lists of the function being rendered.
    static INERT: RefCell<BTreeSet<ValueId>> = RefCell::new(BTreeSet::new());
}

/// The function's inert empty lists, installed for the render of `func`
/// until the guard drops.
pub(crate) fn scope(func: &MirFunction) -> Guard {
    INERT.with(|s| *s.borrow_mut() = inert_empty_lists(func));
    Guard
}

/// Clears the installed set when the function's render ends.
pub(crate) struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        INERT.with(|s| s.borrow_mut().clear());
    }
}

/// Whether `v` is an inert empty list of the function being rendered.
pub(crate) fn is_inert_empty(v: ValueId) -> bool {
    INERT.with(|s| s.borrow().contains(&v))
}

fn inert_empty_lists(func: &MirFunction) -> BTreeSet<ValueId> {
    let mut empties: BTreeSet<ValueId> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::Alloc { dst, init: crate::Init::IntList(vs), .. } if vs.is_empty() => Some(*dst),
            _ => None,
        })
        .collect();
    let mut reads = Vec::new();
    for op in &func.ops {
        let marker = matches!(
            op,
            Op::Consume { .. }
                | Op::Borrow { .. }
                | Op::Else { .. }
                | Op::EndIf { .. }
                | Op::Drop { .. }
        );
        if marker {
            continue;
        }
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        for r in &reads {
            empties.remove(r);
        }
    }
    empties
}
