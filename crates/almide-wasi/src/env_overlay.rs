//! The env.set overlay log (#1716): the guest-side copy of the environment
//! that `env.set` writes and `env.get` reads first. The host environment is a
//! read-only snapshot on every WASI world, as for wasi-libc's `setenv`: a set
//! is local to the instance and never reaches the host.
//!
//! The log is a buffer of `[klen u32][vlen u32][key][val]` entries holding at
//! most ONE entry per name: a set removes the name's old entry (the tail
//! slides down over it) before appending, so re-setting a name never grows
//! the log, and a scan's last match is its only match. The buffer starts as
//! a fixed page ([`OVERLAY_BYTES`]) and, when an entry does not fit, moves to
//! a fresh block of twice the capacity (or the need, if larger) bumped off
//! the guest heap — native has no cap, so neither does the log (C-329). The
//! abandoned buffer is never reused; the doubling bounds the waste by the
//! live size. Only a machine that cannot grow memory refuses, with C-197's
//! out-of-memory line.
//!
//! Both stock worlds emit the log through these functions: the p1 shim
//! (`wasi_shims.rs`, first at `park + OVL`) and the p3 component's env
//! service (#3223, first on its own page past the park). Only where the
//! first page lives, how a block is allocated and what an overlay miss falls
//! through to (p1's `environ_get`, p3's `get-environment` snapshot) differ.

use wasm_encoder::{BlockType, InstructionSink};

use crate::{mem, mem8};

/// The bytes the log's first (fixed) buffer holds: a page.
pub const OVERLAY_BYTES: u64 = 65536;

/// The log's three globals: where its buffer is, how many bytes the buffer
/// holds, and how many are in use. Base and capacity start at the fixed
/// page and move when the log outgrows it.
#[derive(Clone, Copy)]
pub struct OverlayLog {
    pub g_base: u32,
    pub g_cap: u32,
    pub g_len: u32,
}

/// The overlay scan's scratch locals (i32), allocated by the caller.
#[derive(Clone, Copy)]
pub struct ScanLocals {
    pub p: u32,
    pub endp: u32,
    pub klen: u32,
    pub vlen: u32,
    pub best: u32,
    pub j: u32,
}

/// Find the entry whose key equals `a` byte for byte: `best` = its address,
/// or 0 when the name is not in the log (a buffer never sits at address 0).
fn emit_find(i: &mut InstructionSink<'_>, log: OverlayLog, l: ScanLocals) {
    let (a_ptr, a_len) = (1u32, 2u32);
    let ScanLocals { p, endp, klen, vlen, best, j } = l;
    i.i32_const(0).local_set(best);
    i.global_get(log.g_base).local_set(p);
    i.global_get(log.g_base).global_get(log.g_len).i32_add().local_set(endp);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(endp).i32_ge_u().br_if(1);
    i.local_get(p).i32_load(mem(0)).local_set(klen);
    i.local_get(p).i32_load(mem(4)).local_set(vlen);
    i.local_get(klen).local_get(a_len).i32_eq().if_(BlockType::Empty);
    // byte compare key at p+8 vs a_ptr
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(klen).i32_ge_u().if_(BlockType::Empty);
    i.local_get(p).local_set(best); // full match
    i.br(2);
    i.end();
    i.local_get(p).i32_const(8).i32_add().local_get(j).i32_add().i32_load8_u(mem8(0));
    i.local_get(a_ptr).local_get(j).i32_add().i32_load8_u(mem8(0));
    i.i32_ne().br_if(1);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.end();
    i.local_get(p).i32_const(8).i32_add().local_get(klen).i32_add().local_get(vlen).i32_add().local_set(p);
    i.br(0).end().end();
}

/// Remove the entry `best` points at (when nonzero): slide the log's tail
/// down over it and shrink the length. `size` is a scratch i32 local.
fn emit_remove(i: &mut InstructionSink<'_>, log: OverlayLog, best: u32, size: u32) {
    i.local_get(best).if_(BlockType::Empty);
    i.local_get(best).i32_load(mem(0)).local_get(best).i32_load(mem(4)).i32_add();
    i.i32_const(8).i32_add().local_set(size);
    i.local_get(best);
    i.local_get(best).local_get(size).i32_add();
    // tail bytes = (base + len) - (best + size)
    i.global_get(log.g_base).global_get(log.g_len).i32_add();
    i.local_get(best).local_get(size).i32_add().i32_sub();
    i.memory_copy(0, 0);
    i.global_get(log.g_len).local_get(size).i32_sub().global_set(log.g_len);
    i.end();
}

/// Make the buffer hold `need` bytes: when it is short, `alloc` a block of
/// max(2 × capacity, need) bytes, copy the live entries over and adopt it.
/// `alloc` consumes nothing, reads the size from `cap` and leaves the new
/// block's address on the stack (refusing on its own when memory cannot
/// grow).
fn emit_reserve(
    i: &mut InstructionSink<'_>,
    log: OverlayLog,
    (need, cap): (u32, u32),
    alloc: impl FnOnce(&mut InstructionSink<'_>, u32),
) {
    i.local_get(need).global_get(log.g_cap).i32_gt_u().if_(BlockType::Empty);
    i.global_get(log.g_cap).i32_const(1).i32_shl().local_tee(cap);
    i.local_get(need);
    i.local_get(cap).local_get(need).i32_ge_u();
    i.select().local_set(cap);
    alloc(i, cap);
    i.local_tee(need); // `need` is recomputed by the caller
    i.global_get(log.g_base).global_get(log.g_len).memory_copy(0, 0);
    i.local_get(need).global_set(log.g_base);
    i.local_get(cap).global_set(log.g_cap);
    i.end();
}

/// The append's scratch locals (i32): the scan's six, plus two.
#[derive(Clone, Copy)]
pub struct SetLocals {
    pub scan: ScanLocals,
    pub need: u32,
    pub cap: u32,
}

/// op 37 (`env.set`) on the fs_call ABI (`a` = the name, `b` = the value):
/// drop the name's old entry, make room, append one entry and answer 0.
/// `alloc` is the world's block allocator (see [`emit_reserve`]).
pub fn emit_append(
    i: &mut InstructionSink<'_>,
    log: OverlayLog,
    l: SetLocals,
    alloc: impl FnOnce(&mut InstructionSink<'_>, u32),
) {
    let (a_ptr, a_len, b_ptr, b_len) = (1u32, 2u32, 3u32, 4u32);
    let SetLocals { scan, need, cap } = l;
    let at = scan.p;
    emit_find(i, log, scan);
    emit_remove(i, log, scan.best, need);
    let need_now = |i: &mut InstructionSink<'_>| {
        i.global_get(log.g_len).i32_const(8).i32_add().local_get(a_len).i32_add().local_get(b_len).i32_add();
        i.local_set(need);
    };
    need_now(i);
    emit_reserve(i, log, (need, cap), alloc);
    need_now(i);
    i.global_get(log.g_base).global_get(log.g_len).i32_add().local_set(at);
    i.local_get(at).local_get(a_len).i32_store(mem(0));
    i.local_get(at).local_get(b_len).i32_store(mem(4));
    i.local_get(at).i32_const(8).i32_add().local_get(a_ptr).local_get(a_len).memory_copy(0, 0);
    i.local_get(at).i32_const(8).i32_add().local_get(a_len).i32_add();
    i.local_get(b_ptr).local_get(b_len).memory_copy(0, 0);
    i.local_get(need).global_set(log.g_len);
    i.i64_const(0).return_();
}

/// op 26 (`env.get`), first half: find the name's entry in the log. A hit
/// points `g_ppos` / `g_plen` at the value where it sits in the log and
/// returns `pack(0, len)`; a miss falls through to the caller's
/// host-environment lookup.
pub fn emit_scan(i: &mut InstructionSink<'_>, log: OverlayLog, (g_ppos, g_plen): (u32, u32), l: ScanLocals) {
    let ScanLocals { vlen, best, .. } = l;
    emit_find(i, log, l);
    // A hit: the value already sits in the log, so point at it in place
    // rather than copying it into a page that may not hold it (#2120).
    i.local_get(best).i32_const(0).i32_ne().if_(BlockType::Empty);
    i.local_get(best).i32_load(mem(4)).local_set(vlen);
    i.local_get(best).i32_const(8).i32_add().local_get(best).i32_load(mem(0)).i32_add().global_set(g_ppos);
    i.local_get(vlen).global_set(g_plen);
    i.local_get(vlen).i64_extend_i32_u().return_();
    i.end();
}
