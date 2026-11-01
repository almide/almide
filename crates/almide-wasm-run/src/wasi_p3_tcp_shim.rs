// `include!`d part of wasi_p3.rs (#3620, wasi_p3_tcp.rs): the tcp shim
// itself (ops 91..=103) and its copy-with-deadline helper.

/// The tcp shim's locals past the ones it shares with the udp emitters.
#[derive(Clone, Copy)]
struct TcpLocals {
    /// The slot's address.
    p: u32,
    /// A copy's `count << 4 | status` word (bit 3: the timer won).
    word: u32,
    x: u32,
    y: u32,
    /// The deadline in ms (i64; 0 = none) and an i64 scratch.
    ms: u32,
    t64: u32,
}

/// One of the two tables a handle names: its globals, stride and texts.
#[derive(Clone, Copy)]
struct TcpTable {
    tab: u32,
    n: u32,
    cap: u32,
    stride: i32,
    invalid: [NetPiece; 2],
    closed: [NetPiece; 2],
}

/// A copy's status values (the canonical ABI's `CopyResult`).
const COPY_DROPPED: i32 = 1;
/// Bit 3 of the word `$tcp_await` answers: the deadline passed first.
const COPY_TIMER: i32 = 8;

/// `(end, rc, ms, write) -> word`: complete the `[async-lower]` copy on
/// `end` whose call answered `rc`. Without a deadline (`ms <= 0`) this is
/// `$await`. With one, the copy races an async-lowered `wait-for(ms)` in a
/// fresh waitable set: the copy's event answers its word and the timer
/// subtask is cancelled; the timer's `returned` event cancels the copy (the
/// baseline sync `stream.cancel-read` / `cancel-write` — whatever it moved
/// still counts) and answers that word with bit 3 set.
fn tcp_fn_await(g: P3Globals, t: &NetTexts, imp: NetImports) -> Function {
    let (end, rc, ms, write) = (0u32, 1u32, 2u32, 3u32);
    let (st, sub, ws, word) = (4u32, 5u32, 6u32, 7u32);
    let ev = t.scratch(NET_SCR_EV);
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
    let cancel = |i: &mut Sink<'_>| {
        i.local_get(write).if_(BlockType::Result(ValType::I32));
        i.local_get(end).call(imp.at(NI_T_SCANCEL));
        i.else_();
        i.local_get(end).call(imp.at(NI_T_RCANCEL));
        i.end();
    };
    i.local_get(rc).i32_const(-1).i32_ne().if_(BlockType::Empty).local_get(rc).return_().end();
    i.local_get(ms).i64_const(0).i64_le_s().if_(BlockType::Empty);
    i.local_get(end).local_get(rc).call(g.f_await).return_();
    i.end();
    // ns = ms * 1e6, the ms clamped so the product stays an i64.
    i.local_get(ms).i64_const(9_223_372_036_854).local_get(ms).i64_const(9_223_372_036_854).i64_lt_s().select();
    i.i64_const(1_000_000).i64_mul().call(imp.at(NI_T_WAIT)).local_set(st);
    // Returned at once (a zero wait): the copy loses on the spot.
    i.local_get(st).i32_const(15).i32_and().i32_const(2).i32_eq().if_(BlockType::Empty);
    cancel(&mut i);
    i.i32_const(COPY_TIMER).i32_or().return_();
    i.end();
    i.local_get(st).i32_const(4).i32_shr_u().local_set(sub);
    i.call(I_WS_NEW).local_set(ws);
    i.local_get(end).local_get(ws).call(I_WS_JOIN);
    i.local_get(sub).local_get(ws).call(I_WS_JOIN);
    i.loop_(BlockType::Empty);
    i.local_get(ws).i32_const(ev).call(I_WS_WAIT).drop();
    // The copy's event: its word; the timer is cancelled and dropped.
    i.i32_const(ev).i32_load(net_m32(0)).local_get(end).i32_eq().if_(BlockType::Empty);
    i.i32_const(ev).i32_load(net_m32(4)).local_set(word);
    i.local_get(end).i32_const(0).call(I_WS_JOIN);
    i.local_get(sub).i32_const(0).call(I_WS_JOIN);
    i.local_get(sub).call(I_SUBTASK_CANCEL).drop();
    i.local_get(sub).call(I_SUBTASK_DROP);
    i.local_get(ws).call(I_WS_DROP);
    i.local_get(word).return_();
    i.end();
    // The timer returned: the copy is cancelled.
    i.i32_const(ev).i32_load(net_m32(0)).local_get(sub).i32_eq();
    i.i32_const(ev).i32_load(net_m32(4)).i32_const(2).i32_eq().i32_and();
    i.if_(BlockType::Empty);
    i.local_get(end).i32_const(0).call(I_WS_JOIN);
    i.local_get(sub).i32_const(0).call(I_WS_JOIN);
    i.local_get(sub).call(I_SUBTASK_DROP);
    i.local_get(ws).call(I_WS_DROP);
    cancel(&mut i);
    i.i32_const(COPY_TIMER).i32_or().return_();
    i.end();
    i.br(0);
    i.end();
    i.unreachable();
    i.end();
    f
}

/// A handle past `tbl` (a negative one included): its invalid text.
fn tcp_invalid_check(i: &mut Sink<'_>, cx: NetCx<'_>, l: NetLocals, tbl: TcpTable) {
    let NetCx { g, fns, ng, .. } = cx;
    i.local_get(l.h).global_get(tbl.n).i64_extend_i32_u().i64_ge_u().if_(BlockType::Empty);
    net_put(i, l.w, tbl.invalid[0]);
    i.local_get(l.w).local_get(l.h).call(fns.dec).local_set(l.w);
    net_put(i, l.w, tbl.invalid[1]);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// `p` = slot `h` of `tbl`, or the invalid / closed err.
fn tcp_slot_get(i: &mut Sink<'_>, cx: NetCx<'_>, l: NetLocals, tl: TcpLocals, tbl: TcpTable) {
    let NetCx { g, fns, ng, .. } = cx;
    tcp_invalid_check(i, cx, l, tbl);
    i.global_get(tbl.tab).local_get(l.h).i32_wrap_i64().i32_const(tbl.stride).i32_mul().i32_add().local_tee(tl.p);
    i.i32_load(net_m32(TS_SOCK)).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    net_put(i, l.w, tbl.closed[0]);
    i.local_get(l.w).local_get(l.h).call(fns.dec).local_set(l.w);
    net_put(i, l.w, tbl.closed[1]);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// The lowest free slot of `tbl` (native's `almide_net_slot`): its index in
/// `j`, its address in `p`. A full table doubles; its new tail is zeroed
/// (the bump may hand back memory an earlier op rewound) and the rewind mark
/// moves past it.
fn tcp_slot_put(i: &mut Sink<'_>, cx: NetCx<'_>, l: NetLocals, tl: TcpLocals, tbl: TcpTable) {
    let NetCx { g, .. } = cx;
    let at = |i: &mut Sink<'_>, idx: u32| {
        i.global_get(tbl.tab).local_get(idx).i32_const(tbl.stride).i32_mul().i32_add();
    };
    i.i32_const(0).local_set(l.j);
    i.block(BlockType::Empty);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.j).global_get(tbl.n).i32_ge_u().br_if(1);
    at(i, l.j);
    i.local_tee(tl.p).i32_load(net_m32(TS_SOCK)).i32_const(0).i32_lt_s().br_if(2);
    i.local_get(l.j).i32_const(1).i32_add().local_set(l.j);
    i.br(0).end().end();
    i.global_get(tbl.n).global_get(tbl.cap).i32_eq().if_(BlockType::Empty);
    i.global_get(tbl.cap).i32_const(1).i32_shl().i32_const(8).global_get(tbl.cap).select().local_set(l.i1);
    i.i32_const(0).i32_const(0).i32_const(8).local_get(l.i1).i32_const(tbl.stride).i32_mul().call(g.f_alloc).local_set(l.i2);
    i.local_get(l.i2).i32_const(0).local_get(l.i1).i32_const(tbl.stride).i32_mul().memory_fill(0);
    i.local_get(l.i2).global_get(tbl.tab).global_get(tbl.n).i32_const(tbl.stride).i32_mul().memory_copy(0, 0);
    i.local_get(l.i2).global_set(tbl.tab);
    i.local_get(l.i1).global_set(tbl.cap);
    i.global_get(g.heap).local_set(l.h0);
    i.end();
    i.global_get(tbl.n).local_set(l.j);
    i.global_get(tbl.n).i32_const(1).i32_add().global_set(tbl.n);
    at(i, l.j);
    i.local_set(tl.p);
    i.end();
}

/// A fresh stream in slot `p`: socket `s`, no pair opened, open, no
/// lookahead bytes, no timeout (the lookahead buffer, if the slot had one,
/// stays for reuse).
fn tcp_init_stream(i: &mut Sink<'_>, l: NetLocals, tl: TcpLocals) {
    i.local_get(tl.p).local_get(l.s).i32_store(net_m32(TS_SOCK));
    for off in [TS_RX, TS_RXF, TS_TX, TS_TXF] {
        i.local_get(tl.p).i32_const(-1).i32_store(net_m32(off));
    }
    for off in [TS_EOF, TS_LOFF, TS_LLEN] {
        i.local_get(tl.p).i32_const(0).i32_store(net_m32(off));
    }
    i.local_get(tl.p).i64_const(0).i64_store(MemArg { offset: TS_TMO, align: 3, memory_index: 0 });
}

/// Open the stream's receive pair on first use.
fn tcp_ensure_rx(i: &mut Sink<'_>, cx: NetCx<'_>, a: &NetAbi, imp: NetImports, tl: TcpLocals) {
    let ret = (cx.g.park + STATRET) as i32;
    i.local_get(tl.p).i32_load(net_m32(TS_RX)).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.i32_const(NET_RESERVE).call(cx.g.f_reserve);
    i.local_get(tl.p).i32_load(net_m32(TS_SOCK)).i32_const(ret).call(imp.at(NI_T_RECEIVE));
    i.local_get(tl.p).i32_const(ret).i32_load(net_m32(a.tcp_recv_fields[0])).i32_store(net_m32(TS_RX));
    i.local_get(tl.p).i32_const(ret).i32_load(net_m32(a.tcp_recv_fields[1])).i32_store(net_m32(TS_RXF));
    i.end();
}

/// Open the stream's send pair on first use: a fresh `stream<u8>` whose
/// readable end goes to `send`, the writable end and the future kept.
fn tcp_ensure_tx(i: &mut Sink<'_>, imp: NetImports, tl: TcpLocals) {
    i.local_get(tl.p).i32_load(net_m32(TS_TX)).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.call(imp.at(NI_T_SNEW)).local_set(tl.t64);
    i.local_get(tl.p).local_get(tl.t64).i64_const(32).i64_shr_u().i32_wrap_i64().i32_store(net_m32(TS_TX));
    i.local_get(tl.p);
    i.local_get(tl.p).i32_load(net_m32(TS_SOCK));
    i.local_get(tl.t64).i32_wrap_i64().call(imp.at(NI_T_SEND));
    i.i32_store(net_m32(TS_TXF));
    i.end();
}

/// `<head>: <class>` for the `error-code` discriminant in local `disc`.
fn tcp_err_class(i: &mut Sink<'_>, cx: NetCx<'_>, l: NetLocals, head: NetPiece, disc: u32) {
    net_put(i, l.w, head);
    net_put(i, l.w, cx.t.class_sep);
    net_put_class(i, cx.t, l, disc);
    net_answer(i, cx.g, cx.ng, l, 1);
}

/// `<head>: <class>` for a discriminant the shim names itself.
fn tcp_err_const(i: &mut Sink<'_>, cx: NetCx<'_>, l: NetLocals, head: NetPiece, disc: i32) {
    i.i32_const(disc).local_set(l.last);
    tcp_err_class(i, cx, l, head, l.last);
}

/// Read the stream's receive future (its stream ended): the reset's class,
/// or end of stream.
fn tcp_rx_future(i: &mut Sink<'_>, cx: NetCx<'_>, a: &NetAbi, imp: NetImports, (l, tl): (NetLocals, TcpLocals), head: NetPiece) {
    let ret = (cx.g.park + STATRET) as i32;
    i.local_get(tl.p).i32_const(2).i32_store(net_m32(TS_EOF));
    i.i32_const(NET_RESERVE).call(cx.g.f_reserve);
    i.local_get(tl.p).i32_load(net_m32(TS_RXF));
    i.local_get(tl.p).i32_load(net_m32(TS_RXF)).i32_const(ret).call(imp.at(NI_T_RFREAD));
    i.call(cx.g.f_await).drop();
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    i.i32_const(ret).i32_load8_u(net_m8(a.tcp_unit_payload)).local_set(l.last);
    tcp_err_class(i, cx, l, head, l.last);
    i.end();
    net_answer(i, cx.g, cx.ng, l, 0);
}

/// One read of at most `max` (≤ 64 KiB) bytes from slot `p` under deadline
/// `ms`: the lookahead first, then one copy; empty at end of stream.
fn tcp_read_core(i: &mut Sink<'_>, cx: NetCx<'_>, a: &NetAbi, imp: NetImports, (l, tl): (NetLocals, TcpLocals), head: NetPiece) {
    let NetCx { g, fns, ng, .. } = cx;
    i.local_get(l.max).i64_eqz().if_(BlockType::Empty);
    net_answer(i, g, ng, l, 0);
    i.end();
    i.local_get(l.max).i32_wrap_i64().local_set(l.n);
    // The lookahead `tcp_available` filled.
    i.local_get(tl.p).i32_load(net_m32(TS_LLEN)).local_tee(l.k).if_(BlockType::Empty);
    i.local_get(l.k).local_get(l.n).local_get(l.k).local_get(l.n).i32_lt_u().select().local_set(l.k);
    i.local_get(l.w);
    i.local_get(tl.p).i32_load(net_m32(TS_LOOK)).local_get(tl.p).i32_load(net_m32(TS_LOFF)).i32_add();
    i.local_get(l.k).memory_copy(0, 0);
    i.local_get(tl.p).local_get(tl.p).i32_load(net_m32(TS_LOFF)).local_get(l.k).i32_add().i32_store(net_m32(TS_LOFF));
    i.local_get(tl.p).local_get(tl.p).i32_load(net_m32(TS_LLEN)).local_get(l.k).i32_sub().i32_store(net_m32(TS_LLEN));
    i.local_get(l.w).local_get(l.k).i32_add().local_set(l.w);
    net_answer(i, g, ng, l, 0);
    i.end();
    i.local_get(tl.p).i32_load(net_m32(TS_EOF)).i32_const(2).i32_eq().if_(BlockType::Empty);
    net_answer(i, g, ng, l, 0);
    i.end();
    i.local_get(tl.p).i32_load(net_m32(TS_EOF)).if_(BlockType::Empty);
    tcp_rx_future(i, cx, a, imp, (l, tl), head);
    i.end();
    tcp_ensure_rx(i, cx, a, imp, tl);
    i.loop_(BlockType::Empty);
    i.local_get(tl.p).i32_load(net_m32(TS_RX));
    i.local_get(tl.p).i32_load(net_m32(TS_RX)).local_get(l.w).local_get(l.n).call(imp.at(NI_T_RREAD));
    i.local_get(tl.ms).i32_const(0).call(fns.tcp_await).local_set(tl.word);
    i.local_get(tl.word).i32_const(4).i32_shr_u().local_tee(l.k).if_(BlockType::Empty);
    i.local_get(l.w).local_get(l.k).i32_add().local_set(l.w);
    net_answer(i, g, ng, l, 0);
    i.end();
    i.local_get(tl.word).i32_const(COPY_TIMER).i32_and().if_(BlockType::Empty);
    tcp_err_const(i, cx, l, head, cx.t.tcp.ec_timeout);
    i.end();
    i.local_get(tl.word).i32_const(7).i32_and().i32_const(COPY_DROPPED).i32_eq().if_(BlockType::Empty);
    tcp_rx_future(i, cx, a, imp, (l, tl), head);
    i.end();
    i.br(0);
    i.end();
}

/// The tcp shim's ops, by family member, for one op `op` (an `if` block).
fn tcp_op(i: &mut Sink<'_>, op: i32, body: impl FnOnce(&mut Sink<'_>)) {
    i.local_get(0).i32_const(op).i32_eq().if_(BlockType::Empty);
    body(i);
    i.end();
}

/// `(op, a_ptr, a_len, b_ptr, b_len) -> i64` over the fs_call ABI: ops
/// 91..=103 with stdlib/net_wasm.almd's operands, answered as net_host.rs
/// answers them (status 0 = ok, 1 = err, the answer parked for `host_read`).
/// The checks run in net_core's order: `tcp_read` refuses a negative length
/// before it looks at the handle, every other call names the handle first.
fn shim_tcp(g: P3Globals, a: &NetAbi, t: &NetTexts, parts: NetParts) -> Function {
    let NetParts { fns, ng, imp } = parts;
    let cx = NetCx { g, t, fns, ng };
    let (a_ptr, a_len, b_ptr, b_len) = (1u32, 2u32, 3u32, 4u32);
    let l = NetLocals {
        w: 5, h0: 6, s: 7, k: 8, cnt: 9, el: 10, last: 11, i1: 12, i2: 13, hp: 14, hl: 15, n: 16, j: 17,
        h: 22, port: 23, max: 24,
    };
    let tl = TcpLocals { p: 18, word: 19, x: 20, y: 21, ms: 25, t64: 26 };
    let streams = TcpTable {
        tab: ng.st_tab, n: ng.st_n, cap: ng.st_cap, stride: TS_STRIDE,
        invalid: t.tcp.invalid_stream, closed: t.tcp.closed_stream,
    };
    let listeners = TcpTable {
        tab: ng.ls_tab, n: ng.ls_n, cap: ng.ls_cap, stride: TL_STRIDE,
        invalid: t.tcp.invalid_listener, closed: t.tcp.closed_listener,
    };
    let ret = (g.park + STATRET) as i32;
    let tmo = MemArg { offset: TS_TMO, align: 3, memory_index: 0 };
    let mut f = Function::new([(17, ValType::I32), (5, ValType::I64)]);
    let mut i = f.instructions();

    // The answer buffer: any head, and for a read a whole chunk.
    i.local_get(a_len).i32_const(12).i32_mul().i32_const(512).i32_add();
    i.i32_const(TCP_CHUNK + 128).i32_const(0);
    i.local_get(0).i32_const(OP_TCP_READ).i32_eq().local_get(0).i32_const(OP_TCP_READ_TIMEOUT).i32_eq().i32_or().select();
    i.i32_add().local_set(l.n);
    net_ensure(&mut i, g, ng, l, l.n);
    i.global_get(g.heap).local_set(l.h0);
    i.global_get(ng.buf).local_set(l.w);
    let handle = |i: &mut Sink<'_>| {
        i.local_get(a_ptr).local_get(a_len).call(fns.int).local_set(l.h);
    };

    // ── tcp_connect(host = a, port = b) / tcp_listen(host = a, port = b) ──
    for (op, open) in [(OP_TCP_CONNECT, t.tcp.connect_open), (OP_TCP_LISTEN, t.tcp.listen_open)] {
        tcp_op(&mut i, op, |i| {
            i.local_get(a_ptr).local_set(l.hp);
            i.local_get(a_len).local_set(l.hl);
            i.local_get(b_ptr).local_get(b_len).call(fns.int).local_set(l.port);
            net_port_check(i, cx, l, open);
            net_resolve_or_err(i, cx, l, open);
            i.i32_const(0).local_set(l.k);
            i.global_get(ng.addrs).local_set(l.el);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(l.k).local_get(l.cnt).i32_ge_u().br_if(1);
            i.block(BlockType::Empty); // the next address
            i.i32_const(NET_RESERVE).call(g.f_reserve);
            i.i32_const(a.fam[0]).i32_const(a.fam[1]);
            i.local_get(l.el).i32_load8_u(net_m8(0)).i32_const(a.ia_case[0]).i32_eq().select();
            i.i32_const(ret).call(imp.at(NI_T_CREATE));
            i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
            i.i32_const(ret).i32_load8_u(net_m8(a.tcp_create_payload)).local_set(l.last);
            i.br(1);
            i.end();
            i.i32_const(ret).i32_load(net_m32(a.tcp_create_payload)).local_set(l.s);
            i.local_get(l.el).local_get(l.port).i32_wrap_i64().call(fns.sa);
            // connect, or bind then listen.
            let steps: &[u32] = if op == OP_TCP_CONNECT { &[NI_T_CONNECT] } else { &[NI_T_BIND, NI_T_LISTEN] };
            for &step in steps {
                i.i32_const(NET_RESERVE).call(g.f_reserve);
                i.local_get(l.s);
                if step != NI_T_LISTEN {
                    net_push_sa(i, a, t);
                }
                i.i32_const(ret).call(imp.at(step));
                i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
                let at = if step == NI_T_LISTEN { a.tcp_listen_payload } else { a.tcp_unit_payload };
                i.i32_const(ret).i32_load8_u(net_m8(at)).local_set(l.last);
                i.local_get(l.s).call(imp.at(NI_T_DROP));
                i.br(1);
                i.end();
            }
            if op == OP_TCP_CONNECT {
                tcp_slot_put(i, cx, l, tl, streams);
                tcp_init_stream(i, l, tl);
            } else {
                i.i32_const(ret).i32_load(net_m32(a.tcp_listen_payload)).local_set(l.i1);
                tcp_slot_put(i, cx, l, tl, listeners);
                i.local_get(tl.p).local_get(l.s).i32_store(net_m32(TL_SOCK));
                i.local_get(tl.p).local_get(l.i1).i32_store(net_m32(TL_STREAM));
            }
            i.local_get(l.w).local_get(l.j).i64_extend_i32_u().call(fns.dec).local_set(l.w);
            net_answer(i, g, ng, l, 0);
            i.end();
            i.local_get(l.el).i32_const(a.ia_size as i32).i32_add().local_set(l.el);
            i.local_get(l.k).i32_const(1).i32_add().local_set(l.k);
            i.br(0).end().end();
            net_head(i, t, fns, l, open);
            net_put(i, l.w, t.class_sep);
            net_put_class(i, t, l, l.last);
            net_answer(i, g, ng, l, 1);
        });
    }

    // ── tcp_accept(listener = a): the next client on the listener's stream ──
    tcp_op(&mut i, OP_TCP_ACCEPT, |i| {
        handle(i);
        tcp_slot_get(i, cx, l, tl, listeners);
        i.i32_const(NET_RESERVE).call(g.f_reserve);
        i.local_get(tl.p).i32_load(net_m32(TL_STREAM));
        i.local_get(tl.p).i32_load(net_m32(TL_STREAM)).i32_const(t.scratch(NET_SCR_ACCEPT)).i32_const(1).call(imp.at(NI_T_LREAD));
        i.call(g.f_await).i32_const(4).i32_shr_u().i32_eqz().if_(BlockType::Empty);
        tcp_err_const(i, cx, l, t.tcp.accept_head, t.tcp.ec_other);
        i.end();
        i.i32_const(t.scratch(NET_SCR_ACCEPT)).i32_load(net_m32(0)).local_set(l.s);
        tcp_slot_put(i, cx, l, tl, streams);
        tcp_init_stream(i, l, tl);
        i.local_get(l.w).local_get(l.j).i64_extend_i32_u().call(fns.dec).local_set(l.w);
        net_answer(i, g, ng, l, 0);
    });

    // ── tcp_local_port(listener = a) ──
    tcp_op(&mut i, OP_TCP_LOCAL_PORT, |i| {
        handle(i);
        tcp_slot_get(i, cx, l, tl, listeners);
        i.i32_const(NET_RESERVE).call(g.f_reserve);
        i.local_get(tl.p).i32_load(net_m32(TL_SOCK)).i32_const(ret).call(imp.at(NI_T_LOCAL));
        i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
        i.i32_const(ret).i32_load8_u(net_m8(a.tcp_local_payload)).local_set(l.last);
        tcp_err_class(i, cx, l, t.tcp.local_head, l.last);
        i.end();
        i.local_get(l.w);
        net_sa_port(i, a, ret + a.tcp_local_payload as i32);
        i.i64_extend_i32_u().call(fns.dec).local_set(l.w);
        net_answer(i, g, ng, l, 0);
    });

    // ── tcp_read(stream = a, len = b): the length is checked first ──
    tcp_op(&mut i, OP_TCP_READ, |i| {
        i.local_get(b_ptr).local_get(b_len).call(fns.int).local_tee(l.max).i64_const(0).i64_lt_s().if_(BlockType::Empty);
        net_put(i, l.w, t.tcp.read_negative[0]);
        i.local_get(l.w).local_get(l.max).call(fns.dec).local_set(l.w);
        net_put(i, l.w, t.tcp.read_negative[1]);
        net_answer(i, g, ng, l, 1);
        i.end();
        handle(i);
        tcp_slot_get(i, cx, l, tl, streams);
        i.local_get(tl.p).i64_load(tmo).local_set(tl.ms);
        tcp_clamp_chunk(i, l);
        tcp_read_core(i, cx, a, imp, (l, tl), t.tcp.read_head);
    });

    // ── tcp_read_timeout(stream = a, b = `<len>\n<ms>`) ──
    tcp_op(&mut i, OP_TCP_READ_TIMEOUT, |i| {
        handle(i);
        tcp_slot_get(i, cx, l, tl, streams);
        net_find_nl(i, l, b_ptr, b_len, 0, l.i1);
        i.local_get(b_ptr).local_get(l.i1).call(fns.int).local_set(l.max);
        i.local_get(b_ptr).local_get(l.i1).i32_add().i32_const(1).i32_add();
        i.local_get(b_len).local_get(l.i1).i32_sub().i32_const(1).i32_sub().call(fns.int).local_set(tl.ms);
        i.local_get(l.max).i64_const(0).i64_lt_s().if_(BlockType::Empty);
        net_put(i, l.w, t.tcp.rt_negative[0]);
        i.local_get(l.w).local_get(l.max).call(fns.dec).local_set(l.w);
        net_put(i, l.w, t.tcp.rt_negative[1]);
        net_answer(i, g, ng, l, 1);
        i.end();
        i.local_get(tl.ms).i64_const(0).i64_le_s().if_(BlockType::Empty);
        net_put(i, l.w, t.tcp.rt_not_positive[0]);
        i.local_get(l.w).local_get(tl.ms).call(fns.dec).local_set(l.w);
        net_put(i, l.w, t.tcp.rt_not_positive[1]);
        net_answer(i, g, ng, l, 1);
        i.end();
        tcp_clamp_chunk(i, l);
        tcp_read_core(i, cx, a, imp, (l, tl), t.tcp.rt_head);
    });

    // ── tcp_write(stream = a, b = the bytes): every byte, under the timeout ──
    tcp_op(&mut i, OP_TCP_WRITE, |i| {
        handle(i);
        tcp_slot_get(i, cx, l, tl, streams);
        i.local_get(tl.p).i64_load(tmo).local_set(tl.ms);
        tcp_ensure_tx(i, imp, tl);
        i.local_get(b_ptr).local_set(tl.x);
        i.local_get(b_len).local_set(tl.y);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(tl.y).i32_eqz().br_if(1);
        i.local_get(tl.p).i32_load(net_m32(TS_TX));
        i.local_get(tl.p).i32_load(net_m32(TS_TX)).local_get(tl.x).local_get(tl.y).call(imp.at(NI_T_SWRITE));
        i.local_get(tl.ms).i32_const(1).call(fns.tcp_await).local_set(tl.word);
        i.local_get(tl.word).i32_const(4).i32_shr_u().local_set(l.k);
        i.local_get(tl.x).local_get(l.k).i32_add().local_set(tl.x);
        i.local_get(tl.y).local_get(l.k).i32_sub().local_set(tl.y);
        i.local_get(tl.word).i32_const(COPY_TIMER).i32_and().local_get(l.k).i32_eqz().i32_and().if_(BlockType::Empty);
        tcp_err_const(i, cx, l, t.tcp.write_head, t.tcp.ec_timeout);
        i.end();
        // The host dropped the send stream: the future names why.
        i.local_get(tl.word).i32_const(7).i32_and().i32_const(COPY_DROPPED).i32_eq().local_get(tl.y).i32_const(0).i32_ne().i32_and();
        i.if_(BlockType::Empty);
        i.i32_const(NET_RESERVE).call(g.f_reserve);
        i.local_get(tl.p).i32_load(net_m32(TS_TXF));
        i.local_get(tl.p).i32_load(net_m32(TS_TXF)).i32_const(ret).call(imp.at(NI_T_SFREAD));
        i.call(g.f_await).drop();
        i.i32_const(t.tcp.ec_broken).local_set(l.last);
        i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
        i.i32_const(ret).i32_load8_u(net_m8(a.tcp_unit_payload)).local_set(l.last);
        i.end();
        tcp_err_class(i, cx, l, t.tcp.write_head, l.last);
        i.end();
        i.br(0).end().end();
        net_answer(i, g, ng, l, 0);
    });

    // ── tcp_available(stream = a): what one copy moves without waiting ──
    tcp_op(&mut i, OP_TCP_AVAILABLE, |i| {
        handle(i);
        tcp_slot_get(i, cx, l, tl, streams);
        i.local_get(tl.p).i32_load(net_m32(TS_LLEN)).local_set(l.k);
        i.local_get(l.k).i32_const(TCP_CHUNK).i32_ge_u();
        i.local_get(tl.p).i32_load(net_m32(TS_EOF)).i32_const(0).i32_ne().i32_or();
        i.i32_eqz().if_(BlockType::Empty);
        // The lookahead buffer, allocated once per slot; its unread bytes
        // move to its front.
        i.local_get(tl.p).i32_load(net_m32(TS_LOOK)).i32_eqz().if_(BlockType::Empty);
        i.local_get(tl.p);
        i.i32_const(0).i32_const(0).i32_const(8).i32_const(TCP_CHUNK).call(g.f_alloc);
        i.i32_store(net_m32(TS_LOOK));
        i.global_get(g.heap).local_set(l.h0);
        i.end();
        i.local_get(tl.p).i32_load(net_m32(TS_LOOK)).local_tee(l.i1);
        i.local_get(l.i1).local_get(tl.p).i32_load(net_m32(TS_LOFF)).i32_add().local_get(l.k).memory_copy(0, 0);
        i.local_get(tl.p).i32_const(0).i32_store(net_m32(TS_LOFF));
        tcp_ensure_rx(i, cx, a, imp, tl);
        i.local_get(tl.p).i32_load(net_m32(TS_RX));
        i.local_get(l.i1).local_get(l.k).i32_add();
        i.i32_const(TCP_CHUNK).local_get(l.k).i32_sub();
        i.call(imp.at(NI_T_RREAD)).local_tee(tl.word);
        i.i32_const(-1).i32_eq().if_(BlockType::Empty);
        i.local_get(tl.p).i32_load(net_m32(TS_RX)).call(imp.at(NI_T_RCANCEL)).local_set(tl.word);
        i.end();
        i.local_get(l.k).local_get(tl.word).i32_const(4).i32_shr_u().i32_add().local_set(l.k);
        i.local_get(tl.p).local_get(l.k).i32_store(net_m32(TS_LLEN));
        i.local_get(tl.word).i32_const(7).i32_and().i32_const(COPY_DROPPED).i32_eq().if_(BlockType::Empty);
        i.local_get(tl.p).i32_const(1).i32_store(net_m32(TS_EOF));
        i.end();
        i.end();
        i.local_get(l.w).local_get(l.k).i32_const(TCP_CHUNK).local_get(l.k).i32_const(TCP_CHUNK).i32_lt_u().select();
        i.i64_extend_i32_u().call(fns.dec).local_set(l.w);
        net_answer(i, g, ng, l, 0);
    });

    // ── tcp_set_timeout(stream = a, ms = b): <= 0 clears it ──
    tcp_op(&mut i, OP_TCP_SET_TIMEOUT, |i| {
        handle(i);
        tcp_slot_get(i, cx, l, tl, streams);
        i.local_get(b_ptr).local_get(b_len).call(fns.int).local_tee(tl.ms);
        i.local_get(tl.p);
        i.local_get(tl.ms).i64_const(0).local_get(tl.ms).i64_const(0).i64_gt_s().select();
        i.i64_store(tmo);
        i.drop();
        net_answer(i, g, ng, l, 0);
    });

    // ── close(stream) / close(listener): a closed one is ok, a never-issued one is not ──
    tcp_op(&mut i, OP_TCP_CLOSE, |i| {
        handle(i);
        tcp_invalid_check(i, cx, l, streams);
        i.global_get(streams.tab).local_get(l.h).i32_wrap_i64().i32_const(TS_STRIDE).i32_mul().i32_add().local_tee(tl.p);
        i.i32_load(net_m32(TS_SOCK)).i32_const(0).i32_ge_s().if_(BlockType::Empty);
        // The send stream ends first and its future is awaited, so every
        // byte a write handed over reaches the peer before the close (native's
        // close leaves them in the kernel's buffer to go out).
        i.local_get(tl.p).i32_load(net_m32(TS_TX)).i32_const(0).i32_ge_s().if_(BlockType::Empty);
        i.local_get(tl.p).i32_load(net_m32(TS_TX)).call(imp.at(NI_T_SDROP));
        i.i32_const(NET_RESERVE).call(g.f_reserve);
        i.local_get(tl.p).i32_load(net_m32(TS_TXF));
        i.local_get(tl.p).i32_load(net_m32(TS_TXF)).i32_const(ret).call(imp.at(NI_T_SFREAD));
        i.call(g.f_await).drop();
        i.local_get(tl.p).i32_load(net_m32(TS_TXF)).call(imp.at(NI_T_SFDROP));
        i.end();
        i.local_get(tl.p).i32_load(net_m32(TS_RX)).i32_const(0).i32_ge_s().if_(BlockType::Empty);
        i.local_get(tl.p).i32_load(net_m32(TS_RX)).call(imp.at(NI_T_RDROP));
        i.local_get(tl.p).i32_load(net_m32(TS_RXF)).call(imp.at(NI_T_RFDROP));
        i.end();
        i.local_get(tl.p).i32_load(net_m32(TS_SOCK)).call(imp.at(NI_T_DROP));
        i.local_get(tl.p).i32_const(-1).i32_store(net_m32(TS_SOCK));
        i.end();
        net_answer(i, g, ng, l, 0);
    });
    tcp_op(&mut i, OP_TCP_CLOSE_LISTENER, |i| {
        handle(i);
        tcp_invalid_check(i, cx, l, listeners);
        i.global_get(listeners.tab).local_get(l.h).i32_wrap_i64().i32_const(TL_STRIDE).i32_mul().i32_add().local_tee(tl.p);
        i.i32_load(net_m32(TL_SOCK)).i32_const(0).i32_ge_s().if_(BlockType::Empty);
        i.local_get(tl.p).i32_load(net_m32(TL_STREAM)).call(imp.at(NI_T_LDROP));
        i.local_get(tl.p).i32_load(net_m32(TL_SOCK)).call(imp.at(NI_T_DROP));
        i.local_get(tl.p).i32_const(-1).i32_store(net_m32(TL_SOCK));
        i.end();
        net_answer(i, g, ng, l, 0);
    });

    // ── tcp_is_open(stream = a): "1" or "0", never an err ──
    tcp_op(&mut i, OP_TCP_IS_OPEN, |i| {
        handle(i);
        i.i32_const('0' as i32).local_set(l.k);
        i.local_get(l.h).global_get(streams.n).i64_extend_i32_u().i64_lt_u().if_(BlockType::Empty);
        i.global_get(streams.tab).local_get(l.h).i32_wrap_i64().i32_const(TS_STRIDE).i32_mul().i32_add();
        i.i32_load(net_m32(TS_SOCK)).i32_const(0).i32_ge_s().if_(BlockType::Empty);
        i.i32_const('1' as i32).local_set(l.k);
        i.end();
        i.end();
        i.local_get(l.w).local_get(l.k).i32_store8(net_m8(0));
        i.local_get(l.w).i32_const(1).i32_add().local_set(l.w);
        net_answer(i, g, ng, l, 0);
    });

    i.unreachable();
    i.end();
    f
}

/// `max` = min(`max`, 64 KiB): one read takes at most a chunk (native's
/// `almide_net_read_once`); a zero length stays zero.
fn tcp_clamp_chunk(i: &mut Sink<'_>, l: NetLocals) {
    i.local_get(l.max).i64_const(TCP_CHUNK as i64).i64_gt_s().if_(BlockType::Empty);
    i.i64_const(TCP_CHUNK as i64).local_set(l.max);
    i.end();
}
