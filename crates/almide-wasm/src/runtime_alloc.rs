//! The allocator family: `$alloc` (size-class free-list take, then
//! bump with geometric grow and the defined C-197 OOM) and `$free`
//! (RC-2 size-class filing) — split from runtime.rs for the file
//! budget.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::*;

/// `$alloc(len: i32) -> i32`: allocate a layout-true block (header
/// rc=1/len/cap=len + payload), returns the block BASE. The size-class
/// free lists (RC-2) are consulted first — a freed block whose class
/// capacity covers the request is reused; otherwise bump, growing
/// memory when needed.
pub(crate) fn emit_alloc(oom_msg: u32) -> Function {
    // params: 0=len i32; locals: 1=base i32, 2=next i32 (class scratch
    // before the bump path claims it), 3=want i32, 4=head i32
    let (len, base, next, want, head) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
    // want = max(16, (PAYLOAD + len + 3) & !3); class = ceil_log2(want) - 4
    i.local_get(len)
        .i32_const(almide_layout::PAYLOAD as i32 + 3)
        .i32_add()
        .i32_const(-4)
        .i32_and()
        .local_set(want);
    i.local_get(want).i32_const(16).i32_lt_u().if_(BlockType::Empty);
    i.i32_const(16).local_set(want);
    i.end();
    i.i32_const(28).local_get(want).i32_const(1).i32_sub().i32_clz().i32_sub().local_set(next);
    i.local_get(next).i32_const(FREELIST_CLASSES as i32).i32_lt_u().if_(BlockType::Empty);
    i.local_get(next)
        .i32_const(2)
        .i32_shl()
        .i32_const(FREELIST_BASE as i32)
        .i32_add()
        .local_set(next); // next = the class slot ADDRESS now
    i.local_get(next).i32_load(word(0)).local_tee(head).if_(BlockType::Empty);
    // pop: slot = head.payload[0]; headers rc=1/len/cap=len; done.
    i.local_get(next);
    i.local_get(head).i32_load(word(almide_layout::PAYLOAD)).i32_store(word(0));
    i.local_get(head).i32_const(1).i32_store(word(almide_layout::RC.offset));
    i.local_get(head).local_get(len).i32_store(word(almide_layout::LEN.offset));
    i.local_get(head).local_get(len).i32_store(word(almide_layout::CAP.offset));
    i.local_get(head).return_();
    i.end();
    i.end();
    // base = G_HEAP; next = (base + PAYLOAD + len + 3) & !3
    i.global_get(G_HEAP).local_set(base);
    i.local_get(base)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .local_get(len)
        .i32_add()
        .i32_const(3)
        .i32_add()
        .i32_const(-4)
        .i32_and()
        .local_set(next);
    // if next > memory.size * 64Ki: grow GEOMETRICALLY — max(needed,
    // current) pages, i.e. at least doubling. Grow-just-enough produced
    // thousands of one-page grows on allocation-heavy kernels (~53ms of
    // the str_build micro-profile, stage 54); memory.size is not
    // observable from the language, so the policy is behavior-free.
    i.local_get(next).memory_size(0).i32_const(16).i32_shl().i32_gt_u().if_(BlockType::Empty);
    i.local_get(next)
        .memory_size(0)
        .i32_const(16)
        .i32_shl()
        .i32_sub()
        .i32_const(65535)
        .i32_add()
        .i32_const(16)
        .i32_shr_u();
    {
        // needed pages on stack; select(needed, current, needed > current)
        i.memory_size(0);
        i.local_get(next)
            .memory_size(0)
            .i32_const(16)
            .i32_shl()
            .i32_sub()
            .i32_const(65535)
            .i32_add()
            .i32_const(16)
            .i32_shr_u();
        i.memory_size(0).i32_gt_u().select();
    }
    i.memory_grow(0)
        .i32_const(0)
        .i32_lt_s()
        .if_(BlockType::Empty)
        // C-197: allocation the machine cannot satisfy is the DEFINED
        // "Error: out of memory" + exit 1 — never a raw trap (T6).
        .i32_const(oom_msg as i32)
        .call(F_EPRINTLN_BLOCK)
        .i32_const(1)
        .call(F_EXIT_IMPORT)
        .unreachable()
        .end();
    i.end();
    // header: rc = 1, len, cap = len; advance the bump head
    i.local_get(base).i32_const(1).i32_store(word(almide_layout::RC.offset));
    i.local_get(base).local_get(len).i32_store(word(almide_layout::LEN.offset));
    i.local_get(base).local_get(len).i32_store(word(almide_layout::CAP.offset));
    i.local_get(next).global_set(G_HEAP);
    i.local_get(base);
    i.end();
    f
}

/// `$free(block)`: file a dead block into its size-class free list —
/// filed by FLOOR class (its actual total covers the class capacity),
/// taken by ceil at alloc, so reuse never under-serves. Blocks too
/// small for a next pointer (empty payloads) and huge blocks (total ≥
/// 2^20) are abandoned to the bump graveyard, exactly as before RC-2.
/// The caller must OWN the block outright — there is no rc check yet;
/// the only callers are the sort machinery's private scratch buffers.
pub(crate) fn emit_free() -> Function {
    // params: 0=block i32; locals: 1=total i32, 2=class i32
    let (block, total, class) = (0u32, 1u32, 2u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    // total = (len + PAYLOAD + 3) & !3; too small to hold the next ptr → abandon
    i.local_get(block)
        .i32_load(word(almide_layout::LEN.offset))
        .i32_const(almide_layout::PAYLOAD as i32 + 3)
        .i32_add()
        .i32_const(-4)
        .i32_and()
        .local_set(total);
    i.local_get(total).i32_const(16).i32_lt_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    // class = floor_log2(total) - 4; huge → abandon
    i.i32_const(27).local_get(total).i32_clz().i32_sub().local_set(class);
    i.local_get(class).i32_const(FREELIST_CLASSES as i32).i32_ge_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    i.local_get(class)
        .i32_const(2)
        .i32_shl()
        .i32_const(FREELIST_BASE as i32)
        .i32_add()
        .local_set(class); // class = the slot ADDRESS now
    // block.payload[0] = head; head = block
    i.local_get(block);
    i.local_get(class).i32_load(word(0));
    i.i32_store(word(almide_layout::PAYLOAD));
    i.local_get(class).local_get(block).i32_store(word(0));
    i.end();
    f
}
