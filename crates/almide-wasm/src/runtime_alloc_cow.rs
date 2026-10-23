//! `$map_reserve` and `$cow`: the growth and copy-on-write helpers of the
//! allocator family, split from runtime_alloc.rs for the file budget.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::*;

/// `$map_reserve(block, esz) -> block`: room for ONE more `esz`-byte
/// map entry — `$list_push`'s growth discipline with the store left to
/// the caller (the entry layout varies per key/value class, the growth
/// does not). Class slack covers the entry → the same block; else the
/// doubled block (min four entries) with the live entries copied and
/// the outgrown block freed iff uniquely held. `len` stays at the OLD
/// length: the caller writes the pair at `payload + len` and bumps it.
/// (#1219 stage 1 — the in-place `map.set` window.)
pub(crate) fn emit_map_reserve() -> Function {
    // params: 0=block i32, 1=esz i32; locals: 2=la i32, 3=cap i32, 4=base i32
    let (block, esz, la, cap, base) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let payload = almide_layout::PAYLOAD as i32;
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).i32_load(len_memarg()).local_set(la);
    i.local_get(block).i32_load(word(almide_layout::CAP.offset)).local_set(cap);
    // fast path: the class slack holds one more entry → same block
    i.local_get(cap).local_get(la).i32_sub().local_get(esz).i32_ge_u().if_(BlockType::Empty);
    i.local_get(block).return_();
    i.end();
    // grow: newcap = max(cap * 2, 4 * esz)
    i.local_get(cap).i32_const(1).i32_shl().local_set(cap);
    i.local_get(cap).local_get(esz).i32_const(2).i32_shl().i32_lt_u().if_(BlockType::Empty);
    i.local_get(esz).i32_const(2).i32_shl().local_set(cap);
    i.end();
    i.local_get(cap).call(F_ALLOC).local_set(base);
    i.local_get(base).i32_const(payload).i32_add();
    i.local_get(block).i32_const(payload).i32_add();
    i.local_get(la);
    i.call(F_COPY);
    // live len = the old len (the caller appends and bumps)
    i.local_get(base).local_get(la).i32_store(word(almide_layout::LEN.offset));
    // the outgrown block is garbage iff this map is uniquely held —
    // the window only reserves at rc == 1, so the chain never leaks.
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_eq().if_(BlockType::Empty);
    i.local_get(block).call(F_FREE);
    i.end();
    i.local_get(base);
    i.end();
    f
}

/// `$cow(block) -> block`: the copy-on-write judge at every in-place
/// mutation entry (RC-5). A uniquely-held block passes through; a
/// SHARED one (rc > 1 — binds now share instead of copying) is copied,
/// the original releases one ref, and the mutation proceeds on the
/// unique copy — value semantics moved from bind time to mutation
/// time, unobservably.
pub(crate) fn emit_cow() -> Function {
    // params: 0=block; locals: 1=copy
    let (block, copy) = (0u32, 1u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).global_get(G_LINE_END).i32_lt_u().if_(BlockType::Empty);
    i.local_get(block).return_();
    i.end();
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_le_u().if_(BlockType::Empty);
    i.local_get(block).return_();
    i.end();
    i.local_get(block).call(F_BLOCK_COPY).local_set(copy);
    i.local_get(block);
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_sub();
    i.i32_store(word(almide_layout::RC.offset));
    i.local_get(copy);
    i.end();
    f
}
