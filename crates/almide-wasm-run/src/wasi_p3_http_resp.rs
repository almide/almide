// The full-response family over wasi:http (op 65, ADR-0023 step 4): the
// p3 shim's answer is `<status>\n<hlen>\n`, then hlen CHARS of header cells
// (`<chars>\n<bytes>` for each name, then its value, in the order the
// response's fields list them), then the body. stdlib/http_framed.almd's
// `__request_response_impl` cuts it apart. The headers are copied out of the
// response BEFORE consume-body moves it; the prefix is written into the body
// buffer before the drain appends the body behind it. Lengths count chars the
// way `string.len` does: every byte that is not a UTF-8 continuation byte.
// A wasi:http field value is `list<u8>`, so each name and value is written
// the way native's client reads one, `String::from_utf8_lossy` (#4390), and
// counted over the decoded bytes — a cell that is not UTF-8 then reaches the
// program as the U+FFFD text native shows, and the guest's char cuts line up.
// An entry whose name is one of the fields the client manages (C-370's nine,
// the same table the request check reads) is left out: a stock host from
// wasmtime 49 on withholds them anyway, and native drops them too.

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

/// The lossy walk's scratch (#4390).
#[derive(Clone, Copy)]
struct LossyLocals {
    k: u32,
    b0: u32,
    extra: u32,
    lo: u32,
    hi: u32,
    vlen: u32,
    ok: u32,
    j: u32,
    c: u32,
    chars: u32,
}

/// Decode `[ptr, ptr + len)` as `String::from_utf8_lossy` does — the
/// Table 3-7 well-formed sequences kept, one U+FFFD per maximal invalid
/// subpart — leaving the decoded text's chars in `l.chars` and adding its
/// byte length to `bytes`. With `dst = Some((buf, pos))` the decoded bytes
/// are also written at `buf + pos`, advancing `pos`. At most 3 bytes out
/// per byte in.
fn resp_lossy(i: &mut wasm_encoder::InstructionSink<'_>, (ptr, len): (u32, u32), bytes: u32, dst: Option<(u32, u32)>, l: LossyLocals) {
    // One decoded unit of `n` bytes: count it, and write it when writing.
    let unit = |i: &mut wasm_encoder::InstructionSink<'_>, fffd: bool, n: u32| {
        i.local_get(l.chars).i32_const(1).i32_add().local_set(l.chars);
        if fffd {
            i.local_get(bytes).i32_const(3).i32_add().local_set(bytes);
        } else {
            i.local_get(bytes).local_get(n).i32_add().local_set(bytes);
        }
        if let Some((buf, pos)) = dst {
            if fffd {
                for (off, b) in [(0u64, 0xEF), (1, 0xBF), (2, 0xBD)] {
                    i.local_get(buf).local_get(pos).i32_add().i32_const(b).i32_store8(mem8(off));
                }
                i.local_get(pos).i32_const(3).i32_add().local_set(pos);
            } else {
                i.local_get(buf).local_get(pos).i32_add();
                i.local_get(ptr).local_get(l.k).i32_add();
                i.local_get(n).memory_copy(0, 0);
                i.local_get(pos).local_get(n).i32_add().local_set(pos);
            }
        }
    };
    i.i32_const(0).local_set(l.chars);
    i.i32_const(0).local_set(l.k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.k).local_get(len).i32_ge_u().br_if(1);
    i.local_get(ptr).local_get(l.k).i32_add().i32_load8_u(mem8(0)).local_set(l.b0);
    // vlen = the bytes this unit consumes (lead included).
    i.i32_const(1).local_set(l.vlen);
    i.local_get(l.b0).i32_const(0x80).i32_lt_u().if_(BlockType::Empty);
    unit(i, false, l.vlen);
    i.else_();
    i.i32_const(0).local_set(l.extra);
    i.i32_const(0x80).local_set(l.lo);
    i.i32_const(0xBF).local_set(l.hi);
    // (first lead, last lead, continuation count, second-byte lo, hi)
    for (a, b, extra, lo, hi) in [
        (0xC2, 0xDF, 1, 0x80, 0xBF),
        (0xE0, 0xE0, 2, 0xA0, 0xBF),
        (0xE1, 0xEC, 2, 0x80, 0xBF),
        (0xED, 0xED, 2, 0x80, 0x9F),
        (0xEE, 0xEF, 2, 0x80, 0xBF),
        (0xF0, 0xF0, 3, 0x90, 0xBF),
        (0xF1, 0xF3, 3, 0x80, 0xBF),
        (0xF4, 0xF4, 3, 0x80, 0x8F),
    ] {
        i.local_get(l.b0).i32_const(a).i32_ge_u().local_get(l.b0).i32_const(b).i32_le_u().i32_and();
        i.if_(BlockType::Empty);
        i.i32_const(extra).local_set(l.extra);
        i.i32_const(lo).local_set(l.lo);
        i.i32_const(hi).local_set(l.hi);
        i.end();
    }
    i.local_get(l.extra).i32_eqz().if_(BlockType::Empty);
    unit(i, true, l.vlen);
    i.else_();
    // Walk the continuations j = 1..=extra; vlen counts the valid ones.
    i.i32_const(1).local_set(l.ok);
    i.i32_const(1).local_set(l.j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.j).local_get(l.extra).i32_gt_u().br_if(1);
    i.local_get(l.k).local_get(l.j).i32_add().local_get(len).i32_ge_u().if_(BlockType::Empty);
    i.i32_const(0).local_set(l.ok);
    i.br(2);
    i.end();
    i.local_get(ptr).local_get(l.k).i32_add().local_get(l.j).i32_add().i32_load8_u(mem8(0)).local_set(l.c);
    i.local_get(l.c);
    i.local_get(l.lo).i32_const(0x80).local_get(l.j).i32_const(1).i32_eq().select();
    i.i32_lt_u();
    i.local_get(l.c);
    i.local_get(l.hi).i32_const(0xBF).local_get(l.j).i32_const(1).i32_eq().select();
    i.i32_gt_u();
    i.i32_or().if_(BlockType::Empty);
    i.i32_const(0).local_set(l.ok);
    i.br(2);
    i.end();
    i.local_get(l.vlen).i32_const(1).i32_add().local_set(l.vlen);
    i.local_get(l.j).i32_const(1).i32_add().local_set(l.j);
    i.br(0).end().end();
    i.local_get(l.ok).if_(BlockType::Empty);
    unit(i, false, l.vlen);
    i.else_();
    unit(i, true, l.vlen);
    i.end();
    i.end();
    i.end();
    i.local_get(l.k).local_get(l.vlen).i32_add().local_set(l.k);
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

/// Open an `if` that runs when entry `l.entry`'s name is not a managed field:
/// the header check (`check`) answers 3 for one, given an empty value.
fn resp_if_kept(i: &mut wasm_encoder::InstructionSink<'_>, check: u32, l: RespLocals) {
    i.local_get(l.entry).i32_load(mem(0)).local_get(l.entry).i32_load(mem(4));
    i.local_get(l.entry).i32_load(mem(0)).i32_const(0).call(check);
    i.i32_const(3).i32_ne().if_(BlockType::Empty);
}

/// After the body buffer exists: write `<status>\n<hlen>\n<cells>` at its
/// start (growing it first if the prefix exceeds `cap`), leaving `total` at
/// the prefix length. `(buf, cap, total)` are shim_http's body-buffer locals.
fn resp_write_prefix(i: &mut wasm_encoder::InstructionSink<'_>, (f_alloc, check): (u32, u32), status: u32, (buf, cap, total): (u32, u32, u32), l: RespLocals, ll: LossyLocals) {
    // hlen in chars, and the bytes the cells take.
    i.i32_const(0).local_set(l.hlen);
    i.i32_const(0).local_set(l.bytes);
    i.i32_const(0).local_set(l.k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.k).local_get(l.n).i32_ge_u().br_if(1);
    i.local_get(l.list).local_get(l.k).i32_const(16).i32_mul().i32_add().local_set(l.entry);
    resp_if_kept(i, check, l);
    for (p, n) in RESP_CELLS {
        i.local_get(l.entry).i32_load(mem(p)).local_set(l.s);
        i.local_get(l.entry).i32_load(mem(n)).local_set(l.c);
        // The decoded cell: ll.chars chars, l.v bytes.
        i.i32_const(0).local_set(l.v);
        resp_lossy(i, (l.s, l.c), l.v, None, ll);
        resp_digits(i, ll.chars, l.d, l.s);
        i.local_get(l.hlen).local_get(l.d).i32_add().i32_const(1).i32_add().local_get(ll.chars).i32_add().local_set(l.hlen);
        i.local_get(l.bytes).local_get(l.d).i32_add().i32_const(1).i32_add();
        i.local_get(l.v).i32_add().local_set(l.bytes);
    }
    i.end();
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
    i.local_get(l.list).local_get(l.k).i32_const(16).i32_mul().i32_add().local_set(l.entry);
    resp_if_kept(i, check, l);
    for (p, n) in RESP_CELLS {
        i.local_get(l.list).local_get(l.k).i32_const(16).i32_mul().i32_add().local_set(l.entry);
        i.local_get(l.entry).i32_load(mem(p)).local_set(l.bytes);
        i.local_get(l.entry).i32_load(mem(n)).local_set(l.hlen);
        resp_lossy(i, (l.bytes, l.hlen), l.v, None, ll);
        resp_write_dec_nl(i, buf, total, ll.chars, l);
        resp_lossy(i, (l.bytes, l.hlen), l.v, Some((buf, total)), ll);
    }
    i.end();
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
