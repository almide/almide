//! The LARGE-block free list (#3348): blocks whose total exceeds the
//! largest size class (512 KiB) are kept on one address-ordered list of
//! free extents with exact sizes — split on take, coalesced with both
//! list neighbours on release — instead of being abandoned to the bump
//! graveyard. The sixteen power-of-two classes below the ceiling are
//! untouched.
//!
//! A free node at base `n` keeps rc 0 at `n+0`, its SIZE (total bytes)
//! at `n+4` and the next node's base at `n+12` (0 ends the list). The
//! list head lives at address 12 — the next field of a SENTINEL node at
//! address 0 whose size word `[4]` is the null block's len, always 0. A
//! walk starts with the predecessor `p = 0`, so relinking is `[p+12] := x`
//! with no head special case, and the sentinel can never be merged into
//! (`0 + [4]` is never a block base). Adjacency is a list-neighbour
//! property because the list is address-ordered: a released block can
//! only touch its predecessor and its successor.
//!
//! The loops are emitted as `block { loop { cond; eqz; br_if 1; body;
//! br 0 } }`, the `LWhile` shape proofs/LargeTree.v transcribes; that
//! file proves the two bodies compute LargeList.v's `ins` / `take`.

use wasm_encoder::{BlockType, InstructionSink, MemArg};

use crate::*;

/// The list head: the sentinel node's next field.
pub(crate) const LARGE_HEAD: u32 = 12;
/// Split a fitting node only when at least this much would stay free —
/// a smaller remainder rides along in the handed-out block's cap.
const SPLIT: i32 = 65536;

fn word(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 2, memory_index: 0 }
}

/// `while (cond) { pp = p; p = q; q = [q+12] }` — the walk both
/// functions share; `cond` leaves an i32 flag.
fn walk(i: &mut InstructionSink<'_>, (pp, p, q): (u32, u32, u32), cond: impl Fn(&mut InstructionSink<'_>)) {
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    cond(i);
    i.i32_eqz().br_if(1);
    i.local_get(p).local_set(pp);
    i.local_get(q).local_set(p);
    i.local_get(q).i32_load(word(12)).local_set(q);
    i.br(0).end().end();
}

/// `$lfree` inlined into `$free`: link the extent `[b, b+t)` into the
/// list, merging with the successor when `b+t` is its base and with the
/// predecessor when it ends at `b`. `pp`, `p`, `q` are fresh locals
/// (zero on function entry, so the walk starts at the sentinel).
pub(crate) fn emit_lfree(i: &mut InstructionSink<'_>, (b, t): (u32, u32), (pp, p, q): (u32, u32, u32)) {
    i.i32_const(0).i32_load(word(LARGE_HEAD)).local_set(q);
    walk(i, (pp, p, q), |i| {
        i.local_get(q).i32_const(0).i32_ne();
        i.local_get(q).local_get(b).i32_lt_u();
        i.i32_and();
    });
    // successor touches: absorb it
    i.local_get(b).local_get(t).i32_add().local_get(q).i32_eq().if_(BlockType::Empty);
    i.local_get(t).local_get(q).i32_load(word(4)).i32_add().local_set(t);
    i.local_get(q).i32_load(word(12)).local_set(q);
    i.end();
    // predecessor touches: it absorbs us
    i.local_get(p).local_get(p).i32_load(word(4)).i32_add().local_get(b).i32_eq().if_(BlockType::Empty);
    i.local_get(t).local_get(p).i32_load(word(4)).i32_add().local_set(t);
    i.local_get(p).local_set(b);
    i.local_get(pp).local_set(p);
    i.end();
    i.local_get(b).i32_const(0).i32_store(word(0));
    i.local_get(b).local_get(t).i32_store(word(4));
    i.local_get(b).local_get(q).i32_store(word(12));
    i.local_get(p).local_get(b).i32_store(word(12));
}

/// `$ltake` inlined into `$alloc`'s above-ceiling arm: the first node of
/// at least `want` bytes is handed out (split when SPLIT or more would
/// remain) with header rc=1/len/cap and RETURNED; on no fit, a last node
/// ending at the frontier is unlinked and the frontier lowered to it, so
/// the bump that follows extends it in place. `reused` bumps the counter
/// on a hit (armed builds).
pub(crate) fn emit_ltake(
    i: &mut InstructionSink<'_>,
    (want, len): (u32, u32),
    (pp, p, q, z, r): (u32, u32, u32, u32, u32),
    reused: impl Fn(&mut InstructionSink<'_>),
) {
    i.i32_const(0).i32_load(word(LARGE_HEAD)).local_set(q);
    walk(i, (pp, p, q), |i| {
        i.local_get(q).i32_const(0).i32_ne();
        i.local_get(q).i32_load(word(4)).local_get(want).i32_lt_u();
        i.i32_and();
    });
    i.local_get(q).i32_const(0).i32_ne().if_(BlockType::Empty);
    {
        i.local_get(q).i32_load(word(4)).local_set(z);
        i.local_get(z).local_get(want).i32_sub().i32_const(SPLIT).i32_ge_u().if_(BlockType::Empty);
        i.local_get(q).local_get(want).i32_add().local_set(r);
        i.local_get(r).i32_const(0).i32_store(word(0));
        i.local_get(r).local_get(z).local_get(want).i32_sub().i32_store(word(4));
        i.local_get(r).local_get(q).i32_load(word(12)).i32_store(word(12));
        i.local_get(p).local_get(r).i32_store(word(12));
        i.local_get(want).local_set(z);
        i.else_();
        i.local_get(p).local_get(q).i32_load(word(12)).i32_store(word(12));
        i.end();
        reused(i);
        i.local_get(q).i32_const(1).i32_store(word(almide_layout::RC.offset));
        i.local_get(q).local_get(len).i32_store(word(almide_layout::LEN.offset));
        i.local_get(q)
            .local_get(z)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_sub()
            .i32_store(word(almide_layout::CAP.offset));
        i.local_get(q).return_();
    }
    i.end();
    // no fit: a tail node ending at the frontier becomes the bump's start
    i.local_get(p).local_get(p).i32_load(word(4)).i32_add().global_get(G_HEAP).i32_eq().if_(BlockType::Empty);
    i.local_get(pp).i32_const(0).i32_store(word(12));
    i.local_get(p).global_set(G_HEAP);
    i.end();
}
