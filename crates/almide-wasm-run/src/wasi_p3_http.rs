// `include!`d part of wasi_p3.rs (codopsy max-lines split, mechanical text move —
// shares the parent module's imports and items; nothing here is pub beyond the parent).

/// Retire the response body: drop its readable and the trailers future,
/// and write the handling result `ok` into the kept writable, then drop it.
fn http_body_retire(i: &mut wasm_encoder::InstructionSink<'_>, park: u64, (body_rx, trlfut, cb_tx): (u32, u32, u32), f_await: u32) {
    i.local_get(body_rx).call(I_HTTP_BODY_DROPR);
    i.local_get(trlfut).call(I_HTTP_TRL_DROPR);
    i.i32_const((park + RET) as i32).i64_const(0).i64_store(mem64(16));
    i.local_get(cb_tx);
    i.local_get(cb_tx).i32_const((park + RET + 16) as i32).call(I_HTTP_CB_FWRITE);
    i.call(f_await).drop();
    i.local_get(cb_tx).call(I_HTTP_CB_FDROPW);
}

/// Feed the request body (#1710 PR B): issue async stream-writes until the
/// write blocks (join the writable end to the exchange's waitable set and
/// mark pending) or the body is fully accepted, at which point the writable
/// end drops — the EOF the server needs before it will answer. Slot numbers
/// are shim_http's fixed local map.
fn http_body_pump(i: &mut wasm_encoder::InstructionSink<'_>) {
    let (b_ptr, b_len, str_tx) = (3u32, 4u32, 15u32);
    let (n, k, hws, str_pend) = (26u32, 27u32, 29u32, 31u32);
    i.i32_const(0).local_set(str_pend);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(b_len).i32_eqz().br_if(1);
    i.local_get(str_tx).local_get(b_ptr).local_get(b_len).call(I_HTTP_REQ_SWRITE).local_set(n);
    i.local_get(n).i32_const(-1).i32_eq().if_(BlockType::Empty);
    i.local_get(hws).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.call(I_WS_NEW).local_set(hws);
    i.end();
    i.local_get(str_tx).local_get(hws).call(I_WS_JOIN);
    i.i32_const(1).local_set(str_pend);
    i.end();
    i.local_get(str_pend).br_if(1);
    i.local_get(n).i32_const(4).i32_shr_u().local_set(k);
    i.local_get(b_ptr).local_get(k).i32_add().local_set(b_ptr);
    i.local_get(b_len).local_get(k).i32_sub().local_set(b_len);
    i.local_get(n).i32_const(15).i32_and().i32_const(1).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).local_set(b_len); // DROPPED: the reader closed the body
    i.end();
    i.br(0).end().end();
    i.local_get(str_pend).i32_eqz().if_(BlockType::Empty);
    i.local_get(str_tx).call(I_HTTP_REQ_SDROPW);
    i.end();
}

/// One http_framed cell at `cur`: `<decimal char count>\n<payload>`. On
/// exit `tmp` = the payload's first byte, `cur` = one past its last byte
/// (the next cell), the char walk having counted every byte that is not a
/// UTF-8 continuation (10xxxxxx). A malformed frame (no '\n', a count past
/// the end) stops at `frame_end`; this lane's guest builds the frame
/// itself (stdlib/http_framed.almd), so the shape holds by construction.
fn http_frame_cell(
    i: &mut wasm_encoder::InstructionSink<'_>,
    cur: u32,
    cell_len: u32,
    frame_end: u32,
    tmp: u32,
    digit: u32,
) {
    i.i32_const(0).local_set(cell_len);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).local_get(frame_end).i32_ge_u().br_if(1);
    i.local_get(cur).i32_load8_u(mem8(0)).local_set(digit);
    i.local_get(cur).i32_const(1).i32_add().local_set(cur);
    i.local_get(digit).i32_const(10).i32_eq().br_if(1);
    i.local_get(cell_len).i32_const(10).i32_mul();
    i.local_get(digit).i32_const(48).i32_sub().i32_add().local_set(cell_len);
    i.br(0).end().end();
    i.local_get(cur).local_set(tmp);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cell_len).i32_eqz().br_if(1);
    i.local_get(cur).local_get(frame_end).i32_ge_u().br_if(1);
    i.local_get(cur).i32_const(1).i32_add().local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).local_get(frame_end).i32_ge_u().br_if(1);
    i.local_get(cur).i32_load8_u(mem8(0)).i32_const(0xC0).i32_and().i32_const(0x80).i32_ne().br_if(1);
    i.local_get(cur).i32_const(1).i32_add().local_set(cur);
    i.br(0).end().end();
    i.local_get(cell_len).i32_const(1).i32_sub().local_set(cell_len);
    i.br(0).end().end();
}

/// The p3 http string client (#1710 PR B): serve fs_call ops 43..=47 over
/// `wasi:http/client@0.3.0`'s async-lowered `send`. Sequence (the recorded
/// blueprint): url parse in-shim (scheme prefix, authority to '/', path
/// rest); empty `fields`; the trailers future written `ok(none)` up front;
/// an optional contents stream fed by async writes while send is in flight
/// (the guest scheduler below — a body past the host's buffer
/// (`-S http-outgoing-body-buffer-chunks`) is the empirical fixture); the
/// sent-future's readable dropped; on `ok`, the response body drains
/// through the same realloc'd read loop the fs shim uses (each read
/// through `$await`), into the parked-result convention. Transport errors answer
/// `pack(1, len)` with the static E_HTTP text (host-specific wording is
/// bounded by contract — fixtures assert err-ness). The p3 stream delivers
/// the DECODED body, so no chunked handling exists here by design.
fn shim_http(g: P3Globals, h: &HttpAbi, t: &HttpErrTexts, fns: HttpErrFns) -> Function {
    let P3Globals { park, g_plen, g_ppos, f_alloc, f_await, .. } = g;
    // Emit-time bisect knob (#1710 PR B bring-up): ALMIDE_P3_HTTP_STOP=N
    // makes the shim answer the static err right after stage N, so a hang
    // localizes to the first stage whose stop-build still hangs. Stages:
    // 1 = fields+trailers written, 2 = request.new, 3 = setters,
    // 4 = body fed + writable dropped, 5 = sent-future dropped.
    let stop = almide_base::env::var("ALMIDE_P3_HTTP_STOP")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let (op, a_ptr, a_len, b_ptr, b_len) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let (sch, rest) = (5u32, 6u32);
    let (auth_ptr, auth_len, path_ptr, path_len) = (7u32, 8u32, 9u32, 10u32);
    let (headers, trl_rx, trl_tx, str_rx, str_tx) = (11u32, 12u32, 13u32, 14u32, 15u32);
    let (request, sentfut, response, cb_rx, cb_tx) = (16u32, 17u32, 18u32, 19u32, 20u32);
    let (body_rx, trlfut, buf, cap, total) = (21u32, 22u32, 23u32, 24u32, 25u32);
    let (n, k) = (26u32, 27u32);
    let s64 = 28u32;
    let (hws, trl_pend, str_pend, snd_done) = (29u32, 30u32, 31u32, 32u32);
    // The framed family (ops 48..=50, #1710): the frame's cells.
    let (m_ptr, m_len, cur, cell_len, frame_end, hdr_ptr, digit, tmp) =
        (33u32, 34u32, 35u32, 36u32, 37u32, 38u32, 39u32, 40u32);
    let (key_ptr, key_len) = (41u32, 42u32);
    // ADR-0023 step 2: the scheme check, the request options, the error
    // entry and the body limit.
    let (colon, valid, opts_some, pre, ent) = (43u32, 44u32, 45u32, 46u32, 47u32);
    let (secs, lim) = (48u32, 49u32);
    let mut f = Function::new([
        (23, ValType::I32),
        (1, ValType::I64),
        (14, ValType::I32),
        (5, ValType::I32),
        (2, ValType::I64),
    ]);
    let mut i = f.instructions();
    // ── ops 48..=50: parse the http_framed cell frame in `b` ──
    // `<len>\n<payload>` cells with CHAR-count lengths (string.len
    // semantics — a char starts at every byte that is not 10xxxxxx):
    // method, body, then key/value pairs to the end of the frame. The
    // method cell lands in (m_ptr, m_len), the body cell REPLACES
    // (b_ptr, b_len) so the contents pump below feeds it unchanged, and
    // hdr_ptr..frame_end is the header run the fields loop appends.
    i.i32_const(0).local_set(m_len);
    i.local_get(b_ptr).local_set(hdr_ptr);
    i.local_get(b_ptr).local_get(b_len).i32_add().local_set(frame_end);
    i.local_get(op).i32_const(48).i32_ge_s().if_(BlockType::Empty);
    i.local_get(b_ptr).local_set(cur);
    for which in 0..2u32 {
        http_frame_cell(&mut i, cur, cell_len, frame_end, tmp, digit);
        if which == 0 {
            i.local_get(tmp).local_set(m_ptr);
            i.local_get(cur).local_get(tmp).i32_sub().local_set(m_len);
        } else {
            i.local_get(tmp).local_set(b_ptr);
            i.local_get(cur).local_get(tmp).i32_sub().local_set(b_len);
        }
    }
    i.local_get(cur).local_set(hdr_ptr);
    i.end();

    // ── URL parse: the scheme as rt-core reads it (its three reasons are
    // answered here), authority to '/', path = rest ──
    let url_locals = UrlLocals { sch, rest, colon, valid, b: n, k, low: key_ptr };
    http_url_scheme(&mut i, h, t, fns, f_alloc, url_locals);
    i.local_get(a_ptr).local_get(rest).i32_add().local_set(auth_ptr);
    i.local_get(rest).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(a_len).i32_ge_u().br_if(1);
    i.local_get(a_ptr)
        .local_get(k)
        .i32_add()
        .i32_load8_u(mem8(0))
        .i32_const('/' as i32)
        .i32_eq()
        .br_if(1);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(k).local_get(rest).i32_sub().local_set(auth_len);
    i.local_get(k).local_get(a_len).i32_lt_u().if_(BlockType::Empty);
    i.local_get(a_ptr).local_get(k).i32_add().local_set(path_ptr);
    i.local_get(a_len).local_get(k).i32_sub().local_set(path_len);
    i.else_();
    i.i32_const((park + RET + 24) as i32).i32_const('/' as i32).i32_store8(mem8(0));
    i.i32_const((park + RET + 24) as i32).local_set(path_ptr);
    i.i32_const(1).local_set(path_len);
    i.end();

    // ── the framed family's headers, refused before anything exists ──
    http_check_headers(&mut i, t, fns, (hdr_ptr, frame_end), (cur, cell_len, tmp, digit), (key_ptr, key_len), n);

    // ── empty fields; trailers future written ok(none) up front ──
    i.call(I_HTTP_FIELDS_NEW).local_set(headers);
    // The framed family's headers: key/value cells appended in frame order
    // (the host lane's parse_http_frame order, so the wire carries the
    // same header sequence on both lanes). The checks above passed every
    // pair, so a header-error here is the host refusing a name it manages
    // beyond the nine: answered as the forbidden text, the fields dropped.
    i.local_get(op).i32_const(48).i32_ge_s().if_(BlockType::Empty);
    i.local_get(hdr_ptr).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).local_get(frame_end).i32_ge_u().br_if(1);
    http_frame_cell(&mut i, cur, cell_len, frame_end, tmp, digit);
    // The key span keeps its OWN locals: the URL parse ran ABOVE, so
    // parking it in auth_* clobbered the authority with the first header
    // NAME (`set-authority "x-probe"` → a DNS error on every framed
    // request that carried a header; #1924's "transport error").
    i.local_get(tmp).local_set(key_ptr);
    i.local_get(cur).local_get(tmp).i32_sub().local_set(key_len);
    i.local_get(cur).local_get(frame_end).i32_ge_u().br_if(1);
    http_frame_cell(&mut i, cur, cell_len, frame_end, tmp, digit);
    i.local_get(headers);
    i.local_get(key_ptr).local_get(key_len);
    i.local_get(tmp);
    i.local_get(cur).local_get(tmp).i32_sub();
    i.i32_const((park + RET) as i32);
    i.call(I_HTTP_FIELDS_APPEND);
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(0)).if_(BlockType::Empty);
    i.local_get(headers).call(I_HTTP_FIELDS_DROP);
    http_err_call(&mut i, fns, (key_ptr, key_len), t.hdr[2][0], t.hdr[2][1], None, Piece::default());
    i.end();
    i.br(0).end().end();
    i.end();
    // content-length for a non-empty body (decimal, back to front —
    // the op-49 status rendering's shape). A host that refuses the name
    // answers header-error, which is ignored like any other rejection.
    i.local_get(b_len).i32_const(0).i32_gt_s().if_(BlockType::Empty);
    i.local_get(b_len).local_set(tmp);
    i.i32_const(1).local_set(digit);
    i.local_get(tmp).local_set(cell_len);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cell_len).i32_const(10).i32_lt_u().br_if(1);
    i.local_get(cell_len).i32_const(10).i32_div_u().local_set(cell_len);
    i.local_get(digit).i32_const(1).i32_add().local_set(digit);
    i.br(0).end().end();
    i.local_get(digit).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).i32_eqz().br_if(1);
    i.local_get(cur).i32_const(1).i32_sub().local_set(cur);
    i.i32_const((park + CLEN_BUF) as i32).local_get(cur).i32_add();
    i.local_get(tmp).i32_const(10).i32_rem_u().i32_const(48).i32_add();
    i.i32_store8(mem8(0));
    i.local_get(tmp).i32_const(10).i32_div_u().local_set(tmp);
    i.br(0).end().end();
    i.local_get(headers);
    i.i32_const((park + MSG_CLEN) as i32).i32_const(E_CLEN.len() as i32);
    i.i32_const((park + CLEN_BUF) as i32).local_get(digit);
    i.i32_const((park + RET) as i32);
    i.call(I_HTTP_FIELDS_APPEND);
    i.end();
    if stop == 11 {
        fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    }
    i.call(I_HTTP_REQ_FNEW).local_set(s64);
    if stop == 12 {
        fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    }
    // tx = high half, rx = low half (the sync-streams.wast packing).
    i.local_get(s64).i64_const(32).i64_shr_u().i32_wrap_i64().local_set(trl_tx);
    i.local_get(s64).i32_wrap_i64().local_set(trl_rx);
    // ok(none): result disc 0 @0, option disc 0 @payload — 8 zeroed bytes.
    i.i32_const((park + RET) as i32).i64_const(0).i64_store(mem64(16));
    if stop == 14 {
        fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    }
    // The write is the ASYNC lower: the sync form parks this fiber on a
    // rendezvous whose reader (the host's request machinery) only appears
    // inside `send` — the stop=13 bring-up probe hung exactly there. On
    // BLOCKED the write-end joins a fresh waitable set and is retired at
    // the end of the exchange, when send has forced the host to read it.
    // The ok(none) buffer at park+RET+16 must stay intact until then.
    i.i32_const(-1).local_set(hws);
    i.i32_const(0).local_set(trl_pend);
    i.local_get(trl_tx).i32_const((park + RET + 16) as i32).call(I_HTTP_REQ_FWRITE).local_set(n);
    i.local_get(n).i32_const(-1).i32_eq().if_(BlockType::Empty);
    i.call(I_WS_NEW).local_set(hws);
    i.local_get(trl_tx).local_get(hws).call(I_WS_JOIN);
    i.i32_const(1).local_set(trl_pend);
    i.else_();
    i.local_get(trl_tx).call(I_HTTP_REQ_FDROPW);
    i.end();
    if stop == 13 {
        fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    }
    if stop == 1 {
        fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    }

    // ── optional contents stream ──
    i.i32_const(-1).local_set(str_tx);
    i.i32_const(0).local_set(str_rx);
    i.local_get(b_len).i32_const(0).i32_gt_s().if_(BlockType::Empty);
    i.call(I_HTTP_REQ_SNEW).local_set(s64);
    i.local_get(s64).i64_const(32).i64_shr_u().i32_wrap_i64().local_set(str_tx);
    i.local_get(s64).i32_wrap_i64().local_set(str_rx);
    i.end();

    // ── request options: ALMIDE_HTTP_TIMEOUT_SECS (default 30, 0 = none)
    // as the connect, first-byte and between-bytes timeouts — rt-core's
    // connect and read timeouts. Each setter's result lands in SENDRET,
    // which is free until send. ──
    i.i32_const(t.key_timeout.at).i32_const(t.key_timeout.len).i64_const(30).call(fns.num).local_set(secs);
    i.i32_const(0).local_set(opts_some);
    i.i32_const(0).local_set(ent);
    i.local_get(secs).i64_const(0).i64_gt_s().if_(BlockType::Empty);
    i.local_get(secs).i64_const(18_000_000_000).i64_gt_s().if_(BlockType::Empty);
    i.i64_const(18_000_000_000).local_set(secs);
    i.end();
    i.call(I_HTTP_OPT_NEW).local_set(ent);
    for setter in [I_HTTP_OPT_CONNECT, I_HTTP_OPT_FIRST, I_HTTP_OPT_BETWEEN] {
        i.local_get(ent).i32_const(1);
        i.local_get(secs).i64_const(1_000_000_000).i64_mul();
        i.i32_const((park + SENDRET) as i32).call(setter);
    }
    i.i32_const(1).local_set(opts_some);
    i.end();

    // ── request.new(headers, contents?, trailers_rx, options, retptr) ──
    i.local_get(headers);
    i.local_get(str_tx).i32_const(0).i32_ge_s().if_(BlockType::Result(ValType::I32));
    i.i32_const(1);
    i.else_();
    i.i32_const(0);
    i.end();
    i.local_get(str_rx);
    i.local_get(trl_rx);
    i.local_get(opts_some);
    i.local_get(ent);
    i.i32_const((park + RET) as i32);
    i.call(I_HTTP_REQ_NEW);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).local_set(request);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).local_set(sentfut);
    if stop == 2 {
        fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    }

    // ── setters (a syntactically bad component answers the static err) ──
    // method by op code (43 get / 44 post / 45 put / 46 patch / 47 delete).
    i.local_get(op).i32_const(43).i32_eq().if_(BlockType::Result(ValType::I32));
    i.i32_const(h.m_get);
    i.else_();
    i.local_get(op).i32_const(44).i32_eq().if_(BlockType::Result(ValType::I32));
    i.i32_const(h.m_post);
    i.else_();
    i.local_get(op).i32_const(45).i32_eq().if_(BlockType::Result(ValType::I32));
    i.i32_const(h.m_put);
    i.else_();
    i.local_get(op).i32_const(46).i32_eq().if_(BlockType::Result(ValType::I32));
    i.i32_const(h.m_patch);
    i.else_();
    i.local_get(op).i32_const(47).i32_eq().if_(BlockType::Result(ValType::I32));
    i.i32_const(h.m_delete);
    i.else_();
    // The framed family's method is a STRING: the nine named cases by
    // byte compare, anything else `other(string)` with the cell as payload.
    let named: [(&[u8], i32); 9] = [
        (b"GET", h.m_get),
        (b"HEAD", h.m_head),
        (b"POST", h.m_post),
        (b"PUT", h.m_put),
        (b"DELETE", h.m_delete),
        (b"CONNECT", h.m_connect),
        (b"OPTIONS", h.m_options),
        (b"TRACE", h.m_trace),
        (b"PATCH", h.m_patch),
    ];
    for (name, case) in named.iter() {
        i.local_get(m_len).i32_const(name.len() as i32).i32_eq().if_(BlockType::Result(ValType::I32));
        for (kb, ch) in name.iter().enumerate() {
            i.local_get(m_ptr).i32_load8_u(mem8(kb as u64)).i32_const(*ch as i32).i32_eq();
            if kb > 0 {
                i.i32_and();
            }
        }
        i.else_().i32_const(0).end();
        i.if_(BlockType::Result(ValType::I32));
        i.i32_const(*case);
        i.else_();
    }
    i.i32_const(h.m_other);
    for _ in 0..named.len() {
        i.end();
    }
    i.end();
    i.end();
    i.end();
    i.end();
    i.end();
    i.local_set(n);
    // `other(string)` carries the method cell as its payload; the named
    // cases carry none.
    i.local_get(request).local_get(n);
    i.local_get(n).i32_const(h.m_other).i32_eq().if_(BlockType::Result(ValType::I32));
    i.local_get(m_ptr);
    i.else_().i32_const(0).end();
    i.local_get(n).i32_const(h.m_other).i32_eq().if_(BlockType::Result(ValType::I32));
    i.local_get(m_len);
    i.else_().i32_const(0).end();
    i.call(I_HTTP_SET_METHOD).local_set(k);
    i.local_get(request)
        .i32_const(1)
        .local_get(sch)
        .i32_const(0)
        .i32_const(0)
        .call(I_HTTP_SET_SCHEME);
    i.local_get(k).i32_or().local_set(k);
    i.local_get(request)
        .i32_const(1)
        .local_get(auth_ptr)
        .local_get(auth_len)
        .call(I_HTTP_SET_AUTH);
    i.local_get(k).i32_or().local_set(k);
    i.local_get(request)
        .i32_const(1)
        .local_get(path_ptr)
        .local_get(path_len)
        .call(I_HTTP_SET_PATH);
    i.local_get(k).i32_or();
    i.if_(BlockType::Empty);
    i.local_get(request).call(I_HTTP_REQ_DROP);
    i.local_get(sentfut).call(I_HTTP_REQ_SENTDROP);
    // A request line the host will not take: the unclassified text naming
    // HTTP-request-URI-invalid (the reasons are rt-core's, ADR-0023 step 6).
    i.i32_const(t.ec_uri_invalid).local_set(ent);
    http_err_entry(&mut i, fns, (a_ptr, a_len), ent);
    i.end();
    if stop == 3 {
        fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    }

    // ── body: async writes, EOF (drop-writable) only once fully accepted.
    // A sync write pre-send deadlocks exactly like the trailers future,
    // and EOF cannot wait for a post-send retire either — the server may
    // hold the response until it sees the request complete. The pump +
    // scheduler below is therefore the required shape (#1710 risk #1).
    i.local_get(str_tx).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    http_body_pump(&mut i);
    i.end();
    if stop == 4 {
        fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    }
    // The transmit-result future (`sentfut`) is NOT dropped here (#2955):
    // wasmtime 49 ties the connection driver to it, so dropping its
    // readable before the response body drains ends a body that arrives
    // after the headers as EMPTY (GET/PUT answered `""` on Linux CI). It
    // drops on every exit past this point instead: send error, too-large,
    // and after the drain.
    if stop == 5 {
        fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    }


    // ── the async send + the guest scheduler ──
    // send is the async lower: `(subtask<<4)|state`, state 2 = returned
    // inline (the aopen convention). While it runs, the host reads the
    // trailers future and the body stream; their completion events drive
    // the loop until the response has landed and both writes retired.
    i.local_get(request).i32_const((park + SENDRET) as i32).call(I_HTTP_SEND).local_set(n);
    i.local_get(n).i32_const(15).i32_and().i32_const(2).i32_eq().if_(BlockType::Empty);
    i.i32_const(1).local_set(snd_done);
    i.else_();
    i.local_get(hws).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.call(I_WS_NEW).local_set(hws);
    i.end();
    i.local_get(n).i32_const(4).i32_shr_u().local_get(hws).call(I_WS_JOIN);
    i.end();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(snd_done);
    i.local_get(trl_pend).i32_eqz().i32_and();
    i.local_get(str_pend).i32_eqz().i32_and();
    i.br_if(1);
    i.local_get(hws).i32_const((park + RET) as i32).call(I_WS_WAIT).local_set(n);
    i.local_get(n).i32_const(5).i32_eq().if_(BlockType::Empty); // FUTURE_WRITE
    i.local_get(trl_tx).call(I_HTTP_REQ_FDROPW);
    i.i32_const(0).local_set(trl_pend);
    i.end();
    i.local_get(n).i32_const(3).i32_eq().if_(BlockType::Empty); // STREAM_WRITE
    i.i32_const((park + RET) as i32).i32_load(mem(4)).local_set(k); // count<<4|status
    i.local_get(b_ptr).local_get(k).i32_const(4).i32_shr_u().i32_add().local_set(b_ptr);
    i.local_get(b_len).local_get(k).i32_const(4).i32_shr_u().i32_sub().local_set(b_len);
    i.local_get(k).i32_const(15).i32_and().i32_const(1).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).local_set(b_len); // reader dropped: nothing more to feed
    i.end();
    http_body_pump(&mut i);
    i.end();
    i.local_get(n).i32_const(1).i32_eq().if_(BlockType::Empty); // SUBTASK
    i.i32_const((park + RET) as i32).i32_load(mem(4)).i32_const(2).i32_eq().if_(BlockType::Empty);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).call(I_SUBTASK_DROP);
    i.i32_const(1).local_set(snd_done);
    i.end();
    i.end();
    i.br(0).end().end();
    // Both writes are retired here (the loop above waits for their
    // events, on the err leg too — the host drops the request's readers),
    // so the set is dead on every leg: drop it BEFORE the err check. A
    // set leaked per failed exchange was the other half of #1924.
    i.local_get(hws).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.local_get(hws).call(I_WS_DROP);
    i.i32_const(-1).local_set(hws);
    i.end();
    i.i32_const((park + SENDRET) as i32).i32_load8_u(mem8(0));
    i.if_(BlockType::Empty);
    i.local_get(sentfut).call(I_HTTP_REQ_SENTDROP);
    http_send_err(&mut i, park, h, t, fns, ent);
    i.end();
    i.i32_const((park + SENDRET) as i32).i32_load(mem(h.send_payload)).local_set(response);
    // op 49 reads the status HERE: consume-body below takes `this:
    // response` OWNED, so a status read after it is a use of a moved
    // handle (`index N is not a resource` — #1924's bare `get_status`
    // trap; the live-echo test masked it because the index happened to
    // name another live handle there).
    i.local_get(op).i32_const(49).i32_eq().if_(BlockType::Empty);
    i.local_get(response).call(I_HTTP_STATUS).i32_const(0xFFFF).i32_and().local_set(tmp);
    i.end();

    // ── consume-body + the realloc'd drain (the fs read loop's shape) ──
    i.call(I_HTTP_CB_FNEW).local_set(s64);
    i.local_get(s64).i64_const(32).i64_shr_u().i32_wrap_i64().local_set(cb_tx);
    i.local_get(s64).i32_wrap_i64().local_set(cb_rx);
    i.local_get(response).local_get(cb_rx).i32_const((park + RET) as i32).call(I_HTTP_CONSUME);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).local_set(body_rx);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).local_set(trlfut);
    i.i32_const(0).i32_const(0).i32_const(8).i32_const(65536).call(f_alloc).local_set(buf);
    i.i32_const(65536).local_set(cap);
    i.i32_const(0).local_set(total);
    // op 49 (`request_status`): `<status>\n` precedes the body — the host
    // lane's `format!("{code}\n{text}")`. Decimal, no padding: count the
    // digits, then write them back to front.
    i.local_get(op).i32_const(49).i32_eq().if_(BlockType::Empty);
    i.i32_const(1).local_set(digit);
    i.local_get(tmp).local_set(cell_len);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cell_len).i32_const(10).i32_lt_u().br_if(1);
    i.local_get(cell_len).i32_const(10).i32_div_u().local_set(cell_len);
    i.local_get(digit).i32_const(1).i32_add().local_set(digit);
    i.br(0).end().end();
    i.local_get(digit).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).i32_eqz().br_if(1);
    i.local_get(cur).i32_const(1).i32_sub().local_set(cur);
    i.local_get(buf).local_get(cur).i32_add();
    i.local_get(tmp).i32_const(10).i32_rem_u().i32_const(48).i32_add();
    i.i32_store8(mem8(0));
    i.local_get(tmp).i32_const(10).i32_div_u().local_set(tmp);
    i.br(0).end().end();
    i.local_get(buf).local_get(digit).i32_add().i32_const(10).i32_store8(mem8(0));
    i.local_get(digit).i32_const(1).i32_add().local_set(total);
    i.end();
    // The response limit (ALMIDE_HTTP_MAX_RESPONSE_BYTES, default 1 GiB, 0 =
    // none), counted on the body as it arrives — past it, the exchange is
    // abandoned and answers the too-large text.
    i.local_get(total).local_set(pre);
    i.i32_const(t.key_max.at).i32_const(t.key_max.len).i64_const(1 << 30).call(fns.num).local_set(lim);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(total).local_get(cap).i32_ge_u();
    i.if_(BlockType::Empty);
    i.local_get(buf).local_get(cap).i32_const(8);
    i.local_get(cap).i32_const(1).i32_shl();
    i.call(f_alloc).local_set(buf);
    i.local_get(cap).i32_const(1).i32_shl().local_set(cap);
    i.end();
    i.local_get(body_rx);
    i.local_get(body_rx);
    i.local_get(buf).local_get(total).i32_add();
    i.local_get(cap).local_get(total).i32_sub();
    i.call(I_HTTP_BODY_READ);
    // #2955: `n` holds the RAW read result, `(count << 4) | status`. The end
    // of the body is the status (DROPPED/CANCELLED), never a zero count: a
    // read may COMPLETE with 0 items when the host's producer has nothing
    // yet (wasmtime 49 does this when the body arrives after the headers),
    // and reading that as EOF answered an empty body.
    i.call(f_await).local_set(n);
    i.local_get(total).local_get(n).i32_const(4).i32_shr_u().i32_add().local_set(total);
    i.local_get(lim).i64_const(0).i64_gt_s();
    i.local_get(total).local_get(pre).i32_sub().i64_extend_i32_u().local_get(lim).i64_gt_s();
    i.i32_and().if_(BlockType::Empty);
    http_body_retire(&mut i, park, (body_rx, trlfut, cb_tx), f_await);
    i.local_get(sentfut).call(I_HTTP_REQ_SENTDROP);
    http_decimal_i64(&mut i, (park + CLEN_BUF) as i32, lim, secs, key_len, cur);
    i.i32_const((park + CLEN_BUF) as i32).local_set(key_ptr);
    let [tl0, tl1, tl2] = t.too_large;
    http_err_call(&mut i, fns, (a_ptr, a_len), tl0, tl1, Some((key_ptr, key_len)), tl2);
    i.end();
    i.local_get(n).i32_const(15).i32_and().br_if(1);
    i.br(0).end().end();
    http_body_retire(&mut i, park, (body_rx, trlfut, cb_tx), f_await);
    i.local_get(sentfut).call(I_HTTP_REQ_SENTDROP);

    i.local_get(buf).global_set(g_ppos);
    i.local_get(total).global_set(g_plen);
    i.local_get(total).i64_extend_i32_u().return_();
    i.unreachable();
    i.end();
    f
}
