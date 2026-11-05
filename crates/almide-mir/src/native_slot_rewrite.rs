//! NATIVE-ONLY storage-slot rewrite (#4006, family I).
//!
//! A mutable module-level binding lives in a storage slot: the shared
//! lowering reads a scalar one as `ConstInt a = mg_slot_addr(i); Load8(a)` and
//! writes it as `Store8(a, v)`. The native render has no linear memory, so
//! this pass maps exactly those two shapes (an 8-byte access whose address is
//! a constant inside the slot region) onto `NativeSlotLoad { slot }` /
//! `NativeSlotStore { slot }`, which render as the slot's own `static`. A
//! heap slot (`LoadHandle`, `__mg_take`, the `Store` of a block's handle) is
//! left as it is: the render serves one whose declared type has a native
//! kind from its own thread-local cell (`heap_slot`), and walls on any other.
//! The wasm leg never sees any of this.

use crate::{MirFunction, Op, PrimKind, ValueId};
use std::collections::BTreeMap;

/// Rewrite the scalar slot accesses of every fn; `slots` is the program's
/// slot count (`mg_slot_addr(0..slots)` is the slot region).
pub fn rewrite_slot_ops(functions: &mut [MirFunction], slots: u32) {
    if slots == 0 {
        return;
    }
    let base = i64::from(crate::mg_slot_addr(0));
    let end = i64::from(crate::mg_slot_addr(slots));
    let slot_of = |addr: i64| -> Option<u32> {
        (addr >= base && addr < end && (addr - base) % 8 == 0)
            .then(|| ((addr - base) / 8) as u32)
            .filter(|s| heap_slot(*s).is_none())
    };
    for f in functions {
        let mut consts: BTreeMap<ValueId, i64> = BTreeMap::new();
        for op in f.ops.iter_mut() {
            let rewritten = match op {
                Op::ConstInt { dst, value } => {
                    consts.insert(*dst, *value);
                    None
                }
                Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(d), args } if args.len() == 1 => consts
                    .get(&args[0])
                    .and_then(|a| slot_of(*a))
                    .map(|slot| Op::Prim { kind: PrimKind::NativeSlotLoad { slot }, dst: Some(*d), args: vec![] }),
                Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args } if args.len() == 2 => consts
                    .get(&args[0])
                    .and_then(|a| slot_of(*a))
                    .map(|slot| Op::Prim { kind: PrimKind::NativeSlotStore { slot }, dst: None, args: vec![args[1]] }),
                _ => None,
            };
            if let Some(r) = rewritten {
                *op = r;
            }
        }
    }
}

/// The `static` a slot renders as, once per slot (a `used_shims` item).
pub(crate) fn slot_static(slot: u32) -> &'static str {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<BTreeMap<u32, &'static str>>> = OnceLock::new();
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    c.entry(slot).or_insert_with(|| {
        Box::leak(
            format!(
                "static ALMD_SLOT_{slot}: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);"
            )
            .into_boxed_str(),
        )
    })
}

// ── heap slots (#4006 C1 class D) ──

thread_local! {
    /// The storage slots of the program being rendered that hold a heap
    /// value, and its native kind (set by the pipeline before the rewrite).
    static HEAP_SLOTS: std::cell::RefCell<BTreeMap<u32, crate::render_native::NTy>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
    /// The heap slots a render read or wrote.
    static HEAP_USED: std::cell::RefCell<std::collections::BTreeSet<u32>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// Install the heap slots of the program about to be rewritten and rendered.
pub(crate) fn set_heap_slots(slots: BTreeMap<u32, crate::render_native::NTy>) {
    HEAP_SLOTS.with(|h| *h.borrow_mut() = slots);
    HEAP_USED.with(|u| u.borrow_mut().clear());
}

/// The native kind of heap slot `slot`, when the render serves it.
pub(crate) fn heap_slot(slot: u32) -> Option<crate::render_native::NTy> {
    HEAP_SLOTS.with(|h| h.borrow().get(&slot).copied())
}

/// The heap slot at address `addr`, and its kind; noted as used.
pub(crate) fn heap_slot_at(addr: i64) -> Option<(u32, crate::render_native::NTy)> {
    let base = i64::from(crate::mg_slot_addr(0));
    if addr < base || (addr - base) % 8 != 0 {
        return None;
    }
    let slot = u32::try_from((addr - base) / 8).ok()?;
    let t = heap_slot(slot)?;
    HEAP_USED.with(|u| u.borrow_mut().insert(slot));
    Some((slot, t))
}

/// The thread-local cell of every heap slot the render used (`used_shims`
/// items): the slot's value, its type's default until the program's
/// `__mg_init` stores it.
pub(crate) fn heap_slot_statics() -> Vec<&'static str> {
    use std::sync::{Mutex, OnceLock};
    static TEXTS: OnceLock<Mutex<std::collections::BTreeSet<&'static str>>> = OnceLock::new();
    let used: Vec<(u32, crate::render_native::NTy)> =
        HEAP_USED.with(|u| u.borrow().iter().filter_map(|s| heap_slot(*s).map(|t| (*s, t))).collect());
    let mut texts = TEXTS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    used.into_iter()
        .map(|(slot, t)| {
            let text = format!(
                "thread_local! {{ static ALMD_HSLOT_{slot}: std::cell::RefCell<{ty}> = std::cell::RefCell::new(Default::default()); }}",
                ty = crate::render_native_rec::owned_ty(t)
            );
            match texts.get(text.as_str()) {
                Some(t) => *t,
                None => {
                    let leaked: &'static str = Box::leak(text.into_boxed_str());
                    texts.insert(leaked);
                    leaked
                }
            }
        })
        .collect()
}
