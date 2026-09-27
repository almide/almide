// `include!`d part of wasi_p3.rs (codopsy max-lines split — shares the parent
// module's imports and items; nothing here is pub beyond the parent).
//
// `http.serve` on the STOCK artifact (#2659, C-367): the guest-owned serve
// loop (stdlib/http_serve.almd) calls four byte-moving host ops, and this
// block serves them over `wasi:sockets/types@0.3.0` so `wasmtime run -S p3
// -S inherit-network` runs the server. It is the same loop the embedded host
// serves (host_serve.rs): main runs once, one instance answers every request
// sequentially, and the request parse and the response bytes are the
// guest's, so both wasm legs share one code path for everything the client
// sees.
//
//   70 bind    a = the port in decimal. tcp-socket.create(ipv4), bind
//              0.0.0.0:<port>, listen; the listen stream is kept.
//   73 accept  a sync `stream.read` of ONE tcp-socket off the listen stream
//              (blocks until a client connects), then `receive` on it. A
//              connection still held (a request the guest dropped) closes.
//   74 recv    a sync read of up to RECV_CAP bytes from the receive stream
//              into the park's DATA span; 0 bytes = the peer closed. At
//              the stream's end its future says whether the read failed.
//   75 send    a fresh stream<u8> handed to `send`, written in full, its
//              writable end dropped and the completion future read (every
//              byte left the guest), then the connection closes.
//
// Plus op 29 (`env.args`): `wasi:cli/environment.get-arguments`, re-framed
// as the almide frames encoding (`[len u32][bytes]` per entry, the runner's
// argv0 first — the guest skips it, as on every leg).
//
// Import signatures are DERIVED from the vendored WIT (`wasm_signature`),
// never hand-counted: `bind`'s flattened ip-socket-address is twelve core
// params, and a drifted count would mis-wire the port.

/// The serve block's import indices, appended after the fs table (and the
/// http block when present). `args` is op 29's get-arguments.
#[derive(Clone, Copy)]
struct ServeImports {
    create: u32,
    bind: u32,
    listen: u32,
    lst_read: u32,
    receive: u32,
    rx_read: u32,
    rx_drop: u32,
    rxf_read: u32,
    rxf_drop: u32,
    send: u32,
    tx_new: u32,
    tx_write: u32,
    tx_drop: u32,
    txf_read: u32,
    txf_drop: u32,
    sock_drop: u32,
}

/// The serve shim's globals: the listening socket and its stream, the
/// connection being served, its receive stream + future, and its EOF flag.
#[derive(Clone, Copy)]
struct ServeGlobals {
    lsock: u32,
    lst: u32,
    conn: u32,
    crx: u32,
    crxf: u32,
    ceof: u32,
}

/// Canonical-ABI facts the serve shim stores through, derived from the
/// vendored wasi:sockets WIT (the fs_abi doctrine).
struct SockAbi {
    fam_ipv4: i32,
    sa_ipv4: i32,
    /// result<own<_>, error-code> / result<stream<_>, error-code> payload offset.
    handle_payload: u64,
    /// result<_, error-code> payload offset.
    unit_payload: u64,
    /// `bind`'s core params (self, the flattened address, retptr).
    bind_params: Vec<ValType>,
    /// error-code case names, in discriminant order.
    ec_names: Vec<String>,
}

// The serve statics live on the park's fifth page — the p1 env overlay's
// page (OVL), which the p3 shims never use.
const SV_PORT_MSG: u64 = crate::wasi::OVL;
const SV_GEN_MSG: u64 = crate::wasi::OVL + 64;
const SV_LENS: u64 = crate::wasi::OVL + 128;
const SV_BIND_TAB: u64 = crate::wasi::OVL + 256;
const SV_BIND_STRIDE: u64 = 128;
/// `to_socket_addrs`' text for a port that is not a u16 — native's line.
const E_PORT: &[u8] = b"bind failed: invalid port value";
const E_SERVE_GEN: &[u8] = b"http.serve: the connection failed";
/// One receive read's ceiling (the native reader's BufReader size).
const RECV_CAP: i32 = 8192;
/// The reservation before a call whose err can land an `other(string)`.
const ERR_RESERVE: i32 = 4096;
/// get-arguments' list lands via cabi_realloc at a length the host
/// chooses: a declared ceiling, as the preopen table's (#2119).
const ARGS_RESERVE: i32 = 65536;
// Stream/future status codes (the canonical ABI's copy result low nibble).
const COPY_COMPLETED: i32 = 0;

const _: () = {
    assert!(SV_PORT_MSG + E_PORT.len() as u64 <= SV_GEN_MSG);
    assert!(SV_GEN_MSG + E_SERVE_GEN.len() as u64 <= SV_LENS);
    assert!(crate::wasi::DATA + RECV_CAP as u64 <= crate::wasi::OVL);
};

/// The human text for one error-code case of a failed bind: the case name
/// with spaces, and the runtime flag a denied socket most often means.
fn bind_err_text(case: &str) -> String {
    let name = case.replace('-', " ");
    if case == "access-denied" {
        format!("bind failed: {name} (the runtime grants no network: run with -S inherit-network)")
    } else {
        format!("bind failed: {name}")
    }
}

fn core_ty(t: wit_parser::abi::WasmType) -> ValType {
    use wit_parser::abi::WasmType as W;
    match t {
        W::I32 | W::Pointer | W::Length => ValType::I32,
        W::I64 | W::PointerOrI64 => ValType::I64,
        W::F32 => ValType::F32,
        W::F64 => ValType::F64,
    }
}

/// The core signature of an imported WIT function, as the canonical ABI
/// lowers it (retptr included).
fn wit_import_sig(
    resolve: &wit_parser::Resolve,
    pkg: &str,
    iface: &str,
    func: &str,
) -> anyhow::Result<(Vec<ValType>, Vec<ValType>)> {
    let (_, p) = resolve
        .packages
        .iter()
        .find(|(_, p)| format!("{}:{}", p.name.namespace, p.name.name) == pkg)
        .ok_or_else(|| anyhow::anyhow!("{pkg} package not in the resolve"))?;
    let id = *p.interfaces.get(iface).ok_or_else(|| anyhow::anyhow!("{pkg}/{iface} not found"))?;
    let f = resolve.interfaces[id]
        .functions
        .get(func)
        .ok_or_else(|| anyhow::anyhow!("{pkg}/{iface}#{func} not found"))?;
    let sig = resolve.wasm_signature(wit_parser::abi::AbiVariant::GuestImport, f);
    Ok((sig.params.into_iter().map(core_ty).collect(), sig.results.into_iter().map(core_ty).collect()))
}

fn sock_abi(resolve: &wit_parser::Resolve) -> anyhow::Result<SockAbi> {
    use wit_parser::{Type, TypeDefKind};
    let (_, pkg) = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "wasi" && p.name.name == "sockets")
        .ok_or_else(|| anyhow::anyhow!("wasi:sockets package not in the resolve"))?;
    let iface = &resolve.interfaces[*pkg
        .interfaces
        .get("types")
        .ok_or_else(|| anyhow::anyhow!("wasi:sockets/types interface not found"))?];
    let find = |name: &str| -> anyhow::Result<wit_parser::TypeId> {
        iface.types.get(name).copied().ok_or_else(|| anyhow::anyhow!("wit type {name} not found in wasi:sockets/types"))
    };
    let ec = find("error-code")?;
    let ec_names: Vec<String> = match &resolve.types[ec].kind {
        TypeDefKind::Variant(v) => v.cases.iter().map(|c| c.name.clone()).collect(),
        k => anyhow::bail!("error-code: expected variant, got {k:?}"),
    };
    let fam_ipv4 = match &resolve.types[find("ip-address-family")?].kind {
        TypeDefKind::Enum(e) => e.cases.iter().position(|c| c.name == "ipv4"),
        _ => None,
    }
    .ok_or_else(|| anyhow::anyhow!("ip-address-family.ipv4 not found"))? as i32;
    let sa_ipv4 = match &resolve.types[find("ip-socket-address")?].kind {
        TypeDefKind::Variant(v) => v.cases.iter().position(|c| c.name == "ipv4"),
        _ => None,
    }
    .ok_or_else(|| anyhow::anyhow!("ip-socket-address.ipv4 not found"))? as i32;
    let mut sa = wit_parser::SizeAlign::default();
    sa.fill(resolve)?;
    let ec_align = sa.align(&Type::Id(ec)).align_wasm32() as u64;
    let (bind_params, _) = wit_import_sig(resolve, "wasi:sockets", "types", "[method]tcp-socket.bind")?;
    Ok(SockAbi {
        fam_ipv4,
        sa_ipv4,
        // A handle (own<tcp-socket>, stream<tcp-socket>) aligns 4; the
        // discriminant byte rounds up to the larger of that and error-code's.
        handle_payload: 4u64.max(ec_align),
        unit_payload: ec_align,
        bind_params,
        ec_names,
    })
}

/// The serve block's data: the port and generic messages, the per-case bind
/// messages and their lengths.
fn serve_data(data: &mut wasm_encoder::DataSection, park: u64, abi: &SockAbi) {
    data.active(0, &ConstExpr::i32_const((park + SV_PORT_MSG) as i32), E_PORT.iter().copied());
    data.active(0, &ConstExpr::i32_const((park + SV_GEN_MSG) as i32), E_SERVE_GEN.iter().copied());
    let mut lens = Vec::new();
    for (k, case) in abi.ec_names.iter().enumerate() {
        let text = bind_err_text(case);
        assert!(text.len() as u64 <= SV_BIND_STRIDE, "bind message past its stride");
        lens.extend_from_slice(&(text.len() as u32).to_le_bytes());
        data.active(
            0,
            &ConstExpr::i32_const((park + SV_BIND_TAB + SV_BIND_STRIDE * k as u64) as i32),
            text.into_bytes(),
        );
    }
    assert!(SV_LENS + lens.len() as u64 <= SV_BIND_TAB, "bind message lengths past their slot");
    assert!(
        SV_BIND_TAB + SV_BIND_STRIDE * abi.ec_names.len() as u64 <= crate::wasi::PARK_SPAN,
        "bind messages past the park"
    );
    data.active(0, &ConstExpr::i32_const((park + SV_LENS) as i32), lens);
}

/// Answer `pack(1, len)` with a static message.
fn serve_err(i: &mut wasm_encoder::InstructionSink<'_>, g: P3Globals, off: u64, len: usize) {
    i.i32_const((g.park + off) as i32).global_set(g.g_ppos);
    i.i32_const(len as i32).global_set(g.g_plen);
    i.i64_const((1i64 << 32) | len as i64).return_();
}

/// A failed socket call's retptr at park+RET: when its discriminant is set,
/// answer the bind message for the error-code case at `payload`.
fn serve_bind_check(i: &mut wasm_encoder::InstructionSink<'_>, g: P3Globals, abi: &SockAbi, payload: u64, k: u32) {
    let ret = (g.park + RET) as i32;
    i.i32_const(ret).i32_load8_u(mem8(0));
    i.if_(BlockType::Empty);
    i.i32_const(ret).i32_load8_u(mem8(payload)).local_set(k);
    i.local_get(k).i32_const(abi.ec_names.len() as i32).i32_ge_u();
    i.if_(BlockType::Empty);
    serve_err(i, g, SV_GEN_MSG, E_SERVE_GEN.len());
    i.end();
    i.i32_const((g.park + SV_BIND_TAB) as i32);
    i.local_get(k).i32_const(SV_BIND_STRIDE as i32).i32_mul().i32_add().global_set(g.g_ppos);
    i.i32_const((g.park + SV_LENS) as i32);
    i.local_get(k).i32_const(4).i32_mul().i32_add().i32_load(mem(0)).global_set(g.g_plen);
    i.i64_const(1i64 << 32).global_get(g.g_plen).i64_extend_i32_u().i64_or().return_();
    i.end();
}

/// Close the connection being served: its receive stream and future, then
/// the socket (the close the client reads as the response's end).
fn serve_close(i: &mut wasm_encoder::InstructionSink<'_>, s: &ServeImports, sg: ServeGlobals) {
    for (g, drop) in [(sg.crx, s.rx_drop), (sg.crxf, s.rxf_drop), (sg.conn, s.sock_drop)] {
        i.global_get(g).i32_const(0).i32_ge_s();
        i.if_(BlockType::Empty);
        i.global_get(g).call(drop);
        i.i32_const(-1).global_set(g);
        i.end();
    }
    i.i32_const(0).global_set(sg.ceof);
}

/// fs_call-shaped `(op, a_ptr, a_len, b_ptr, b_len) -> i64` for ops 70 and
/// 73..=75. See the file header for the sequence of each.
fn shim_serve(g: P3Globals, s: &ServeImports, sg: ServeGlobals, abi: &SockAbi) -> Function {
    let (op, a_ptr, a_len) = (0u32, 1u32, 2u32);
    let (port, k, c, r, tx, rx, fut, n) = (5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32);
    let s64 = 13u32;
    let mut f = Function::new([(8, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();
    let ret = (g.park + RET) as i32;

    // ── op 70: bind ──────────────────────────────────────────────────
    i.local_get(op).i32_const(70).i32_eq().if_(BlockType::Empty);
    // The port: ASCII digits, at most 65535 — anything else is native's
    // `invalid port value` (the text to_socket_addrs answers).
    i.local_get(a_len).i32_eqz().if_(BlockType::Empty);
    serve_err(&mut i, g, SV_PORT_MSG, E_PORT.len());
    i.end();
    i.i32_const(0).local_set(port);
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(a_len).i32_ge_u().br_if(1);
    i.local_get(a_ptr).local_get(k).i32_add().i32_load8_u(mem8(0)).i32_const(48).i32_sub().local_tee(c);
    i.i32_const(9).i32_gt_u();
    i.if_(BlockType::Empty);
    serve_err(&mut i, g, SV_PORT_MSG, E_PORT.len());
    i.end();
    i.local_get(port).i32_const(10).i32_mul().local_get(c).i32_add().local_tee(port);
    i.i32_const(65535).i32_gt_u();
    i.if_(BlockType::Empty);
    serve_err(&mut i, g, SV_PORT_MSG, E_PORT.len());
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    // create(ipv4) → the socket
    i.i32_const(ERR_RESERVE).call(g.f_reserve);
    i.i32_const(abi.fam_ipv4).i32_const(ret).call(s.create);
    serve_bind_check(&mut i, g, abi, abi.handle_payload, k);
    i.i32_const(ret).i32_load(mem(abi.handle_payload)).global_set(sg.lsock);
    // bind(ipv4 0.0.0.0:<port>) — native's `0.0.0.0:{port}`
    i.i32_const(ERR_RESERVE).call(g.f_reserve);
    let bp = &abi.bind_params;
    assert!(bp.len() >= 8, "bind's flattened params: self, case, port, 4 octets, retptr");
    i.global_get(sg.lsock);
    i.i32_const(abi.sa_ipv4);
    i.local_get(port);
    for t in &bp[3..bp.len() - 1] {
        match t {
            ValType::I64 => i.i64_const(0),
            ValType::F32 => i.f32_const(0.0_f32.into()),
            ValType::F64 => i.f64_const(0.0_f64.into()),
            _ => i.i32_const(0),
        };
    }
    i.i32_const(ret).call(s.bind);
    serve_bind_check(&mut i, g, abi, abi.unit_payload, k);
    // listen → the stream of accepted connections
    i.i32_const(ERR_RESERVE).call(g.f_reserve);
    i.global_get(sg.lsock).i32_const(ret).call(s.listen);
    serve_bind_check(&mut i, g, abi, abi.handle_payload, k);
    i.i32_const(ret).i32_load(mem(abi.handle_payload)).global_set(sg.lst);
    i.i32_const(0).global_set(g.g_plen);
    i.i64_const(0).return_();
    i.end();

    // ── op 73: accept ────────────────────────────────────────────────
    i.local_get(op).i32_const(73).i32_eq().if_(BlockType::Empty);
    i.global_get(sg.lst).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    serve_err(&mut i, g, SV_GEN_MSG, E_SERVE_GEN.len());
    i.end();
    serve_close(&mut i, s, sg);
    i.global_get(sg.lst).i32_const(ret).i32_const(1).call(s.lst_read);
    i.i32_const(4).i32_shr_u().i32_eqz().if_(BlockType::Empty);
    serve_err(&mut i, g, SV_GEN_MSG, E_SERVE_GEN.len());
    i.end();
    i.i32_const(ret).i32_load(mem(0)).global_set(sg.conn);
    i.global_get(sg.conn).i32_const(ret).call(s.receive);
    i.i32_const(ret).i32_load(mem(0)).global_set(sg.crx);
    i.i32_const(ret).i32_load(mem(4)).global_set(sg.crxf);
    i.i32_const(0).global_set(g.g_plen);
    i.i64_const(0).return_();
    i.end();

    // ── op 74: recv ──────────────────────────────────────────────────
    i.local_get(op).i32_const(74).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).global_set(g.g_plen);
    i.global_get(sg.crx).i32_const(0).i32_lt_s().global_get(sg.ceof).i32_or().if_(BlockType::Empty);
    i.i64_const(0).return_();
    i.end();
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.global_get(sg.crx).i32_const((g.park + DATA) as i32).i32_const(RECV_CAP).call(s.rx_read).local_tee(r);
    i.i32_const(4).i32_shr_u().local_tee(n);
    i.if_(BlockType::Empty);
    i.i32_const((g.park + DATA) as i32).global_set(g.g_ppos);
    i.local_get(n).global_set(g.g_plen);
    i.local_get(n).i64_extend_i32_u().return_();
    i.end();
    // An empty COMPLETED copy carries no news: read again.
    i.local_get(r).i32_const(15).i32_and().i32_const(COPY_COMPLETED).i32_eq().br_if(0);
    i.end().end();
    // The stream ended: its future says whether the peer closed (ok) or
    // the read failed (err — native's read error, the request is dropped).
    i.i32_const(1).global_set(sg.ceof);
    i.i32_const(ERR_RESERVE).call(g.f_reserve);
    i.global_get(sg.crxf).i32_const(ret).call(s.rxf_read).drop();
    i.global_get(sg.crxf).call(s.rxf_drop);
    i.i32_const(-1).global_set(sg.crxf);
    i.i32_const(ret).i32_load8_u(mem8(0)).if_(BlockType::Empty);
    serve_err(&mut i, g, SV_GEN_MSG, E_SERVE_GEN.len());
    i.end();
    i.i64_const(0).return_();
    i.end();

    // ── op 75: send, then close ──────────────────────────────────────
    i.local_get(op).i32_const(75).i32_eq().if_(BlockType::Empty);
    i.global_get(sg.conn).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    serve_err(&mut i, g, SV_GEN_MSG, E_SERVE_GEN.len());
    i.end();
    i.call(s.tx_new).local_set(s64);
    i.local_get(s64).i64_const(32).i64_shr_u().i32_wrap_i64().local_set(tx);
    i.local_get(s64).i32_wrap_i64().local_set(rx);
    i.global_get(sg.conn).local_get(rx).call(s.send).local_set(fut);
    // write all of a; a DROPPED status (the host stopped reading — the
    // peer is gone) ends the loop: native ignores a failed write.
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(a_len).i32_eqz().br_if(1);
    i.local_get(tx).local_get(a_ptr).local_get(a_len).call(s.tx_write);
    i.i32_const(4).i32_shr_u().local_tee(n).i32_eqz().br_if(1);
    i.local_get(a_ptr).local_get(n).i32_add().local_set(a_ptr);
    i.local_get(a_len).local_get(n).i32_sub().local_set(a_len);
    i.br(0).end().end();
    i.local_get(tx).call(s.tx_drop);
    i.i32_const(ERR_RESERVE).call(g.f_reserve);
    i.local_get(fut).i32_const(ret).call(s.txf_read).drop();
    i.local_get(fut).call(s.txf_drop);
    serve_close(&mut i, s, sg);
    i.i32_const(0).global_set(g.g_plen);
    i.i64_const(0).return_();
    i.end();

    serve_err(&mut i, g, SV_GEN_MSG, E_SERVE_GEN.len());
    i.end();
    f
}

/// op 29 (`env.args`): get-arguments, re-framed `[len u32][bytes]` per
/// entry into a buffer from the shims' own allocator.
fn shim_args(g: P3Globals, i_args: u32) -> Function {
    let (list, cnt, idx, total, buf, out, s, n) = (5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32);
    let mut f = Function::new([(8, ValType::I32)]);
    let mut i = f.instructions();
    let ret = (g.park + RET) as i32;
    i.i32_const(ARGS_RESERVE).call(g.f_reserve);
    i.i32_const(ret).call(i_args);
    i.i32_const(ret).i32_load(mem(0)).local_set(list);
    i.i32_const(ret).i32_load(mem(4)).local_set(cnt);
    // total = Σ (4 + len)
    i.i32_const(0).local_set(total);
    i.i32_const(0).local_set(idx);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(idx).local_get(cnt).i32_ge_u().br_if(1);
    i.local_get(total).i32_const(4).i32_add();
    i.local_get(list).local_get(idx).i32_const(8).i32_mul().i32_add().i32_load(mem(4));
    i.i32_add().local_set(total);
    i.local_get(idx).i32_const(1).i32_add().local_set(idx);
    i.br(0).end().end();
    i.i32_const(0).i32_const(0).i32_const(4).local_get(total).call(g.f_alloc).local_tee(buf).local_set(out);
    i.i32_const(0).local_set(idx);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(idx).local_get(cnt).i32_ge_u().br_if(1);
    i.local_get(list).local_get(idx).i32_const(8).i32_mul().i32_add().local_tee(s).i32_load(mem(4)).local_set(n);
    i.local_get(out).local_get(n).i32_store(mem(0));
    i.local_get(out).i32_const(4).i32_add().local_get(s).i32_load(mem(0)).local_get(n).memory_copy(0, 0);
    i.local_get(out).i32_const(4).i32_add().local_get(n).i32_add().local_set(out);
    i.local_get(idx).i32_const(1).i32_add().local_set(idx);
    i.br(0).end().end();
    i.local_get(buf).global_set(g.g_ppos);
    i.local_get(total).global_set(g.g_plen);
    i.local_get(total).i64_extend_i32_u();
    i.end();
    f
}
