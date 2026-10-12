// The full-response family over wasi:http (op 65, ADR-0023 step 4): the
// p3 shim's answer is `<status>\n<hlen>\n`, then hlen CHARS of header cells
// (`<chars>\n<bytes>` for each name, then its value, in the order the
// response's fields list them), then the body. stdlib/http_framed.almd's
// `__request_response_impl` cuts it apart. The headers are copied out of the
// response BEFORE consume-body moves it; the prefix is written into the body
// buffer before the drain appends the body behind it. Lengths count chars the
// way `string.len` does: every byte that is not a UTF-8 continuation byte.

/// The scratch locals op 65 needs, beyond `shim_http`'s own.
#[derive(Clone, Copy)]
struct RespLocals {
    list: u32,
    n: u32,
    k: u32,
    entry: u32,
    hlen: u32,
    bytes: u32,
    v: u32,
    d: u32,
    s: u32,
    c: u32,
}

/// `out` = the chars in `[ptr, ptr + len)`; `k` is scratch.
fn resp_count_chars(i: &mut wasm_encoder::InstructionSink<'_>, ptr: u32, len: u32, out: u32, k: u32) {
    i.i32_const(0).local_set(out);
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(k).i32_add().i32_load8_u(mem8(0));
    i.i32_const(0xC0).i32_and().i32_const(0x80).i32_ne();
    i.local_get(out).i32_add().local_set(out);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
}

/// `d` = the decimal digits of `v` (≥ 1); `s` is scratch.
fn resp_digits(i: &mut wasm_encoder::InstructionSink<'_>, v: u32, d: u32, s: u32) {
    i.i32_const(1).local_set(d);
    i.local_get(v).local_set(s);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(s).i32_const(10).i32_lt_u().br_if(1);
    i.local_get(s).i32_const(10).i32_div_u().local_set(s);
    i.local_get(d).i32_const(1).i32_add().local_set(d);
    i.br(0).end().end();
}

/// Write `v` in decimal and a `\n` at `buf + pos`, advancing `pos`.
fn resp_write_dec_nl(i: &mut wasm_encoder::InstructionSink<'_>, buf: u32, pos: u32, v: u32, l: RespLocals) {
    resp_digits(i, v, l.d, l.s);
    i.local_get(v).local_set(l.s);
    i.local_get(l.d).local_set(l.c);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.c).i32_eqz().br_if(1);
    i.local_get(l.c).i32_const(1).i32_sub().local_set(l.c);
    i.local_get(buf).local_get(pos).i32_add().local_get(l.c).i32_add();
    i.local_get(l.s).i32_const(10).i32_rem_u().i32_const(48).i32_add();
    i.i32_store8(mem8(0));
    i.local_get(l.s).i32_const(10).i32_div_u().local_set(l.s);
    i.br(0).end().end();
    i.local_get(buf).local_get(pos).i32_add().local_get(l.d).i32_add().i32_const(10).i32_store8(mem8(0));
    i.local_get(pos).local_get(l.d).i32_add().i32_const(1).i32_add().local_set(pos);
}

/// Before consume-body: copy the response's header fields out (list, n) and
/// drop the fields handle. `ret` is a free retptr slot.
fn resp_headers_copy(i: &mut wasm_encoder::InstructionSink<'_>, response: u32, ret: i32, l: RespLocals) {
    i.local_get(response).call(I_HTTP_RESP_HEADERS).local_set(l.v);
    i.local_get(l.v).i32_const(ret).call(I_HTTP_COPY_ALL);
    i.i32_const(ret).i32_load(mem(0)).local_set(l.list);
    i.i32_const(ret).i32_load(mem(4)).local_set(l.n);
    i.local_get(l.v).call(I_HTTP_FIELDS_DROP);
}

/// Each cell of entry `l.entry`: name at +0/+4, value at +8/+12.
const RESP_CELLS: [(u64, u64); 2] = [(0, 4), (8, 12)];

/// After the body buffer exists: write `<status>\n<hlen>\n<cells>` at its
/// start (growing it first if the prefix exceeds `cap`), leaving `total` at
/// the prefix length. `(buf, cap, total)` are shim_http's body-buffer locals.
fn resp_write_prefix(i: &mut wasm_encoder::InstructionSink<'_>, f_alloc: u32, status: u32, (buf, cap, total): (u32, u32, u32), l: RespLocals) {
    // hlen in chars, and the bytes the cells take.
    i.i32_const(0).local_set(l.hlen);
    i.i32_const(0).local_set(l.bytes);
    i.i32_const(0).local_set(l.k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.k).local_get(l.n).i32_ge_u().br_if(1);
    i.local_get(l.list).local_get(l.k).i32_const(16).i32_mul().i32_add().local_set(l.entry);
    for (p, n) in RESP_CELLS {
        i.local_get(l.entry).i32_load(mem(p)).local_set(l.s);
        i.local_get(l.entry).i32_load(mem(n)).local_set(l.c);
        resp_count_chars(i, l.s, l.c, l.v, l.d);
        resp_digits(i, l.v, l.d, l.s);
        i.local_get(l.hlen).local_get(l.d).i32_add().i32_const(1).i32_add().local_get(l.v).i32_add().local_set(l.hlen);
        i.local_get(l.bytes).local_get(l.d).i32_add().i32_const(1).i32_add();
        i.local_get(l.entry).i32_load(mem(n)).i32_add().local_set(l.bytes);
    }
    i.local_get(l.k).i32_const(1).i32_add().local_set(l.k);
    i.br(0).end().end();
    // The two decimal lines take at most 24 bytes; grow past the prefix.
    i.local_get(l.bytes).i32_const(24).i32_add().local_get(cap).i32_ge_u().if_(BlockType::Empty);
    i.local_get(buf).local_get(cap).i32_const(8);
    i.local_get(l.bytes).i32_const(24 + 65536).i32_add().call(f_alloc).local_set(buf);
    i.local_get(l.bytes).i32_const(24 + 65536).i32_add().local_set(cap);
    i.end();
    i.i32_const(0).local_set(total);
    resp_write_dec_nl(i, buf, total, status, l);
    resp_write_dec_nl(i, buf, total, l.hlen, l);
    i.i32_const(0).local_set(l.k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.k).local_get(l.n).i32_ge_u().br_if(1);
    for (p, n) in RESP_CELLS {
        i.local_get(l.list).local_get(l.k).i32_const(16).i32_mul().i32_add().local_set(l.entry);
        i.local_get(l.entry).i32_load(mem(p)).local_set(l.bytes);
        i.local_get(l.entry).i32_load(mem(n)).local_set(l.hlen);
        resp_count_chars(i, l.bytes, l.hlen, l.v, l.d);
        resp_write_dec_nl(i, buf, total, l.v, l);
        i.local_get(buf).local_get(total).i32_add().local_get(l.bytes).local_get(l.hlen).memory_copy(0, 0);
        i.local_get(total).local_get(l.hlen).i32_add().local_set(total);
    }
    i.local_get(l.k).i32_const(1).i32_add().local_set(l.k);
    i.br(0).end().end();
}

/// `out` = 1 when the request's method is GET or HEAD: op 43, or a framed op
/// (≥ 48) whose method cell spells either.
fn req_is_get_or_head(i: &mut wasm_encoder::InstructionSink<'_>, op: u32, (m_ptr, m_len): (u32, u32), out: u32) {
    i.local_get(op).i32_const(43).i32_eq().local_set(out);
    for name in [&b"GET"[..], &b"HEAD"[..]] {
        i.local_get(op).i32_const(48).i32_ge_s().local_get(m_len).i32_const(name.len() as i32).i32_eq().i32_and();
        i.if_(BlockType::Empty);
        for (k, ch) in name.iter().enumerate() {
            i.local_get(m_ptr).i32_load8_u(mem8(k as u64)).i32_const(*ch as i32).i32_eq();
            if k > 0 {
                i.i32_and();
            }
        }
        i.local_get(out).i32_or().local_set(out);
        i.end();
    }
}
