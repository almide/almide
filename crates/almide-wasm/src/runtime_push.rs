//! The two push helpers (`$list_push_{8,4}`, `$bytes_push`), split from
//! runtime.rs for the file budget when they gained their RC signatures
//! (#4278 B1, helper_sig.rs).

const _UNCLASSIFIED_RC_SITE: u32 = almide_layout::RC.offset;
use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::helper_sig::{HelperSig, Mode};
use crate::*;

/// `$list_push_{8,4}(list: i32, v) -> i32`: append with AMORTIZED growth.
/// In-place when `cap - len >= stride` (sound because every List bind/
/// assign deep-copies — a local's block is uniquely its own, and the
/// checker only lets `push` target mut vars); otherwise a fresh block
/// with doubled capacity. Returns the (possibly new) base for write-back.
pub(crate) fn emit_list_push(s: Scalar) -> (Function, HelperSig) {
    // params: 0=list i32, 1=v; locals: 2=la i32, 3=cap i32, 4=base i32,
    // 5=k i32 (the shared-block element walk)
    let (list, v, la, cap, base, k) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
    let stride = s.slot_size();
    // #4278 B1: the list is consumed (returned in place, or released once
    // grown); a block-handle value is consumed (stored into its slot), an
    // 8-byte scalar holds no reference.
    let mut sig = HelperSig::new(&[Mode::C, if stride == 4 { Mode::C } else { Mode::B }]);
    let fresh = sig.object(None);
    let (lo, vo) = (list as usize, v as usize);
    let payload = almide_layout::PAYLOAD as i32;
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(if stride == 4 { 4 } else { 3 }, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(list).i32_load(len_memarg()).local_set(la);
    i.local_get(list).i32_load(word(almide_layout::CAP.offset)).local_set(cap);
    // fast path: room in cap → store at len, bump len, return same block
    i.local_get(cap).local_get(la).i32_sub().i32_const(stride as i32).i32_ge_u().if_(BlockType::Empty);
    sig.branch();
    i.local_get(list).local_get(la).i32_add();
    i.local_get(v);
    match s {
        Scalar::Int => i.i64_store(slot_memarg(0)),
        _ => i.i32_store(slot_memarg(0)),
    };
    if stride == 4 {
        sig.op(vo, 'm');
    }
    i.local_get(list).local_get(la).i32_const(stride as i32).i32_add().i32_store(word(almide_layout::LEN.offset));
    i.local_get(list).return_();
    sig.op(lo, 'm');
    sig.exit();
    i.end();
    sig.arm();
    sig.join();
    // grow: newcap = max(cap * 2, 4 * stride), the doubling saturated
    // (#4396: a cap at 2^31 wrapped to 0 and the old len was copied into
    // the 4-slot block)
    crate::runtime_alloc::emit_double_cap_sat(&mut i, cap);
    i.local_get(cap).i32_const((4 * stride) as i32).i32_lt_u().if_(BlockType::Empty);
    i.i32_const((4 * stride) as i32).local_set(cap);
    i.end();
    i.local_get(cap).call(F_ALLOC).local_set(base); // len = cap = newcap for now
    sig.op(fresh, 'i');
    i.local_get(base).i32_const(payload).i32_add();
    i.local_get(list).i32_const(payload).i32_add();
    i.local_get(la);
    i.call(F_COPY);
    i.local_get(base).local_get(la).i32_add();
    i.local_get(v);
    match s {
        Scalar::Int => i.i64_store(slot_memarg(0)),
        _ => i.i32_store(slot_memarg(0)),
    };
    if stride == 4 {
        sig.op(vo, 'm');
    }
    // live len = old len + stride (cap field keeps newcap from $alloc)
    i.local_get(base).local_get(la).i32_const(stride as i32).i32_add().i32_store(word(almide_layout::LEN.offset));
    // RC-3: the caller hands its reference to `list` in and takes `base`
    // back, so the grow path RELEASES `list` on every path (#4278's
    // every-path rule) — `$dec_flat` frees it when uniquely owned (the
    // growth CHAIN was the churn gate's whole leak), only drops the
    // caller's share when the block is shared, and ignores a static.
    // The old `rc == 1 → $free` judge kept the caller's share of a shared
    // block forever (#4304). A shared block of 4-byte slots also hands its
    // element handles to `base`, so those copies are new references
    // (`$inc` ignores Bool slots and statics, both below the heap floor);
    // a unique block's elements MOVE and need none.
    if stride == 4 {
        // Each copied element handle is a new reference the grown block
        // holds: a share of the element (`a`) moved into its slot (`m`),
        // any number of times (the walk runs only for a shared block).
        // A temporary's event: the walk's one `$inc` (the slot address is a
        // sum, which names no single holder).
        let elem = sig.object(None);
        sig.loop_open();
        i.local_get(list).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_ne().if_(BlockType::Empty);
        i.i32_const(0).local_set(k);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(k).local_get(la).i32_ge_u().br_if(1);
        i.local_get(base).local_get(k).i32_add().i32_load(slot_memarg(0)).call(F_INC);
        sig.rc(elem, 'a');
        sig.op(elem, 'm');
        i.local_get(k).i32_const(4).i32_add().local_set(k);
        i.br(0);
        i.end().end();
        i.end();
        sig.loop_close();
    }
    i.local_get(list).call(F_DEC_FLAT);
    sig.rc(lo, 'd');
    i.local_get(base);
    sig.op(fresh, 'm');
    i.end();
    (f, sig)
}

/// `$bytes_push(b: i32, v: i64) -> i32`: the byte-stride twin of
/// `$list_push` (#1689) — in-place when `cap - len >= 1`, else a fresh
/// block at `max(cap*2, 16)` with the outgrown block released.
/// The old inline `emit_copy_grow(b, 1)` protocol reallocated len+1 on
/// EVERY push and never freed the outgrown block, so n pushes retained
/// n²/2 bytes and 69k pushes OOM'd where native completes.
pub(crate) fn emit_bytes_push() -> (Function, HelperSig) {
    // params: 0=b i32, 1=v i64; locals: 2=la i32, 3=cap i32, 4=base i32
    let (b, v, la, cap, base) = (0u32, 1u32, 2u32, 3u32, 4u32);
    // #4278 B1: the bytes block is consumed (returned in place, or released
    // once grown); the byte holds no reference.
    let mut sig = HelperSig::new(&[Mode::C, Mode::B]);
    let fresh = sig.object(None);
    let payload = almide_layout::PAYLOAD as i32;
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let byte = MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 };
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(b).i32_load(len_memarg()).local_set(la);
    i.local_get(b).i32_load(word(almide_layout::CAP.offset)).local_set(cap);
    // fast path: room in cap → store at len, bump len, return same block
    i.local_get(cap).local_get(la).i32_sub().i32_const(1).i32_ge_u().if_(BlockType::Empty);
    sig.branch();
    i.local_get(b).local_get(la).i32_add();
    i.local_get(v);
    i.i64_store8(byte);
    i.local_get(b).local_get(la).i32_const(1).i32_add().i32_store(word(almide_layout::LEN.offset));
    i.local_get(b).return_();
    sig.op(b as usize, 'm');
    sig.exit();
    i.end();
    sig.arm();
    sig.join();
    // grow: newcap = max(cap * 2, 16), the doubling saturated (#4396)
    crate::runtime_alloc::emit_double_cap_sat(&mut i, cap);
    i.local_get(cap).i32_const(16).i32_lt_u().if_(BlockType::Empty);
    i.i32_const(16).local_set(cap);
    i.end();
    i.local_get(cap).call(F_ALLOC).local_set(base);
    sig.op(fresh, 'i');
    i.local_get(base).i32_const(payload).i32_add();
    i.local_get(b).i32_const(payload).i32_add();
    i.local_get(la);
    i.call(F_COPY);
    i.local_get(base).local_get(la).i32_add();
    i.local_get(v);
    i.i64_store8(byte);
    // live len = old len + 1 (cap field keeps newcap from $alloc)
    i.local_get(base).local_get(la).i32_const(1).i32_add().i32_store(word(almide_layout::LEN.offset));
    // RC-3: release the outgrown block on every path, as $list_push and
    // $str_append do — freed when uniquely owned, the caller's share
    // dropped when shared, a static left alone (#4304).
    i.local_get(b).call(F_DEC_FLAT);
    sig.rc(b as usize, 'd');
    i.local_get(base);
    sig.op(fresh, 'm');
    i.end();
    (f, sig)
}
