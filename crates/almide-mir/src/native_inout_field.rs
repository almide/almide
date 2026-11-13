//! A field of the caller's record lent to an in-out param (`f(st.budget)`,
//! `crate::native_inout`): the call lends the field's place (`&mut st.f1`),
//! so the callee writes the caller's field, as the source's `mut` argument
//! names that place (C-132).
//!
//! The MIR passes the argument as a read of the field (`LoadHandle` of the
//! record's heap slot), which the render would spell as a copy. Where that
//! read's one use is the in-out position and nothing touches the record
//! between, the read IS the place: [`note_function`] notes it, [`lends`]
//! renders nothing for it, and [`arg`] lends `&mut place`. A field read at an
//! in-out position that cannot lend its place is refused: writing a
//! detached copy would drop the callee's writes.

use crate::{CallArg, MirFunction, Op, ValueId};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

thread_local! {
    /// The field reads of the fn being rendered that lend their place, and
    /// the place once the read renders.
    static LENT: RefCell<BTreeMap<ValueId, Option<String>>> = const { RefCell::new(BTreeMap::new()) };
    /// The field reads at an in-out position that lend no place.
    static REFUSED: RefCell<BTreeSet<ValueId>> = const { RefCell::new(BTreeSet::new()) };
}

/// Note `func`'s field reads (`LoadHandle` of a record's heap field) whose
/// one use is an in-out argument: lent where neither the record nor the read
/// is touched between the read and the call, refused otherwise.
pub(crate) fn note_function(func: &MirFunction) {
    let roots = crate::render_native_borrow_read::roots(func);
    let mut lent = BTreeMap::new();
    let mut refused = BTreeSet::new();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::Prim { kind: crate::PrimKind::LoadHandle, dst: Some(d), args } = op else { continue };
        let Some(call) = in_out_use(func, i, *d) else { continue };
        let root = args.first().and_then(|a| roots.get(a));
        let ok = root.is_some_and(|r| untouched_between(func, i, call, &[*r]));
        if ok {
            lent.insert(*d, None);
        } else {
            refused.insert(*d);
        }
    }
    LENT.with(|m| *m.borrow_mut() = lent);
    REFUSED.with(|m| *m.borrow_mut() = refused);
}

/// Whether the field read `d` lends its place (`place`, as the read
/// renders it): the read then renders nothing.
pub(crate) fn lends(d: ValueId, place: &str) -> bool {
    LENT.with(|m| match m.borrow_mut().get_mut(&d) {
        Some(slot) => {
            *slot = Some(place.to_string());
            true
        }
        None => false,
    })
}

/// The place the read `v` lends ([`lends`]).
fn lent_place(v: ValueId) -> Option<String> {
    LENT.with(|m| m.borrow().get(&v).cloned().flatten())
}

/// Whether `v` is a field read at an in-out position that lends no place:
/// one noted lent whose read rendered another way is refused too.
fn refused_copy(v: ValueId) -> bool {
    REFUSED.with(|m| m.borrow().contains(&v)) || LENT.with(|m| m.borrow().get(&v).is_some_and(Option::is_none))
}

/// The index of the one call that reads `dst`, at an in-out position and at
/// no other: every other read of `dst` is its release.
fn in_out_use(func: &MirFunction, at: usize, dst: ValueId) -> Option<usize> {
    let mut call = None;
    let mut reads = Vec::new();
    for (j, op) in func.ops.iter().enumerate().skip(at + 1) {
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        if !reads.contains(&dst) {
            continue;
        }
        match op {
            Op::CallFn { name, args, .. } if call.is_none() => {
                let at_inout = args.iter().enumerate().filter(|(_, a)| matches!(a, CallArg::Handle(v) if *v == dst));
                let ks: Vec<usize> = at_inout.map(|(k, _)| k).collect();
                if ks.len() != 1 || !crate::native_inout::is_inout_position(name, ks[0]) {
                    return None;
                }
                call = Some(j);
            }
            _ if crate::render_native_move::bookkeeping(op) && call.is_some() => {}
            _ => return None,
        }
    }
    call
}

/// No op strictly between `from` and `to` reads any of `vs` (the record
/// and its field view stay as the copy saw them up to the call).
fn untouched_between(func: &MirFunction, from: usize, to: usize, vs: &[ValueId]) -> bool {
    let mut reads = Vec::new();
    func.ops[from + 1..to].iter().all(|op| {
        reads.clear();
        crate::mir_ops::op_reads(op, &mut reads);
        !reads.iter().any(|r| vs.contains(r)) && !matches!(op, Op::SetLocal { local, .. } if vs.contains(local))
    })
}

/// The in-out argument the copy `v` renders as: its lent place, or a wall
/// for a field copy that lends none; `None` for any other value.
pub(crate) fn arg(name: &str, v: ValueId) -> Option<Result<String, crate::lower::LowerError>> {
    if let Some(place) = lent_place(v) {
        return Some(Ok(format!("&mut {place}")));
    }
    refused_copy(v).then(|| {
        Err(crate::render_native::wall(format!(
            "native: in-out argument of `{name}` is a copy of a field whose place the call cannot lend"
        )))
    })
}
