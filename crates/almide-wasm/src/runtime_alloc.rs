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
    // cap = the class's PHYSICAL payload capacity (16<<class − header):
    // free re-derives the class from cap, so filing always lands where
    // the next taker looks.
    i.local_get(head);
    i.i32_const(16)
        .local_get(next)
        .i32_const(FREELIST_BASE as i32)
        .i32_sub()
        .i32_const(2)
        .i32_shr_u()
        .i32_shl()
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_sub()
        .i32_store(word(almide_layout::CAP.offset));
    i.local_get(head).return_();
    i.end();
    // Freelist miss: round the bump request UP to the class size —
    // file-by-class == take-by-class is what makes reuse actually fire
    // (a 44-byte block filed by floor could never serve a 44-byte
    // request taken by ceil; the churn gate measured 123 MB of misses).
    i.i32_const(28).local_get(want).i32_const(1).i32_sub().i32_clz().i32_sub().local_set(next);
    i.i32_const(16).local_get(next).i32_shl().local_set(want);
    i.end();
    // base = G_HEAP; next = base + want (class-rounded; huge stays exact)
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
    // class-rounded requests advance by the full class capacity
    i.local_get(want).i32_const(16 << (FREELIST_CLASSES - 1)).i32_le_u().if_(BlockType::Empty);
    i.local_get(base).local_get(want).i32_add().local_set(next);
    i.end();
    // The bump head is an i32: a request whose end lies past 4 GiB WRAPS
    // `next` below `base`, and the grow guard below (a comparison against
    // memory.size) then sees a small, in-range frontier — the header
    // store lands past the end of memory as a raw OOB trap, not the
    // C-197 abort (#1908: a 2 GiB buffer's append growth). A wrapped
    // frontier is an allocation the machine cannot satisfy: die in the
    // defined form before the grow guard can misjudge it.
    i.local_get(next).local_get(base).i32_lt_u().if_(BlockType::Empty);
    i.i32_const(oom_msg as i32).call(F_EPRINTLN_BLOCK);
    i.i32_const(1).call(F_EXIT_IMPORT).unreachable();
    i.end();
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
    i.local_get(base)
        .local_get(want)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_sub()
        .i32_store(word(almide_layout::CAP.offset));
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
        .i32_load(word(almide_layout::CAP.offset))
        .i32_const(almide_layout::PAYLOAD as i32 + 3)
        .i32_add()
        .i32_const(-4)
        .i32_and()
        .local_set(total);
    i.local_get(total).i32_const(16).i32_lt_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    // class = CEIL class of the block's want — the class alloc rounded
    // it to, so filing lands exactly where the next taker looks.
    i.i32_const(28).local_get(total).i32_const(1).i32_sub().i32_clz().i32_sub().local_set(class);
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

/// `$inc(block)`: rc += 1 for a HEAP block; addresses below the heap
/// floor (pool statics, null, scalars-in-disguise) no-op — the compiler
/// blind-emits on the grain doctrine and the guard keeps statics
/// untouchable.
pub(crate) fn emit_inc() -> Function {
    let block = 0u32;
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(block).global_get(G_LINE_END).i32_lt_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    i.local_get(block);
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_add();
    i.i32_store(word(almide_layout::RC.offset));
    i.end();
    f
}

/// `$dec_flat(block)`: rc -= 1; at zero, file the block into the free
/// lists. FLAT blocks only (Str/Bytes/List-of-scalar — no heap
/// interiors), the v1 droppable set; the same heap-floor guard no-ops
/// statics and null.
pub(crate) fn emit_dec_flat() -> Function {
    // params: 0=block; locals: 1=rc
    let (block, rc) = (0u32, 1u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).global_get(G_LINE_END).i32_lt_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_sub().local_set(rc);
    // Diagnostic knob (`ALMIDE_RC_TRAP_DOUBLE_FREE=1`, emit time): a dec
    // of a block already at rc 0 — a freed block — traps instead of
    // wrapping to 0xFFFF_FFFF and silently keeping a dangling block
    // alive. Off by default: the proof-transcribed runtime tree (the
    // hash below) is the shipped one.
    if std::env::var_os("ALMIDE_RC_TRAP_DOUBLE_FREE").is_some() {
        i.local_get(rc).i32_const(-1).i32_eq().if_(BlockType::Empty);
        i.unreachable();
        i.end();
    }
    i.local_get(block).local_get(rc).i32_store(word(almide_layout::RC.offset));
    i.local_get(rc).i32_eqz().if_(BlockType::Empty);
    i.local_get(block).call(F_FREE);
    i.end();
    i.end();
    f
}

/// `$drop_list(block)` — `$dec_flat` for a List of heap HANDLES (#2010
/// stage 2b): the same heap-floor guard and trap knob; at rc 0 every
/// element handle (a 4-byte slot) is released through `elem_dec` before
/// the spine is freed.
pub(crate) fn emit_drop_list(elem_dec: u32) -> Function {
    // params: 0=block; locals: 1=rc, 2=p, 3=end
    let (block, rc, p, end) = (0u32, 1u32, 2u32, 3u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).global_get(G_LINE_END).i32_lt_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_sub().local_set(rc);
    if std::env::var_os("ALMIDE_RC_TRAP_DOUBLE_FREE").is_some() {
        i.local_get(rc).i32_const(-1).i32_eq().if_(BlockType::Empty);
        i.unreachable();
        i.end();
    }
    i.local_get(block).local_get(rc).i32_store(word(almide_layout::RC.offset));
    i.local_get(rc).i32_eqz().if_(BlockType::Empty);
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(block).i32_load(word(almide_layout::LEN.offset)).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p).i32_load(word(0)).call(elem_dec);
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0).end().end();
    i.local_get(block).call(F_FREE);
    i.end();
    i.end();
    f
}

/// `$drop_<shape>(block)` — the typed drop of a block with handle slots
/// at fixed payload offsets (#2010 stage 2c): `$dec_flat`'s guard and
/// trap knob; at rc 0 every `(offset, dec_fn)` slot is released, then
/// the block freed. `tagged` names a tag word and per-tag slot tables
/// (a Result: tag 0 = Ok payload, 1 = Err payload, both at SUM_FIELD).
pub(crate) fn emit_drop_shape(slots: &[(u32, u32)], tagged: Option<(u32, Vec<(u32, Vec<(u32, u32)>)>)>) -> Function {
    // params: 0=block; locals: 1=rc
    let (block, rc) = (0u32, 1u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).global_get(G_LINE_END).i32_lt_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_sub().local_set(rc);
    if std::env::var_os("ALMIDE_RC_TRAP_DOUBLE_FREE").is_some() {
        i.local_get(rc).i32_const(-1).i32_eq().if_(BlockType::Empty);
        i.unreachable();
        i.end();
    }
    i.local_get(block).local_get(rc).i32_store(word(almide_layout::RC.offset));
    i.local_get(rc).i32_eqz().if_(BlockType::Empty);
    for &(off, dec) in slots {
        i.local_get(block).i32_load(word(almide_layout::PAYLOAD + off)).call(dec);
    }
    if let Some((tag_off, cases)) = tagged {
        for (tag, cslots) in cases {
            i.local_get(block).i32_load(word(almide_layout::PAYLOAD + tag_off)).i32_const(tag as i32).i32_eq();
            i.if_(BlockType::Empty);
            for (off, dec) in cslots {
                i.local_get(block).i32_load(word(almide_layout::PAYLOAD + off)).call(dec);
            }
            i.end();
        }
    }
    i.local_get(block).call(F_FREE);
    i.end();
    i.end();
    f
}

/// `$drop_map(block)` — `$dec_flat` for a Map: at rc 0 the index
/// side-table entry for this address is cleared (a stale index on a
/// reused address would answer for the wrong map), then the entries
/// array freed. Entries keep their credits (Map stage a).
pub(crate) fn emit_drop_map_spine(side_clear: u32) -> Function {
    let (block, rc) = (0u32, 1u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).global_get(G_LINE_END).i32_lt_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_sub().local_set(rc);
    if std::env::var_os("ALMIDE_RC_TRAP_DOUBLE_FREE").is_some() {
        i.local_get(rc).i32_const(-1).i32_eq().if_(BlockType::Empty);
        i.unreachable();
        i.end();
    }
    i.local_get(block).local_get(rc).i32_store(word(almide_layout::RC.offset));
    i.local_get(rc).i32_eqz().if_(BlockType::Empty);
    i.global_get(G_MAPIDX).if_(BlockType::Empty);
    i.local_get(block).i32_const(0).call(side_clear).drop();
    i.end();
    i.local_get(block).call(F_FREE);
    i.end();
    i.end();
    f
}

/// `$drop_entries(block)` — the typed drop of a Map / Set with handle
/// slots in its entries (#2010, Map stage b): `$dec_flat`'s guard and
/// trap knob; at rc 0 every entry's `(offset, dec_fn)` slots are
/// released in insertion order, the index side-table entry cleared, and
/// the entries array freed.
pub(crate) fn emit_drop_entries(stride: u32, slots: [Option<(u32, u32)>; 2], side_clear: u32) -> Function {
    // params: 0=block; locals: 1=rc, 2=p, 3=end
    let (block, rc, p, end) = (0u32, 1u32, 2u32, 3u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).global_get(G_LINE_END).i32_lt_u().if_(BlockType::Empty);
    i.return_();
    i.end();
    i.local_get(block).i32_load(word(almide_layout::RC.offset)).i32_const(1).i32_sub().local_set(rc);
    if std::env::var_os("ALMIDE_RC_TRAP_DOUBLE_FREE").is_some() {
        i.local_get(rc).i32_const(-1).i32_eq().if_(BlockType::Empty);
        i.unreachable();
        i.end();
    }
    i.local_get(block).local_get(rc).i32_store(word(almide_layout::RC.offset));
    i.local_get(rc).i32_eqz().if_(BlockType::Empty);
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(block).i32_load(word(almide_layout::LEN.offset)).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    for (off, dec) in slots.into_iter().flatten() {
        i.local_get(p).i32_load(word(off)).call(dec);
    }
    i.local_get(p).i32_const(stride as i32).i32_add().local_set(p);
    i.br(0).end().end();
    i.global_get(G_MAPIDX).if_(BlockType::Empty);
    i.local_get(block).i32_const(0).call(side_clear).drop();
    i.end();
    i.local_get(block).call(F_FREE);
    i.end();
    i.end();
    f
}

/// `$inc_entries(block, nbytes)`: +1 on every handle slot of the entries
/// in the first `nbytes` payload bytes (the credits a copied entries
/// array holds, #2010 Map stage b).
pub(crate) fn emit_inc_entries(stride: u32, slots: [Option<u32>; 2]) -> Function {
    // params: 0=block, 1=nbytes; locals: 2=p, 3=end
    let (block, nbytes, p, end) = (0u32, 1u32, 2u32, 3u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(nbytes).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    for off in slots.into_iter().flatten() {
        i.local_get(p).i32_load(word(off)).call(F_INC);
    }
    i.local_get(p).i32_const(stride as i32).i32_add().local_set(p);
    i.br(0).end().end();
    i.end();
    f
}

/// `$copy_entries(block) -> block`: `$block_copy`, then the copy takes
/// the credits of every entry it holds.
pub(crate) fn emit_copy_entries(inc_entries: u32) -> Function {
    let block = 0u32;
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).call(F_BLOCK_COPY).local_tee(1);
    i.local_get(1).i32_load(word(almide_layout::LEN.offset)).call(inc_entries);
    i.local_get(1);
    i.end();
    f
}

/// `$inc_<shape>(block)`: +1 on every handle slot of a fixed-slot block
/// (the tagged half per case, as `emit_drop_shape`).
pub(crate) fn emit_inc_shape(slots: &[(u32, u32)], tagged: Option<(u32, Vec<(u32, Vec<(u32, u32)>)>)>) -> Function {
    let block = 0u32;
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([]);
    let mut i = f.instructions();
    for &(off, _) in slots {
        i.local_get(block).i32_load(word(almide_layout::PAYLOAD + off)).call(F_INC);
    }
    if let Some((tag_off, cases)) = tagged {
        for (tag, cslots) in cases {
            i.local_get(block).i32_load(word(almide_layout::PAYLOAD + tag_off)).i32_const(tag as i32).i32_eq();
            i.if_(BlockType::Empty);
            for (off, _) in cslots {
                i.local_get(block).i32_load(word(almide_layout::PAYLOAD + off)).call(F_INC);
            }
            i.end();
        }
    }
    i.end();
    f
}

/// `$inc_elems(block)`: +1 on every element handle of a spine of 4-byte
/// handle slots (the credits a copied spine must hold, #2010 stage 2b).
pub(crate) fn emit_inc_elems() -> Function {
    // params: 0=block; locals: 1=p, 2=end
    let (block, p, end) = (0u32, 1u32, 2u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(p);
    i.local_get(p).local_get(block).i32_load(word(almide_layout::LEN.offset)).i32_add().local_set(end);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(end).i32_ge_u().br_if(1);
    i.local_get(p).i32_load(word(0)).call(F_INC);
    i.local_get(p).i32_const(4).i32_add().local_set(p);
    i.br(0).end().end();
    i.end();
    f
}

/// `$copy_elems(block) -> block`: `$block_copy`, then the copy takes its
/// element credits.
pub(crate) fn emit_copy_elems(inc_elems: u32) -> Function {
    let block = 0u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).call(F_BLOCK_COPY).local_tee(1).call(inc_elems);
    i.local_get(1);
    i.end();
    f
}

/// `$cow_elems(block) -> block`: `$cow`; when it copied (the result is a
/// different block), the copy takes its element credits.
pub(crate) fn emit_cow_elems(inc_elems: u32) -> Function {
    let block = 0u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(block).call(F_COW).local_tee(1).local_get(block).i32_ne().if_(BlockType::Empty);
    i.local_get(1).call(inc_elems);
    i.end();
    i.local_get(1);
    i.end();
    f
}

/// The signatures assembly promises for the rc-glue helpers (#2010).
pub(crate) fn helper_params(h: &Helper) -> Option<Vec<ValType>> {
    if matches!(h, Helper::IncEntries { .. }) {
        return Some(vec![ValType::I32, ValType::I32]);
    }
    matches!(
        h,
        Helper::DropList { .. }
            | Helper::IncElems
            | Helper::CopyElems { .. }
            | Helper::CowElems { .. }
            | Helper::DropShape { .. }
            | Helper::IncShape { .. }
            | Helper::DropMapSpine { .. }
            | Helper::DropEntries { .. }
            | Helper::CopyEntries { .. }
    )
    .then(|| vec![ValType::I32])
}

/// The result type of a helper: the drops and the credit walk return
/// nothing; the copy variants hand the block back; everything else i32
/// unless assembly says f64.
pub(crate) fn helper_result(h: &Helper) -> Option<ValType> {
    match h {
        Helper::DropList { .. }
        | Helper::IncElems
        | Helper::DropShape { .. }
        | Helper::IncShape { .. }
        | Helper::DropMapSpine { .. }
        | Helper::DropEntries { .. }
        | Helper::IncEntries { .. } => None,
        _ => Some(ValType::I32),
    }
}

/// The rc-glue helper bodies; a `DropShape` body was built at registration
/// (`work.drop_bodies`) — a missing one is a loud stub.
pub(crate) fn helper_body(h: &Helper, work: &crate::work::FnWork) -> Option<Function> {
    Some(match h {
        Helper::DropList { elem_dec } => emit_drop_list(*elem_dec),
        Helper::IncElems => emit_inc_elems(),
        Helper::CopyElems { inc_elems } => emit_copy_elems(*inc_elems),
        Helper::CowElems { inc_elems } => emit_cow_elems(*inc_elems),
        Helper::DropMapSpine { side_clear } => emit_drop_map_spine(*side_clear),
        Helper::DropEntries { stride, slots, side_clear } => emit_drop_entries(*stride, *slots, *side_clear),
        Helper::IncEntries { stride, slots } => emit_inc_entries(*stride, *slots),
        Helper::CopyEntries { inc_entries } => emit_copy_entries(*inc_entries),
        Helper::DropShape { .. } | Helper::IncShape { .. } => {
            let mut bodies = work.drop_bodies.borrow_mut();
            let built = bodies.iter_mut().find(|(k, _)| k == h).and_then(|(_, f)| f.take());
            built.unwrap_or_else(|| {
                let mut f = Function::new([]);
                f.instructions().unreachable().end();
                f
            })
        }
        _ => return None,
    })
}

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

#[cfg(test)]
mod tests {
    use wasm_encoder::{CodeSection, Function};

    fn body_bytes(f: &Function) -> Vec<u8> {
        // Encode through a code section so the byte form is exactly what
        // ships (locals prefix + operators + end).
        let mut cs = CodeSection::new();
        cs.function(f);
        let mut out = Vec::new();
        use wasm_encoder::Encode as _;
        cs.encode(&mut out);
        out
    }

    /// #576: proofs/StructuralRuntime.v + proofs/StructuralAlloc.v
    /// transcribe THESE trees. The pin
    /// makes drift bidirectional and loud: change an emitted runtime
    /// body and this hash moves, which is the signal to re-transcribe
    /// the .v side (and re-prove); the .v file's header names this test
    /// back. The oom_msg parameter of $alloc keeps it out of this
    /// constant-byte pin — its slice is the next .v increment.
    #[test]
    fn runtime_trees_match_the_proof_transcription() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        body_bytes(&super::emit_inc()).hash(&mut h);
        body_bytes(&super::emit_dec_flat()).hash(&mut h);
        body_bytes(&super::emit_free()).hash(&mut h);
        // $alloc with a FIXED probe immediate for its one per-program
        // parameter (the OOM message address): the tree shape is pinned;
        // the immediate's value is not part of the transcription.
        body_bytes(&super::emit_alloc(0)).hash(&mut h);
        let got = h.finish();
        // Recorded at the StructuralRuntime.v landing. A mismatch means
        // the emitted trees moved: update proofs/StructuralRuntime.v to
        // the new trees (re-proving what changed), then this constant.
        assert_eq!(
            got, 0x2312b47da07c14b0,
            "runtime tree bytes drifted from the proofs/StructuralRuntime.v transcription (got {got:#x})"
        );
    }
}

#[cfg(test)]
mod byte_dump {
    /// The proofs/StructuralDecode.v byte lists' source: dump the trees'
    /// code-section bytes as decimal lists. proofs/check-structural-bytes.sh
    /// runs this per check and diffs the output against the .v lists, so
    /// neither side can drift silently. Manual run:
    /// `cargo test -p almide-wasm --lib dump_runtime_bytes -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn dump_runtime_bytes() {
        use wasm_encoder::{CodeSection, Encode as _};
        for (name, f) in [
            ("inc", super::emit_inc()),
            ("dec_flat", super::emit_dec_flat()),
            ("free", super::emit_free()),
            // 0 stands in for the per-program OOM message address; the
            // .v side carries that immediate as a parameter.
            ("alloc", super::emit_alloc(0)),
        ] {
            let mut cs = CodeSection::new();
            cs.function(&f);
            let mut out = Vec::new();
            cs.encode(&mut out);
            println!("{name}: {:?}", out);
        }
    }
}
