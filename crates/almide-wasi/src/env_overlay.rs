//! The env.set overlay log (#1716): the guest-side copy of the environment
//! that `env.set` writes and `env.get` reads first. The host environment is a
//! read-only snapshot on every WASI world, as for wasi-libc's `setenv`: a set
//! is local to the instance and never reaches the host.
//!
//! The log is one page of `[klen u32][vlen u32][key][val]` entries,
//! append-only and scanned last-write-wins, so a re-set key needs no
//! in-place edit. A full page takes the defined refusal (`ENV_FULL_MSG`),
//! never a silent drop.
//!
//! Both stock worlds emit the log through these two functions: the p1 shim
//! (`wasi_shims.rs`, the log at `park + OVL`) and the p3 component's env
//! service (#3223, the log on its own page past the park). Only where the
//! log lives, how a refusal is printed and what an overlay miss falls
//! through to (p1's `environ_get`, p3's `get-environment` snapshot) differ.

use wasm_encoder::{BlockType, InstructionSink};

use crate::{mem, mem8};

/// The bytes one log holds: a page.
pub const OVERLAY_BYTES: u64 = 65536;

/// Where a log lives: its first byte, and the global holding the bytes
/// appended so far.
#[derive(Clone, Copy)]
pub struct OverlayLog {
    pub base: u64,
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

/// op 37 (`env.set`) on the fs_call ABI (`a` = the name, `b` = the value):
/// append one entry and answer 0. `at` is a scratch i32 local; `refuse`
/// prints `ENV_FULL_MSG` and exits when the entry does not fit the page.
pub fn emit_append(i: &mut InstructionSink<'_>, log: OverlayLog, at: u32, refuse: impl FnOnce(&mut InstructionSink<'_>)) {
    let (a_ptr, a_len, b_ptr, b_len) = (1u32, 2u32, 3u32, 4u32);
    let OverlayLog { base, g_len } = log;
    i.i32_const(base as i32).global_get(g_len).i32_add().local_set(at);
    // Room check: the entry must fit under the log's end.
    i.local_get(at).i32_const(8).i32_add().local_get(a_len).i32_add().local_get(b_len).i32_add();
    i.i32_const((base + OVERLAY_BYTES) as i32).i32_gt_u().if_(BlockType::Empty);
    refuse(i);
    i.end();
    i.local_get(at).local_get(a_len).i32_store(mem(0));
    i.local_get(at).local_get(b_len).i32_store(mem(4));
    i.local_get(at).i32_const(8).i32_add().local_get(a_ptr).local_get(a_len).memory_copy(0, 0);
    i.local_get(at).i32_const(8).i32_add().local_get(a_len).i32_add();
    i.local_get(b_ptr).local_get(b_len).memory_copy(0, 0);
    i.global_get(g_len).i32_const(8).i32_add().local_get(a_len).i32_add().local_get(b_len).i32_add();
    i.global_set(g_len);
    i.i64_const(0).return_();
}

/// op 26 (`env.get`), first half: scan the log for the LAST entry whose key
/// equals `a` byte for byte. A hit points `g_ppos` / `g_plen` at the value
/// where it sits in the log and returns `pack(0, len)`; a miss falls through
/// to the caller's host-environment lookup.
pub fn emit_scan(i: &mut InstructionSink<'_>, log: OverlayLog, (g_ppos, g_plen): (u32, u32), l: ScanLocals) {
    let (a_ptr, a_len) = (1u32, 2u32);
    let OverlayLog { base, g_len } = log;
    let ScanLocals { p, endp, klen, vlen, best, j } = l;
    i.i32_const(0).local_set(best);
    i.i32_const(base as i32).local_set(p);
    i.i32_const(base as i32).global_get(g_len).i32_add().local_set(endp);
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
    // A hit: the value already sits in the log, so point at it in place
    // rather than copying it into a page that may not hold it (#2120).
    i.local_get(best).i32_const(0).i32_ne().if_(BlockType::Empty);
    i.local_get(best).i32_load(mem(4)).local_set(vlen);
    i.local_get(best).i32_const(8).i32_add().local_get(best).i32_load(mem(0)).i32_add().global_set(g_ppos);
    i.local_get(vlen).global_set(g_plen);
    i.local_get(vlen).i64_extend_i32_u().return_();
    i.end();
}
