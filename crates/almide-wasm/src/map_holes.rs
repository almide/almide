//! The hole family of the keyed-lookup index (#3691): the bodies of the
//! helpers a tombstoning `map.delete` relies on — the index doubled with its
//! tombstones dropped, the delete's tombstone, the in-place compaction and
//! the hole count. The design is map_index.rs's "Holes" section; the index
//! layout and its probe primitives live there.

use wasm_encoder::{BlockType, Function, ValType};

use crate::map_index::{if_flooded, load_key, probe_empty_counted, w, IdxFns, IdxKey, DEAD, IDX_HDR, INDEX_THRESHOLD, SLOTS, TOMB};
use crate::map_ordered::{ORDERED, ROOT};
use crate::*;

/// Push the absolute address of entry ordinal `ord - 1`'s key slot
/// (`ord` is a slot value: ordinal + 1).
fn key_addr(i: &mut wasm_encoder::InstructionSink<'_>, block: u32, ord: u32, esz: u32, koff: u32) {
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(ord).i32_const(1).i32_sub().local_get(esz).i32_mul().i32_add();
    i.local_get(koff).i32_add();
}

/// `$mapidx_grow_<key>(block, v, esz, koff) -> index`: `v` doubled — the
/// live slots re-placed, the tombstones dropped, `stamp` and `dead`
/// carried over — and `v` freed. The holes stay in the block; only their
/// slots go, which a load check counting `len` never misses. A re-place
/// that walks a flooded chain hands `v` to the ordered index instead
/// (#4104, map_ordered.rs).
pub(crate) fn emit_grow(key: IdxKey, hash: u32, from_index: u32) -> Function {
    // params: 0=block, 1=v, 2=esz, 3=koff; locals: 4=cap, 5=n, 6=p, 7=end,
    // 8=s, 9=h, 10=q, 11=mask, 12=steps
    let (block, v, esz, koff) = (0u32, 1u32, 2u32, 3u32);
    let (cap, n, p, end, s, h, q, mask, steps) = (4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32);
    let mut f = Function::new([(9, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).i32_load(w(almide_layout::PAYLOAD + 4)).i32_const(1).i32_add().local_tee(cap);
    i.i32_const(1).i32_shl().local_tee(cap).i32_const(1).i32_sub().local_set(mask);
    i.i32_const(IDX_HDR).local_get(cap).i32_const(2).i32_shl().i32_add().call(F_ALLOC).local_set(n);
    i.local_get(n).i32_const(SLOTS as i32).i32_add().i32_const(0).local_get(cap).i32_const(2).i32_shl().memory_fill(0);
    i.local_get(n).local_get(v).i32_load(w(almide_layout::PAYLOAD)).i32_store(w(almide_layout::PAYLOAD));
    i.local_get(n).local_get(mask).i32_store(w(almide_layout::PAYLOAD + 4));
    i.local_get(n).local_get(v).i32_load(w(DEAD)).i32_store(w(DEAD));
    i.local_get(v).local_tee(p).local_get(cap).i32_const(1).i32_shl().i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    // a live slot: s ∉ {0, TOMB} ⇔ s + 1 >u 1
    i.local_get(p).i32_load(w(SLOTS)).local_tee(s).i32_const(1).i32_add().i32_const(1).i32_gt_u().if_(BlockType::Empty);
    key_addr(&mut i, block, s, esz, koff);
    load_key(&mut i, key);
    i.call(hash).local_get(mask).i32_and().local_set(h);
    probe_empty_counted(&mut i, n, mask, h, q, Some(steps));
    if_flooded(&mut i, steps, |i| {
        i.local_get(n).call(F_FREE);
        i.local_get(block).local_get(v).local_get(esz).local_get(koff).call(from_index).return_();
    });
    i.local_get(q).local_get(s).i32_store(w(SLOTS));
    i.end();
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0).end().end();
    i.local_get(v).call(F_FREE);
    i.local_get(n);
    i.end();
    f
}

/// `$mapidx_kill_<key>(block, esz, koff, e) -> 1 | 0`: the in-place delete
/// of entry `e` (absolute) through the index — its slot tombstoned, the
/// hole counted; the entry stays where it is for the caller to release.
/// 0 under the threshold: the caller shifts. A block not indexed yet is
/// indexed here (its delete already counted as a lookup), so a
/// delete-then-insert loop rides the index instead of re-scanning.
pub(crate) fn emit_kill(key: IdxKey, fns: IdxFns, build: u32) -> Function {
    // params: 0=block, 1=esz, 2=koff, 3=e; locals: 4=len, 5=v, 6=mask, 7=h,
    // 8=want
    let (block, esz, koff, e) = (0u32, 1u32, 2u32, 3u32);
    let (len, v, mask, h, want) = (4u32, 5u32, 6u32, 7u32, 8u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).i32_load(len_memarg()).local_tee(len);
    i.local_get(esz).i32_const(INDEX_THRESHOLD as i32).i32_mul().i32_lt_u().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.local_get(block).call(fns.side_get).local_tee(v).i32_const(1).i32_le_u();
    i.local_get(v).i32_load(w(almide_layout::PAYLOAD)).local_get(len).i32_ne();
    i.i32_or().if_(BlockType::Empty);
    i.local_get(v).i32_const(1).i32_gt_u().if_(BlockType::Empty);
    i.local_get(v).call(F_FREE);
    i.end();
    i.local_get(block).local_get(esz).local_get(koff).call(build).local_set(v);
    i.local_get(block).local_get(v).call(fns.side_set).drop();
    i.end();
    // an ordered index unlinks the node while the key is still in place
    i.local_get(v).i32_load(w(almide_layout::PAYLOAD + 4)).i32_const(ORDERED).i32_eq().if_(BlockType::Empty);
    i.local_get(v);
    i.local_get(v).local_get(v).i32_load(w(ROOT)).local_get(block).local_get(esz).local_get(koff);
    i.local_get(e).local_get(koff).i32_add();
    load_key(&mut i, key);
    i.call(fns.ord.delete).i32_store(w(ROOT));
    i.local_get(v).local_get(v).i32_load(w(DEAD)).i32_const(1).i32_add().i32_store(w(DEAD));
    i.i32_const(1).return_();
    i.end();
    i.local_get(e).local_get(block).i32_sub().i32_const(almide_layout::PAYLOAD as i32).i32_sub();
    i.local_get(esz).i32_div_u().i32_const(1).i32_add().local_set(want);
    i.local_get(v).i32_load(w(almide_layout::PAYLOAD + 4)).local_set(mask);
    i.local_get(e).local_get(koff).i32_add();
    load_key(&mut i, key);
    i.call(fns.hash).local_get(mask).i32_and().local_set(h);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(v).local_get(h).i32_const(2).i32_shl().i32_add().i32_load(w(SLOTS)).local_get(want).i32_eq().br_if(1);
    i.local_get(h).i32_const(1).i32_add().local_get(mask).i32_and().local_set(h);
    i.br(0).end().end();
    i.local_get(v).local_get(h).i32_const(2).i32_shl().i32_add().i32_const(TOMB).i32_store(w(SLOTS));
    i.local_get(v).local_get(v).i32_load(w(DEAD)).i32_const(1).i32_add().i32_store(w(DEAD));
    i.i32_const(1);
    i.end();
    f
}

/// Is the entry at slot value `ord` live? Leaves an i32 bool. A String
/// hole's key slot was zeroed by its delete; an Int hole keeps its key, so
/// the index decides: the key's probe reaches `ord` only for a live entry
/// (a hole's slot is a tombstone, and a re-inserted key sits at a later
/// ordinal).
///
/// Under an ordered index (#4104) the tree holds live entries only, so an
/// Int entry is live when its key finds exactly this entry.
fn emit_live(
    i: &mut wasm_encoder::InstructionSink<'_>,
    key: IdxKey,
    (hash, ord_find): (u32, u32),
    ix: (u32, u32),
    (block, esz, p, koff): (u32, u32, u32, u32),
    (ord, h, s): (u32, u32, u32),
) {
    let (v, mask) = ix;
    if key == IdxKey::Str {
        i.local_get(p).local_get(koff).i32_add().i32_load(w(0)).i32_const(0).i32_ne();
        return;
    }
    i.local_get(mask).i32_const(ORDERED).i32_eq().if_(BlockType::Result(ValType::I32));
    i.local_get(v).local_get(block).local_get(esz).local_get(koff);
    i.local_get(p).local_get(koff).i32_add();
    load_key(i, key);
    i.call(ord_find).local_get(p).i32_eq();
    i.else_();
    emit_live_hashed(i, key, hash, ix, (p, koff), (ord, h, s));
    i.end();
}

/// The hash-index half of `emit_live` (Int keys).
fn emit_live_hashed(i: &mut wasm_encoder::InstructionSink<'_>, key: IdxKey, hash: u32, ix: (u32, u32), at: (u32, u32), (ord, h, s): (u32, u32, u32)) {
    let ((v, mask), (p, koff)) = (ix, at);
    i.local_get(p).local_get(koff).i32_add();
    load_key(i, key);
    i.call(hash).local_get(mask).i32_and().local_set(h);
    i.block(BlockType::Result(ValType::I32)).loop_(BlockType::Empty);
    i.local_get(v).local_get(h).i32_const(2).i32_shl().i32_add().i32_load(w(SLOTS)).local_tee(s);
    i.i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).br(2);
    i.end();
    i.i32_const(1).local_get(s).local_get(ord).i32_eq().br_if(1).drop();
    i.local_get(h).i32_const(1).i32_add().local_get(mask).i32_and().local_set(h);
    i.br(0).end().unreachable().end();
}

/// `$mapidx_settle_<key>(block, esz, koff, force) -> block`: compact a
/// block with holes IN PLACE — the live entries move down in order, `len`
/// shrinks, the index is freed and the record reset to "seen once" — when
/// `force` (a read that must not see holes) or when the holes outnumber
/// the live entries (the delete's amortization point). The holes' handle
/// slots were released by their deletes, so nothing is released here.
pub(crate) fn emit_settle(key: IdxKey, fns: IdxFns) -> Function {
    // params: 0=block, 1=esz, 2=koff, 3=force; locals: 4=v, 5=p, 6=end,
    // 7=wr, 8=ord, 9=mask, 10=h, 11=s
    let (block, esz, koff, force) = (0u32, 1u32, 2u32, 3u32);
    let (v, p, end, wr, ord, mask, h, s) = (4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32);
    let mut f = Function::new([(8, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).call(fns.side_get).local_tee(v).i32_const(1).i32_le_u().if_(BlockType::Empty);
    i.local_get(block).return_();
    i.end();
    // no holes, or (unforced) no more holes than live entries
    i.local_get(v).i32_load(w(DEAD)).i32_eqz();
    i.local_get(force).i32_eqz();
    i.local_get(v).i32_load(w(DEAD)).i32_const(1).i32_shl();
    i.local_get(block).i32_load(len_memarg()).local_get(esz).i32_div_u().i32_le_u();
    i.i32_and().i32_or().if_(BlockType::Empty);
    i.local_get(block).return_();
    i.end();
    i.local_get(v).i32_load(w(almide_layout::PAYLOAD + 4)).local_set(mask);
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_tee(p).local_tee(wr);
    i.local_get(block).i32_load(len_memarg()).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(ord).i32_const(1).i32_add().local_set(ord);
    emit_live(&mut i, key, (fns.hash, fns.ord.find), (v, mask), (block, esz, p, koff), (ord, h, s));
    i.if_(BlockType::Empty);
    i.local_get(wr).local_get(p).i32_ne().if_(BlockType::Empty);
    i.local_get(wr).local_get(p).local_get(esz).memory_copy(0, 0);
    i.end();
    i.local_get(wr).local_get(esz).i32_add().local_set(wr);
    i.end();
    i.local_get(p).local_get(esz).i32_add().local_set(p);
    i.br(0).end().end();
    i.local_get(block).local_get(wr).local_get(block).i32_sub().i32_const(almide_layout::PAYLOAD as i32).i32_sub();
    i.i32_store(len_memarg());
    i.local_get(v).call(F_FREE);
    i.local_get(block).i32_const(1).call(fns.side_set).drop();
    i.local_get(block);
    i.end();
    f
}

/// `$mapidx_dead(block) -> holes`: the hole count of a block's index (0
/// with none) — `map.len` of a tombstone var subtracts it.
pub(crate) fn emit_dead(side_get: u32) -> Function {
    let (block, v) = (0u32, 1u32);
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).call(side_get).local_tee(v).i32_const(1).i32_gt_u().if_(BlockType::Result(ValType::I32));
    i.local_get(v).i32_load(w(DEAD));
    i.else_();
    i.i32_const(0);
    i.end();
    i.end();
    f
}
