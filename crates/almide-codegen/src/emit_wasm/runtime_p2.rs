/// __alloc(size: i32) -> i32
/// Bump allocator: returns current heap_ptr (8-byte aligned), then advances by size.
/// All returned pointers are guaranteed to be 8-byte aligned, matching wasi-libc
/// and Emscripten conventions. This ensures i64 loads/stores never trap on alignment.
fn compile_alloc(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.alloc];
    let hdr = emitter.layout_reg.header_size(ALLOC_HEADER) as i32;
    let rc_off = emitter.layout_reg.fixed_offset(ALLOC_HEADER, alloc::RC);
    let size_off = emitter.layout_reg.fixed_offset(ALLOC_HEADER, alloc::SIZE);
    let rc_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::RC).ty;
    let size_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::SIZE).ty;
    let free_list = emitter.free_list_global;
    let heap_ptr = emitter.heap_ptr_global;

    // locals: 0=request_size, 1=ptr, 2=grow_pages, 3=prev, 4=cur, 5=steps
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32),
    ]);
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);

        // --- Free list walk ---
        // The walk carries two tripwires (free when the list is empty, i.e.
        // whenever frees are off): a STEP BOUND that traps on a cycle (a
        // double-free that slipped past the rc sentinel pushes a block twice
        // and links the list to itself — without the bound this loop spins
        // forever, the hang that forced the first activation revert), and a
        // SIZE SANITY check that traps when a node's header was clobbered
        // (e.g. a host-written buffer freed and overwritten — the fs scratch
        // poison class). No free list can have more nodes than 8-byte blocks
        // in the heap, so heap_ptr >> 3 bounds any acyclic walk.
        w.i32c(0).set(3);                       // prev = null
        w.gget(free_list).set(4);               // cur = free_list_head
        w.i32c(0).set(5);                       // steps = 0
        w.block(|w| { w.loop_(|w| {
            w.get(4).eqz().br_if(1);            // cur == null → bump
            // steps++; steps > cap → cycle → trap. The cap is ABSOLUTE:
            // a heap-derived bound (heap_ptr >> 3) lets a multi-hundred-MB
            // heap walk tens of millions of steps PER ALLOC before tripping —
            // a corrupted cycle then spins the host at 100% CPU for hours
            // instead of trapping (observed killing the dev machine). No sane
            // free list approaches a million nodes in this runtime.
            const FREE_LIST_WALK_CAP: i32 = 1 << 20;
            w.get(5).i32c(1).add().tee(5);
            w.i32c(FREE_LIST_WALK_CAP);
            w.gt_u();
            w.if_void(|w| { w.unreachable_(); }, |_| {});
            // NOTE: a size-sanity bound (cur+hdr+size <= heap_ptr) was tried
            // here and removed: it false-positived on legitimate freed nodes
            // (first churn loop), and the corruption classes it aimed at are
            // covered by the step cap above (cycles), the rc==0 sentinel in
            // rc_dec (double-free), and the rc==0 trap in rc_inc
            // (resurrection). Host-clobbered headers (the fs scratch class)
            // are addressed by construction via pinned allocations.
            w.get(4).emit_load(size_off, size_ty); // cur.size
            w.get(0).ge_u();                     // >= request_size?
            w.if_void(|w| {
                // Found: unlink
                w.get(3).eqz();
                w.if_void(|w| {
                    // prev == null → cur is head: head = cur.next
                    w.get(4).i32c(hdr).add().emit_load(0, MemType::I32);
                    w.gset(free_list);
                }, |w| {
                    // prev.next = cur.next
                    w.get(3).i32c(hdr).add();
                    w.get(4).i32c(hdr).add().emit_load(0, MemType::I32);
                    w.emit_store(0, MemType::I32);
                });
                // RC = 1
                w.get(4).i32c(1).emit_store(rc_off, rc_ty);
                // Zero-fill reused block's data area to prevent stale data
                // (critical for Swiss Table tag arrays)
                w.get(4).i32c(hdr).add();  // data_ptr
                w.i32c(0);                 // fill value
                w.get(4).emit_load(size_off, size_ty); // size
                w.raw(wasm_encoder::Instruction::MemoryFill(0));
                // Return data ptr
                w.get(4).i32c(hdr).add().ret();
            }, |_| {});
            // Advance: prev = cur, cur = cur.next
            w.get(4).set(3);
            w.get(4).i32c(hdr).add().emit_load(0, MemType::I32).set(4);
            w.br(0);
        }); });

        // --- Bump path ---
        // Align heap_ptr to header boundary
        let align_mask = hdr - 1;       // hdr is power of 2 (8) → mask = 7
        w.gget(heap_ptr).i32c(align_mask).add().i32c(-hdr).and().set(1);
        // Advance: ptr + size + header
        w.get(1).get(0).add().i32c(hdr).add().gset(heap_ptr);
        // Grow memory if needed
        w.gget(heap_ptr);
        w.raw(wasm_encoder::Instruction::I64ExtendI32U);
        w.raw(wasm_encoder::Instruction::I64Const(65535));
        w.raw(wasm_encoder::Instruction::I64Add);
        w.raw(wasm_encoder::Instruction::I64Const(16));
        w.raw(wasm_encoder::Instruction::I64ShrU);
        w.raw(wasm_encoder::Instruction::I32WrapI64);
        w.memory_size().sub().tee(2);
        w.i32c(0);
        w.raw(wasm_encoder::Instruction::I32GtS);
        w.if_void(|w| {
            w.memory_size().get(2);
            w.memory_size().get(2);
            w.gt_u();
            w.raw(wasm_encoder::Instruction::Select);
            w.memory_grow();
            w.i32c(-1).eq();
            w.if_void(|w| { w.unreachable_(); }, |_| {});
        }, |_| {});
        // Write header
        w.get(1).get(0).emit_store(size_off, size_ty);
        w.get(1).i32c(1).emit_store(rc_off, rc_ty);
        // Return data ptr
        w.get(1).i32c(hdr).add();
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.alloc, type_idx, f));
}

/// Whether this emission activates real reference-count frees — the DEFAULT
/// since 0.27.0 (the true-Perceus flip: quadruple bar green ×3 — native
/// corpus + wasm corpus both modes + byte gate + churn; see
/// docs/roadmap/active/wasm-frees-ownership-discipline.md and contract
/// C-066). `ALMIDE_WASM_FREES=0` is the opt-out escape hatch back to the
/// bump-allocate-and-leak model. Env-conditional emission is DECLARED
/// behavior: the host-determinism gates pin the environment, and the same
/// env must always produce identical bytes.
pub(super) fn wasm_frees_enabled() -> bool {
    std::env::var_os("ALMIDE_WASM_FREES").is_none_or(|v| v != "0")
}

fn compile_rc_inc(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.rc_inc];

    if !wasm_frees_enabled() {
        // Bump-and-leak model (default): true no-op, return the pointer
        // untouched. The old header-guard `ptr < global0(heap_ptr)` returned
        // early for every VALID heap pointer, so an increment here could only
        // ever execute on a GARBAGE pointer (#470) — touching memory is pure
        // downside while frees are off.
        let mut f = Function::new([]);
        {
            let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
            w.get(0);
        }
        f.instruction(&wasm_encoder::Instruction::End);
        emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.rc_inc, type_idx, f));
        return;
    }

    // ALMIDE_WASM_FREES=1: real reference counting. The guard uses the
    // IMMUTABLE heap_start low bound (HEAP_START_GLOBAL_IDX) — the legacy
    // `emitter.rt.heap_start_global` field is still 0 (= the moving heap_ptr)
    // at compile_runtime time, which is exactly what baked the old body into
    // a no-op for years.
    let rc_neg = emitter.layout_reg.alloc_header_neg_offset(alloc::RC) as i32;
    let rc_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::RC).ty;
    let heap_start = HEAP_START_GLOBAL_IDX;

    let heap_ptr = emitter.heap_ptr_global;
    let mut f = Function::new([(1, ValType::I32)]); // local 1: $rc
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        // Data-section constants have no header: pass through.
        w.get(0).gget(heap_start).lt_u();
        w.if_void(|w| { w.get(0).ret(); }, |_| {});
        // Dead-zone guard: after __heap_restore moved the frontier DOWN, a
        // stale pointer at/above heap_ptr has no live header — touching it
        // would corrupt whatever gets bump-allocated there next. Skip (the
        // leak direction is the safe one).
        w.get(0).gget(heap_ptr).ge_u();
        w.if_void(|w| { w.get(0).ret(); }, |_| {});
        // Resurrection tripwire: Inc of a FREED block (rc==0 sentinel) is
        // always a compiler bug — without this trap it silently revives a
        // block already on the free list and the next alloc hands out live
        // memory (observed as silent value corruption, not a crash).
        w.get(0).i32c(rc_neg).sub().emit_load(0, rc_ty).tee(1);
        w.eqz();
        w.if_void(|w| { w.unreachable_(); }, |_| {});
        // PINNED blocks are immortal: pass through untouched (a +1 would
        // creep the sentinel toward wrap/unpin).
        w.get(1).i32c(PINNED_RC).eq();
        w.if_void(|w| { w.get(0).ret(); }, |_| {});
        // *(ptr - rc_neg) = rc + 1
        w.get(0).i32c(rc_neg).sub();
        w.get(1).i32c(1).add();
        w.emit_store(0, rc_ty);
        w.get(0); // return ptr
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.rc_inc, type_idx, f));
}

fn compile_rc_dec(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.rc_dec];

    if !wasm_frees_enabled() {
        // Bump-and-leak model (default): true no-op (see compile_rc_inc).
        let mut f = Function::new([]);
        f.instruction(&wasm_encoder::Instruction::End);
        emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.rc_dec, type_idx, f));
        return;
    }

    // ALMIDE_WASM_FREES=1: real decrement + free-list push, with the
    // DOUBLE-FREE SENTINEL: a freed block is stamped rc=0; a Dec that sees
    // rc==0 traps `unreachable` LOUDLY instead of pushing the block onto the
    // free list a second time — a second push forms a cycle that spins
    // __alloc's walk forever (the silent hang that forced the first revert).
    // __alloc restores rc=1 on reuse, so the sentinel only marks dead blocks.
    let rc_neg = emitter.layout_reg.alloc_header_neg_offset(alloc::RC) as i32;
    let rc_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::RC).ty;
    let hdr = emitter.layout_reg.header_size(ALLOC_HEADER) as i32;
    let heap_start = HEAP_START_GLOBAL_IDX;
    let free_list = emitter.free_list_global;
    let heap_ptr_g = emitter.heap_ptr_global;

    let mut f = Function::new([(1, ValType::I32)]); // local 1: $rc
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        w.get(0).gget(heap_start).lt_u();
        w.if_void(|w| { w.ret(); }, |_| {});
        // Dead-zone guard (see compile_rc_inc): a stale pointer at/above the
        // restored bump frontier has no header — freeing it would re-poison
        // the just-reset free list. Skip = bounded leak.
        w.get(0).gget(heap_ptr_g).ge_u();
        w.if_void(|w| { w.ret(); }, |_| {});
        // rc = *(ptr - rc_neg)
        w.get(0).i32c(rc_neg).sub().emit_load(0, rc_ty).tee(1);
        // PINNED blocks never free (host-written scratch; see __alloc_pinned).
        w.i32c(PINNED_RC).eq();
        w.if_void(|w| { w.ret(); }, |_| {});
        w.get(1);
        w.i32c(1).gt_u();
        w.if_void(|w| {
            // rc > 1: decrement
            w.get(0).i32c(rc_neg).sub();
            w.get(1).i32c(1).sub();
            w.emit_store(0, rc_ty);
        }, |w| {
            // rc <= 1: about to free. Sentinel: rc==0 = already freed → trap.
            w.get(1).eqz();
            w.if_void(|w| { w.unreachable_(); }, |_| {});
            // Push to free list for reuse (next ptr lives at data[0]).
            w.get(0).gget(free_list).emit_store(0, MemType::I32);
            w.get(0).i32c(hdr).sub().gset(free_list);
            // Stamp rc=0 (the sentinel).
            w.get(0).i32c(rc_neg).sub();
            w.i32c(0);
            w.emit_store(0, rc_ty);
        });
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.rc_dec, type_idx, f));
}

/// __cow_check(ptr) -> ptr. See registration comment in `register_runtime`.
///
/// Returns a FRESH, uniquely-owned copy of the heap object so an in-place mutation
/// of the result is invisible through any other binding that aliased `ptr` (Almide
/// value semantics; only emitted at the mutation sites of `AliasCowPass`-marked
/// vars). The data byte length is read from the alloc header's SIZE field, so the
/// body carries no hardcoded element-size — it works uniformly for List/String/
/// Map/Record/Bytes/variant blocks, all of which __alloc stamps with their size.
///
/// This clones UNCONDITIONALLY (a data-section pointer, which has no header, is the
/// only pass-through). It does NOT branch on the refcount: in the current WASM
/// runtime the rc header guard (rc_inc/rc_dec) is a no-op (a bump-allocate-and-leak
/// model — see `HEAP_START_GLOBAL_IDX`), so the rc never reflects aliasing and a
/// `rc>1` test would never fire. Unconditional clone matches the Rust target's
/// eager `.clone()` at the bind: correct, and the extra copy when the alias is
/// already dead is the accepted, conservative cost of `needs_cow` marking. The
/// original block is left untouched (it leaks like every other block today), so no
/// refcount bookkeeping is needed.
fn compile_cow_check(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.cow_check];
    let size_neg = emitter.layout_reg.alloc_header_neg_offset(alloc::SIZE) as i32;
    let size_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::SIZE).ty;
    let alloc_fn = emitter.rt.alloc;

    // locals: 1 = $size (data byte count), 2 = $new_ptr
    let mut f = Function::new([(2, ValType::I32)]);
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        // A data-section ptr (below heap_start) has no alloc header → not a heap
        // object → nothing to clone, return as-is. Uses the immutable heap_start
        // global directly (the rt field is still 0 at this compile point).
        w.get(0).gget(HEAP_START_GLOBAL_IDX).lt_u();
        w.if_void(|w| { w.get(0).ret(); }, |_| {});
        // size = header.SIZE; new = alloc(size); memcpy(new, ptr, size); return new.
        w.get(0).i32c(size_neg).sub().emit_load(0, size_ty).set(1);
        w.get(1).call(alloc_fn).set(2);
        w.get(2).get(0).get(1).memory_copy();
        w.get(2); // return the fresh clone
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.cow_check, type_idx, f));
}

// __heap_save() -> i32
// Returns the current heap_ptr. Pair with __heap_restore for arena-style
// scoped allocation: save before a sequence of __alloc calls, restore after
// to free everything allocated since the save.
fn compile_heap_save(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.heap_save];
    let mut f = Function::new([]);
    f.instruction(&wasm_encoder::Instruction::GlobalGet(emitter.heap_ptr_global));
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.heap_save, type_idx, f));
}

// __heap_restore(ptr: i32) -> ()
// Resets heap_ptr to the given checkpoint. Pointers allocated above this
// checkpoint become invalid; any view over them must be discarded by the
// caller before invoking restore.
fn compile_heap_restore(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.heap_restore];
    let mut f = Function::new([]);
    // Reset heap pointer (no zero-fill — alloc writes refcount header,
    // and Swiss Table init zeroes tags via bump allocator's fresh pages).
    wasm!(f, {
        local_get(0);
        global_set(emitter.heap_ptr_global);
        // Forget the free list wholesale: nodes above the restored frontier
        // are dead (the walk's size-sanity tripwire traps on them); nodes
        // below are merely un-remembered — optimization loss, never
        // corruption. Unconditional: a no-op while frees are off, so the
        // emitted bytes stay env-independent here.
        i32_const(0);
        global_set(emitter.free_list_global);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.heap_restore, type_idx, f));
}

/// `__alloc_pinned(size) -> ptr` — `__alloc` + stamp the rc header with
/// `PINNED_RC`. Used for every buffer a WASI host call writes into
/// (fd_out/stat/iov/nread/data scratch in the fs ops, the preopen tables):
/// such a block on the FREE LIST gets its `next` field overwritten by the
/// host (the field lives in the data area) → poisoned walk → OOB. Pinning
/// removes the entire class by construction; the cost is a bounded,
/// deliberate leak of small per-op scratch. Unconditional stamp — in
/// leak-mode (`ALMIDE_WASM_FREES=0`) rc is inert anyway, and keeping the
/// bytes env-independent here preserves the host-determinism story.
fn compile_alloc_pinned(emitter: &mut WasmEmitter) {
    use super::engine::{WasmBuilder, layout::*};
    let type_idx = emitter.func_type_indices[&emitter.rt.alloc_pinned];
    let rc_neg = emitter.layout_reg.alloc_header_neg_offset(alloc::RC) as i32;
    let rc_ty = emitter.layout_reg.field(ALLOC_HEADER, alloc::RC).ty;
    let mut f = Function::new([(1, ValType::I32)]); // local 1: $ptr
    {
        let w = &mut WasmBuilder::new(&mut f, &emitter.layout_reg);
        w.get(0).call(emitter.rt.alloc).set(1);
        w.get(1).i32c(rc_neg).sub();
        w.i32c(PINNED_RC);
        w.emit_store(0, rc_ty);
        w.get(1);
    }
    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.alloc_pinned, type_idx, f));
}

/// __println_str(ptr: i32)
/// Prints string at ptr ([len:i32][cap:i32][data@8]) followed by newline via WASI fd_write.
fn compile_println_str(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.println_str];
    let mut f = Function::new([]);

    // --- Write the string ---
    // iov[0].buf = ptr + string_data_off()  (skip len+cap header)
    wasm!(f, {
        i32_const(0);
        local_get(0);
        i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
        i32_add;
        i32_store(0);
    });
    // iov[0].len = *ptr  (load length)
    wasm!(f, {
        i32_const(4);
        local_get(0);
        i32_load(0);
        i32_store(0);
    });
    // fd_write(stdout=1, iovs=0, iovs_len=1, nwritten=8)
    wasm!(f, {
        i32_const(1);
        i32_const(0);
        i32_const(1);
        i32_const(8);
        call(emitter.rt.fd_write);
        drop;
    });

    // --- Write newline ---
    wasm!(f, {
        i32_const(0);
        i32_const(NEWLINE_OFFSET as i32);
        i32_store(0);
        i32_const(4);
        i32_const(1);
        i32_store(0);
        i32_const(1);
        i32_const(0);
        i32_const(1);
        i32_const(8);
        call(emitter.rt.fd_write);
        drop;
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.println_str, type_idx, f));
}

/// __int_to_string(n: i64) -> i32
/// Converts an i64 to a decimal string on the heap.
/// Uses scratch area [SCRATCH_ITOA..SCRATCH_ITOA+32) for digit buffer.
fn compile_int_to_string(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.int_to_string];
    // Locals: 0=$n (param), 1=$pos, 2=$is_neg, 3=$abs_n(i64), 4=$start, 5=$len, 6=$result, 7=$i
    let mut f = Function::new([
        (1, ValType::I32),  // 1: $pos
        (1, ValType::I32),  // 2: $is_neg
        (1, ValType::I64),  // 3: $abs_n
        (1, ValType::I32),  // 4: $start
        (1, ValType::I32),  // 5: $len
        (1, ValType::I32),  // 6: $result
        (1, ValType::I32),  // 7: $i
    ]);

    let scratch_end = SCRATCH_ITOA + 31;

    // $pos = scratch_end (write backwards from end of scratch buffer)
    wasm!(f, {
        i32_const(scratch_end as i32);
        local_set(1);
    });

    // $is_neg = $n < 0
    f.instruction(&wasm_encoder::Instruction::LocalGet(0));
    f.instruction(&wasm_encoder::Instruction::I64Const(0));
    f.instruction(&wasm_encoder::Instruction::I64LtS);
    wasm!(f, { local_set(2); });

    // $abs_n = if $is_neg then -$n else $n
    wasm!(f, {
        local_get(2);
        if_i64;
        i64_const(0);
        local_get(0);
        i64_sub;
        else_;
        local_get(0);
        end;
        local_set(3);
    });

    // if $abs_n == 0: write '0'
    wasm!(f, {
        local_get(3);
        i64_eqz;
        if_empty;
        local_get(1);
        i32_const(48);
        i32_store8(0);
        local_get(1);
        i32_const(1);
        i32_sub;
        local_set(1);
        else_;
    });
    // while $abs_n > 0: write digits backwards
    wasm!(f, {
        block_empty;
        loop_empty;
        local_get(3);
        i64_eqz;
        br_if(1);
    });
    // mem[$pos] = ($abs_n % 10) + '0'
    wasm!(f, { local_get(1); });
    f.instruction(&wasm_encoder::Instruction::LocalGet(3));
    f.instruction(&wasm_encoder::Instruction::I64Const(10));
    // UNSIGNED rem: `abs_n = 0 - n` produces the correct unsigned magnitude bits
    // even for i64::MIN (0x8000…0 = 2^63), but a SIGNED rem would read those bits
    // as negative and emit bytes below '0'. Unsigned keeps MIN's digits correct.
    f.instruction(&wasm_encoder::Instruction::I64RemU);
    wasm!(f, {
        i32_wrap_i64;
        i32_const(48);
        i32_add;
        i32_store8(0);
    });
    // $pos -= 1
    wasm!(f, {
        local_get(1);
        i32_const(1);
        i32_sub;
        local_set(1);
    });
    // $abs_n /= 10  (UNSIGNED — see the rem note above; keeps i64::MIN correct)
    wasm!(f, {
        local_get(3);
        i64_const(10);
        i64_div_u;
        local_set(3);
        br(0);
        end;
        end;
        end;
    });

    // if $is_neg: write '-'
    wasm!(f, {
        local_get(2);
        if_empty;
        local_get(1);
        i32_const(45);
        i32_store8(0);
        local_get(1);
        i32_const(1);
        i32_sub;
        local_set(1);
        end;
    });

    // $start = $pos + 1
    wasm!(f, {
        local_get(1);
        i32_const(1);
        i32_add;
        local_set(4);
    });

    // $len = scratch_end - $pos
    wasm!(f, {
        i32_const(scratch_end as i32);
        local_get(1);
        i32_sub;
        local_set(5);
    });

    // $result = __alloc(string_hdr() + $len)
    // String layout: [len:i32][cap:i32][data@8]
    wasm!(f, {
        local_get(5);
        i32_const(emitter.layout_reg.header_size(super::engine::layout::STRING) as i32);
        i32_add;
        call(emitter.rt.alloc);
        local_set(6);
    });

    // mem32[$result+0] = $len, mem32[$result+4] = $len (cap = len)
    wasm!(f, {
        local_get(6);
        local_get(5);
        i32_store(0);
        local_get(6);
        local_get(5);
        i32_store(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32, 0);
    });

    // memcpy: copy $len bytes from $start to $result+string_data_off()
    wasm!(f, {
        i32_const(0);
        local_set(7);
        block_empty;
        loop_empty;
        local_get(7);
        local_get(5);
        i32_ge_u;
        br_if(1);
    });
    // mem[$result + string_data_off() + $i] = mem[$start + $i]
    wasm!(f, {
        local_get(6);
        i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
        i32_add;
        local_get(7);
        i32_add;
        local_get(4);
        local_get(7);
        i32_add;
        i32_load8_u(0);
        i32_store8(0);
        local_get(7);
        i32_const(1);
        i32_add;
        local_set(7);
        br(0);
        end;
        end;
    });

    // return $result
    wasm!(f, { local_get(6); end; });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.int_to_string, type_idx, f));
}

/// __println_int(n: i64)
/// Convenience: int_to_string then println_str.
fn compile_println_int(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.println_int];
    let mut f = Function::new([]);

    wasm!(f, {
        local_get(0);
        call(emitter.rt.int_to_string);
        call(emitter.rt.println_str);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.println_int, type_idx, f));
}
