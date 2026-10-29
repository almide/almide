//! `value.__set_path_steps` on the structural leg (#4315): the walk
//! `value.set_path` runs, in place where the document is this op's alone.
//!
//! The self-host body (stdlib/value.almd `__set_path_pure`) rebuilt every
//! Object it passed: `as_object`, a scan for the key, `list.map`, then
//! `value.object`, whose duplicate-key fold walks the pairs again. A fold of
//! n `set_path` calls over one document was O(n²) (8,000 keys: 4 s, four
//! times the time per doubling) where native, after #4297, is linear.
//!
//! Here the op holds ONE credit on the document — a dying var's (the
//! `@reuse(v)` declaration on `__set_path_steps` lets dying_move.rs hand it
//! over), an owned temporary's, or a share of a live var's — and judges by
//! count: a box and payload list it holds alone (both at rc 1, neither a
//! pool static) are written in place; anything shared is copied one level
//! (the box and its payload spine; the elements take a credit each) and the
//! op's credit on the shared original goes. Value semantics hold by count:
//! any other holder keeps the block at rc > 1, so it never sees a write.
//!
//! The walk, per step, as the pure body does it:
//! * key step on an Object: the key's pair (found through the index below)
//!   takes the rest of the walk in its value slot, its position kept; a
//!   missing key is appended with the walk over `{}` (C-401: insertion
//!   order, one entry per key). A pair tuple another list shares is copied
//!   before its value slot is written.
//! * key step on anything else: `{name: walk({})}`, the old value dropped.
//! * index step on an Array, in range: that element takes the rest of the
//!   walk. Out of range, or not an Array: the value is returned as it is.
//! * no step left: `x`.
//!
//! # The Object key index
//!
//! An Object box is `[rc][len][cap][tag @SUM_TAG][pad][pairs @SUM_FIELD]`;
//! the pad word (`SUM_TAG + 4`) was unused. It now holds the box's key
//! index, or 0: every Object box is born with 0 there (value.rs writes the
//! tag and the pad as one i64, no more bytes than the tag), the Value drop glue
//! releases it with the box (rc_ownership.rs `shape_slots`), and a box copy
//! shares it with a credit (the same glue's inc). The index is a flat block
//! `[hdr][pairs][count][mask][slot × (mask+1)]` — open addressing over the
//! keys' FNV-1a hash (map_index.rs `$mapidx_hash_str`), slot = pair ordinal
//! + 1. It answers only when it agrees with the box: the pairs address and
//! count it was built over are the box's own and nobody else holds it;
//! otherwise it is rebuilt from the pairs (below 16 pairs the scan is
//! used, the map index's threshold). Only this op writes an Object's pairs,
//! and only while it holds the box and the pairs alone, so an index that
//! agrees is exact. An append keeps it current (the new ordinal inserted)
//! until the load would pass 1/2; then it is dropped and the next lookup
//! rebuilds it at double size: amortized O(1) per insert.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::arm::ArgMode;
use crate::emitter::Emitter;
use crate::value::{VT_ARRAY, VT_OBJECT};
use crate::work::Helper;
use crate::*;

/// The err `json.parse` gives past its nesting bound (#4209, C-433), which
/// a value `set_path` would build past it gives too.
const DEEP_ERR: &str = "json: nesting deeper than 1000";
/// The nesting bound itself (crates/almide-base/src/json_limits.rs).
const MAX_DEPTH: i32 = 1000;
/// Pairs (in bytes of 4-byte slots) below which a lookup scans.
const SCAN_BYTES: i32 = 16 * 4;
/// An index's first slot count.
const MIN_SLOTS: i32 = 32;

/// The box's payload-relative words.
const PAY: u32 = almide_layout::SUM_FIELD;
const IDX: u32 = crate::value::OBJECT_INDEX;
const TAG: u32 = almide_layout::SUM_TAG;
/// The index block's payload-relative words.
const I_PAIRS: u32 = 0;
const I_COUNT: u32 = 4;
const I_MASK: u32 = 8;
const I_SLOTS: u32 = 12;

fn word(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 2, memory_index: 0 }
}

fn rc() -> MemArg {
    word(almide_layout::RC.offset)
}

/// The helper indices and the two tuple layouts the walk reads.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct VsetFns {
    pub(crate) empty: u32,
    pub(crate) uniq_obj: u32,
    pub(crate) uniq_arr: u32,
    pub(crate) find: u32,
    pub(crate) push: u32,
    pub(crate) dec_value: u32,
    pub(crate) dec_pair: u32,
    /// The `(String, Value)` pair tuple: key / value offsets and its size.
    pub(crate) key_off: u32,
    pub(crate) val_off: u32,
    pub(crate) pair_size: u32,
    /// The `(Bool, String, Int)` step tuple's offsets.
    pub(crate) is_key_off: u32,
    pub(crate) name_off: u32,
    pub(crate) index_off: u32,
}

/// Leaves `1` when the block in `b` is a heap block at rc 1.
fn unique(i: &mut wasm_encoder::InstructionSink<'_>, b: u32) {
    i.local_get(b).global_get(G_LINE_END).i32_ge_u();
    i.local_get(b).i32_load(rc()).i32_const(1).i32_eq();
    i.i32_and();
}

/// A fresh Object box over the pairs list in `pairs`, left on the stack.
fn new_object(i: &mut wasm_encoder::InstructionSink<'_>, pairs: u32, scratch: u32) {
    i.i32_const(16).call(F_ALLOC).local_tee(scratch);
    crate::value::store_object_tag(i);
    i.local_get(scratch).local_get(pairs).i32_store(slot_memarg(PAY));
    i.local_get(scratch);
}

/// `$vset_empty() -> box`: `{}`.
fn emit_empty() -> Function {
    let (p, b) = (0u32, 1u32);
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const(0).call(F_ALLOC).local_set(p);
    new_object(&mut i, p, b);
    i.end();
    f
}

/// `$vset_unique(v) -> v'`: the box (tag `tag`) and its payload list held
/// by this credit alone — `v` itself, or a one-level copy (each element
/// takes a credit) with the credit on `v` released.
fn emit_unique(dec_value: u32, tag: i32) -> Function {
    let (v, p, q, n, j, b) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).i32_load(slot_memarg(PAY)).local_set(p);
    unique(&mut i, v);
    unique(&mut i, p);
    i.i32_and().if_(BlockType::Empty);
    i.local_get(v).return_();
    i.end();
    i.local_get(p).i32_load(len_memarg()).local_tee(n).call(F_ALLOC).local_set(q);
    i.local_get(q).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(p).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(n).call(F_COPY);
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(n).i32_ge_u().br_if(1);
    i.local_get(q).local_get(j).i32_add().i32_load(slot_memarg(0)).call(F_INC);
    i.local_get(j).i32_const(4).i32_add().local_set(j);
    i.br(0).end().end();
    // The tag with a zero pad word: an Object's no-index word, unused
    // under an Array.
    i.i32_const(16).call(F_ALLOC).local_tee(b);
    i.i64_const(i64::from(tag)).i64_store(slot_memarg(TAG));
    i.local_get(b).local_get(q).i32_store(slot_memarg(PAY));
    i.local_get(v).call(dec_value);
    i.local_get(b);
    i.end();
    f
}

/// The key String of the pair at byte offset `j` of the pairs list `p`.
fn pair_key(i: &mut wasm_encoder::InstructionSink<'_>, p: u32, j: u32, key_off: u32) {
    i.local_get(p).local_get(j).i32_add().i32_load(slot_memarg(0));
    i.i32_load(slot_memarg(key_off));
}

/// Probe the index `ix` from slot `h` (masked by `mask`) to an empty slot;
/// leaves its address in `q`.
fn probe_empty(i: &mut wasm_encoder::InstructionSink<'_>, ix: u32, mask: u32, h: u32, q: u32) {
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(ix).local_get(h).i32_const(2).i32_shl().i32_add().local_tee(q);
    i.i32_load(slot_memarg(I_SLOTS)).i32_eqz().br_if(1);
    i.local_get(h).i32_const(1).i32_add().local_get(mask).i32_and().local_set(h);
    i.br(0).end().end();
}

/// `$vset_find(v, name) -> i32`: the byte offset of `name`'s pair in the
/// pairs of the Object `v` (held alone), or -1. A scan below 16 pairs;
/// else the box's index, rebuilt when it does not agree with the box.
fn emit_find(hash: u32, key_off: u32) -> Function {
    let (v, name) = (0u32, 1u32);
    let (p, n, j, ix, mask, h, q, cap) = (2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32);
    let mut f = Function::new([(8, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).i32_load(slot_memarg(PAY)).local_tee(p).i32_load(len_memarg()).local_set(n);
    i.local_get(n).i32_const(SCAN_BYTES).i32_lt_u().if_(BlockType::Empty);
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(n).i32_ge_u().br_if(1);
    pair_key(&mut i, p, j, key_off);
    i.local_get(name).call(F_STR_EQ).if_(BlockType::Empty);
    i.local_get(j).return_();
    i.end();
    i.local_get(j).i32_const(4).i32_add().local_set(j);
    i.br(0).end().end();
    i.i32_const(-1).return_();
    i.end();
    // The index agrees with the box: built over these pairs, at this count,
    // and held by this box alone.
    i.local_get(v).i32_load(slot_memarg(IDX)).local_tee(ix).i32_eqz();
    i.local_get(ix).i32_load(slot_memarg(I_PAIRS)).local_get(p).i32_ne().i32_or();
    i.local_get(ix).i32_load(slot_memarg(I_COUNT)).local_get(n).i32_const(2).i32_shr_u().i32_ne().i32_or();
    i.local_get(ix).i32_load(rc()).i32_const(1).i32_ne().i32_or();
    i.if_(BlockType::Empty);
    // Rebuild at a load of at most 1/4: cap = the power of two >= 4·count.
    i.local_get(ix).call(F_DEC_FLAT);
    i.i32_const(MIN_SLOTS).local_set(cap);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cap).local_get(n).i32_ge_u().br_if(1); // n bytes = 4·count
    i.local_get(cap).i32_const(1).i32_shl().local_set(cap);
    i.br(0).end().end();
    i.local_get(cap).i32_const(2).i32_shl().i32_const(I_SLOTS as i32).i32_add().call(F_ALLOC).local_set(ix);
    i.local_get(ix).i32_const((almide_layout::PAYLOAD + I_SLOTS) as i32).i32_add();
    i.i32_const(0).local_get(cap).i32_const(2).i32_shl().memory_fill(0);
    i.local_get(ix).local_get(p).i32_store(slot_memarg(I_PAIRS));
    i.local_get(ix).local_get(n).i32_const(2).i32_shr_u().i32_store(slot_memarg(I_COUNT));
    i.local_get(ix).local_get(cap).i32_const(1).i32_sub().local_tee(mask).i32_store(slot_memarg(I_MASK));
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(n).i32_ge_u().br_if(1);
    pair_key(&mut i, p, j, key_off);
    i.call(hash).local_get(mask).i32_and().local_set(h);
    probe_empty(&mut i, ix, mask, h, q);
    i.local_get(q).local_get(j).i32_const(2).i32_shr_u().i32_const(1).i32_add().i32_store(slot_memarg(I_SLOTS));
    i.local_get(j).i32_const(4).i32_add().local_set(j);
    i.br(0).end().end();
    i.local_get(v).local_get(ix).i32_store(slot_memarg(IDX));
    i.end();
    i.local_get(ix).i32_load(slot_memarg(I_MASK)).local_set(mask);
    i.local_get(name).call(hash).local_get(mask).i32_and().local_set(h);
    i.loop_(BlockType::Empty);
    i.local_get(ix).local_get(h).i32_const(2).i32_shl().i32_add().i32_load(slot_memarg(I_SLOTS)).local_tee(q).i32_eqz().if_(BlockType::Empty);
    i.i32_const(-1).return_();
    i.end();
    i.local_get(q).i32_const(1).i32_sub().i32_const(2).i32_shl().local_set(j);
    pair_key(&mut i, p, j, key_off);
    i.local_get(name).call(F_STR_EQ).if_(BlockType::Empty);
    i.local_get(j).return_();
    i.end();
    i.local_get(h).i32_const(1).i32_add().local_get(mask).i32_and().local_set(h);
    i.br(0);
    i.end();
    i.i32_const(-1);
    i.end();
    f
}

/// `$vset_push(v, pair) -> 0`: append the pair (its credit moves in) to
/// the Object `v` held alone with its pairs; keep the index current, or
/// drop it once it would pass load 1/2 or does not agree with the box.
fn emit_push(hash: u32, key_off: u32) -> Function {
    let (v, t) = (0u32, 1u32);
    let (p, q, n, ix, mask, h, s) = (2u32, 3u32, 4u32, 5u32, 6u32, 7u32, 8u32);
    let mut f = Function::new([(7, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).i32_load(slot_memarg(PAY)).local_tee(p).i32_load(len_memarg()).local_set(n);
    // In place when the spine has room; else the doubled spine, the
    // outgrown one freed (it is at rc 1).
    i.local_get(p).local_get(t).call(F_LIST_PUSH_4).local_set(q);
    i.local_get(v).local_get(q).i32_store(slot_memarg(PAY));
    i.local_get(v).i32_load(slot_memarg(IDX)).local_tee(ix).if_(BlockType::Empty);
    i.local_get(ix).i32_load(slot_memarg(I_MASK)).local_set(mask);
    i.local_get(ix).i32_load(rc()).i32_const(1).i32_eq();
    i.local_get(ix).i32_load(slot_memarg(I_PAIRS)).local_get(p).i32_eq().i32_and();
    i.local_get(ix).i32_load(slot_memarg(I_COUNT)).local_get(n).i32_const(2).i32_shr_u().i32_eq().i32_and();
    // (count + 1) · 2 <= slots
    i.local_get(n).i32_const(2).i32_shr_u().i32_const(1).i32_add().i32_const(1).i32_shl();
    i.local_get(mask).i32_const(1).i32_add().i32_le_u().i32_and();
    i.if_(BlockType::Empty);
    i.local_get(t).i32_load(slot_memarg(key_off)).call(hash).local_get(mask).i32_and().local_set(h);
    probe_empty(&mut i, ix, mask, h, s);
    i.local_get(s).local_get(n).i32_const(2).i32_shr_u().i32_const(1).i32_add().i32_store(slot_memarg(I_SLOTS));
    i.local_get(ix).local_get(n).i32_const(2).i32_shr_u().i32_const(1).i32_add().i32_store(slot_memarg(I_COUNT));
    i.local_get(ix).local_get(q).i32_store(slot_memarg(I_PAIRS));
    i.else_();
    i.local_get(ix).call(F_DEC_FLAT);
    i.local_get(v).i32_const(0).i32_store(slot_memarg(IDX));
    i.end();
    i.end();
    i.i32_const(0);
    i.end();
    f
}

/// `$vset_nests(x, budget) -> i32`: x holds Arrays / Objects nested more
/// than `budget` deep (stdlib/value.almd `__nests_past`, the same walk).
fn emit_nests(self_idx: u32, val_off: u32) -> Function {
    let (x, budget) = (0u32, 1u32);
    let (tag, p, n, j, e) = (2u32, 3u32, 4u32, 5u32, 6u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(x).i32_load(slot_memarg(TAG)).local_tee(tag).i32_const(VT_ARRAY).i32_eq();
    i.local_get(tag).i32_const(VT_OBJECT).i32_eq().i32_or().i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.local_get(budget).i32_eqz().if_(BlockType::Empty);
    i.i32_const(1).return_();
    i.end();
    i.local_get(x).i32_load(slot_memarg(PAY)).local_tee(p).i32_load(len_memarg()).local_set(n);
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(n).i32_ge_u().br_if(1);
    i.local_get(p).local_get(j).i32_add().i32_load(slot_memarg(0)).local_set(e);
    i.local_get(tag).i32_const(VT_OBJECT).i32_eq().if_(BlockType::Empty);
    i.local_get(e).i32_load(slot_memarg(val_off)).local_set(e);
    i.end();
    i.local_get(e).local_get(budget).i32_const(1).i32_sub().call(self_idx).if_(BlockType::Empty);
    i.i32_const(1).return_();
    i.end();
    i.local_get(j).i32_const(4).i32_add().local_set(j);
    i.br(0).end().end();
    i.i32_const(0);
    i.end();
    f
}

/// A fresh `(name, child)` pair tuple (`name` takes a credit, `child`'s
/// moves in), left in `t`.
fn new_pair(i: &mut wasm_encoder::InstructionSink<'_>, fns: &VsetFns, name: u32, child: u32, t: u32) {
    i.i32_const(fns.pair_size as i32).call(F_ALLOC).local_tee(t);
    i.local_get(name).i32_store(slot_memarg(fns.key_off));
    i.local_get(t).local_get(child).i32_store(slot_memarg(fns.val_off));
    i.local_get(name).call(F_INC);
}

/// `$vset_steps(v, steps, k, x) -> v'`: the walk from step `k`. `v` and `x`
/// are owned (one credit each, both spent), `steps` borrowed.
fn emit_steps(self_idx: u32, fns: VsetFns) -> Function {
    let (v, steps, k, x) = (0u32, 1u32, 2u32, 3u32);
    let (t, tag, name, p, j, pair, child, b) = (4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32);
    let idx = 12u32;
    let mut f = Function::new([(8, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();
    let recurse = |i: &mut wasm_encoder::InstructionSink<'_>| {
        i.local_get(steps).local_get(k).i32_const(1).i32_add().local_get(x).call(self_idx);
    };
    // No step left: x.
    i.local_get(k).i32_const(2).i32_shl().local_get(steps).i32_load(len_memarg()).i32_ge_u().if_(BlockType::Empty);
    i.local_get(v).call(fns.dec_value);
    i.local_get(x).return_();
    i.end();
    i.local_get(steps).local_get(k).i32_const(2).i32_shl().i32_add().i32_load(slot_memarg(0)).local_set(t);
    i.local_get(v).i32_load(slot_memarg(TAG)).local_set(tag);
    i.local_get(t).i32_load(slot_memarg(fns.is_key_off)).if_(BlockType::Result(ValType::I32));
    // ── a key step ──
    i.local_get(t).i32_load(slot_memarg(fns.name_off)).local_set(name);
    i.local_get(tag).i32_const(VT_OBJECT).i32_ne().if_(BlockType::Result(ValType::I32));
    // Not an Object: {name: walk({})}.
    i.local_get(v).call(fns.dec_value);
    i.call(fns.empty);
    recurse(&mut i);
    i.local_set(child);
    new_pair(&mut i, &fns, name, child, pair);
    i.i32_const(4).call(F_ALLOC).local_tee(p).local_get(pair).i32_store(slot_memarg(0));
    new_object(&mut i, p, b);
    i.else_();
    i.local_get(v).call(fns.uniq_obj).local_tee(v).local_get(name).call(fns.find).local_tee(j);
    i.i32_const(0).i32_ge_s().if_(BlockType::Empty);
    // Present: the pair's value slot takes the rest of the walk.
    i.local_get(v).i32_load(slot_memarg(PAY)).local_tee(p).local_get(j).i32_add().i32_load(slot_memarg(0)).local_set(pair);
    unique(&mut i, pair);
    i.i32_eqz().if_(BlockType::Empty);
    // Another list shares the pair: this Object takes its own copy.
    i.i32_const(fns.pair_size as i32).call(F_ALLOC).local_tee(b);
    i.local_get(pair).i32_load(slot_memarg(fns.key_off)).i32_store(slot_memarg(fns.key_off));
    i.local_get(b).local_get(pair).i32_load(slot_memarg(fns.val_off)).i32_store(slot_memarg(fns.val_off));
    i.local_get(b).i32_load(slot_memarg(fns.key_off)).call(F_INC);
    i.local_get(b).i32_load(slot_memarg(fns.val_off)).call(F_INC);
    i.local_get(pair).call(fns.dec_pair);
    i.local_get(p).local_get(j).i32_add().local_get(b).i32_store(slot_memarg(0));
    i.local_get(b).local_set(pair);
    i.end();
    i.local_get(pair).i32_load(slot_memarg(fns.val_off)).local_set(child);
    i.local_get(pair).i32_const(0).i32_store(slot_memarg(fns.val_off));
    i.local_get(pair);
    i.local_get(child);
    recurse(&mut i);
    i.i32_store(slot_memarg(fns.val_off));
    i.else_();
    // Missing: appended, the walk over {}.
    i.call(fns.empty);
    recurse(&mut i);
    i.local_set(child);
    new_pair(&mut i, &fns, name, child, pair);
    i.local_get(v).local_get(pair).call(fns.push).drop();
    i.end();
    i.local_get(v);
    i.end();
    i.else_();
    // ── an index step ──
    i.local_get(t).i64_load(slot_memarg(fns.index_off)).local_set(idx);
    // Missed unless an Array (its payload is a list only then) has idx.
    i.local_get(tag).i32_const(VT_ARRAY).i32_ne().if_(BlockType::Result(ValType::I32));
    i.i32_const(1);
    i.else_();
    i.local_get(idx).i64_const(0).i64_lt_s();
    i.local_get(idx);
    i.local_get(v).i32_load(slot_memarg(PAY)).i32_load(len_memarg()).i32_const(2).i32_shr_u().i64_extend_i32_u();
    i.i64_ge_s().i32_or();
    i.end();
    i.if_(BlockType::Result(ValType::I32));
    // Missed: the value as it is.
    i.local_get(x).call(fns.dec_value);
    i.local_get(v);
    i.else_();
    i.local_get(v).call(fns.uniq_arr).local_tee(v).i32_load(slot_memarg(PAY));
    i.local_get(idx).i32_wrap_i64().i32_const(2).i32_shl().i32_add().local_set(p);
    i.local_get(p).i32_load(slot_memarg(0)).local_set(child);
    i.local_get(p).i32_const(0).i32_store(slot_memarg(0));
    i.local_get(p);
    i.local_get(child);
    recurse(&mut i);
    i.i32_store(slot_memarg(0));
    i.local_get(v);
    i.end();
    i.end();
    i.end();
    f
}

/// The wasm params of a set-path helper (None = not one).
pub(crate) fn helper_params(h: &Helper) -> Option<Vec<ValType>> {
    let i = ValType::I32;
    Some(match h {
        Helper::VsetEmpty => vec![],
        Helper::VsetUnique { .. } => vec![i],
        Helper::VsetFind { .. } | Helper::VsetPush { .. } | Helper::VsetNests { .. } => vec![i, i],
        Helper::VsetSteps { .. } => vec![i, i, i, i],
        _ => return None,
    })
}

/// The set-path helper bodies; `self_idx` is the helper's own index.
pub(crate) fn helper_body(h: &Helper, self_idx: u32) -> Option<Function> {
    Some(match h {
        Helper::VsetEmpty => emit_empty(),
        Helper::VsetUnique { dec_value, tag } => emit_unique(*dec_value, *tag),
        Helper::VsetFind { hash, key_off } => emit_find(*hash, *key_off),
        Helper::VsetPush { hash, key_off } => emit_push(*hash, *key_off),
        Helper::VsetNests { val_off } => emit_nests(self_idx, *val_off),
        Helper::VsetSteps { fns } => emit_steps(self_idx, *fns),
        _ => return None,
    })
}

impl Emitter<'_> {
    /// `value.__set_path_steps(v, steps, x)`: `ok(v')`, or json.parse's
    /// nesting err (#4209) when the result would nest past 1000 — the path's
    /// steps plus x's own nesting — with v and x dropped.
    pub(crate) fn lower_value_set_path_steps(
        &mut self,
        v: &IrExpr,
        steps: &IrExpr,
        x: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        let pair_ti = self.types.tuple(vec![STR, SliceTy::Value]);
        let pair = self.types.tuple_def(pair_ti);
        let dec_value = self.dec_fn_of(SliceTy::Value);
        let dec_pair = self.dec_fn_of(SliceTy::Tuple(pair_ti));
        let hash = self.work.helper(Helper::MapIdxHash { key: crate::map_index::IdxKey::Str });
        let (key_off, val_off) = (pair.fields[0].1, pair.fields[1].1);
        // The source: a block this op holds a credit on — taken from a dying
        // var or an owned temporary, else a share of the live value. The
        // helpers write only what that credit holds alone.
        if self.owned_reuse_source(v, &[steps, x], Some(SliceTy::Value))?.is_none() {
            self.lower_arg(v, Some(SliceTy::Value), ArgMode::Retain)?;
        }
        let hv = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        let step_ti = match self.lower_arg(steps, None, ArgMode::Borrow)? {
            SliceTy::List(h) => match self.types.el(h) {
                SliceTy::Tuple(ti) => ti,
                other => return Err(EmitError::Unsupported(format!("value.set_path-step:{other:?}"))),
            },
            other => return Err(EmitError::Unsupported(format!("value.set_path-steps:{other:?}"))),
        };
        let step = self.types.tuple_def(step_ti);
        if step.fields.len() != 3 || step.fields[0].0 != BOOL || step.fields[1].0 != STR || step.fields[2].0 != INT {
            return Err(EmitError::Unsupported("value.set_path-step-shape".into()));
        }
        let hs = self.hold_i32()?;
        self.f.instructions().local_set(hs);
        self.lower_arg(x, Some(SliceTy::Value), ArgMode::Retain)?;
        let hx = self.hold_i32()?;
        self.f.instructions().local_set(hx);
        let fns = VsetFns {
            empty: self.work.helper(Helper::VsetEmpty),
            uniq_obj: self.work.helper(Helper::VsetUnique { dec_value, tag: VT_OBJECT }),
            uniq_arr: self.work.helper(Helper::VsetUnique { dec_value, tag: VT_ARRAY }),
            find: self.work.helper(Helper::VsetFind { hash, key_off }),
            push: self.work.helper(Helper::VsetPush { hash, key_off }),
            dec_value,
            dec_pair,
            key_off,
            val_off,
            pair_size: pair.size,
            is_key_off: step.fields[0].1,
            name_off: step.fields[1].1,
            index_off: step.fields[2].1,
        };
        let walk = self.work.helper(Helper::VsetSteps { fns });
        let nests = self.work.helper(Helper::VsetNests { val_off });
        let msg = self.pool.intern(DEEP_ERR);
        let hn = self.hold_i32()?;
        let hr = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.i32_const(16).call(F_ALLOC).local_set(hr);
        i.local_get(hs).i32_load(len_memarg()).i32_const(2).i32_shr_u().local_tee(hn);
        i.i32_const(MAX_DEPTH).i32_gt_u().if_(BlockType::Result(ValType::I32));
        i.i32_const(1);
        i.else_();
        i.local_get(hx).i32_const(MAX_DEPTH).local_get(hn).i32_sub().call(nests);
        i.end();
        i.if_(BlockType::Empty);
        i.local_get(hr).i32_const(1).i32_store(slot_memarg(TAG));
        i.local_get(hr).i32_const(msg as i32).i32_store(slot_memarg(PAY));
        i.local_get(hv).call(dec_value);
        i.local_get(hx).call(dec_value);
        i.else_();
        i.local_get(hr).i32_const(0).i32_store(slot_memarg(TAG));
        i.local_get(hr);
        i.local_get(hv).local_get(hs).i32_const(0).local_get(hx).call(walk);
        i.i32_store(slot_memarg(PAY));
        i.end();
        i.local_get(hr);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(SliceTy::Result(self.types.intern(SliceTy::Value), self.types.intern(STR)))
    }
}
