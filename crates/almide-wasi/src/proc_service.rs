//! The p1 artifact's subprocess forwarder (#2589, ADR-0025).
//!
//! Stock WASI has no subprocess API, and none is coming. The subprocess
//! family therefore leaves the artifact through the PRIVATE import
//! `almide:process/spawn.call` — the canonical-ABI lowering of
//! `call: func(op: op, a: string, b: string) -> result<string, string>`
//! (crates/almide-wasm-run/wit/process/spawn.wit) — which ships only when the
//! module's op set names a process op (80..=90), the selection the environ
//! pair gets from op 26. A stock runtime has no such import and refuses the
//! module at load, before `_start` runs: the failure is loud and early,
//! never a wrong answer. A host that implements the import (the embedded
//! host does: `almide_wasm_run::link_spawn_import`) runs the artifact.
//!
//! Two functions ship with it:
//!
//! - the forwarder, a service shim behind `fs_call`: it hands the op's case
//!   index and the two operand buffers to the import, then points the
//!   staging pair (`g_ppos`, `g_plen`) at the answer, so `host_read` copies it
//!   out exactly as it copies any staged result;
//! - `cabi_realloc`, the canonical ABI's landing zone for the answer's bytes:
//!   8-aligned raw bytes bumped off the module's own `__heap` frontier (never
//!   freed — the block allocator's free lists stay undisturbed), growing
//!   memory on demand. It may not call an import (the lowering window
//!   forbids it), so a refused grow is a bare `unreachable`.

use wasm_encoder::{BlockType, Function, ValType};

/// The process ops: `almide:process/spawn`'s `enum op`, case index = op − 80.
pub const PROC_OPS: std::ops::RangeInclusive<i32> = 80..=90;

/// Where the import writes its `result<string, string>` (discriminant at +0,
/// ptr at +4, len at +8): the park's free bytes between the newline byte and
/// the first message.
pub const PROC_RET: u64 = 32;
const _: () = assert!(PROC_RET >= crate::NL + 8 && PROC_RET + 12 <= crate::MSG);

/// Does the op set reach the subprocess service?
pub fn wants_proc(host_ops: &[i32]) -> bool {
    host_ops.iter().any(|op| PROC_OPS.contains(op))
}

/// The forwarder: `(op, a_ptr, a_len, b_ptr, b_len) -> i64`, the `fs_call`
/// answer packing (status in the high half, length in the low).
pub fn shim_proc(park: u64, g_plen: u32, g_ppos: u32, i_call: u32) -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(0).i32_const(*PROC_OPS.start()).i32_sub();
    for p in 1..5u32 {
        i.local_get(p);
    }
    i.i32_const((park + PROC_RET) as i32).call(i_call);
    i.i32_const(park as i32).i32_load(crate::mem(PROC_RET + 4)).global_set(g_ppos);
    i.i32_const(park as i32).i32_load(crate::mem(PROC_RET + 8)).global_set(g_plen);
    i.i32_const(park as i32).i32_load8_u(crate::mem8(PROC_RET)).i64_extend_i32_u().i64_const(32).i64_shl();
    i.global_get(g_plen).i64_extend_i32_u().i64_or();
    i.end();
    f
}

/// `cabi_realloc(old, old_size, align, new_size) -> ptr` over `heap`.
pub fn shim_cabi_realloc(heap: u32) -> Function {
    // params: 0=old 1=old_size 2=align 3=new_size; local 4 = ptr.
    let (end, new_size, ptr) = (1u32, 3u32, 4u32);
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.global_get(heap).i32_const(7).i32_add().i32_const(-8).i32_and().local_tee(ptr);
    i.local_get(new_size).i32_add().local_set(end);
    i.local_get(end).memory_size(0).i32_const(16).i32_shl().i32_gt_u();
    i.if_(BlockType::Empty);
    i.local_get(end).memory_size(0).i32_const(16).i32_shl().i32_sub();
    i.i32_const(0xFFFF).i32_add().i32_const(16).i32_shr_u();
    i.memory_grow(0).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    i.unreachable();
    i.end();
    i.end();
    i.local_get(end).global_set(heap);
    i.local_get(ptr);
    i.end();
    f
}
