//! The LINE-BUFFER family (#1826): the interpolation build region and
//! its helpers, split from runtime.rs for the file budget.
//!
//! A cursor is a LOGICAL address, `line_start + offset`. The region
//! starts in the fixed room `[line_start, heap_start)` the memory map
//! reserves; when a build outgrows it, `$line_grow` RELOCATES the
//! region to a heap arena — the live content `[line_start, cur)` is
//! copied, `G_LINE_DELTA` becomes `arena_payload − line_start`, and
//! `G_LINE_ROOM` moves to the arena's end. Cursors do not change, so
//! the `start` locals every nested build holds stay valid; every write
//! and every read-out (`$buf_to_block`, `$line_println`) adds the
//! delta. A build of any length therefore completes, geometrically.
//!
//! Why not `memory.grow`: the room sits BETWEEN the static pool and the
//! heap (assembly.rs: `heap_start = line_start + LINE_BUF_MIN`), so
//! growing memory extends the heap's end, never the room — and the bump
//! allocator is the proven core (StructuralAlloc.v), not a thing to
//! relocate. Growing the ROOM by relocating the build is the sound
//! shape; the arena stays for the program's lifetime, so the next large
//! build pays nothing.

use wasm_encoder::{BlockType, Function, ValType};

use crate::*;

/// `$append_copy(cur: i32, src: i32, len: i32) -> i32`: copy `len`
/// bytes to the logical cursor, return it advanced. When the write
/// would leave the room, the room grows first — never a trap, never a
/// write past the region into the heap.
pub(crate) fn emit_append_copy() -> Function {
    let (cur, src, len) = (0u32, 1u32, 2u32);
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(cur).local_get(len).i32_add().global_get(G_LINE_ROOM).i32_gt_u().if_(BlockType::Empty);
    i.local_get(cur).local_get(len).call(F_LINE_GROW);
    i.end();
    i.local_get(cur).global_get(G_LINE_DELTA).i32_add();
    i.local_get(src);
    i.local_get(len);
    i.call(F_COPY);
    i.local_get(cur).local_get(len).i32_add();
    i.end();
    f
}

/// `$line_grow(cur: i32, len: i32)`: make room for `len` more bytes at
/// logical cursor `cur`. Capacity at least doubles (and covers the
/// request); the live content `[line_start, cur)` — every enclosing
/// build's partial text included — moves to a fresh heap block, the
/// outgrown arena (if the region already lived in one) is released,
/// and the delta/room globals follow. `$alloc` answers an unsatisfiable
/// request with the defined C-197 OOM abort, so exhaustion is loud.
pub(crate) fn emit_line_grow() -> Function {
    // params: 0=cur, 1=len; locals: 2=live (bytes in [line_start, cur)),
    // 3=cap (the new capacity), 4=blk (the new arena's block base)
    let (cur, len, live, cap, blk) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let payload = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(cur).global_get(G_LINE_START).i32_sub().local_set(live);
    // cap = 2 × (room − line_start); at least live + len
    i.global_get(G_LINE_ROOM).global_get(G_LINE_START).i32_sub().i32_const(1).i32_shl().local_set(cap);
    i.local_get(cap).local_get(live).local_get(len).i32_add().i32_lt_u().if_(BlockType::Empty);
    i.local_get(live).local_get(len).i32_add().local_set(cap);
    i.end();
    i.local_get(cap).call(F_ALLOC).local_set(blk);
    // the live content, from its current physical home to the new arena
    i.local_get(blk).i32_const(payload).i32_add();
    i.global_get(G_LINE_START).global_get(G_LINE_DELTA).i32_add();
    i.local_get(live);
    i.call(F_COPY);
    // a non-zero delta means the region already lived in an arena block
    // this family owns outright — file it back (after the copy).
    i.global_get(G_LINE_DELTA).if_(BlockType::Empty);
    i.global_get(G_LINE_START)
        .global_get(G_LINE_DELTA)
        .i32_add()
        .i32_const(payload)
        .i32_sub()
        .call(F_FREE);
    i.end();
    i.local_get(blk).i32_const(payload).i32_add().global_get(G_LINE_START).i32_sub().global_set(G_LINE_DELTA);
    i.global_get(G_LINE_START).local_get(cap).i32_add().global_set(G_LINE_ROOM);
    i.end();
    f
}

/// `$append_i64(cur: i32, v: i64) -> i32`: itoa into the scratch, then
/// append the digits through `$append_copy` (which owns the room check
/// — the direct `$copy` this used to do had none, so a rendering landing
/// at the room's edge wrote past it).
pub(crate) fn emit_append_i64() -> Function {
    // params: 0=cur i32, 1=v i64; locals: 2=len i32
    let (cur, v, len) = (0u32, 1u32, 2u32);
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).call(F_ITOA).local_set(len);
    i.local_get(cur);
    i.i32_const(ITOA_END as i32).local_get(len).i32_sub(); // src
    i.local_get(len);
    i.call(F_APPEND_COPY);
    i.end();
    f
}

/// `$buf_to_block(start: i32, cur: i32) -> i32`: capture a finished
/// line-buffer build as a REAL layout block (value-position `"${...}"`)
/// — read from the region's physical home.
pub(crate) fn emit_buf_to_block() -> Function {
    // params: 0=start i32, 1=cur i32; locals: 2=len i32, 3=base i32
    let (start, cur, len, bbase) = (0u32, 1u32, 2u32, 3u32);
    let payload = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(cur).local_get(start).i32_sub().local_set(len);
    i.local_get(len).call(F_ALLOC).local_set(bbase);
    i.local_get(bbase).i32_const(payload).i32_add();
    i.local_get(start).global_get(G_LINE_DELTA).i32_add();
    i.local_get(len);
    i.call(F_COPY);
    i.local_get(bbase);
    i.end();
    f
}

/// `$line_println(start: i32, cur: i32)` / `$line_eprintln`: flush a
/// finished statement-position build `[start, cur)` to the stream
/// import from its physical address. The caller then restores the
/// build cursor to `start`.
pub(crate) fn emit_line_print(import: u32) -> Function {
    let (start, cur) = (0u32, 1u32);
    let mut f = Function::new([]);
    f.instructions()
        .local_get(start)
        .global_get(G_LINE_DELTA)
        .i32_add()
        .local_get(cur)
        .local_get(start)
        .i32_sub()
        .call(import)
        .end();
    f
}
