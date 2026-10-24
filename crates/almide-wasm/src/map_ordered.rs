//! The flood fallback of the keyed-lookup index (#4104): the wasm twin of
//! native's ordered `AlmideKeyLookup`. The hash index (map_index.rs) hashes
//! with fixed functions (Fibonacci for Int, FNV-1a for String), so anyone
//! who knows them can pick keys that all land on one probe chain and make
//! every insert walk it: building a map of n such keys costs n²/2 probes.
//!
//! An insert that walks past `FLOOD_PROBES` slots turns that block's index
//! into an AVL tree over the same entry ordinals, ordered by the key itself
//! (Int by value, String by `$str_cmp`'s byte order). The tree is O(log n)
//! per find / insert / delete whatever the keys are, and it is
//! deterministic: no seed, no entropy import, so the wasm imports still
//! match the program's effects (#4102) and the output is byte-identical
//! across runs. Blocks whose keys hash well never reach the bound and stay
//! on the hash index.
//!
//! # Layout
//!
//! An ordered index sits where a hash index would (the side record of the
//! block points at it) and keeps the hash index's first three header words,
//! so `stamp` / `dead` readers need no branch:
//!
//!   `[stamp][mask = ORDERED][dead][root][count][cap][node × (cap+1)]`
//!
//! node = `[slot][left][right][height]` (slot = entry ordinal + 1, children
//! are node numbers, 0 = none). Node 0 is a zeroed sentinel, so the height
//! of a missing child reads as 0 with no branch. Nodes are only appended; a
//! deleted node is unlinked and its storage stays until the block's holes
//! settle (map_holes.rs), which frees the whole index.
//!
//! The tree holds live entries only: a delete unlinks the node while the
//! entry's key is still in place (`$mapidx_kill` runs before the caller
//! releases the key), so every node's key is comparable.

use wasm_encoder::{BlockType, Function, ValType};

use crate::map_index::{load_key, w, IdxKey, SLOTS};
use crate::work::Helper;
use crate::*;

/// The `mask` word of an ordered index. A hash index's mask is its
/// capacity minus one (at least 31), never all ones.
pub(crate) const ORDERED: i32 = -1;
/// Probe steps one insert may take before its block leaves the hash index
/// (native's `ALMIDE_MAP_FLOOD_PROBES`). At load 1/2 a well-spread hash
/// averages under 2.5 steps; 128 is reached only by colliding keys.
pub(crate) const FLOOD_PROBES: i32 = 128;

const STAMP: u32 = almide_layout::PAYLOAD;
pub(crate) const MASK: u32 = almide_layout::PAYLOAD + 4;
const DEAD: u32 = almide_layout::PAYLOAD + 8;
pub(crate) const ROOT: u32 = almide_layout::PAYLOAD + 12;
const COUNT: u32 = almide_layout::PAYLOAD + 16;
const CAP: u32 = almide_layout::PAYLOAD + 20;
const HDR: i32 = 24;
const NODE: i32 = 16;
const NODES: u32 = almide_layout::PAYLOAD + 24;
const SLOT: u32 = NODES;
const LEFT: u32 = NODES + 4;
const RIGHT: u32 = NODES + 8;
const HEIGHT: u32 = NODES + 12;

/// The ordered-index helpers the hash family calls into.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct OrdFns {
    pub(crate) find: u32,
    pub(crate) add: u32,
    pub(crate) delete: u32,
    pub(crate) from_index: u32,
    pub(crate) build_all: u32,
}

type Sink<'a> = wasm_encoder::InstructionSink<'a>;

/// Push node `n`'s base address (field offsets are the memargs).
fn at(i: &mut Sink<'_>, t: u32, n: u32) {
    i.local_get(t).local_get(n).i32_const(4).i32_shl().i32_add();
}

/// Push the height of node `n`'s child in `field` (0 for none, through the
/// sentinel).
fn child_height(i: &mut Sink<'_>, t: u32, n: u32, field: u32) {
    at(i, t, n);
    i.i32_load(w(field)).i32_const(4).i32_shl().local_get(t).i32_add().i32_load(w(HEIGHT));
}

/// `n.height = 1 + max(height(n.left), height(n.right))`.
fn fix(i: &mut Sink<'_>, t: u32, n: u32, (a, b): (u32, u32)) {
    child_height(i, t, n, LEFT);
    i.local_set(a);
    child_height(i, t, n, RIGHT);
    i.local_set(b);
    at(i, t, n);
    i.local_get(a).local_get(b).local_get(a).local_get(b).i32_gt_u().select();
    i.i32_const(1).i32_add().i32_store(w(HEIGHT));
}

/// Push the absolute address of the key of entry `slot` (a slot value:
/// ordinal + 1) held in local `slot`.
fn key_addr_of_slot(i: &mut Sink<'_>, (block, esz, koff): (u32, u32, u32), slot: u32) {
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(slot).i32_const(1).i32_sub().local_get(esz).i32_mul().i32_add();
    i.local_get(koff).i32_add();
}

/// Push the entry address of node `n`.
fn entry_of_node(i: &mut Sink<'_>, t: u32, n: u32, (block, esz): (u32, u32)) {
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    at(i, t, n);
    i.i32_load(w(SLOT)).i32_const(1).i32_sub().local_get(esz).i32_mul().i32_add();
}

/// Push `sign(needle - key)` as -1 / 0 / 1, the key read from the address
/// `push_addr` leaves. `k` is an i64 scratch local (Int only).
fn cmp(i: &mut Sink<'_>, key: IdxKey, needle: u32, k: u32, push_addr: impl FnOnce(&mut Sink<'_>)) {
    match key {
        IdxKey::Int => {
            push_addr(i);
            i.i64_load(w(0)).local_set(k);
            i.local_get(needle).local_get(k).i64_gt_s();
            i.local_get(needle).local_get(k).i64_lt_s();
            i.i32_sub();
        }
        IdxKey::Str => {
            i.local_get(needle);
            push_addr(i);
            i.i32_load(w(0)).call(F_STR_CMP);
        }
    }
}

/// `$mapord_rot_{l,r}(t, n) -> node`: a single rotation.
fn emit_rot(left: bool) -> Function {
    let (t, n, c, a, b) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let (near, far) = if left { (RIGHT, LEFT) } else { (LEFT, RIGHT) };
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    at(&mut i, t, n);
    i.i32_load(w(near)).local_set(c);
    at(&mut i, t, n);
    at(&mut i, t, c);
    i.i32_load(w(far)).i32_store(w(near));
    at(&mut i, t, c);
    i.local_get(n).i32_store(w(far));
    fix(&mut i, t, n, (a, b));
    fix(&mut i, t, c, (a, b));
    i.local_get(c);
    i.end();
    f
}

/// `$mapord_balance(t, n) -> node`: `n`'s height refreshed and, when its
/// children differ by two, the AVL rotation that restores the bound.
fn emit_balance(rot_l: u32, rot_r: u32) -> Function {
    let (t, n, c, a, b, bf) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
    fix(&mut i, t, n, (a, b));
    child_height(&mut i, t, n, LEFT);
    child_height(&mut i, t, n, RIGHT);
    i.i32_sub().local_set(bf);
    for (heavy, light, inner, outer, sign) in [(LEFT, RIGHT, rot_l, rot_r, 1), (RIGHT, LEFT, rot_r, rot_l, -1)] {
        i.local_get(bf);
        i.i32_const(sign);
        if sign > 0 { i.i32_gt_s() } else { i.i32_lt_s() };
        i.if_(BlockType::Empty);
        at(&mut i, t, n);
        i.i32_load(w(heavy)).local_set(c);
        child_height(&mut i, t, c, heavy);
        child_height(&mut i, t, c, light);
        i.i32_lt_u().if_(BlockType::Empty);
        at(&mut i, t, n);
        i.local_get(t).local_get(c).call(inner).i32_store(w(heavy));
        i.end();
        i.local_get(t).local_get(n).call(outer).return_();
        i.end();
    }
    i.local_get(n);
    i.end();
    f
}

/// `$mapord_delmin(t, n) -> node`: the subtree without its leftmost node.
fn emit_delmin(me: u32, balance: u32) -> Function {
    let (t, n) = (0u32, 1u32);
    let mut f = Function::new([]);
    let mut i = f.instructions();
    at(&mut i, t, n);
    i.i32_load(w(LEFT)).i32_eqz().if_(BlockType::Empty);
    at(&mut i, t, n);
    i.i32_load(w(RIGHT)).return_();
    i.end();
    at(&mut i, t, n);
    i.local_get(t);
    at(&mut i, t, n);
    i.i32_load(w(LEFT)).call(me).i32_store(w(LEFT));
    i.local_get(t).local_get(n).call(balance);
    i.end();
    f
}

/// `$mapord_insert_<key>(t, n, block, esz, koff, slot, needle) -> node`:
/// a new node for `slot` in the subtree at `n` (capacity ensured by the
/// caller). Live keys are distinct, so equality never arises.
fn emit_insert(me: u32, key: IdxKey, balance: u32) -> Function {
    let (t, n, block, esz, koff, slot, needle) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32);
    let (c, k) = (7u32, 8u32);
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();
    i.local_get(n).i32_eqz().if_(BlockType::Empty);
    i.local_get(t).i32_load(w(COUNT)).i32_const(1).i32_add().local_set(c);
    i.local_get(t).local_get(c).i32_store(w(COUNT));
    for (field, val) in [(SLOT, None), (LEFT, Some(0)), (RIGHT, Some(0)), (HEIGHT, Some(1))] {
        at(&mut i, t, c);
        match val {
            None => i.local_get(slot),
            Some(v) => i.i32_const(v),
        };
        i.i32_store(w(field));
    }
    i.local_get(c).return_();
    i.end();
    cmp(&mut i, key, needle, k, |i| {
        entry_of_node(i, t, n, (block, esz));
        i.local_get(koff).i32_add();
    });
    i.i32_const(0).i32_lt_s().if_(BlockType::Empty);
    for field in [LEFT, RIGHT] {
        at(&mut i, t, n);
        i.local_get(t);
        at(&mut i, t, n);
        i.i32_load(w(field));
        i.local_get(block).local_get(esz).local_get(koff).local_get(slot).local_get(needle).call(me);
        i.i32_store(w(field));
        if field == LEFT {
            i.else_();
        }
    }
    i.end();
    i.local_get(t).local_get(n).call(balance);
    i.end();
    f
}

/// `$mapord_delete_<key>(t, n, block, esz, koff, needle) -> node`: the
/// subtree at `n` without the node whose key equals `needle`.
fn emit_delete(me: u32, key: IdxKey, (balance, delmin): (u32, u32)) -> Function {
    let (t, n, block, esz, koff, needle) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let (c, m, k) = (6u32, 7u32, 8u32);
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();
    i.local_get(n).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    cmp(&mut i, key, needle, k, |i| {
        entry_of_node(i, t, n, (block, esz));
        i.local_get(koff).i32_add();
    });
    i.local_set(c);
    for (field, sign) in [(LEFT, -1), (RIGHT, 1)] {
        i.local_get(c).i32_const(0);
        if sign < 0 { i.i32_lt_s() } else { i.i32_gt_s() };
        i.if_(BlockType::Empty);
        at(&mut i, t, n);
        i.local_get(t);
        at(&mut i, t, n);
        i.i32_load(w(field));
        i.local_get(block).local_get(esz).local_get(koff).local_get(needle).call(me);
        i.i32_store(w(field));
        i.local_get(t).local_get(n).call(balance).return_();
        i.end();
    }
    // found: splice it out
    for (gone, other) in [(LEFT, RIGHT), (RIGHT, LEFT)] {
        at(&mut i, t, n);
        i.i32_load(w(gone)).i32_eqz().if_(BlockType::Empty);
        at(&mut i, t, n);
        i.i32_load(w(other)).return_();
        i.end();
    }
    at(&mut i, t, n);
    i.i32_load(w(RIGHT)).local_set(m);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    at(&mut i, t, m);
    i.i32_load(w(LEFT)).i32_eqz().br_if(1);
    at(&mut i, t, m);
    i.i32_load(w(LEFT)).local_set(m);
    i.br(0).end().end();
    at(&mut i, t, m);
    i.local_get(t);
    at(&mut i, t, n);
    i.i32_load(w(RIGHT)).call(delmin).i32_store(w(RIGHT));
    at(&mut i, t, m);
    at(&mut i, t, n);
    i.i32_load(w(LEFT)).i32_store(w(LEFT));
    i.local_get(t).local_get(m).call(balance);
    i.end();
    f
}

/// `$mapord_find_<key>(t, block, esz, koff, needle) -> entry | 0`.
fn emit_find(key: IdxKey) -> Function {
    let (t, block, esz, koff, needle) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let (n, e, c, k) = (5u32, 6u32, 7u32, 8u32);
    let mut f = Function::new([(3, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();
    i.local_get(t).i32_load(w(ROOT)).local_set(n);
    i.loop_(BlockType::Empty);
    i.local_get(n).i32_eqz().if_(BlockType::Empty);
    i.i32_const(almide_layout::NULL_ADDR as i32).return_();
    i.end();
    entry_of_node(&mut i, t, n, (block, esz));
    i.local_set(e);
    cmp(&mut i, key, needle, k, |i| {
        i.local_get(e).local_get(koff).i32_add();
    });
    i.local_tee(c).i32_eqz().if_(BlockType::Empty);
    i.local_get(e).return_();
    i.end();
    at(&mut i, t, n);
    i.i32_load(w(LEFT));
    at(&mut i, t, n);
    i.i32_load(w(RIGHT));
    i.local_get(c).i32_const(0).i32_lt_s().select().local_set(n);
    i.br(0);
    i.end();
    i.i32_const(almide_layout::NULL_ADDR as i32);
    i.end();
    f
}

/// `$mapord_new(cap) -> t`: an empty ordered index with room for `cap`
/// nodes.
fn emit_new() -> Function {
    let (cap, t) = (0u32, 1u32);
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const(HDR).local_get(cap).i32_const(1).i32_add().i32_const(4).i32_shl().i32_add().call(F_ALLOC).local_set(t);
    i.local_get(t).i32_const(almide_layout::PAYLOAD as i32).i32_add().i32_const(0).i32_const(HDR + NODE).memory_fill(0);
    i.local_get(t).i32_const(ORDERED).i32_store(w(MASK));
    i.local_get(t).local_get(cap).i32_store(w(CAP));
    i.local_get(t);
    i.end();
    f
}

/// `$mapord_add_<key>(t, block, esz, koff, slot) -> t`: insert entry `slot`
/// (its key read from the block), doubling the node store first when it
/// is full. The result may be a new address; the caller records it.
fn emit_add(key: IdxKey, insert: u32) -> Function {
    let (t, block, esz, koff, slot) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let (cap, t2, needle) = (5u32, 6u32, 7u32);
    let mut f = Function::new([(2, ValType::I32), (1, match key {
        IdxKey::Int => ValType::I64,
        IdxKey::Str => ValType::I32,
    })]);
    let mut i = f.instructions();
    i.local_get(t).i32_load(w(COUNT)).local_get(t).i32_load(w(CAP)).i32_ge_u().if_(BlockType::Empty);
    i.local_get(t).i32_load(w(CAP)).local_set(cap);
    i.i32_const(HDR).local_get(cap).i32_const(1).i32_shl().i32_const(1).i32_add().i32_const(4).i32_shl().i32_add();
    i.call(F_ALLOC).local_set(t2);
    i.local_get(t2).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(t).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.i32_const(HDR).local_get(cap).i32_const(1).i32_add().i32_const(4).i32_shl().i32_add();
    i.memory_copy(0, 0);
    i.local_get(t2).local_get(cap).i32_const(1).i32_shl().i32_store(w(CAP));
    i.local_get(t).call(F_FREE);
    i.local_get(t2).local_set(t);
    i.end();
    key_addr_of_slot(&mut i, (block, esz, koff), slot);
    load_key(&mut i, key);
    i.local_set(needle);
    i.local_get(t);
    i.local_get(t).local_get(t).i32_load(w(ROOT));
    i.local_get(block).local_get(esz).local_get(koff).local_get(slot).local_get(needle).call(insert);
    i.i32_store(w(ROOT));
    i.local_get(t);
    i.end();
    f
}

/// `$mapord_from_index(block, v, esz, koff) -> t`: the ordered index over
/// the live slots of hash index `v` (its tombstones are holes and stay
/// out), `stamp` and `dead` carried over, `v` freed.
fn emit_from_index(add: u32, new: u32) -> Function {
    let (block, v, esz, koff) = (0u32, 1u32, 2u32, 3u32);
    let (t, p, end, s) = (4u32, 5u32, 6u32, 7u32);
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).i32_load(w(MASK)).i32_const(1).i32_add().i32_const(1).i32_shr_u().call(new).local_set(t);
    i.local_get(v).local_tee(p).local_get(v).i32_load(w(MASK)).i32_const(1).i32_add().i32_const(2).i32_shl().i32_add();
    i.local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    // a live slot: s ∉ {0, TOMB} ⇔ s + 1 >u 1
    i.local_get(p).i32_load(w(SLOTS)).local_tee(s).i32_const(1).i32_add().i32_const(1).i32_gt_u().if_(BlockType::Empty);
    i.local_get(t).local_get(block).local_get(esz).local_get(koff).local_get(s).call(add).local_set(t);
    i.end();
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0).end().end();
    i.local_get(t).local_get(v).i32_load(w(STAMP)).i32_store(w(STAMP));
    i.local_get(t).local_get(v).i32_load(w(DEAD)).i32_store(w(DEAD));
    i.local_get(v).call(F_FREE);
    i.local_get(t);
    i.end();
    f
}

/// `$mapord_build_<key>(block, esz, koff) -> t`: the ordered index over
/// every entry of a block with no index (a zeroed String key is a hole and
/// stays out).
fn emit_build_all(key: IdxKey, add: u32, new: u32) -> Function {
    let (block, esz, koff) = (0u32, 1u32, 2u32);
    let (t, len, ord, n) = (3u32, 4u32, 5u32, 6u32);
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).i32_load(len_memarg()).local_tee(len).local_get(esz).i32_div_u().local_set(n);
    i.local_get(n).i32_const(16).local_get(n).i32_const(16).i32_gt_u().select().call(new).local_set(t);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(ord).local_get(n).i32_ge_u().br_if(1);
    i.local_get(ord).i32_const(1).i32_add().local_set(ord);
    if key == IdxKey::Str {
        key_addr_of_slot(&mut i, (block, esz, koff), ord);
        i.i32_load(w(0)).if_(BlockType::Empty);
    }
    i.local_get(t).local_get(block).local_get(esz).local_get(koff).local_get(ord).call(add).local_set(t);
    if key == IdxKey::Str {
        i.end();
    }
    i.br(0).end().end();
    i.local_get(t).local_get(len).i32_store(w(STAMP));
    i.local_get(t);
    i.end();
    f
}

/// Register the ordered family for one key class.
pub(crate) fn ord_fns(work: &crate::work::FnWork, key: IdxKey) -> OrdFns {
    let rot_l = work.helper(Helper::MapOrdRot { left: true });
    let rot_r = work.helper(Helper::MapOrdRot { left: false });
    let balance = work.helper(Helper::MapOrdBalance { rot_l, rot_r });
    let delmin = work.helper(Helper::MapOrdDelMin { balance });
    let insert = work.helper(Helper::MapOrdInsert { key, balance });
    let new = work.helper(Helper::MapOrdNew);
    let add = work.helper(Helper::MapOrdAdd { key, insert });
    OrdFns {
        find: work.helper(Helper::MapOrdFind { key }),
        add,
        delete: work.helper(Helper::MapOrdDelete { key, balance, delmin }),
        from_index: work.helper(Helper::MapOrdFromIndex { add, new }),
        build_all: work.helper(Helper::MapOrdBuildAll { key, add, new }),
    }
}

/// The helper signatures of the ordered family (every one returns i32).
pub(crate) fn helper_params(h: &Helper) -> Option<Vec<ValType>> {
    let i = ValType::I32;
    Some(match h {
        Helper::MapOrdRot { .. } | Helper::MapOrdBalance { .. } | Helper::MapOrdDelMin { .. } => vec![i, i],
        Helper::MapOrdInsert { key, .. } => vec![i, i, i, i, i, i, needle(*key)],
        Helper::MapOrdDelete { key, .. } => vec![i, i, i, i, i, needle(*key)],
        Helper::MapOrdFind { key } => vec![i, i, i, i, needle(*key)],
        Helper::MapOrdNew => vec![i],
        Helper::MapOrdAdd { .. } => vec![i, i, i, i, i],
        Helper::MapOrdFromIndex { .. } => vec![i, i, i, i],
        Helper::MapOrdBuildAll { .. } => vec![i, i, i],
        _ => return None,
    })
}

fn needle(key: IdxKey) -> ValType {
    match key {
        IdxKey::Int => ValType::I64,
        IdxKey::Str => ValType::I32,
    }
}

/// The helper bodies of the ordered family; `me` is the helper's own
/// function index (the tree walks recurse).
pub(crate) fn helper_body(h: &Helper, me: u32) -> Option<Function> {
    Some(match h {
        Helper::MapOrdRot { left } => emit_rot(*left),
        Helper::MapOrdBalance { rot_l, rot_r } => emit_balance(*rot_l, *rot_r),
        Helper::MapOrdDelMin { balance } => emit_delmin(me, *balance),
        Helper::MapOrdInsert { key, balance } => emit_insert(me, *key, *balance),
        Helper::MapOrdDelete { key, balance, delmin } => emit_delete(me, *key, (*balance, *delmin)),
        Helper::MapOrdFind { key } => emit_find(*key),
        Helper::MapOrdNew => emit_new(),
        Helper::MapOrdAdd { key, insert } => emit_add(*key, *insert),
        Helper::MapOrdFromIndex { add, new } => emit_from_index(*add, *new),
        Helper::MapOrdBuildAll { key, add, new } => emit_build_all(*key, *add, *new),
        _ => return None,
    })
}
