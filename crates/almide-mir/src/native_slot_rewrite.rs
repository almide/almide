//! NATIVE-ONLY storage-slot rewrite (#4006, family I).
//!
//! A mutable module-level binding lives in a storage slot: the shared
//! lowering reads a scalar one as `ConstInt a = mg_slot_addr(i); Load8(a)` and
//! writes it as `Store8(a, v)`. The native render has no linear memory, so
//! this pass maps exactly those two shapes (an 8-byte access whose address is
//! a constant inside the slot region) onto `NativeSlotLoad { slot }` /
//! `NativeSlotStore { slot }`, which render as the slot's own `static`. A
//! heap slot (`LoadHandle`, `__mg_take`) is left as it is and walls at the
//! render. The wasm leg never sees any of this.

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
        (addr >= base && addr < end && (addr - base) % 8 == 0).then(|| ((addr - base) / 8) as u32)
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
