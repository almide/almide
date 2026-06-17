//! Record / tuple VALUE-MODEL layout — the field offset decision for aggregate
//! construction and projection (`r.x`, `t.0`).
//!
//! # Block shape (v1 internal, NOT v0's pointer layout)
//! A record/tuple is a heap block `[rc@0][len@4][cap@8][slot0@12][slot1@20]…` —
//! the SAME `[rc][len][cap]` header every v1 `$alloc` block carries (so `Dup`/
//! `Drop`/the free-list reuse all work unchanged), with one UNIFORM 8-byte (i64)
//! slot per field in DECLARATION order. This is byte-identical to how the
//! existing scalar-tuple / scalar-list / `Init::IntList` machinery (`binds.rs`,
//! `alloc_init`) already lays a `(3, 7)` tuple or `[1, 2, 3]` list out — so a
//! record and a tuple share ONE layout, and a tuple LITERAL materialized as
//! `Init::IntList` reads back through the same slots.
//!
//! ## Why uniform 8-byte slots (not v0's width-packed layout)
//! v0 tight-packs record fields at their `byte_size` (Int8 = 1, Int = 8, …) from
//! a headerless pointer. v1 does NOT need to match that POINTER layout: the
//! dual-oracle byte-matches on STDOUT, not raw memory. A scalar value of ANY
//! width round-trips losslessly through an i64 slot (store the value, load it
//! back — `127:Int8` stays `127`, `-128` stays `-128`), so a uniform slot is
//! correct for the observable output AND avoids the prim floor's missing width-2
//! (Int16) store. The single invariant is store-width == load-width per slot,
//! which holds trivially: every slot is a `store64`/`load64`.
//!
//! # Scope (this brick = the VALUE MODEL)
//! Only SCALAR-field aggregates are materialized. A HEAP field (String/List/
//! nested record) is an i32 handle the record would OWN, needing a per-field
//! recursive DROP the current op set cannot express without a new ownership op
//! (a certificate change, out of scope) — so [`scalar_slots`] returns `None`
//! for it and the caller WALLS the aggregate cleanly (never wrong bytes).

use crate::lower::is_heap_ty;
use almide_lang::types::Ty;

/// Bytes of the `[rc@0][len@4][cap@8]` block header every v1 `$alloc` block
/// carries; aggregate slots tight-pack AFTER it. Mirrors `render_wasm::
/// LIST_HEADER` (private there) — the data area starts here, where the existing
/// scalar-tuple machinery (`binds.rs`) already stores its first slot.
pub(crate) const BLOCK_HEADER: u32 = 12;
/// One i64 slot per field, matching `render_wasm::ELEM_SIZE` and the IntList /
/// scalar-tuple materialization.
pub(crate) const SLOT_SIZE: u32 = 8;

/// The byte offset (from the block pointer, past the header) of field/element `i`
/// in the uniform-slot layout. The SINGLE offset computation construction and
/// projection both consult, so they cannot desync.
pub(crate) fn slot_offset(i: usize) -> u32 {
    BLOCK_HEADER + (i as u32) * SLOT_SIZE
}

/// `true` iff every field type is a SCALAR this brick can materialize (not heap,
/// and a value that fits/round-trips in an i64 slot — every supported scalar).
/// Int16/UInt16 round-trip fine through an i64 slot (store/load the full value),
/// so unlike a width-packed layout this admits them.
fn all_scalar(field_tys: &[Ty]) -> bool {
    !field_tys.is_empty() && field_tys.iter().all(|t| !is_heap_ty(t))
}

/// The number of uniform i64 slots for a DECLARATION-ordered scalar field-type
/// list — i.e. the field count — or `None` if it is empty or contains a HEAP
/// field (the aggregate is then walled). The block byte size is
/// `BLOCK_HEADER + slots * SLOT_SIZE`.
pub(crate) fn scalar_slots(field_tys: &[Ty]) -> Option<usize> {
    if all_scalar(field_tys) {
        Some(field_tys.len())
    } else {
        None
    }
}
