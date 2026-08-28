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
    i.local_get(block).local_get(rc).i32_store(word(almide_layout::RC.offset));
    i.local_get(rc).i32_eqz().if_(BlockType::Empty);
    i.local_get(block).call(F_FREE);
    i.end();
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
            got, 0x71738094f6c49c05,
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
