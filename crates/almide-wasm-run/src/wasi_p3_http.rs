// `include!`d part of wasi_p3.rs (codopsy max-lines split, mechanical text move —
// shares the parent module's imports and items; nothing here is pub beyond the parent).

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
/// `wasi:http/client@0.3.0`'s sync-lowered `send`. Sequence (the recorded
/// blueprint): url parse in-shim (scheme prefix, authority to '/', path
/// rest); empty `fields`; the trailers future written `ok(none)` up front;
/// an optional contents stream fed and its writable end dropped BEFORE the
/// sync send (the rendezvous-ordering probe the blueprint demands — a body
/// past the host's buffer (`-S http-outgoing-body-buffer-chunks`) is the
/// empirical fixture); the sent-future's readable dropped; on `ok`, the
/// response body drains through the same realloc'd sync-read loop the fs
/// shim uses, into the parked-result convention. Transport errors answer
/// `pack(1, len)` with the static E_HTTP text (host-specific wording is
/// bounded by contract — fixtures assert err-ness). The p3 stream delivers
/// the DECODED body, so no chunked handling exists here by design.
fn shim_http(park: u64, g_plen: u32, g_ppos: u32, f_realloc: u32, h: &HttpAbi) -> Function {
    // Emit-time bisect knob (#1710 PR B bring-up): ALMIDE_P3_HTTP_STOP=N
    // makes the shim answer the static err right after stage N, so a hang
    // localizes to the first stage whose stop-build still hangs. Stages:
    // 1 = fields+trailers written, 2 = request.new, 3 = setters,
    // 4 = body fed + writable dropped, 5 = sent-future dropped.
    let stop = std::env::var("ALMIDE_P3_HTTP_STOP")
        .ok()
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
    let mut f = Function::new([(23, ValType::I32), (1, ValType::I64), (14, ValType::I32)]);
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

    // ── URL parse: scheme by prefix, authority to '/', path = rest ──
    i.i32_const(h.sch_http).local_set(sch);
    i.i32_const(0).local_set(rest);
    i.local_get(a_len).i32_const(8).i32_ge_u().if_(BlockType::Empty);
    for (kb, ch) in b"https://".iter().enumerate() {
        i.local_get(a_ptr).i32_load8_u(mem8(kb as u64)).i32_const(*ch as i32).i32_eq();
        if kb > 0 {
            i.i32_and();
        }
    }
    i.if_(BlockType::Empty);
    i.i32_const(h.sch_https).local_set(sch);
    i.i32_const(8).local_set(rest);
    i.end();
    i.end();
    i.local_get(rest).i32_eqz();
    i.local_get(a_len).i32_const(7).i32_ge_u();
    i.i32_and().if_(BlockType::Empty);
    for (kb, ch) in b"http://".iter().enumerate() {
        i.local_get(a_ptr).i32_load8_u(mem8(kb as u64)).i32_const(*ch as i32).i32_eq();
        if kb > 0 {
            i.i32_and();
        }
    }
    i.if_(BlockType::Empty);
    i.i32_const(7).local_set(rest);
    i.end();
    i.end();
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

    // ── empty fields; trailers future written ok(none) up front ──
    i.call(I_HTTP_FIELDS_NEW).local_set(headers);
    // The framed family's headers: key/value cells appended in frame order
    // (the host lane's parse_http_frame order, so the wire carries the
    // same header sequence on both lanes). A rejected name/value
    // (header-error) is ignored, as the rt-core client's builder does.
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

    // ── request.new(headers, contents?, trailers_rx, options none, retptr) ──
    i.local_get(headers);
    i.local_get(str_tx).i32_const(0).i32_ge_s().if_(BlockType::Result(ValType::I32));
    i.i32_const(1);
    i.else_();
    i.i32_const(0);
    i.end();
    i.local_get(str_rx);
    i.local_get(trl_rx);
    i.i32_const(0);
    i.i32_const(0);
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
    fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
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
    i.local_get(sentfut).call(I_HTTP_REQ_SENTDROP);
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
    fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
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
    i.i32_const(0).i32_const(0).i32_const(8).i32_const(65536).call(f_realloc).local_set(buf);
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
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(total).local_get(cap).i32_ge_u();
    i.if_(BlockType::Empty);
    i.local_get(buf).local_get(cap).i32_const(8);
    i.local_get(cap).i32_const(1).i32_shl();
    i.call(f_realloc).local_set(buf);
    i.local_get(cap).i32_const(1).i32_shl().local_set(cap);
    i.end();
    i.local_get(body_rx);
    i.local_get(buf).local_get(total).i32_add();
    i.local_get(cap).local_get(total).i32_sub();
    i.call(I_HTTP_BODY_READ);
    i.i32_const(4).i32_shr_u().local_set(n);
    i.local_get(n).i32_eqz().br_if(1);
    i.local_get(total).local_get(n).i32_add().local_set(total);
    i.br(0).end().end();
    i.local_get(body_rx).call(I_HTTP_BODY_DROPR);
    i.local_get(trlfut).call(I_HTTP_TRL_DROPR);
    // handling result: ok written into the kept writable, then dropped.
    i.i32_const((park + RET) as i32).i64_const(0).i64_store(mem64(16));
    i.local_get(cb_tx).i32_const((park + RET + 16) as i32).call(I_HTTP_CB_FWRITE).drop();
    i.local_get(cb_tx).call(I_HTTP_CB_FDROPW);

    i.local_get(buf).global_set(g_ppos);
    i.local_get(total).global_set(g_plen);
    i.local_get(total).i64_extend_i32_u().return_();
    i.unreachable();
    i.end();
    f
}
