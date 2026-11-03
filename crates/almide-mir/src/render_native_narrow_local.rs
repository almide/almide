//! Narrow loop-carried locals in their own width (#4006, native only).
//!
//! The MIR carries every integer in one i64 lane, and the native render spells
//! a value the way the MIR carries it. A narrow ring op already computes in its
//! own type (`native_narrow_rewrite`: `(a as i32).wrapping_mul(b as i32) as
//! i64`), but a loop-carried `Int32` local still crosses each iteration as an
//! i64 holding the sign-extended value. LLVM then keeps the extension on the
//! loop's dependency chain and cannot fold the recurrence the way it folds the
//! standard codegen's real `i32` local (`int32_recurrence`: 1.106x).
//!
//! A local qualifies when every value it ever holds is, by construction, the
//! widening of a value of one narrow type `T`:
//! - it is declared by an integer constant inside `T`'s range, and
//! - every `SetLocal` of it stores a narrow ring op of `T`
//!   (`PrimKind::NativeNarrowBin`), or the result of a call whose declared
//!   return is `T` (the narrow call ABI widens it with `as i64`).
//!
//! Such a local is declared as `T` itself: its constant is written in `T`, each
//! store narrows with `as T` (exact, the stored value being a widened `T`), and
//! each read widens back with `as i64` (sign- or zero-extending as the lane
//! holds it). Every read therefore yields the very i64 the lane spelling
//! yielded; only the local's storage type changes. Integer locals carry no
//! ownership and no element access, so the ownership and bounds witnesses
//! taken from the MIR are the ones they were. The wasm leg never sees this.

use crate::render_native::{NativeSigKind, NativeSigs};
use crate::{MirFunction, Op, PrimKind, ValueId};
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    /// The narrow locals of the function being rendered: value → Rust type.
    static NARROW: RefCell<BTreeMap<ValueId, &'static str>> = RefCell::new(BTreeMap::new());
}

/// The function's narrow locals, installed for the render of `func` until the
/// guard drops.
pub(crate) fn scope(func: &MirFunction, sigs: &NativeSigs) -> Guard {
    NARROW.with(|n| *n.borrow_mut() = narrow_locals(func, sigs));
    Guard
}

/// Clears the installed set when the function's render ends.
pub(crate) struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        NARROW.with(|n| n.borrow_mut().clear());
    }
}

/// The Rust type `v` is stored as, when it is a narrow local.
pub(crate) fn narrow_type(v: ValueId) -> Option<&'static str> {
    NARROW.with(|n| n.borrow().get(&v).copied())
}

/// `(bits, signed)` of a narrow Rust type.
fn kind_of(t: &str) -> (u8, bool) {
    match t {
        "i8" => (8, true),
        "i16" => (16, true),
        "i32" => (32, true),
        "u8" => (8, false),
        "u16" => (16, false),
        _ => (32, false),
    }
}

fn in_range(value: i64, t: &str) -> bool {
    let (bits, signed) = kind_of(t);
    if signed {
        let half = 1i64 << (bits - 1);
        (-half..half).contains(&value)
    } else {
        (0..(1i64 << bits)).contains(&value)
    }
}

fn narrow_locals(func: &MirFunction, sigs: &NativeSigs) -> BTreeMap<ValueId, &'static str> {
    // What each value is, as far as this rule reads it.
    enum Def {
        Const(i64),
        Narrow(&'static str),
        Other,
    }
    let mut defs: BTreeMap<ValueId, Vec<Def>> = BTreeMap::new();
    let mut stores: BTreeMap<ValueId, Vec<ValueId>> = BTreeMap::new();
    for op in &func.ops {
        match op {
            Op::SetLocal { local, src } => stores.entry(*local).or_default().push(*src),
            Op::ConstInt { dst, value } => defs.entry(*dst).or_default().push(Def::Const(*value)),
            Op::Prim { kind: PrimKind::NativeNarrowBin { bits, signed, .. }, dst: Some(d), .. } => {
                let t = NativeSigKind::Narrow { bits: *bits, signed: *signed }.narrow_rust();
                defs.entry(*d).or_default().push(t.map_or(Def::Other, Def::Narrow));
            }
            Op::CallFn { dst: Some(d), name, .. } => {
                let t = sigs.get(name.as_str()).and_then(|(_, r)| r.and_then(NativeSigKind::narrow_rust));
                defs.entry(*d).or_default().push(t.map_or(Def::Other, Def::Narrow));
            }
            other => {
                // Every value the op touches beyond what it reads is one it
                // writes (counted per occurrence, so an op that both reads and
                // writes a value still counts as a write of it).
                let (mut all, mut reads) = (Vec::new(), Vec::new());
                crate::mir_ops::op_values(other, &mut all);
                crate::mir_ops::op_reads(other, &mut reads);
                for r in reads {
                    if let Some(k) = all.iter().position(|v| *v == r) {
                        all.swap_remove(k);
                    }
                }
                for w in all {
                    defs.entry(w).or_default().push(Def::Other);
                }
            }
        }
    }
    let params: std::collections::BTreeSet<ValueId> = func.params.iter().map(|p| p.value).collect();
    let mut out = BTreeMap::new();
    for (local, srcs) in &stores {
        if params.contains(local) {
            continue;
        }
        let Some([Def::Const(init)]) = defs.get(local).map(Vec::as_slice) else { continue };
        let mut ty: Option<&'static str> = None;
        let all = srcs.iter().all(|s| match defs.get(s).map(Vec::as_slice) {
            Some([Def::Narrow(t)]) => *ty.get_or_insert(t) == *t,
            _ => false,
        });
        if let (true, Some(t)) = (all, ty) {
            if in_range(*init, t) {
                out.insert(*local, t);
            }
        }
    }
    out
}
