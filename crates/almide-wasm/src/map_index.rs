//! The keyed-lookup index (#1219 stage 2, the structural leg's twin of
//! native's fingerprint sidecar, a712f2cba): `map.get` / `contains` /
//! `set` / `insert` and every set membership probe on an Int- or
//! String-keyed block past 16 entries resolve through a hash index
//! instead of the `$scan_*` linear walk. The entry array stays the ONLY
//! authority for order, equality and repr (C-013 / C-014 / C-016 never
//! consult the index); the index is a private runtime structure that
//! caches entry ORDINALS, so no observable moves — the win is the
//! complexity class of the everyday build-then-read loop (the per-insert
//! miss scan and the read loop were both O(n) per op, ×3.9 per doubling
//! after stage 1 removed the copy term).
//!
//! # Layout
//!
//! Map and Set blocks keep their layout (`[rc][len][cap][entries…]`),
//! because the Set↔List cast (`set.to_list` shares the block) and every
//! reader's `PAYLOAD .. PAYLOAD+len` walk rule out an in-block trailer.
//! The index lives BESIDE the block, reached through a side table keyed by
//! block ADDRESS:
//!
//!   side table (global `G_MAPIDX`, 0 until the first indexed lookup):
//!     `[hdr][mask][count][(block, val) × (mask+1)]` — open addressing,
//!     linear probing on a Fibonacci hash of the address; grows at load
//!     1/2 (the outgrown table is freed). `val` is 0 = no record (a
//!     tombstone), 1 = SEEN ONCE, else the index block's address.
//!   index block (one per indexed map/set):
//!     `[hdr][stamp][mask][slot × (mask+1)]` — slot = entry ordinal + 1,
//!     0 = empty; capacity is the power of two ≥ 2·entries (min 32);
//!     `stamp` is the block's `len` when the index last agreed with it.
//!
//! # The three invariants that make an address a sound key
//!
//! 1. A block is indexed on its SECOND lookup, never its first. The
//!    functional builders (`s = set.insert(s, x)`, a shared `map.set`
//!    rebind, `from_list`'s copy-grow) yield a fresh address per step
//!    that is probed exactly once, so they stay on the linear scan and
//!    never pay for an index they would throw away.
//! 2. A stale index can never answer. Map and Set blocks leave the
//!    droppable set (never `$dec`'d, never `$cow`'d), so the ONLY path
//!    that frees one is `$map_reserve`'s relocation inside the in-place
//!    `map.set` window — and that window calls `$mapidx_append`, which
//!    tombstones the outgrown address before the allocator can hand it
//!    to a new block, and carries the index to the new address. On top
//!    of that, `stamp != len` rebuilds (the belt over the braces).
//! 3. Contents change under an index only through the window: an
//!    overwrite keeps every key (index valid), an append inserts the new
//!    ordinal into the index or, when the load would pass 1/2, drops the
//!    index (`val = 1`) so the next lookup rebuilds it at double size —
//!    amortized O(1) per insert, geometric like the block itself.
//!
//! # The key-class matrix (`IdxKey`)
//!
//! | key class                 | lookup            | why                                    |
//! |---------------------------|-------------------|----------------------------------------|
//! | Int                       | indexed           | Fibonacci hash of the i64              |
//! | String                    | indexed           | FNV-1a over the bytes, `$str_eq` confirm |
//! | Bool                      | linear `$scan_w32`| at most 2 entries — never past threshold |
//! | Float                     | linear `$scan_f64`| native keeps floats linear too (NaN/-0.0) |
//! | Tuple / Named (deep keys) | linear `$scan_deep`| native fingerprints (i64,i64)/(String,String); the deep lane here stays linear — a cost gap only, recorded in #1219 |
//!
//! `index_key` IS the matrix; `keyed_find` routes through it, and the
//! unit test below pins each row so the family cannot drift point-wise.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::emitter::Emitter;
use crate::work::Helper;
use crate::*;

/// Entry count from which a block is indexed (native's
/// `ALMIDE_MAP_INDEX_THRESHOLD`): below it the linear scan wins.
pub(crate) const INDEX_THRESHOLD: u32 = 16;

/// The indexable key classes — the rows of the matrix above that carry
/// a hash lane.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IdxKey {
    Int,
    Str,
}

/// The matrix: which key class takes the index lane.
pub(crate) fn index_key(k: SliceTy) -> Option<IdxKey> {
    match k {
        INT => Some(IdxKey::Int),
        STR => Some(IdxKey::Str),
        _ => None,
    }
}

impl IdxKey {
    fn needle(self) -> ValType {
        match self {
            IdxKey::Int => ValType::I64,
            IdxKey::Str => ValType::I32,
        }
    }
    fn scan(self) -> u32 {
        match self {
            IdxKey::Int => F_SCAN_W64,
            IdxKey::Str => F_SCAN_STR,
        }
    }
}

fn w(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 2, memory_index: 0 }
}

/// Side-table pair `i` sits at `table + PAIRS + i*8`; index slot `h` at
/// `index + SLOTS + h*4`. Both structures spend their first two payload
/// words on `[mask|stamp][count|mask]`.
const PAIRS: u32 = almide_layout::PAYLOAD + 8;
const SLOTS: u32 = almide_layout::PAYLOAD + 8;
const HDR_WORDS: i32 = 8;
const FIRST_SIDE_CAP: i32 = 64;

/// `h = fib(addr >> 2) ^ (h >> 15)` — the side table's address hash;
/// leaves the masked slot on the stack. Expects `[addr, mask]` locals.
fn addr_hash(i: &mut wasm_encoder::InstructionSink<'_>, addr: u32, mask: u32, h: u32) {
    i.local_get(addr)
        .i32_const(2)
        .i32_shr_u()
        .i32_const(0x9E37_79B1_u32 as i32)
        .i32_mul()
        .local_tee(h)
        .i32_const(15)
        .i32_shr_u()
        .local_get(h)
        .i32_xor()
        .local_get(mask)
        .i32_and()
        .local_set(h);
}

/// `$mapidx_side_get(block) -> val`: the side record for a block address
/// (0 = none).
pub(crate) fn emit_side_get() -> Function {
    // params: 0=block; locals: 1=s, 2=mask, 3=h, 4=p, 5=k
    let (block, s, mask, h, p, k) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.global_get(G_MAPIDX).local_tee(s).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.local_get(s).i32_load(w(almide_layout::PAYLOAD)).local_set(mask);
    addr_hash(&mut i, block, mask, h);
    i.loop_(BlockType::Empty);
    i.local_get(s).local_get(h).i32_const(3).i32_shl().i32_add().local_tee(p);
    i.i32_load(w(PAIRS)).local_tee(k).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.local_get(k).local_get(block).i32_eq().if_(BlockType::Empty);
    i.local_get(p).i32_load(w(PAIRS + 4)).return_();
    i.end();
    i.local_get(h).i32_const(1).i32_add().local_get(mask).i32_and().local_set(h);
    i.br(0);
    i.end();
    i.i32_const(0);
    i.end();
    f
}

/// `$mapidx_side_raw(table, block, val) -> i32`: write one record into a
/// table (overwrite in place, or claim an empty pair and bump the count).
/// No growth — `$mapidx_side_set` owns that.
pub(crate) fn emit_side_raw() -> Function {
    // params: 0=s, 1=block, 2=val; locals: 3=mask, 4=h, 5=p, 6=k
    let (s, block, val, mask, h, p, k) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32);
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(s).i32_load(w(almide_layout::PAYLOAD)).local_set(mask);
    addr_hash(&mut i, block, mask, h);
    i.loop_(BlockType::Empty);
    i.local_get(s).local_get(h).i32_const(3).i32_shl().i32_add().local_tee(p);
    i.i32_load(w(PAIRS)).local_tee(k).local_get(block).i32_eq().if_(BlockType::Empty);
    i.local_get(p).local_get(val).i32_store(w(PAIRS + 4));
    i.i32_const(0).return_();
    i.end();
    i.local_get(k).i32_eqz().if_(BlockType::Empty);
    i.local_get(p).local_get(block).i32_store(w(PAIRS));
    i.local_get(p).local_get(val).i32_store(w(PAIRS + 4));
    i.local_get(s);
    i.local_get(s).i32_load(w(almide_layout::PAYLOAD + 4)).i32_const(1).i32_add();
    i.i32_store(w(almide_layout::PAYLOAD + 4));
    i.i32_const(1).return_();
    i.end();
    i.local_get(h).i32_const(1).i32_add().local_get(mask).i32_and().local_set(h);
    i.br(0);
    i.end();
    i.i32_const(0);
    i.end();
    f
}

/// `$mapidx_side_set(block, val) -> 0`: record `val` for a block address,
/// creating the table on first use and doubling it past load 1/2 (live
/// records re-hashed, tombstones dropped, the old table freed).
pub(crate) fn emit_side_set(raw: u32) -> Function {
    // params: 0=block, 1=val; locals: 2=s, 3=cap, 4=n, 5=p, 6=end
    let (block, val, s, cap, n, p, end) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.global_get(G_MAPIDX).local_tee(s).i32_eqz().if_(BlockType::Empty);
    i.i32_const(HDR_WORDS + FIRST_SIDE_CAP * 8).call(F_ALLOC).local_set(s);
    i.local_get(s).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.i32_const(0).i32_const(HDR_WORDS + FIRST_SIDE_CAP * 8).memory_fill(0);
    i.local_get(s).i32_const(FIRST_SIDE_CAP - 1).i32_store(w(almide_layout::PAYLOAD));
    i.local_get(s).global_set(G_MAPIDX);
    i.end();
    i.local_get(s).local_get(block).local_get(val).call(raw).drop();
    // grow past load 1/2
    i.local_get(s).i32_load(w(almide_layout::PAYLOAD)).i32_const(1).i32_add().local_set(cap);
    i.local_get(s).i32_load(w(almide_layout::PAYLOAD + 4)).i32_const(1).i32_shl();
    i.local_get(cap).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(HDR_WORDS).local_get(cap).i32_const(4).i32_shl().i32_add().call(F_ALLOC).local_set(n);
    i.local_get(n).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.i32_const(0).i32_const(HDR_WORDS).local_get(cap).i32_const(4).i32_shl().i32_add().memory_fill(0);
    i.local_get(n).local_get(cap).i32_const(1).i32_shl().i32_const(1).i32_sub();
    i.i32_store(w(almide_layout::PAYLOAD));
    i.local_get(s).i32_const(PAIRS as i32).i32_add().local_tee(p);
    i.local_get(cap).i32_const(3).i32_shl().i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p).i32_load(w(0)).if_(BlockType::Empty);
    i.local_get(p).i32_load(w(4)).if_(BlockType::Empty);
    i.local_get(n).local_get(p).i32_load(w(0)).local_get(p).i32_load(w(4)).call(raw).drop();
    i.end();
    i.end();
    i.local_get(p).i32_const(8).i32_add().local_set(p);
    i.br(0).end().end();
    i.local_get(n).global_set(G_MAPIDX);
    i.local_get(s).call(F_FREE);
    i.end();
    i.i32_const(0);
    i.end();
    f
}

/// `$mapidx_hash_int(i64) -> i32`: the high word of the Fibonacci product.
pub(crate) fn emit_hash_int() -> Function {
    let mut f = Function::new([]);
    f.instructions()
        .local_get(0)
        .i64_const(0x9E37_79B9_7F4A_7C15_u64 as i64)
        .i64_mul()
        .i64_const(32)
        .i64_shr_u()
        .i32_wrap_i64()
        .end();
    f
}

/// `$mapidx_hash_str(block) -> i32`: FNV-1a over the payload bytes,
/// folded once so the low bits carry the whole word.
pub(crate) fn emit_hash_str() -> Function {
    // params: 0=s; locals: 1=h, 2=p, 3=end
    let (s, h, p, end) = (0u32, 1u32, 2u32, 3u32);
    let byte = MemArg { offset: 0, align: 0, memory_index: 0 };
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const(0x811C_9DC5_u32 as i32).local_set(h);
    i.local_get(s).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_tee(p);
    i.local_get(s).i32_load(len_memarg()).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(h).local_get(p).i32_load8_u(byte).i32_xor().i32_const(0x0100_0193).i32_mul().local_set(h);
    i.local_get(p).i32_const(1).i32_add().local_set(p);
    i.br(0).end().end();
    i.local_get(h).local_get(h).i32_const(15).i32_shr_u().i32_xor();
    i.end();
    f
}

/// Load the key slot at the absolute address on the stack.
fn load_key(i: &mut wasm_encoder::InstructionSink<'_>, key: IdxKey) {
    match key {
        IdxKey::Int => i.i64_load(w(0)),
        IdxKey::Str => i.i32_load(w(0)),
    };
}

/// Probe `index` for an EMPTY slot starting at `h` (masked); leaves the
/// slot's absolute address in `q`.
fn probe_empty(i: &mut wasm_encoder::InstructionSink<'_>, index: u32, mask: u32, h: u32, q: u32) {
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(index).local_get(h).i32_const(2).i32_shl().i32_add().local_tee(q);
    i.i32_load(w(SLOTS)).i32_eqz().br_if(1);
    i.local_get(h).i32_const(1).i32_add().local_get(mask).i32_and().local_set(h);
    i.br(0).end().end();
}

/// `$mapidx_build_<key>(block, esz, koff) -> index`: a fresh index over
/// every entry of the block (capacity = pow2 ≥ 2·entries, min 32).
pub(crate) fn emit_build(key: IdxKey, hash: u32) -> Function {
    // params: 0=block, 1=esz, 2=koff; locals: 3=len, 4=cap, 5=idx, 6=p,
    // 7=end, 8=ord, 9=h, 10=q, 11=mask
    let (block, esz, koff) = (0u32, 1u32, 2u32);
    let (len, cap, idx, p, end, ord, h, q, mask) = (3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32);
    let mut f = Function::new([(9, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).i32_load(len_memarg()).local_set(len);
    i.i32_const(32).local_set(cap);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cap).local_get(len).local_get(esz).i32_div_u().i32_const(1).i32_shl().i32_ge_u().br_if(1);
    i.local_get(cap).i32_const(1).i32_shl().local_set(cap);
    i.br(0).end().end();
    i.i32_const(HDR_WORDS).local_get(cap).i32_const(2).i32_shl().i32_add().call(F_ALLOC).local_set(idx);
    i.local_get(idx).i32_const(SLOTS as i32).i32_add().i32_const(0).local_get(cap).i32_const(2).i32_shl().memory_fill(0);
    i.local_get(idx).local_get(len).i32_store(w(almide_layout::PAYLOAD));
    i.local_get(cap).i32_const(1).i32_sub().local_set(mask);
    i.local_get(idx).local_get(mask).i32_store(w(almide_layout::PAYLOAD + 4));
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_tee(p);
    i.local_get(len).i32_add().local_set(end);
    i.i32_const(0).local_set(ord);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p).local_get(koff).i32_add();
    load_key(&mut i, key);
    i.call(hash).local_get(mask).i32_and().local_set(h);
    probe_empty(&mut i, idx, mask, h, q);
    i.local_get(q).local_get(ord).i32_const(1).i32_add().i32_store(w(SLOTS));
    i.local_get(ord).i32_const(1).i32_add().local_set(ord);
    i.local_get(p).local_get(esz).i32_add().local_set(p);
    i.br(0).end().end();
    i.local_get(idx);
    i.end();
    f
}

/// The dependency indices a find/append body calls into.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct IdxFns {
    pub(crate) side_get: u32,
    pub(crate) side_set: u32,
    pub(crate) hash: u32,
}

/// `$mapidx_find_<key>(block, esz, koff, needle) -> entry | 0`: the
/// `$scan_*` signature, so every scan call site can take it verbatim.
/// Under the threshold, or on a block's FIRST lookup, it IS the scan.
pub(crate) fn emit_find(key: IdxKey, fns: IdxFns, build: u32) -> Function {
    // params: 0=block, 1=esz, 2=koff, 3=needle; locals: 4=len, 5=v,
    // 6=mask, 7=h, 8=s, 9=e
    let (block, esz, koff, needle) = (0u32, 1u32, 2u32, 3u32);
    let (len, v, mask, h, s, e) = (4u32, 5u32, 6u32, 7u32, 8u32, 9u32);
    let mut f = Function::new([(6, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).i32_load(len_memarg()).local_tee(len);
    i.local_get(esz).i32_const(INDEX_THRESHOLD as i32).i32_mul().i32_lt_u().if_(BlockType::Empty);
    i.local_get(block).local_get(esz).local_get(koff).local_get(needle).call(key.scan()).return_();
    i.end();
    i.local_get(block).call(fns.side_get).local_tee(v).i32_eqz().if_(BlockType::Empty);
    // first sight: remember it, scan this once
    i.local_get(block).i32_const(1).call(fns.side_set).drop();
    i.local_get(block).local_get(esz).local_get(koff).local_get(needle).call(key.scan()).return_();
    i.end();
    // seen before: build (or rebuild past a stale stamp)
    i.local_get(v).i32_const(1).i32_eq();
    i.local_get(v).i32_load(w(almide_layout::PAYLOAD)).local_get(len).i32_ne();
    i.i32_or().if_(BlockType::Empty);
    i.local_get(v).i32_const(1).i32_ne().if_(BlockType::Empty);
    i.local_get(v).call(F_FREE);
    i.end();
    i.local_get(block).local_get(esz).local_get(koff).call(build).local_set(v);
    i.local_get(block).local_get(v).call(fns.side_set).drop();
    i.end();
    i.local_get(v).i32_load(w(almide_layout::PAYLOAD + 4)).local_set(mask);
    i.local_get(needle).call(fns.hash).local_get(mask).i32_and().local_set(h);
    i.loop_(BlockType::Empty);
    i.local_get(v).local_get(h).i32_const(2).i32_shl().i32_add().i32_load(w(SLOTS)).local_tee(s);
    i.i32_eqz().if_(BlockType::Empty);
    i.i32_const(almide_layout::NULL_ADDR as i32).return_();
    i.end();
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(s).i32_const(1).i32_sub().local_get(esz).i32_mul().i32_add().local_tee(e);
    i.local_get(koff).i32_add();
    load_key(&mut i, key);
    i.local_get(needle);
    match key {
        IdxKey::Int => i.i64_eq(),
        IdxKey::Str => i.call(F_STR_EQ),
    };
    i.if_(BlockType::Empty);
    i.local_get(e).return_();
    i.end();
    i.local_get(h).i32_const(1).i32_add().local_get(mask).i32_and().local_set(h);
    i.br(0);
    i.end();
    i.i32_const(almide_layout::NULL_ADDR as i32);
    i.end();
    f
}

/// `$mapidx_append_<key>(old, new, esz, koff) -> 0`: the in-place window
/// appended one entry (len already bumped) — and may have relocated the
/// block from `old` to `new`. Retire the old address, then either insert
/// the new ordinal or, past load 1/2 (or a stale stamp), drop the index
/// so the next lookup rebuilds it at the doubled size.
pub(crate) fn emit_append(key: IdxKey, fns: IdxFns) -> Function {
    // params: 0=old, 1=new, 2=esz, 3=koff; locals: 4=v, 5=len, 6=mask,
    // 7=h, 8=q
    let (old, new, esz, koff) = (0u32, 1u32, 2u32, 3u32);
    let (v, len, mask, h, q) = (4u32, 5u32, 6u32, 7u32, 8u32);
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(old).call(fns.side_get).local_tee(v).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.local_get(new).local_get(old).i32_ne().if_(BlockType::Empty);
    i.local_get(old).i32_const(0).call(fns.side_set).drop();
    i.end();
    i.local_get(v).i32_const(1).i32_eq().if_(BlockType::Empty);
    i.local_get(new).local_get(old).i32_ne().if_(BlockType::Empty);
    i.local_get(new).i32_const(1).call(fns.side_set).drop();
    i.end();
    i.i32_const(0).return_();
    i.end();
    i.local_get(new).i32_load(len_memarg()).local_set(len);
    i.local_get(v).i32_load(w(almide_layout::PAYLOAD + 4)).local_set(mask);
    // stale stamp, or the load would pass 1/2: drop and rebuild lazily
    i.local_get(v).i32_load(w(almide_layout::PAYLOAD)).local_get(len).local_get(esz).i32_sub().i32_ne();
    i.local_get(len).local_get(esz).i32_div_u().i32_const(1).i32_shl().local_get(mask).i32_const(1).i32_add().i32_gt_u();
    i.i32_or().if_(BlockType::Empty);
    i.local_get(v).call(F_FREE);
    i.local_get(new).i32_const(1).call(fns.side_set).drop();
    i.i32_const(0).return_();
    i.end();
    // insert ordinal len/esz - 1 (slot value = ordinal + 1 = len/esz)
    i.local_get(new).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(len).i32_add().local_get(esz).i32_sub().local_get(koff).i32_add();
    load_key(&mut i, key);
    i.call(fns.hash).local_get(mask).i32_and().local_set(h);
    probe_empty(&mut i, v, mask, h, q);
    i.local_get(q).local_get(len).local_get(esz).i32_div_u().i32_store(w(SLOTS));
    i.local_get(v).local_get(len).i32_store(w(almide_layout::PAYLOAD));
    i.local_get(new).local_get(old).i32_ne().if_(BlockType::Empty);
    i.local_get(new).local_get(v).call(fns.side_set).drop();
    i.end();
    i.i32_const(0);
    i.end();
    f
}

/// The helper signatures assembly promises for the index family.
pub(crate) fn helper_params(h: &Helper) -> Option<Vec<ValType>> {
    Some(match h {
        Helper::MapIdxSideGet => vec![ValType::I32],
        Helper::MapIdxSideRaw => vec![ValType::I32, ValType::I32, ValType::I32],
        Helper::MapIdxSideSet { .. } => vec![ValType::I32, ValType::I32],
        Helper::MapIdxHash { key } => vec![key.needle()],
        Helper::MapIdxBuild { .. } => vec![ValType::I32, ValType::I32, ValType::I32],
        Helper::MapIdxFind { key, .. } => vec![ValType::I32, ValType::I32, ValType::I32, key.needle()],
        Helper::MapIdxAppend { .. } => vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        _ => return None,
    })
}

/// The helper bodies for the index family.
pub(crate) fn helper_body(h: &Helper) -> Option<Function> {
    Some(match h {
        Helper::MapIdxSideGet => emit_side_get(),
        Helper::MapIdxSideRaw => emit_side_raw(),
        Helper::MapIdxSideSet { raw } => emit_side_set(*raw),
        Helper::MapIdxHash { key } => match key {
            IdxKey::Int => emit_hash_int(),
            IdxKey::Str => emit_hash_str(),
        },
        Helper::MapIdxBuild { key, hash } => emit_build(*key, *hash),
        Helper::MapIdxFind { key, fns, build } => emit_find(*key, *fns, *build),
        Helper::MapIdxAppend { key, fns } => emit_append(*key, *fns),
        _ => return None,
    })
}

impl Emitter<'_> {
    fn idx_fns(&mut self, key: IdxKey) -> IdxFns {
        let raw = self.work.helper(Helper::MapIdxSideRaw);
        IdxFns {
            side_get: self.work.helper(Helper::MapIdxSideGet),
            side_set: self.work.helper(Helper::MapIdxSideSet { raw }),
            hash: self.work.helper(Helper::MapIdxHash { key }),
        }
    }

    /// The keyed lookup for a STABLE block (one that outlives the loop
    /// probing it): the index lane for the matrix's indexed classes, the
    /// plain `$scan_*` for the rest. Same signature as `scan_helper`.
    /// Blocks rebuilt per step (`from_list`'s copy-grow, the set-map
    /// accumulator) keep `scan_helper` — see invariant 1 in the module doc.
    pub(crate) fn keyed_find(&mut self, k: SliceTy) -> Result<u32, EmitError> {
        let Some(key) = index_key(k) else {
            return self.scan_helper(k);
        };
        let fns = self.idx_fns(key);
        let build = self.work.helper(Helper::MapIdxBuild { key, hash: fns.hash });
        Ok(self.work.helper(Helper::MapIdxFind { key, fns, build }))
    }

    /// The append hook for the in-place window (`None` for a key class
    /// with no index lane).
    pub(crate) fn keyed_append(&mut self, k: SliceTy) -> Option<u32> {
        let key = index_key(k)?;
        let fns = self.idx_fns(key);
        Some(self.work.helper(Helper::MapIdxAppend { key, fns }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key-class matrix, pinned row by row: extending the index to a
    /// class (or dropping one) is a deliberate edit HERE, never a drift.
    #[test]
    fn index_lane_matrix() {
        assert_eq!(index_key(INT), Some(IdxKey::Int));
        assert_eq!(index_key(STR), Some(IdxKey::Str));
        assert_eq!(index_key(BOOL), None, "Bool never reaches the threshold");
        assert_eq!(index_key(FLOAT), None, "Float keys stay linear (native too)");
        assert_eq!(index_key(SliceTy::Tuple(0)), None, "deep keys stay on the scan_deep lane");
        assert_eq!(index_key(SliceTy::Named(0)), None, "deep keys stay on the scan_deep lane");
    }
}
