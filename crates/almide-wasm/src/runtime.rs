//! The emitted-wasm runtime helpers — every one is BUILT structurally
//! with wasm-encoder, never templated.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::*;

// ── emitted runtime helpers ─────────────────────────────────────────────

/// `$*_block(base: i32)`: derive (payload, len) from the layout and call
/// the given host import — the ONLY place a block is unpacked for printing.
pub(crate) fn emit_block_print(import: u32) -> Function {
    let mut f = Function::new([]);
    f.instructions()
        .local_get(0)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add() // payload ptr
        .local_get(0)
        .i32_load(len_memarg()) // len from the header
        .call(import)
        .end();
    f
}

/// `$append_copy(cur: i32, src: i32, len: i32) -> i32`: memory.copy bytes
/// to the cursor, return the advanced cursor. Traps LOUDLY when the write
/// would leave the line buffer (never corrupts the heap behind it).
pub(crate) fn emit_append_copy() -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(0).local_get(2).i32_add().global_get(G_LINE_END).i32_gt_u().if_(BlockType::Empty);
    i.unreachable();
    i.end();
    i.local_get(0)
        .local_get(1)
        .local_get(2)
        .memory_copy(0, 0)
        .local_get(0)
        .local_get(2)
        .i32_add()
        .end();
    f
}

/// `$buf_to_block(start: i32, cur: i32) -> i32`: capture a finished
/// line-buffer build as a REAL layout block (value-position `"${...}"`).
pub(crate) fn emit_buf_to_block() -> Function {
    // params: 0=start i32, 1=cur i32; locals: 2=len i32, 3=base i32
    let (start, cur, len, bbase) = (0u32, 1u32, 2u32, 3u32);
    let payload = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(cur).local_get(start).i32_sub().local_set(len);
    i.local_get(len).call(F_ALLOC).local_set(bbase);
    i.local_get(bbase).i32_const(payload).i32_add();
    i.local_get(start);
    i.local_get(len);
    i.memory_copy(0, 0);
    i.local_get(bbase);
    i.end();
    f
}

/// `$str_len_chars(base: i32) -> i64`: codepoint count — the oracle's
/// `string.len` is `chars().count()`, i.e. bytes that are NOT UTF-8
/// continuation bytes (`b & 0xC0 != 0x80`).
pub(crate) fn emit_str_len_chars() -> Function {
    // params: 0=base i32; locals: 1=i i32, 2=blen i32, 3=n i64
    let (bbase, idx, blen, n) = (0u32, 1u32, 2u32, 3u32);
    let byte = MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 };
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();
    i.local_get(bbase).i32_load(len_memarg()).local_set(blen);
    i.i32_const(0).local_set(idx);
    i.i64_const(0).local_set(n);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(idx).local_get(blen).i32_ge_u().br_if(1);
    i.local_get(bbase).local_get(idx).i32_add().i32_load8_u(byte);
    i.i32_const(0xC0).i32_and().i32_const(0x80).i32_ne().if_(BlockType::Empty);
    i.local_get(n).i64_const(1).i64_add().local_set(n);
    i.end();
    i.local_get(idx).i32_const(1).i32_add().local_set(idx);
    i.br(0).end().end();
    i.local_get(n);
    i.end();
    f
}

/// `$itoa(v: i64) -> i32`: decimal-render `v` into the scratch region
/// (digits back-to-front, ending at ITOA_END) and return the byte length —
/// the rendering starts at `ITOA_END - len`. Works in the NEGATIVE domain
/// so `i64::MIN` never overflows: `u = v < 0 ? v : -v`, digit =
/// `-(u % 10)`, and `u / 10` truncates toward zero so the loop terminates.
pub(crate) fn emit_itoa() -> Function {
    // params: 0=v i64; locals: 1=p i32, 2=u i64, 3=neg i32
    let (v, p, u, neg) = (0u32, 1u32, 2u32, 3u32);
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64), (1, ValType::I32)]);
    let byte = MemArg { offset: 0, align: 0, memory_index: 0 };
    let mut i = f.instructions();
    // p = ITOA_END; neg = v < 0; u = neg ? v : 0 - v
    i.i32_const(ITOA_END as i32).local_set(p);
    i.local_get(v).i64_const(0).i64_lt_s().local_set(neg);
    i.local_get(neg).if_(BlockType::Empty);
    i.local_get(v).local_set(u);
    i.else_();
    i.i64_const(0).local_get(v).i64_sub().local_set(u);
    i.end();
    // do-while: write digits back-to-front (always at least one → renders 0)
    i.loop_(BlockType::Empty);
    i.local_get(p).i32_const(1).i32_sub().local_set(p);
    i.local_get(p);
    i.i64_const(i64::from(b'0'));
    i.i64_const(0).local_get(u).i64_const(10).i64_rem_s().i64_sub(); // -(u%10) ∈ 0..=9
    i.i64_add().i32_wrap_i64().i32_store8(byte);
    i.local_get(u).i64_const(10).i64_div_s().local_set(u);
    i.local_get(u).i64_const(0).i64_ne().br_if(0);
    i.end();
    // sign
    i.local_get(neg).if_(BlockType::Empty);
    i.local_get(p).i32_const(1).i32_sub().local_set(p);
    i.local_get(p).i32_const(i32::from(b'-')).i32_store8(byte);
    i.end();
    // return ITOA_END - p
    i.i32_const(ITOA_END as i32).local_get(p).i32_sub();
    i.end();
    f
}

/// `$append_i64(cur: i32, v: i64) -> i32`: itoa then copy to the cursor;
/// returns the advanced cursor.
pub(crate) fn emit_append_i64() -> Function {
    // params: 0=cur i32, 1=v i64; locals: 2=len i32
    let (cur, v, len) = (0u32, 1u32, 2u32);
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).call(F_ITOA).local_set(len);
    i.local_get(cur);
    i.i32_const(ITOA_END as i32).local_get(len).i32_sub(); // src
    i.local_get(len);
    i.memory_copy(0, 0);
    i.local_get(cur).local_get(len).i32_add();
    i.end();
    f
}

/// `$alloc(len: i32) -> i32`: bump-allocate a layout-true block (header
/// rc=1/len/cap=len + payload), growing memory when needed; returns the
/// block BASE. Blocks are never freed in this slice.
pub(crate) fn emit_alloc() -> Function {
    // params: 0=len i32; locals: 1=base i32, 2=next i32
    let (len, base, next) = (0u32, 1u32, 2u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
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
    // if next > memory.size * 64Ki: grow just enough; a failed grow traps.
    i.local_get(next).memory_size(0).i32_const(16).i32_shl().i32_gt_u().if_(BlockType::Empty);
    i.local_get(next)
        .memory_size(0)
        .i32_const(16)
        .i32_shl()
        .i32_sub()
        .i32_const(65535)
        .i32_add()
        .i32_const(16)
        .i32_shr_u()
        .memory_grow(0)
        .i32_const(0)
        .i32_lt_s()
        .if_(BlockType::Empty)
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

/// `$int_to_string(v: i64) -> i32`: itoa into the scratch, then a fresh
/// layout block holding the rendering; returns the block BASE.
pub(crate) fn emit_int_to_string() -> Function {
    // params: 0=v i64; locals: 1=len i32, 2=base i32
    let (v, len, base) = (0u32, 1u32, 2u32);
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).call(F_ITOA).local_set(len);
    i.local_get(len).call(F_ALLOC).local_set(base);
    i.local_get(base).i32_const(almide_layout::PAYLOAD as i32).i32_add(); // dst
    i.i32_const(ITOA_END as i32).local_get(len).i32_sub(); // src
    i.local_get(len);
    i.memory_copy(0, 0);
    i.local_get(base);
    i.end();
    f
}

/// `$concat(a: i32, b: i32) -> i32`: fresh block holding a's bytes then
/// b's bytes; returns the block BASE.
pub(crate) fn emit_concat() -> Function {
    // params: 0=a i32, 1=b i32; locals: 2=la i32, 3=lb i32, 4=base i32
    let (a, b, la, lb, base) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let payload = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(a).i32_load(len_memarg()).local_set(la);
    i.local_get(b).i32_load(len_memarg()).local_set(lb);
    i.local_get(la).local_get(lb).i32_add().call(F_ALLOC).local_set(base);
    // copy a
    i.local_get(base).i32_const(payload).i32_add();
    i.local_get(a).i32_const(payload).i32_add();
    i.local_get(la);
    i.memory_copy(0, 0);
    // copy b (dst offset by la)
    i.local_get(base).i32_const(payload).i32_add().local_get(la).i32_add();
    i.local_get(b).i32_const(payload).i32_add();
    i.local_get(lb);
    i.memory_copy(0, 0);
    i.local_get(base);
    i.end();
    f
}

/// `$append_bool(cur: i32, b: i32) -> i32`: append `"true"`/`"false"`
/// (interned pool blocks) at the cursor, return the advanced cursor.
pub(crate) fn emit_append_bool(true_base: u32, false_base: u32) -> Function {
    let payload = |base: u32| (base + almide_layout::PAYLOAD) as i32;
    let mut f = Function::new([]);
    f.instructions()
        .local_get(0)
        .i32_const(payload(true_base))
        .i32_const(payload(false_base))
        .local_get(1)
        .select()
        .i32_const("true".len() as i32)
        .i32_const("false".len() as i32)
        .local_get(1)
        .select()
        .call(F_APPEND_COPY)
        .end();
    f
}

/// `$str_eq(a: i32, b: i32) -> i32`: byte equality of two blocks.
pub(crate) fn emit_str_eq() -> Function {
    // params: 0=a i32, 1=b i32; locals: 2=la i32, 3=i i32
    let (a, b, la, idx) = (0u32, 1u32, 2u32, 3u32);
    let byte = MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 };
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(a).i32_load(len_memarg()).local_set(la);
    i.local_get(la).local_get(b).i32_load(len_memarg()).i32_ne().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.i32_const(0).local_set(idx);
    i.loop_(BlockType::Empty);
    i.local_get(idx).local_get(la).i32_ge_u().if_(BlockType::Empty);
    i.i32_const(1).return_();
    i.end();
    i.local_get(a).local_get(idx).i32_add().i32_load8_u(byte);
    i.local_get(b).local_get(idx).i32_add().i32_load8_u(byte);
    i.i32_ne().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.local_get(idx).i32_const(1).i32_add().local_set(idx);
    i.br(0);
    i.end();
    i.unreachable();
    i.end();
    f
}

/// `$list_get_{8,4}(list: i32, idx: i64) -> i32`: `list.get` — a fresh
/// `some` block holding element `idx`, or NULL_ADDR when out of bounds.
pub(crate) fn emit_list_get(s: Scalar) -> Function {
    // params: 0=list i32, 1=idx i64; locals: 2=base i32
    let (list, idx, base) = (0u32, 1u32, 2u32);
    let stride = s.slot_size();
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    // out of bounds (idx < 0 or idx >= count) → none
    i.local_get(idx).i64_const(0).i64_lt_s();
    i.local_get(idx);
    i.local_get(list).i32_load(len_memarg()).i32_const(stride as i32).i32_div_u();
    i.i64_extend_i32_u().i64_ge_s();
    i.i32_or().if_(BlockType::Empty);
    i.i32_const(almide_layout::NULL_ADDR as i32).return_();
    i.end();
    // some(element)
    i.i32_const(stride as i32).call(F_ALLOC).local_set(base);
    i.local_get(base);
    i.local_get(list).i64_extend_i32_u().local_get(idx).i64_const(i64::from(stride)).i64_mul().i64_add().i32_wrap_i64();
    match s {
        Scalar::Int => i.i64_load(slot_memarg(almide_layout::OPTION_FIELD)),
        _ => i.i32_load(slot_memarg(almide_layout::OPTION_FIELD)),
    };
    match s {
        Scalar::Int => i.i64_store(slot_memarg(almide_layout::OPTION_FIELD)),
        _ => i.i32_store(slot_memarg(almide_layout::OPTION_FIELD)),
    };
    i.local_get(base);
    i.end();
    f
}

/// `$list_push_{8,4}(list: i32, v) -> i32`: append with AMORTIZED growth.
/// In-place when `cap - len >= stride` (sound because every List bind/
/// assign deep-copies — a local's block is uniquely its own, and the
/// checker only lets `push` target mut vars); otherwise a fresh block
/// with doubled capacity. Returns the (possibly new) base for write-back.
pub(crate) fn emit_list_push(s: Scalar) -> Function {
    // params: 0=list i32, 1=v; locals: 2=la i32, 3=cap i32, 4=base i32
    let (list, v, la, cap, base) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let stride = s.slot_size();
    let payload = almide_layout::PAYLOAD as i32;
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(list).i32_load(len_memarg()).local_set(la);
    i.local_get(list).i32_load(word(almide_layout::CAP.offset)).local_set(cap);
    // fast path: room in cap → store at len, bump len, return same block
    i.local_get(cap).local_get(la).i32_sub().i32_const(stride as i32).i32_ge_u().if_(BlockType::Empty);
    i.local_get(list).local_get(la).i32_add();
    i.local_get(v);
    match s {
        Scalar::Int => i.i64_store(slot_memarg(0)),
        _ => i.i32_store(slot_memarg(0)),
    };
    i.local_get(list).local_get(la).i32_const(stride as i32).i32_add().i32_store(word(almide_layout::LEN.offset));
    i.local_get(list).return_();
    i.end();
    // grow: newcap = max(cap * 2, 4 * stride)
    i.local_get(cap).i32_const(1).i32_shl().local_set(cap);
    i.local_get(cap).i32_const((4 * stride) as i32).i32_lt_u().if_(BlockType::Empty);
    i.i32_const((4 * stride) as i32).local_set(cap);
    i.end();
    i.local_get(cap).call(F_ALLOC).local_set(base); // len = cap = newcap for now
    i.local_get(base).i32_const(payload).i32_add();
    i.local_get(list).i32_const(payload).i32_add();
    i.local_get(la);
    i.memory_copy(0, 0);
    i.local_get(base).local_get(la).i32_add();
    i.local_get(v);
    match s {
        Scalar::Int => i.i64_store(slot_memarg(0)),
        _ => i.i32_store(slot_memarg(0)),
    };
    // live len = old len + stride (cap field keeps newcap from $alloc)
    i.local_get(base).local_get(la).i32_const(stride as i32).i32_add().i32_store(word(almide_layout::LEN.offset));
    i.local_get(base);
    i.end();
    f
}

/// `$block_copy(src: i32) -> i32`: a fresh block with src's live bytes —
/// the deep copy behind List value semantics at every bind/assign.
pub(crate) fn emit_block_copy() -> Function {
    // params: 0=src i32; locals: 1=len i32, 2=base i32
    let (src, len, base) = (0u32, 1u32, 2u32);
    let payload = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(src).i32_load(len_memarg()).local_set(len);
    i.local_get(len).call(F_ALLOC).local_set(base);
    i.local_get(base).i32_const(payload).i32_add();
    i.local_get(src).i32_const(payload).i32_add();
    i.local_get(len);
    i.memory_copy(0, 0);
    i.local_get(base);
    i.end();
    f
}

/// `$list_join(list: i32, sep: i32) -> i32`: join a List[String]'s blocks
/// with `sep` — repeated `$concat` (quadratic, fine for fixture scale).
pub(crate) fn emit_list_join() -> Function {
    // params: 0=list i32, 1=sep i32; locals: 2=n i32, 3=i i32, 4=acc i32
    let (list, sep, n, idx, acc) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(list).i32_load(len_memarg()).i32_const(4).i32_div_u().local_set(n);
    i.i32_const(0).call(F_ALLOC).local_set(acc); // ""
    i.i32_const(0).local_set(idx);
    i.loop_(BlockType::Empty);
    i.local_get(idx).local_get(n).i32_ge_u().if_(BlockType::Empty);
    i.local_get(acc).return_();
    i.end();
    i.local_get(idx).i32_const(0).i32_ne().if_(BlockType::Empty);
    i.local_get(acc).local_get(sep).call(F_CONCAT).local_set(acc);
    i.end();
    i.local_get(acc);
    i.local_get(list).local_get(idx).i32_const(4).i32_mul().i32_add().i32_load(slot_memarg(0));
    i.call(F_CONCAT).local_set(acc);
    i.local_get(idx).i32_const(1).i32_add().local_set(idx);
    i.br(0);
    i.end();
    i.unreachable();
    i.end();
    f
}
