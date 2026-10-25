// `include!`d part of wasi_p3.rs: the URL's userinfo on the p3 http shim
// (C-328, #4205). rt-core never sends the userinfo in the authority: it
// strips everything up to the authority's last `@` and, when that part is
// not empty, sends `Authorization: Basic <base64 of user:password>` (the
// user and password percent-decoded, `user:` when there is no `:`) unless
// the caller sent an `Authorization` of its own (any case). The shim does
// the same, so a URL with userinfo dials the same host and carries the same
// field value on native, the embedded lane and this component.

/// The shim's userinfo locals (past `shim_http`'s fixed map).
#[derive(Clone, Copy)]
struct AuthLocals {
    ui_ptr: u32,
    ui_len: u32,
    j: u32,
    b: u32,
    v: u32,
    w: u32,
    dec: u32,
    dlen: u32,
    enc: u32,
    flag: u32,
}

/// Strip the userinfo from the authority span (`auth_ptr`, `auth_len`), as
/// rt-core's `http_parse_url` reads it: the authority ends at the first `/`
/// (the shim's span), `?` or `#`, and the userinfo is what precedes its LAST
/// `@`. Leaves (`ui_ptr`, `ui_len`) on the userinfo (`ui_len` 0 when there
/// is none) and the authority past the `@`.
fn http_strip_userinfo(i: &mut wasm_encoder::InstructionSink<'_>, (auth_ptr, auth_len): (u32, u32), l: AuthLocals) {
    let AuthLocals { ui_ptr, ui_len, j, b, .. } = l;
    i.i32_const(0).local_set(ui_len);
    i.block(BlockType::Empty);
    // j = the first `?` or `#` in the span, or its end.
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(auth_len).i32_ge_u().br_if(1);
    i.local_get(auth_ptr).local_get(j).i32_add().i32_load8_u(mem8(0)).local_set(b);
    i.local_get(b).i32_const('?' as i32).i32_eq();
    i.local_get(b).i32_const('#' as i32).i32_eq().i32_or().br_if(1);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    // j = the last `@` before it; none leaves the authority as it is.
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).i32_eqz().br_if(2);
    i.local_get(j).i32_const(1).i32_sub().local_set(j);
    i.local_get(auth_ptr).local_get(j).i32_add().i32_load8_u(mem8(0)).i32_const('@' as i32).i32_eq().br_if(1);
    i.br(0).end().end();
    i.local_get(auth_ptr).local_set(ui_ptr);
    i.local_get(j).local_set(ui_len);
    i.local_get(auth_ptr).local_get(j).i32_add().i32_const(1).i32_add().local_set(auth_ptr);
    i.local_get(auth_len).local_get(j).i32_sub().i32_const(1).i32_sub().local_set(auth_len);
    i.end();
}

/// `hex` = the value of the hex digit at `at` (a local address), or -1.
fn http_hex_value(i: &mut wasm_encoder::InstructionSink<'_>, at: u32, hex: u32) {
    i.i32_const(-1).local_set(hex);
    i.local_get(at).i32_load8_u(mem8(0)).i32_const('0' as i32).i32_sub().i32_const(10).i32_lt_u();
    i.if_(BlockType::Empty);
    i.local_get(at).i32_load8_u(mem8(0)).i32_const('0' as i32).i32_sub().local_set(hex);
    i.end();
    i.local_get(at).i32_load8_u(mem8(0)).i32_const(0x20).i32_or().i32_const('a' as i32).i32_sub().i32_const(6).i32_lt_u();
    i.if_(BlockType::Empty);
    i.local_get(at).i32_load8_u(mem8(0)).i32_const(0x20).i32_or().i32_const(87).i32_sub().local_set(hex);
    i.end();
}

/// Whether the framed request's headers (`hdr_ptr..frame_end`, ops 48..=50
/// only) name `Authorization` in any case: `flag` = 1 when they do.
#[allow(clippy::too_many_arguments)]
fn http_caller_sends_authorization(
    i: &mut wasm_encoder::InstructionSink<'_>,
    t: &HttpErrTexts,
    op: u32,
    (hdr_ptr, frame_end): (u32, u32),
    (cur, cell_len, tmp, digit): (u32, u32, u32, u32),
    l: AuthLocals,
) {
    let AuthLocals { j, flag, .. } = l;
    let want = t.auth_lower;
    i.i32_const(0).local_set(flag);
    i.local_get(op).i32_const(48).i32_ge_s().if_(BlockType::Empty);
    i.local_get(hdr_ptr).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).local_get(frame_end).i32_ge_u().br_if(1);
    http_frame_cell(i, cur, cell_len, frame_end, tmp, digit);
    i.local_get(cur).local_get(tmp).i32_sub().i32_const(want.len).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).i32_const(want.len).i32_ge_u().if_(BlockType::Empty);
    i.i32_const(1).local_set(flag);
    i.br(2);
    i.end();
    i.local_get(tmp).local_get(j).i32_add().i32_load8_u(mem8(0)).i32_const(0x20).i32_or();
    i.i32_const(want.at).local_get(j).i32_add().i32_load8_u(mem8(0));
    i.i32_ne().br_if(1);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.end();
    i.local_get(cur).local_get(frame_end).i32_ge_u().br_if(1);
    http_frame_cell(i, cur, cell_len, frame_end, tmp, digit);
    i.br(0).end().end();
    i.end();
}

/// Append rt-core's `Authorization: Basic …` for a non-empty userinfo
/// (`ui_ptr`, `ui_len`) to `headers`, unless `flag` says the caller sent
/// one: the userinfo percent-decoded as `http_percent_decode_bytes` decodes
/// (a `%` and two hex digits), `:` appended when it holds none, then base64
/// (RFC 4648, padded). A host that refuses the field is ignored, as the
/// content-length append is.
fn http_append_authorization(
    i: &mut wasm_encoder::InstructionSink<'_>,
    t: &HttpErrTexts,
    (park, f_alloc, headers): (u64, u32, u32),
    l: AuthLocals,
) {
    let AuthLocals { ui_ptr, ui_len, j, b, v, w, dec, dlen, enc, flag } = l;
    i.local_get(ui_len).i32_const(0).i32_gt_s().local_get(flag).i32_eqz().i32_and().if_(BlockType::Empty);
    // ── percent-decode into `dec`; `flag` now records a raw `:` ──
    i.i32_const(0).i32_const(0).i32_const(1).local_get(ui_len).i32_const(1).i32_add().call(f_alloc).local_set(dec);
    i.i32_const(0).local_set(dlen);
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(ui_len).i32_ge_u().br_if(1);
    i.local_get(ui_ptr).local_get(j).i32_add().local_set(w);
    i.local_get(w).i32_load8_u(mem8(0)).local_set(b);
    i.local_get(b).i32_const(':' as i32).i32_eq().if_(BlockType::Empty);
    i.i32_const(1).local_set(flag);
    i.end();
    i.local_get(b).i32_const('%' as i32).i32_eq().local_get(j).i32_const(2).i32_add().local_get(ui_len).i32_lt_u().i32_and();
    i.if_(BlockType::Empty);
    i.local_get(w).i32_const(1).i32_add().local_set(w);
    http_hex_value(i, w, v);
    i.local_get(w).i32_const(1).i32_add().local_set(w);
    http_hex_value(i, w, b);
    i.local_get(v).i32_const(0).i32_ge_s().local_get(b).i32_const(0).i32_ge_s().i32_and().if_(BlockType::Empty);
    i.local_get(dec).local_get(dlen).i32_add().local_get(v).i32_const(16).i32_mul().local_get(b).i32_add().i32_store8(mem8(0));
    i.local_get(dlen).i32_const(1).i32_add().local_set(dlen);
    i.local_get(j).i32_const(3).i32_add().local_set(j);
    i.br(2); // the next byte: inner if, outer if, then the loop
    i.end();
    i.i32_const('%' as i32).local_set(b);
    i.end();
    i.local_get(dec).local_get(dlen).i32_add().local_get(b).i32_store8(mem8(0));
    i.local_get(dlen).i32_const(1).i32_add().local_set(dlen);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.local_get(flag).i32_eqz().if_(BlockType::Empty);
    i.local_get(dec).local_get(dlen).i32_add().i32_const(':' as i32).i32_store8(mem8(0));
    i.local_get(dlen).i32_const(1).i32_add().local_set(dlen);
    i.end();
    // ── `Basic ` then base64 into `enc` ──
    i.i32_const(0).i32_const(0).i32_const(1);
    i.local_get(dlen).i32_const(2).i32_add().i32_const(3).i32_div_u().i32_const(4).i32_mul().i32_const(t.basic.len).i32_add();
    i.call(f_alloc).local_tee(enc).local_set(w);
    i.local_get(w).i32_const(t.basic.at).i32_const(t.basic.len).memory_copy(0, 0);
    i.local_get(w).i32_const(t.basic.len).i32_add().local_set(w);
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(dlen).i32_ge_u().br_if(1);
    i.local_get(dlen).local_get(j).i32_sub().local_set(b);
    i.local_get(dec).local_get(j).i32_add().local_set(flag);
    i.local_get(flag).i32_load8_u(mem8(0)).i32_const(16).i32_shl().local_set(v);
    i.local_get(b).i32_const(1).i32_gt_u().if_(BlockType::Empty);
    i.local_get(v).local_get(flag).i32_load8_u(mem8(1)).i32_const(8).i32_shl().i32_or().local_set(v);
    i.end();
    i.local_get(b).i32_const(2).i32_gt_u().if_(BlockType::Empty);
    i.local_get(v).local_get(flag).i32_load8_u(mem8(2)).i32_or().local_set(v);
    i.end();
    let digit = |i: &mut wasm_encoder::InstructionSink<'_>, shift: i32| {
        i.local_get(v).i32_const(shift).i32_shr_u().i32_const(63).i32_and().i32_const(t.b64).i32_add().i32_load8_u(mem8(0));
    };
    for (off, shift, need) in [(0u64, 18, 0), (1, 12, 0), (2, 6, 1), (3, 0, 2)] {
        i.local_get(w);
        digit(i, shift);
        if need > 0 {
            i.i32_const('=' as i32);
            i.local_get(b).i32_const(need).i32_gt_u();
            i.select();
        }
        i.i32_store8(mem8(off));
    }
    i.local_get(w).i32_const(4).i32_add().local_set(w);
    i.local_get(j).i32_const(3).i32_add().local_set(j);
    i.br(0).end().end();
    i.local_get(headers);
    i.i32_const(t.auth_name.at).i32_const(t.auth_name.len);
    i.local_get(enc).local_get(w).local_get(enc).i32_sub();
    i.i32_const((park + RET) as i32);
    i.call(I_HTTP_FIELDS_APPEND);
    i.end();
}
