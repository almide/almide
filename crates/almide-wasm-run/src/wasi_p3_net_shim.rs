// `include!`d part of wasi_p3.rs (#4306, wasi_p3_net.rs): the net shim itself (ops 140..=145) and its emitters.

/// What the shim's emitters read besides the locals: the shared globals,
/// the texts, the helpers' indices and the net globals.
#[derive(Clone, Copy)]
struct NetCx<'a> {
    g: P3Globals,
    t: &'a NetTexts,
    fns: NetFns,
    ng: NetGlobals,
}

/// The net block's own indices: its helpers, its globals, its imports.
#[derive(Clone, Copy)]
struct NetParts {
    fns: NetFns,
    ng: NetGlobals,
    imp: NetImports,
}

/// The shim's locals, named once.
#[derive(Clone, Copy)]
struct NetLocals {
    w: u32,
    h0: u32,
    s: u32,
    k: u32,
    cnt: u32,
    el: u32,
    last: u32,
    i1: u32,
    i2: u32,
    hp: u32,
    hl: u32,
    n: u32,
    j: u32,
    h: u32,
    port: u32,
    max: u32,
}

/// `(op, a_ptr, a_len, b_ptr, b_len) -> i64` over the fs_call ABI: ops
/// 140..=145 with stdlib/net_wasm.almd's operands, answered as the
/// embedded host's net_host.rs answers them (status 0 = ok, 1 = err, the
/// answer parked for `host_read`).
fn shim_net(g: P3Globals, a: &NetAbi, t: &NetTexts, parts: NetParts) -> Function {
    let NetParts { fns, ng, imp } = parts;
    let cx = NetCx { g, t, fns, ng };
    let (op, a_ptr, a_len, b_ptr, b_len) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let l = NetLocals {
        w: 5, h0: 6, s: 7, k: 8, cnt: 9, el: 10, last: 11, i1: 12, i2: 13, hp: 14, hl: 15, n: 16, j: 17,
        h: 18, port: 19, max: 20,
    };
    let ret = (g.park + STATRET) as i32;
    let mut f = Function::new([(13, ValType::I32), (3, ValType::I64)]);
    let mut i = f.instructions();

    // The answer buffer holds any head (12 bytes per host byte covers the
    // widest escape) — and, for a receive, a whole datagram past its line.
    i.local_get(a_len).i32_const(12).i32_mul().i32_const(512).i32_add();
    i.i32_const(NET_DATAGRAM + 128).i32_const(0).local_get(op).i32_const(OP_NET_RECV_FROM).i32_eq().select();
    i.i32_add().local_set(l.n);
    net_ensure(&mut i, g, ng, l, l.n);
    i.global_get(g.heap).local_set(l.h0);
    i.global_get(ng.buf).local_set(l.w);

    // ── udp_bind(host = a, port = b) ──
    i.local_get(op).i32_const(OP_NET_BIND).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_set(l.hp);
    i.local_get(a_len).local_set(l.hl);
    i.local_get(b_ptr).local_get(b_len).call(fns.int).local_set(l.port);
    net_port_check(&mut i, cx, l, t.bind_open);
    net_resolve_or_err(&mut i, cx, l, t.bind_open);
    // Each address in turn; the last failure classifies the err.
    i.i32_const(0).local_set(l.k);
    i.global_get(ng.addrs).local_set(l.el);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.k).local_get(l.cnt).i32_ge_u().br_if(1);
    i.block(BlockType::Empty); // the next address
    i.i32_const(NET_RESERVE).call(g.f_reserve);
    i.i32_const(a.fam[0]).i32_const(a.fam[1]);
    i.local_get(l.el).i32_load8_u(net_m8(0)).i32_const(a.ia_case[0]).i32_eq().select();
    i.i32_const(ret).call(imp.at(NI_CREATE));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    i.i32_const(ret).i32_load8_u(net_m8(a.create_payload)).local_set(l.last);
    i.br(1);
    i.end();
    i.i32_const(ret).i32_load(net_m32(a.create_payload)).local_set(l.s);
    i.local_get(l.el).local_get(l.port).i32_wrap_i64().call(fns.sa);
    i.i32_const(NET_RESERVE).call(g.f_reserve);
    i.local_get(l.s);
    net_push_sa(&mut i, a, t);
    i.i32_const(ret).call(imp.at(NI_BIND));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    i.i32_const(ret).i32_load8_u(net_m8(a.unit_payload)).local_set(l.last);
    i.local_get(l.s).call(imp.at(NI_DROP));
    i.br(1);
    i.end();
    // Bound: the lowest free slot holds it.
    net_slot_put(&mut i, g, ng, l);
    i.local_get(l.w).local_get(l.j).i64_extend_i32_u().call(fns.dec).local_set(l.w);
    net_answer(&mut i, g, ng, l, 0);
    i.end();
    i.local_get(l.el).i32_const(a.ia_size as i32).i32_add().local_set(l.el);
    i.local_get(l.k).i32_const(1).i32_add().local_set(l.k);
    i.br(0).end().end();
    net_head(&mut i, t, fns, l, t.bind_open);
    net_put(&mut i, l.w, t.class_sep);
    net_put_class(&mut i, t, l, l.last);
    net_answer(&mut i, g, ng, l, 1);
    i.end();

    // ── udp_local_port(sock = a) ──
    i.local_get(op).i32_const(OP_NET_LOCAL_PORT).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_get(a_len).call(fns.int).local_set(l.h);
    net_slot_get(&mut i, g, t, fns, ng, l);
    i.i32_const(NET_RESERVE).call(g.f_reserve);
    i.local_get(l.s).i32_const(ret).call(imp.at(NI_LOCAL));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    net_put(&mut i, l.w, t.local_head);
    net_put(&mut i, l.w, t.class_sep);
    i.i32_const(ret).i32_load8_u(net_m8(a.local_payload)).local_set(l.last);
    net_put_class(&mut i, t, l, l.last);
    net_answer(&mut i, g, ng, l, 1);
    i.end();
    i.local_get(l.w);
    net_sa_port(&mut i, a, ret + a.local_payload as i32);
    i.i64_extend_i32_u().call(fns.dec).local_set(l.w);
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    // ── udp_send_to(a = `<sock>\n<port>\n<host>`, b = the datagram) ──
    i.local_get(op).i32_const(OP_NET_SEND_TO).i32_eq().if_(BlockType::Empty);
    net_find_nl(&mut i, l, a_ptr, a_len, 0, l.i1);
    net_find_nl(&mut i, l, a_ptr, a_len, 1, l.i2);
    i.local_get(a_ptr).local_get(l.i1).call(fns.int).local_set(l.h);
    i.local_get(a_ptr).local_get(l.i1).i32_add().i32_const(1).i32_add();
    i.local_get(l.i2).local_get(l.i1).i32_sub().i32_const(1).i32_sub().call(fns.int).local_set(l.port);
    i.local_get(a_ptr).local_get(l.i2).i32_add().i32_const(1).i32_add().local_set(l.hp);
    i.local_get(a_len).local_get(l.i2).i32_sub().i32_const(1).i32_sub().local_set(l.hl);
    // The socket first (net_host's `datagram` checks and clones it before
    // the destination resolves), then the destination.
    net_slot_get(&mut i, g, t, fns, ng, l);
    net_port_check(&mut i, cx, l, t.send_open);
    net_resolve_or_err(&mut i, cx, l, t.send_open);
    i.global_get(ng.addrs).local_get(l.port).i32_wrap_i64().call(fns.sa);
    i.i32_const(NET_RESERVE).call(g.f_reserve);
    i.local_get(l.s).local_get(b_ptr).local_get(b_len).i32_const(1); // some(address)
    net_push_sa(&mut i, a, t);
    i.i32_const(ret).call(imp.at(NI_SEND));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    net_put(&mut i, l.w, t.send_head);
    net_put(&mut i, l.w, t.class_sep);
    i.i32_const(ret).i32_load8_u(net_m8(a.unit_payload)).local_set(l.last);
    net_put_class(&mut i, t, l, l.last);
    net_answer(&mut i, g, ng, l, 1);
    i.end();
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    // ── udp_recv_from(sock = a, max = b) ──
    i.local_get(op).i32_const(OP_NET_RECV_FROM).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_get(a_len).call(fns.int).local_set(l.h);
    net_slot_get(&mut i, g, t, fns, ng, l);
    i.local_get(b_ptr).local_get(b_len).call(fns.int).local_tee(l.max);
    i.i64_const(0).i64_lt_s().if_(BlockType::Empty);
    net_put(&mut i, l.w, t.negative[0]);
    i.local_get(l.w).local_get(l.max).call(fns.dec).local_set(l.w);
    net_put(&mut i, l.w, t.negative[1]);
    net_answer(&mut i, g, ng, l, 1);
    i.end();
    i.i32_const(NET_RECV_RESERVE).call(g.f_reserve);
    i.local_get(l.s).i32_const(ret).call(imp.at(NI_RECEIVE));
    i.i32_const(ret).i32_load8_u(net_m8(0)).if_(BlockType::Empty);
    net_put(&mut i, l.w, t.recv_head);
    net_put(&mut i, l.w, t.class_sep);
    i.i32_const(ret).i32_load8_u(net_m8(a.recv_payload)).local_set(l.last);
    net_put_class(&mut i, t, l, l.last);
    net_answer(&mut i, g, ng, l, 1);
    i.end();
    // n = min(datagram length, max, 65535): the rest is dropped, as the OS
    // drops it natively.
    let rp = a.recv_payload;
    i.i32_const(ret).i32_load(net_m32(rp + a.recv_list + 4)).local_set(l.n);
    i.local_get(l.max).i64_const(NET_DATAGRAM as i64).i64_lt_s().if_(BlockType::Empty);
    i.local_get(l.max).i32_wrap_i64().local_get(l.n).i32_lt_u().if_(BlockType::Empty);
    i.local_get(l.max).i32_wrap_i64().local_set(l.n);
    i.end();
    i.else_();
    i.local_get(l.n).i32_const(NET_DATAGRAM).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(NET_DATAGRAM).local_set(l.n);
    i.end();
    i.end();
    // `<port> <ip>\n`, then the bytes.
    let sa_at = ret + (rp + a.recv_addr) as i32;
    i.local_get(l.w);
    net_sa_port(&mut i, a, sa_at);
    i.i64_extend_i32_u().call(fns.dec).local_set(l.w);
    net_put_byte(&mut i, l.w, b' ');
    net_sa_to_el(&mut i, a, sa_at, t.scratch(NET_SCR_EL2));
    i.local_get(l.w).i32_const(t.scratch(NET_SCR_EL2)).call(fns.ip_fmt).local_set(l.w);
    net_put_byte(&mut i, l.w, b'\n');
    i.local_get(l.w).i32_const(ret).i32_load(net_m32(rp + a.recv_list)).local_get(l.n).memory_copy(0, 0);
    i.local_get(l.w).local_get(l.n).i32_add().local_set(l.w);
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    // ── close(sock = a): a closed socket is ok, a never-issued one is not ──
    i.local_get(op).i32_const(OP_NET_CLOSE).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_get(a_len).call(fns.int).local_set(l.h);
    net_invalid_check(&mut i, g, t, fns, ng, l);
    i.global_get(ng.tab).local_get(l.h).i32_wrap_i64().i32_const(2).i32_shl().i32_add().local_tee(l.el);
    i.i32_load(net_m32(0)).local_tee(l.s).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.local_get(l.s).call(imp.at(NI_DROP));
    i.local_get(l.el).i32_const(-1).i32_store(net_m32(0));
    i.end();
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    // ── lookup_host(host = a): one address per line, each once ──
    i.local_get(op).i32_const(OP_NET_LOOKUP).i32_eq().if_(BlockType::Empty);
    i.local_get(a_ptr).local_set(l.hp);
    i.local_get(a_len).local_set(l.hl);
    i.i64_const(0).local_set(l.port);
    net_resolve_or_err(&mut i, cx, l, t.lookup_open);
    // 48 bytes per address (the longest spelling and its newline).
    i.local_get(l.cnt).i32_const(48).i32_mul().i32_const(64).i32_add().local_set(l.n);
    net_ensure(&mut i, g, ng, l, l.n);
    i.global_get(ng.buf).local_set(l.w);
    i.i32_const(0).local_set(l.k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.k).local_get(l.cnt).i32_ge_u().br_if(1);
    i.global_get(ng.addrs).local_get(l.k).i32_const(a.ia_size as i32).i32_mul().i32_add().local_set(l.el);
    // Seen before? j walks the earlier elements; i1 = 1 on a match.
    i.i32_const(0).local_set(l.i1);
    i.i32_const(0).local_set(l.j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.j).local_get(l.k).i32_ge_u().br_if(1);
    i.global_get(ng.addrs).local_get(l.j).i32_const(a.ia_size as i32).i32_mul().i32_add().local_set(l.i2);
    net_same_ip(&mut i, a, l.el, l.i2);
    i.if_(BlockType::Empty).i32_const(1).local_set(l.i1).br(2).end();
    i.local_get(l.j).i32_const(1).i32_add().local_set(l.j);
    i.br(0).end().end();
    i.local_get(l.i1).i32_eqz().if_(BlockType::Empty);
    i.local_get(l.w).global_get(ng.buf).i32_ne().if_(BlockType::Empty);
    net_put_byte(&mut i, l.w, b'\n');
    i.end();
    i.local_get(l.w).local_get(l.el).call(fns.ip_fmt).local_set(l.w);
    i.end();
    i.local_get(l.k).i32_const(1).i32_add().local_set(l.k);
    i.br(0).end().end();
    net_answer(&mut i, g, ng, l, 0);
    i.end();

    i.unreachable();
    i.end();
    f
}

/// Grow the answer buffer to hold `need` bytes (the old one is abandoned;
/// it only ever grows). A grow moves the op's rewind mark past the new
/// buffer, so the rewind never frees it.
fn net_ensure(i: &mut Sink<'_>, g: P3Globals, ng: NetGlobals, l: NetLocals, need: u32) {
    i.local_get(need).global_get(ng.bufcap).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(0).i32_const(0).i32_const(8).local_get(need).call(g.f_alloc).global_set(ng.buf);
    i.local_get(need).global_set(ng.bufcap);
    i.global_get(g.heap).local_set(l.h0);
    i.end();
}

/// Rewind the bump frontier to where the op found it (what the host lowered
/// for this op is dead), park `buf[..w]` and answer `status`.
fn net_answer(i: &mut Sink<'_>, g: P3Globals, ng: NetGlobals, l: NetLocals, status: i64) {
    i.local_get(l.h0).global_set(g.heap);
    i.global_get(ng.buf).global_set(g.g_ppos);
    i.local_get(l.w).global_get(ng.buf).i32_sub().global_set(g.g_plen);
    i.global_get(g.g_plen).i64_extend_i32_u().i64_const(status << 32).i64_or().return_();
}

/// `<open><escaped host><colon><port><close>`: a bind / send / lookup head.
fn net_head(i: &mut Sink<'_>, t: &NetTexts, fns: NetFns, l: NetLocals, open: NetPiece) {
    net_put(i, l.w, open);
    i.local_get(l.w).local_get(l.hp).local_get(l.hl).call(fns.esc).local_set(l.w);
    net_put(i, l.w, t.colon);
    i.local_get(l.w).local_get(l.port).call(fns.dec).local_set(l.w);
    net_put(i, l.w, t.close);
}

/// A port outside 0..=65535: `<head>: port out of range 0..=65535`.
fn net_port_check(i: &mut Sink<'_>, cx: NetCx<'_>, l: NetLocals, open: NetPiece) {
    let NetCx { g, t, fns, ng } = cx;
    i.local_get(l.port).i64_const(65535).i64_gt_u().if_(BlockType::Empty);
    net_head(i, t, fns, l, open);
    net_put(i, l.w, t.port_range);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// Resolve `(hp, hl)` into `cnt`; nothing is `<head>: cannot resolve host
/// "<escaped host>"`.
fn net_resolve_or_err(i: &mut Sink<'_>, cx: NetCx<'_>, l: NetLocals, open: NetPiece) {
    let NetCx { g, t, fns, ng } = cx;
    i.local_get(l.hp).local_get(l.hl).call(fns.resolve).local_tee(l.cnt);
    i.i32_eqz().if_(BlockType::Empty);
    net_head(i, t, fns, l, open);
    net_put(i, l.w, t.unres[0]);
    i.local_get(l.w).local_get(l.hp).local_get(l.hl).call(fns.esc).local_set(l.w);
    net_put(i, l.w, t.unres[1]);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// The class text of the `error-code` discriminant in local `disc`.
fn net_put_class(i: &mut Sink<'_>, t: &NetTexts, l: NetLocals, disc: u32) {
    i.local_get(disc).i32_const(3).i32_shl().i32_const(t.ec_table).i32_add().local_set(l.i2);
    i.local_get(l.w).local_get(l.i2).i32_load(net_m32(0)).local_get(l.i2).i32_load(net_m32(4)).memory_copy(0, 0);
    i.local_get(l.w).local_get(l.i2).i32_load(net_m32(4)).i32_add().local_set(l.w);
}

/// A handle past the table (a negative one included): `invalid UdpSocket
/// handle: <h>`.
fn net_invalid_check(i: &mut Sink<'_>, g: P3Globals, t: &NetTexts, fns: NetFns, ng: NetGlobals, l: NetLocals) {
    i.local_get(l.h).global_get(ng.n).i64_extend_i32_u().i64_ge_u().if_(BlockType::Empty);
    net_put(i, l.w, t.invalid[0]);
    i.local_get(l.w).local_get(l.h).call(fns.dec).local_set(l.w);
    net_put(i, l.w, t.invalid[1]);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// `s` = the socket in slot `h`, or the invalid / closed err.
fn net_slot_get(i: &mut Sink<'_>, g: P3Globals, t: &NetTexts, fns: NetFns, ng: NetGlobals, l: NetLocals) {
    net_invalid_check(i, g, t, fns, ng, l);
    i.global_get(ng.tab).local_get(l.h).i32_wrap_i64().i32_const(2).i32_shl().i32_add().i32_load(net_m32(0)).local_tee(l.s);
    i.i32_const(0).i32_lt_s().if_(BlockType::Empty);
    net_put(i, l.w, t.closed[0]);
    i.local_get(l.w).local_get(l.h).call(fns.dec).local_set(l.w);
    net_put(i, l.w, t.closed[1]);
    net_answer(i, g, ng, l, 1);
    i.end();
}

/// Put socket `s` in the lowest free slot (native's `almide_net_slot`),
/// its index in `j`; a full table doubles (the rewind mark moves past it).
fn net_slot_put(i: &mut Sink<'_>, g: P3Globals, ng: NetGlobals, l: NetLocals) {
    i.i32_const(0).local_set(l.j);
    i.block(BlockType::Empty);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.j).global_get(ng.n).i32_ge_u().br_if(1);
    i.global_get(ng.tab).local_get(l.j).i32_const(2).i32_shl().i32_add().local_tee(l.i1);
    i.i32_load(net_m32(0)).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.local_get(l.i1).local_get(l.s).i32_store(net_m32(0));
    i.br(3); // done
    i.end();
    i.local_get(l.j).i32_const(1).i32_add().local_set(l.j);
    i.br(0).end().end();
    i.global_get(ng.n).global_get(ng.cap).i32_eq().if_(BlockType::Empty);
    // The new capacity: double, eight at first.
    i.global_get(ng.cap).i32_const(1).i32_shl().i32_const(8).global_get(ng.cap).select().local_set(l.i1);
    i.i32_const(0).i32_const(0).i32_const(8).local_get(l.i1).i32_const(2).i32_shl().call(g.f_alloc).local_set(l.i2);
    i.local_get(l.i2).global_get(ng.tab).global_get(ng.n).i32_const(2).i32_shl().memory_copy(0, 0);
    i.local_get(l.i2).global_set(ng.tab);
    i.local_get(l.i1).global_set(ng.cap);
    i.global_get(g.heap).local_set(l.h0);
    i.end();
    i.global_get(ng.tab).global_get(ng.n).i32_const(2).i32_shl().i32_add().local_get(l.s).i32_store(net_m32(0));
    i.global_get(ng.n).local_set(l.j);
    i.global_get(ng.n).i32_const(1).i32_add().global_set(ng.n);
    i.end();
}

/// Push the flat `ip-socket-address` operands `$sa` staged.
fn net_push_sa(i: &mut Sink<'_>, a: &NetAbi, t: &NetTexts) {
    for s in 0..a.flat_len {
        i.i32_const(0).i32_load(net_m32((t.scratch(NET_SCR_SA) + 4 * s as i32) as u64));
    }
}

/// The port of the `ip-socket-address` at `at`.
fn net_sa_port(i: &mut Sink<'_>, a: &NetAbi, at: i32) {
    i.i32_const(0).i32_load16_u(net_m16((at as u64) + a.sa_payload + a.sa4_port));
    i.i32_const(0).i32_load16_u(net_m16((at as u64) + a.sa_payload + a.sa6_port));
    i.i32_const(0).i32_load8_u(net_m8(at as u64)).i32_const(a.sa_case[0]).i32_eq().select();
}

/// The `ip-socket-address` at `at` as an `ip-address` element at `el`.
fn net_sa_to_el(i: &mut Sink<'_>, a: &NetAbi, at: i32, el: i32) {
    let at = at as u64;
    i.i32_const(0).i32_load8_u(net_m8(at)).i32_const(a.sa_case[0]).i32_eq().if_(BlockType::Empty);
    i.i32_const(el).i32_const(a.ia_case[0]).i32_store8(net_m8(0));
    i.i32_const(el + a.ia_payload as i32).i32_const((at + a.sa_payload + a.sa4_addr) as i32).i32_const(4).memory_copy(0, 0);
    i.else_();
    i.i32_const(el).i32_const(a.ia_case[1]).i32_store8(net_m8(0));
    i.i32_const(el + a.ia_payload as i32).i32_const((at + a.sa_payload + a.sa6_addr) as i32).i32_const(16).memory_copy(0, 0);
    i.end();
}

/// Whether the `ip-address` elements at locals `x` and `y` are one address:
/// the same case and the same payload bytes (never the padding).
fn net_same_ip(i: &mut Sink<'_>, a: &NetAbi, x: u32, y: u32) {
    let po = a.ia_payload;
    i.local_get(x).i32_load8_u(net_m8(0)).local_get(y).i32_load8_u(net_m8(0)).i32_eq();
    // Four bytes for either case, then twelve more for IPv6.
    i.local_get(x).i32_load(MemArg { offset: po, align: 0, memory_index: 0 });
    i.local_get(y).i32_load(MemArg { offset: po, align: 0, memory_index: 0 }).i32_eq().i32_and();
    i.local_get(x).i32_load8_u(net_m8(0)).i32_const(a.ia_case[0]).i32_eq();
    i.i32_const(1);
    for off in [4u64, 8, 12] {
        i.local_get(x).i32_load(MemArg { offset: po + off, align: 0, memory_index: 0 });
        i.local_get(y).i32_load(MemArg { offset: po + off, align: 0, memory_index: 0 }).i32_eq().i32_and();
    }
    i.i32_or().i32_and();
}

/// `dst` = the index of the `nth` (0 or 1) `\n` of `a[..a_len]`.
fn net_find_nl(i: &mut Sink<'_>, l: NetLocals, a_ptr: u32, a_len: u32, nth: u32, dst: u32) {
    if nth == 0 {
        i.i32_const(0).local_set(dst);
    } else {
        i.local_get(l.i1).i32_const(1).i32_add().local_set(dst);
    }
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(dst).local_get(a_len).i32_ge_u().br_if(1);
    i.local_get(a_ptr).local_get(dst).i32_add().i32_load8_u(net_m8(0)).i32_const('\n' as i32).i32_eq().br_if(1);
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    i.br(0).end().end();
}

/// The net functions' bodies, in `NetFns` order.
fn push_net_shims(code: &mut CodeSection, g: P3Globals, a: &NetAbi, t: &NetTexts, parts: NetParts) {
    let fns = parts.fns;
    code.function(&shim_net(g, a, t, parts));
    code.function(&net_fn_esc());
    code.function(&net_fn_dec());
    code.function(&net_fn_int());
    code.function(&net_fn_v4());
    code.function(&net_fn_side(fns.v4));
    code.function(&net_fn_ip_parse(a, t, fns));
    code.function(&net_fn_ip_fmt(a, t, fns));
    code.function(&net_fn_sa(a, t));
    code.function(&net_fn_resolve(g, a, t, parts));
}
