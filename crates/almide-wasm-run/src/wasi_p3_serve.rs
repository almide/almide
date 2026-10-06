// `include!`d part of wasi_p3.rs: the stock serve export (#2659, ADR-0020
// §5.4, ADR-0023 §3.3, C-375) — shares the parent module's imports and items.
//
// `to_p3_service` builds a serve-shaped program as a `wasi:http/handler@0.3.0`
// component that `wasmtime serve` runs with no flags. The guest's serve loop
// is the one the embedded lane runs (stdlib/http_serve.almd over ops
// 70..=72); here the SHIM answers those ops guest-side, from the request the
// export was called with:
//
//   handle(request)            the async-lifted export (callback never
//                              reached, as `run`'s): backpressure.inc — one
//                              handler in flight per instance; the request
//                              read (method, path-with-query, headers, the
//                              body stream) and native's limits applied —
//                              414 / 431 / 413 answered here, the handler not
//                              called; else the request framed for op 71 and
//                              `main` called (the build rewrote it to
//                              `http.serve(0, app)`, so it evaluates the app
//                              and serves exactly this request); the reply
//                              frame op 72 left becomes the response:
//                              task.return, then the body stream and the
//                              trailers future; stdout / stderr drained;
//                              backpressure.dec.
//   op 70 bind                 ok: the host owns the address.
//   op 71 next                 the framed request once, then no request — the
//                              guest's loop ends and `main` returns.
//   op 72 reply                the response cells, copied for `handle`.
//
// Each request runs in an arena (#3444): `handle` saves the guest
// allocator's state and the shim's heap-pointing caches on entry and
// restores them once the response, its trailers and the streams are done,
// so the shim's buffers (the header list, the body, the frames, the reply)
// and everything the guest allocated for the request (the parsed request,
// the app's response, the top-lets `main` evaluates) are taken back at
// once. A reused instance's heap use is the largest request's, not the sum.
//
// The request reaches the guest as native's server core reads it: header
// values, the target and the body decoded as UTF-8 with replacement (Rust's
// `from_utf8_lossy`, the maximal-subpart rule), headers in the host's order.

// The service import block, appended after the http block (indices follow
// IMPORTS_HTTP; position-checked by `serve_import_list`).
const S_BP_INC: u32 = IMPORTS_HTTP; // $root [backpressure-inc]
const S_BP_DEC: u32 = IMPORTS_HTTP + 1; // $root [backpressure-dec]
const S_GET_METHOD: u32 = IMPORTS_HTTP + 2; // [method]request.get-method (retptr)
const S_GET_PATH: u32 = IMPORTS_HTTP + 3; // [method]request.get-path-with-query (retptr)
const S_GET_HEADERS: u32 = IMPORTS_HTTP + 4; // [method]request.get-headers
const S_COPY_ALL: u32 = IMPORTS_HTTP + 5; // [method]fields.copy-all (retptr)
const S_REQ_CONSUME: u32 = IMPORTS_HTTP + 6; // [static]request.consume-body (retptr)
const S_REQ_CB_FNEW: u32 = IMPORTS_HTTP + 7; // [future-new-0] of request.consume-body
const S_REQ_CB_FWRITE: u32 = IMPORTS_HTTP + 8; // [async-lower][future-write-0]
const S_REQ_CB_FDROPW: u32 = IMPORTS_HTTP + 9; // [future-drop-writable-0]
const S_REQ_BODY_READ: u32 = IMPORTS_HTTP + 10; // [async-lower][stream-read-1]
const S_REQ_BODY_DROPR: u32 = IMPORTS_HTTP + 11; // [stream-drop-readable-1]
const S_REQ_TRL_DROPR: u32 = IMPORTS_HTTP + 12; // [future-drop-readable-2]
const S_RESP_NEW: u32 = IMPORTS_HTTP + 13; // [static]response.new (retptr)
const S_RESP_SNEW: u32 = IMPORTS_HTTP + 14; // [stream-new-0] of response.new
const S_RESP_SWRITE: u32 = IMPORTS_HTTP + 15; // [async-lower][stream-write-0]
const S_RESP_SDROPW: u32 = IMPORTS_HTTP + 16; // [stream-drop-writable-0]
const S_RESP_FNEW: u32 = IMPORTS_HTTP + 17; // [future-new-1] (trailers)
const S_RESP_FWRITE: u32 = IMPORTS_HTTP + 18; // [async-lower][future-write-1]
const S_RESP_FDROPW: u32 = IMPORTS_HTTP + 19; // [future-drop-writable-1]
const S_RESP_RFUT_DROPR: u32 = IMPORTS_HTTP + 20; // [future-drop-readable-2] (the response's result)
const S_SET_STATUS: u32 = IMPORTS_HTTP + 21; // [method]response.set-status-code
const S_TASK_RETURN: u32 = IMPORTS_HTTP + 22; // [export]wasi:http/handler [task-return]handle
const IMPORTS_SERVE: u32 = IMPORTS_HTTP + 23;

/// Native's server limits (almide-rt-core's http_server_core): the request
/// and header line (8 KiB, `414` / `431` beyond, the CRLF counted), the
/// header count (100, `431` beyond) and `http.serve`'s body (1 MiB, `413`).
const SERVE_MAX_LINE: i32 = 8 * 1024;
const SERVE_MAX_HEADERS: i32 = 100;
const SERVE_MAX_BODY: i32 = 1 << 20;

/// The service statics sit in the http text page, past the client's texts
/// (checked at emit time).
const SERVE_TEXT: u64 = HTTP_TEXT + 32768;
/// The task.return buffer when `result<response, error-code>` does not
/// flatten: the stat/send result slot, idle once the handler returned.
const SERVE_TRBUF: u64 = STATRET;

/// Canonical-ABI facts the service shim reads and writes through — derived
/// from the vendored WIT, never hand-counted (the fs_abi doctrine).
struct ServeAbi {
    /// Per `method` case, in discriminant order: the upper-cased name, or
    /// `None` for `other(string)`.
    method_names: Vec<Option<String>>,
    /// Offset of `other(string)`'s (ptr, len) from the discriminant.
    method_payload: u64,
    /// Offset of `option<string>`'s (ptr, len).
    opt_payload: u64,
    /// task.return's core params for `result<response, error-code>`: the
    /// flattened list, or one pointer when it does not flatten.
    tr_params: Vec<ValType>,
    /// The response handle's offset in the indirect result.
    tr_payload: u64,
}

fn serve_abi(resolve: &wit_parser::Resolve) -> anyhow::Result<ServeAbi> {
    use wit_parser::{Type, TypeDefKind};
    let (_, http_pkg) = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "wasi" && p.name.name == "http")
        .ok_or_else(|| anyhow::anyhow!("wasi:http package not in the resolve"))?;
    let types = &resolve.interfaces[*http_pkg.interfaces.get("types").ok_or_else(|| anyhow::anyhow!("wasi:http/types"))?];
    let handler = &resolve.interfaces[*http_pkg.interfaces.get("handler").ok_or_else(|| anyhow::anyhow!("wasi:http/handler"))?];
    let method = *types.types.get("method").ok_or_else(|| anyhow::anyhow!("wit type method"))?;
    let ec = *types.types.get("error-code").ok_or_else(|| anyhow::anyhow!("wit type error-code"))?;
    let TypeDefKind::Variant(mv) = &resolve.types[method].kind else {
        anyhow::bail!("wasi:http method is not a variant");
    };
    let mut sa = wit_parser::SizeAlign::default();
    sa.fill(resolve)?;
    let method_names = mv.cases.iter().map(|c| c.ty.is_none().then(|| c.name.to_ascii_uppercase())).collect();
    let method_payload = sa.payload_offset(mv.tag(), mv.cases.iter().map(|c| c.ty.as_ref())).size_wasm32() as u64;
    let opt_payload = sa.payload_offset(wit_parser::Int::U8, [None, Some(&Type::String)]).size_wasm32() as u64;
    let handle = handler.functions.get("handle").ok_or_else(|| anyhow::anyhow!("wasi:http/handler.handle"))?;
    let rty = handle.result.ok_or_else(|| anyhow::anyhow!("handle has no result"))?;
    let mut storage = [wit_parser::abi::WasmType::I32; 16];
    let mut flat = wit_parser::abi::FlatTypes::new(&mut storage);
    let vt = |t: &wit_parser::abi::WasmType| match t {
        wit_parser::abi::WasmType::I64 | wit_parser::abi::WasmType::PointerOrI64 => ValType::I64,
        wit_parser::abi::WasmType::F32 => ValType::F32,
        wit_parser::abi::WasmType::F64 => ValType::F64,
        _ => ValType::I32,
    };
    let tr_params = if resolve.push_flat(&rty, &mut flat) { flat.to_vec().iter().map(vt).collect() } else { vec![ValType::I32] };
    let tr_payload = 4u64.max(sa.align(&Type::Id(ec)).align_wasm32() as u64);
    Ok(ServeAbi { method_names, method_payload, opt_payload, tr_params, tr_payload })
}

/// A static frame or text in the service blob.
#[derive(Clone, Copy)]
struct ServePiece {
    at: i32,
    len: i32,
}

/// The service statics, laid out at `park + SERVE_TEXT`.
struct ServeTexts {
    blob: Vec<u8>,
    base: u64,
    /// One (ptr, len) pair per `method` case, in discriminant order.
    method_table: i32,
    /// The answers native's server core gives a request it refuses: the
    /// status, its reason as the body, `Content-Type: text/plain` — as cells.
    r413: ServePiece,
    r414: ServePiece,
    r431: ServePiece,
    connection: ServePiece,
    content_length: ServePiece,
    /// 20 bytes of decimal scratch.
    digits: i32,
}

impl ServeTexts {
    fn new(park: u64, abi: &ServeAbi) -> Self {
        use almide_rt_core::http_server_core::http_server_reason;
        let base = park + SERVE_TEXT;
        let mut blob: Vec<u8> = vec![0; abi.method_names.len() * 8];
        let put = |blob: &mut Vec<u8>, bytes: &[u8]| -> ServePiece {
            let at = (base + blob.len() as u64) as i32;
            blob.extend_from_slice(bytes);
            ServePiece { at, len: bytes.len() as i32 }
        };
        for (k, name) in abi.method_names.iter().enumerate() {
            if let Some(n) = name {
                let p = put(&mut blob, n.as_bytes());
                blob[k * 8..k * 8 + 4].copy_from_slice(&p.at.to_le_bytes());
                blob[k * 8 + 4..k * 8 + 8].copy_from_slice(&p.len.to_le_bytes());
            }
        }
        let cells = |status: i64| -> Vec<u8> {
            let reason = http_server_reason(status);
            [status.to_string().as_str(), reason, "Content-Type", "text/plain"]
                .iter()
                .map(|c| format!("{}\n{c}", c.chars().count()))
                .collect::<String>()
                .into_bytes()
        };
        let r413 = put(&mut blob, &cells(413));
        let r414 = put(&mut blob, &cells(414));
        let r431 = put(&mut blob, &cells(431));
        let connection = put(&mut blob, b"connection");
        let content_length = put(&mut blob, b"content-length");
        let digits = put(&mut blob, &[0u8; 20]).at;
        assert!(SERVE_TEXT + blob.len() as u64 <= crate::wasi::PARK_SPAN, "the service statics overrun the park");
        ServeTexts { blob, base, method_table: base as i32, r413, r414, r431, connection, content_length, digits }
    }
}

/// The service's globals: where `handle` parks the framed request for op 71
/// and where op 72 leaves the reply.
#[derive(Clone, Copy)]
struct ServeGlobals {
    /// 0 idle, 1 request framed, 2 request handed out, 3 reply left.
    state: u32,
    req_ptr: u32,
    req_len: u32,
    rep_ptr: u32,
    rep_len: u32,
}

impl ServeGlobals {
    const COUNT: u32 = 5;

    fn at(first: u32) -> Self {
        ServeGlobals { state: first, req_ptr: first + 1, req_len: first + 2, rep_ptr: first + 3, rep_len: first + 4 }
    }

    fn emit(globals: &mut GlobalSection) {
        let t = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
        for _ in 0..Self::COUNT {
            globals.global(t, &ConstExpr::i32_const(0));
        }
    }
}

/// The service import block, position-checked against the S_* constants.
fn serve_import_list(t: &P3Types, t_bp: u32, t_tr: u32) -> Vec<(u32, &'static str, &'static str, u32)> {
    let ht = "wasi:http/types@0.3.0";
    let list = vec![
        (S_BP_INC, "$root", "[backpressure-inc]", t_bp),
        (S_BP_DEC, "$root", "[backpressure-dec]", t_bp),
        (S_GET_METHOD, ht, "[method]request.get-method", t.ws_join),
        (S_GET_PATH, ht, "[method]request.get-path-with-query", t.ws_join),
        (S_GET_HEADERS, ht, "[method]request.get-headers", t.call),
        (S_COPY_ALL, ht, "[method]fields.copy-all", t.ws_join),
        (S_REQ_CONSUME, ht, "[static]request.consume-body", t.consume),
        (S_REQ_CB_FNEW, ht, "[future-new-0][static]request.consume-body", t.new),
        (S_REQ_CB_FWRITE, ht, "[async-lower][future-write-0][static]request.consume-body", t.fut_read),
        (S_REQ_CB_FDROPW, ht, "[future-drop-writable-0][static]request.consume-body", t.drop),
        (S_REQ_BODY_READ, ht, "[async-lower][stream-read-1][static]request.consume-body", t.rw),
        (S_REQ_BODY_DROPR, ht, "[stream-drop-readable-1][static]request.consume-body", t.drop),
        (S_REQ_TRL_DROPR, ht, "[future-drop-readable-2][static]request.consume-body", t.drop),
        (S_RESP_NEW, ht, "[static]response.new", t.stat),
        (S_RESP_SNEW, ht, "[stream-new-0][static]response.new", t.new),
        (S_RESP_SWRITE, ht, "[async-lower][stream-write-0][static]response.new", t.rw),
        (S_RESP_SDROPW, ht, "[stream-drop-writable-0][static]response.new", t.drop),
        (S_RESP_FNEW, ht, "[future-new-1][static]response.new", t.new),
        (S_RESP_FWRITE, ht, "[async-lower][future-write-1][static]response.new", t.fut_read),
        (S_RESP_FDROPW, ht, "[future-drop-writable-1][static]response.new", t.drop),
        (S_RESP_RFUT_DROPR, ht, "[future-drop-readable-2][static]response.new", t.drop),
        (S_SET_STATUS, ht, "[method]response.set-status-code", t.fut_read),
        (S_TASK_RETURN, "[export]wasi:http/handler@0.3.0", "[task-return]handle", t_tr),
    ];
    assert_eq!(IMPORTS_HTTP + list.len() as u32, IMPORTS_SERVE, "IMPORTS_SERVE count drift");
    list
}

/// The service functions' indices.
#[derive(Clone, Copy)]
struct ServeFns {
    op: u32,
    handle: u32,
    cell: u32,
}

/// `(op, a_ptr, a_len, b_ptr, b_len) -> i64`: ops 70..=72, guest-side.
fn shim_serve_op(g: P3Globals, s: ServeGlobals) -> Function {
    let (op, a_ptr, a_len) = (0u32, 1u32, 2u32);
    let p = 5u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    // Nothing parked unless op 71 hands the request out: the guest may read
    // the park after any answer, and a stale length would copy the last
    // request over its heap.
    i.i32_const(0).global_set(g.g_plen);
    // op 71: the framed request once, then no request.
    i.local_get(op).i32_const(71).i32_eq().if_(BlockType::Empty);
    i.global_get(s.state).i32_const(1).i32_eq().if_(BlockType::Empty);
    i.i32_const(2).global_set(s.state);
    i.global_get(s.req_ptr).global_set(g.g_ppos);
    i.global_get(s.req_len).global_set(g.g_plen);
    i.global_get(s.req_len).i64_extend_i32_u().return_();
    i.end();
    i.i64_const(0).return_();
    i.end();
    // op 72: copy the reply cells out of the guest's string.
    i.local_get(op).i32_const(72).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).i32_const(0).i32_const(1).local_get(a_len).call(g.f_alloc).local_set(p);
    i.local_get(p).local_get(a_ptr).local_get(a_len).memory_copy(0, 0);
    i.local_get(p).global_set(s.rep_ptr);
    i.local_get(a_len).global_set(s.rep_len);
    i.i32_const(3).global_set(s.state);
    i.end();
    // op 70 (and 72's answer): ok.
    i.i64_const(0);
    i.end();
    f
}

/// `(dst, src, len) -> end`: one guest list frame, `[u32 n][n bytes]`, of
/// `src[..len]` decoded as UTF-8 with replacement — Rust's
/// `String::from_utf8_lossy`: each maximal invalid subpart becomes one
/// U+FFFD. `dst` needs room for `4 + 3 * len` bytes.
fn shim_serve_cell() -> Function {
    let (dst, src, len) = (0u32, 1u32, 2u32);
    let (o, ix, b, need, lo, hi, j, k, c) = (3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32);
    let mut f = Function::new([(9, ValType::I32)]);
    let mut i = f.instructions();
    let byte_at = |i: &mut wasm_encoder::InstructionSink<'_>, at: u32| {
        i.local_get(src).local_get(at).i32_add().i32_load8_u(mem8(0));
    };
    i.local_get(dst).i32_const(4).i32_add().local_set(o);
    i.i32_const(0).local_set(ix);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(ix).local_get(len).i32_ge_u().br_if(1);
    byte_at(&mut i, ix);
    i.local_set(b);
    // ASCII: copied as is.
    i.local_get(b).i32_const(0x80).i32_lt_u().if_(BlockType::Empty);
    i.local_get(o).local_get(b).i32_store8(mem8(0));
    i.local_get(o).i32_const(1).i32_add().local_set(o);
    i.local_get(ix).i32_const(1).i32_add().local_set(ix);
    i.br(1);
    i.end();
    // The lead byte's sequence length and its second byte's range (the
    // Unicode well-formed table, RFC 3629 §4).
    i.i32_const(0).local_set(need);
    i.i32_const(0x80).local_set(lo);
    i.i32_const(0xBF).local_set(hi);
    let ranges: [(i32, i32, i32, i32, i32); 7] = [
        (0xC2, 0xDF, 1, 0x80, 0xBF),
        (0xE0, 0xE0, 2, 0xA0, 0xBF),
        (0xE1, 0xEC, 2, 0x80, 0xBF),
        (0xED, 0xED, 2, 0x80, 0x9F),
        (0xEE, 0xEF, 2, 0x80, 0xBF),
        (0xF0, 0xF0, 3, 0x90, 0xBF),
        (0xF1, 0xF3, 3, 0x80, 0xBF),
    ];
    for (first, last, n, l, h) in ranges.into_iter().chain([(0xF4, 0xF4, 3, 0x80, 0x8F)]) {
        i.local_get(b).i32_const(first).i32_ge_u();
        i.local_get(b).i32_const(last).i32_le_u();
        i.i32_and().if_(BlockType::Empty);
        i.i32_const(n).local_set(need);
        i.i32_const(l).local_set(lo);
        i.i32_const(h).local_set(hi);
        i.end();
    }
    // j walks the continuation bytes: the first within [lo, hi], the rest
    // within [0x80, 0xBF]; k counts them; it stops at the first misfit.
    i.local_get(ix).i32_const(1).i32_add().local_set(j);
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(need).i32_ge_u().br_if(1);
    i.local_get(j).local_get(len).i32_ge_u().br_if(1);
    byte_at(&mut i, j);
    i.local_set(c);
    i.local_get(c).local_get(lo).i32_lt_u().br_if(1);
    i.local_get(c).local_get(hi).i32_gt_u().br_if(1);
    i.i32_const(0x80).local_set(lo);
    i.i32_const(0xBF).local_set(hi);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(need).i32_eqz();
    i.local_get(k).local_get(need).i32_lt_u();
    i.i32_or().if_(BlockType::Empty);
    // Invalid: one U+FFFD for the maximal subpart [ix, j).
    i.local_get(o).i32_const(0xEF).i32_store8(mem8(0));
    i.local_get(o).i32_const(0xBF).i32_store8(mem8(1));
    i.local_get(o).i32_const(0xBD).i32_store8(mem8(2));
    i.local_get(o).i32_const(3).i32_add().local_set(o);
    i.else_();
    i.local_get(o).local_get(src).local_get(ix).i32_add().local_get(j).local_get(ix).i32_sub().memory_copy(0, 0);
    i.local_get(o).local_get(j).local_get(ix).i32_sub().i32_add().local_set(o);
    i.end();
    i.local_get(j).local_set(ix);
    i.br(0).end().end();
    i.local_get(dst).local_get(o).local_get(dst).i32_sub().i32_const(4).i32_sub().i32_store(mem(0));
    i.local_get(o);
    i.end();
    f
}

/// `handle`'s locals, by role.
struct HandleLocals;
impl HandleLocals {
    const REQ: u32 = 0;
    const M_PTR: u32 = 1;
    const M_LEN: u32 = 2;
    const T_PTR: u32 = 3;
    const T_LEN: u32 = 4;
    const FIELDS: u32 = 5;
    const H_LIST: u32 = 6;
    const H_N: u32 = 7;
    const BODY_RX: u32 = 8;
    const TRLFUT: u32 = 9;
    const CB_TX: u32 = 10;
    const BUF: u32 = 11;
    const CAP: u32 = 12;
    const TOTAL: u32 = 13;
    const N: u32 = 14;
    const K: u32 = 15;
    const REJ_PTR: u32 = 16;
    const REJ_LEN: u32 = 17;
    const OUT: u32 = 18;
    const SIZE: u32 = 19;
    const CUR: u32 = 20;
    const END: u32 = 21;
    const CELL_LEN: u32 = 22;
    const TMP: u32 = 23;
    const DIGIT: u32 = 24;
    const STATUS: u32 = 25;
    const B_PTR: u32 = 26;
    const B_LEN: u32 = 27;
    const KEY_PTR: u32 = 28;
    const KEY_LEN: u32 = 29;
    const RESP: u32 = 30;
    const RFUT: u32 = 31;
    const S_TX: u32 = 32;
    const S_RX: u32 = 33;
    const F_TX: u32 = 34;
    const F_RX: u32 = 35;
    const NOBODY: u32 = 36;
    const HEAD: u32 = 37;
    const S64: u32 = 38;
    const I32S: u32 = 37;
    /// The arena's saved words, past `S64` (ServeArena::LOCALS of them).
    const ARENA: u32 = 39;
}

/// Split the `stream.new` / `future.new` pair in `S64`: tx = high half,
/// rx = low half (the sync-streams.wast packing).
fn split_pair(i: &mut wasm_encoder::InstructionSink<'_>, tx: u32, rx: u32) {
    use HandleLocals as L;
    i.local_get(L::S64).i64_const(32).i64_shr_u().i32_wrap_i64().local_set(tx);
    i.local_get(L::S64).i32_wrap_i64().local_set(rx);
}

/// Read the request: method, path-with-query, the header list, the body
/// stream — and native's limits, which set `REJ_PTR`/`REJ_LEN` to the
/// refusal's frame.
fn serve_read_request(i: &mut wasm_encoder::InstructionSink<'_>, g: P3Globals, abi: &ServeAbi, t: &ServeTexts) {
    use HandleLocals as L;
    let ret = (g.park + RET) as i32;
    // The method: a named case from the table, `other(string)` its payload.
    i.local_get(L::REQ).i32_const(ret).call(S_GET_METHOD);
    i.i32_const(ret).i32_load8_u(mem8(0)).local_set(L::K);
    let other = abi.method_names.iter().position(Option::is_none).unwrap_or(usize::MAX) as i32;
    i.local_get(L::K).i32_const(other).i32_eq().if_(BlockType::Empty);
    i.i32_const(ret).i32_load(mem(abi.method_payload)).local_set(L::M_PTR);
    i.i32_const(ret).i32_load(mem(abi.method_payload + 4)).local_set(L::M_LEN);
    i.else_();
    i.local_get(L::K).i32_const(8).i32_mul().i32_const(t.method_table).i32_add().local_set(L::TMP);
    i.local_get(L::TMP).i32_load(mem(0)).local_set(L::M_PTR);
    i.local_get(L::TMP).i32_load(mem(4)).local_set(L::M_LEN);
    i.end();
    // HEAD gets no body (native's core: RFC 9110 §9.3.2).
    i.local_get(L::M_LEN).i32_const(4).i32_eq().if_(BlockType::Result(ValType::I32));
    for (k, ch) in b"HEAD".iter().enumerate() {
        i.local_get(L::M_PTR).i32_load8_u(mem8(k as u64)).i32_const(*ch as i32).i32_eq();
        if k > 0 {
            i.i32_and();
        }
    }
    i.else_().i32_const(0).end();
    i.local_set(L::HEAD);
    // The target: path-with-query (none reads as empty).
    i.i32_const(0).local_set(L::T_LEN);
    i.i32_const(0).local_set(L::T_PTR);
    i.local_get(L::REQ).i32_const(ret).call(S_GET_PATH);
    i.i32_const(ret).i32_load8_u(mem8(0)).if_(BlockType::Empty);
    i.i32_const(ret).i32_load(mem(abi.opt_payload)).local_set(L::T_PTR);
    i.i32_const(ret).i32_load(mem(abi.opt_payload + 4)).local_set(L::T_LEN);
    i.end();
    // The headers: copied out, the fields resource dropped.
    i.local_get(L::REQ).call(S_GET_HEADERS).local_set(L::FIELDS);
    i.local_get(L::FIELDS).i32_const(ret).call(S_COPY_ALL);
    i.i32_const(ret).i32_load(mem(0)).local_set(L::H_LIST);
    i.i32_const(ret).i32_load(mem(4)).local_set(L::H_N);
    i.local_get(L::FIELDS).call(I_HTTP_FIELDS_DROP);
    // The body: consume-body moves the request.
    i.call(S_REQ_CB_FNEW).local_set(L::S64);
    split_pair(i, L::CB_TX, L::N);
    i.local_get(L::REQ).local_get(L::N).i32_const(ret).call(S_REQ_CONSUME);
    i.i32_const(ret).i32_load(mem(0)).local_set(L::BODY_RX);
    i.i32_const(ret).i32_load(mem(4)).local_set(L::TRLFUT);

    // Native's order: the request line (414), the header lines (431), the
    // body (413).
    i.i32_const(0).local_set(L::REJ_PTR);
    i.local_get(L::M_LEN).local_get(L::T_LEN).i32_add().i32_const(12).i32_add();
    i.i32_const(SERVE_MAX_LINE).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(t.r414.at).local_set(L::REJ_PTR);
    i.i32_const(t.r414.len).local_set(L::REJ_LEN);
    i.end();
    i.local_get(L::REJ_PTR).i32_eqz().local_get(L::H_N).i32_const(SERVE_MAX_HEADERS).i32_gt_u().i32_and();
    i.if_(BlockType::Empty);
    i.i32_const(t.r431.at).local_set(L::REJ_PTR);
    i.i32_const(t.r431.len).local_set(L::REJ_LEN);
    i.end();
    i.i32_const(0).local_set(L::K);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(L::REJ_PTR).br_if(1);
    i.local_get(L::K).local_get(L::H_N).i32_ge_u().br_if(1);
    i.local_get(L::H_LIST).local_get(L::K).i32_const(16).i32_mul().i32_add().local_set(L::TMP);
    i.local_get(L::TMP).i32_load(mem(4)).local_get(L::TMP).i32_load(mem(12)).i32_add().i32_const(4).i32_add();
    i.i32_const(SERVE_MAX_LINE).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(t.r431.at).local_set(L::REJ_PTR);
    i.i32_const(t.r431.len).local_set(L::REJ_LEN);
    i.end();
    i.local_get(L::K).i32_const(1).i32_add().local_set(L::K);
    i.br(0).end().end();
    // The body, counted as it arrives: past the limit, 413.
    i.i32_const(0).local_set(L::TOTAL);
    i.local_get(L::REJ_PTR).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).i32_const(0).i32_const(8).i32_const(65536).call(g.f_alloc).local_set(L::BUF);
    i.i32_const(65536).local_set(L::CAP);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(L::TOTAL).local_get(L::CAP).i32_ge_u().if_(BlockType::Empty);
    i.local_get(L::BUF).local_get(L::CAP).i32_const(8).local_get(L::CAP).i32_const(1).i32_shl();
    i.call(g.f_alloc).local_set(L::BUF);
    i.local_get(L::CAP).i32_const(1).i32_shl().local_set(L::CAP);
    i.end();
    i.local_get(L::BODY_RX);
    i.local_get(L::BODY_RX);
    i.local_get(L::BUF).local_get(L::TOTAL).i32_add();
    i.local_get(L::CAP).local_get(L::TOTAL).i32_sub();
    i.call(S_REQ_BODY_READ);
    // `(count << 4) | status`: the end is the status, never a zero count
    // (#2955).
    i.call(g.f_await).local_set(L::N);
    i.local_get(L::TOTAL).local_get(L::N).i32_const(4).i32_shr_u().i32_add().local_set(L::TOTAL);
    i.local_get(L::TOTAL).i32_const(SERVE_MAX_BODY).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(t.r413.at).local_set(L::REJ_PTR);
    i.i32_const(t.r413.len).local_set(L::REJ_LEN);
    i.br(2);
    i.end();
    i.local_get(L::N).i32_const(15).i32_and().br_if(1);
    i.br(0).end().end();
    i.end();
    // Retire the body: its readables dropped, `ok` written into the
    // consume-body result future (the client shim's `http_body_retire`).
    i.local_get(L::BODY_RX).call(S_REQ_BODY_DROPR);
    i.local_get(L::TRLFUT).call(S_REQ_TRL_DROPR);
    i.i32_const(ret).i64_const(0).i64_store(mem64(16));
    i.local_get(L::CB_TX);
    i.local_get(L::CB_TX).i32_const(ret + 16).call(S_REQ_CB_FWRITE);
    i.call(g.f_await).drop();
    i.local_get(L::CB_TX).call(S_REQ_CB_FDROPW);
}

/// Frame the request for op 71 — `[method, target, body, k1, v1, …]` as
/// guest list frames, each decoded with replacement — and run `main`; the
/// reply it leaves becomes `CUR..END`.
fn serve_run_guest(i: &mut wasm_encoder::InstructionSink<'_>, g: P3Globals, s: ServeGlobals, fns: ServeFns, main: u32) {
    use HandleLocals as L;
    let entry = |i: &mut wasm_encoder::InstructionSink<'_>, field: u64| {
        i.local_get(L::H_LIST).local_get(L::K).i32_const(16).i32_mul().i32_add().i32_load(mem(field));
    };
    // size = 12 + 3 * (m + t + body) + Σ (8 + 3 * (name + value)): a
    // replaced byte grows to the three of U+FFFD.
    i.local_get(L::M_LEN).local_get(L::T_LEN).i32_add().local_get(L::TOTAL).i32_add();
    i.i32_const(3).i32_mul().i32_const(12).i32_add().local_set(L::SIZE);
    i.i32_const(0).local_set(L::K);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(L::K).local_get(L::H_N).i32_ge_u().br_if(1);
    i.local_get(L::SIZE).i32_const(8).i32_add();
    entry(i, 4);
    entry(i, 12);
    i.i32_add().i32_const(3).i32_mul().i32_add().local_set(L::SIZE);
    i.local_get(L::K).i32_const(1).i32_add().local_set(L::K);
    i.br(0).end().end();
    i.i32_const(0).i32_const(0).i32_const(8).local_get(L::SIZE).call(g.f_alloc).local_tee(L::OUT).global_set(s.req_ptr);
    for (p, n) in [(L::M_PTR, L::M_LEN), (L::T_PTR, L::T_LEN), (L::BUF, L::TOTAL)] {
        i.local_get(L::OUT).local_get(p).local_get(n).call(fns.cell).local_set(L::OUT);
    }
    i.i32_const(0).local_set(L::K);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(L::K).local_get(L::H_N).i32_ge_u().br_if(1);
    for (p, n) in [(0u64, 4u64), (8, 12)] {
        i.local_get(L::OUT);
        entry(i, p);
        entry(i, n);
        i.call(fns.cell).local_set(L::OUT);
    }
    i.local_get(L::K).i32_const(1).i32_add().local_set(L::K);
    i.br(0).end().end();
    i.local_get(L::OUT).global_get(s.req_ptr).i32_sub().global_set(s.req_len);
    i.i32_const(1).global_set(s.state);
    i.call(main);
    // `main` returns once the loop has answered the request; one that did
    // not leave a reply is a guest defect the host answers 500 for.
    i.global_get(s.state).i32_const(3).i32_ne().if_(BlockType::Empty);
    i.unreachable();
    i.end();
    i.i32_const(0).global_set(s.state);
    i.global_get(s.rep_ptr).local_set(L::CUR);
    i.global_get(s.rep_ptr).global_get(s.rep_len).i32_add().local_set(L::END);
}

/// Parse the reply cells at `CUR..END` (status, body, then header pairs;
/// http_framed CHAR-count lengths) into a response, return it, then write
/// its body and trailers.
fn serve_respond(i: &mut wasm_encoder::InstructionSink<'_>, g: P3Globals, abi: &ServeAbi, t: &ServeTexts) {
    use HandleLocals as L;
    let ret = (g.park + RET) as i32;
    let cell = |i: &mut wasm_encoder::InstructionSink<'_>| http_frame_cell(i, L::CUR, L::CELL_LEN, L::END, L::TMP, L::DIGIT);
    // The status: a decimal cell.
    cell(i);
    i.i32_const(0).local_set(L::STATUS);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(L::TMP).local_get(L::CUR).i32_ge_u().br_if(1);
    i.local_get(L::STATUS).i32_const(10).i32_mul();
    i.local_get(L::TMP).i32_load8_u(mem8(0)).i32_const(48).i32_sub().i32_add().local_set(L::STATUS);
    i.local_get(L::TMP).i32_const(1).i32_add().local_set(L::TMP);
    i.br(0).end().end();
    cell(i);
    i.local_get(L::TMP).local_set(L::B_PTR);
    i.local_get(L::CUR).local_get(L::TMP).i32_sub().local_set(L::B_LEN);
    // No body for HEAD, 1xx, 204 and 304 (native's core).
    i.local_get(L::HEAD);
    i.local_get(L::STATUS).i32_const(100).i32_ge_u().local_get(L::STATUS).i32_const(200).i32_lt_u().i32_and().i32_or();
    i.local_get(L::STATUS).i32_const(204).i32_eq().i32_or();
    i.local_get(L::STATUS).i32_const(304).i32_eq().i32_or();
    i.local_set(L::NOBODY);
    // The header pairs, in order; the handler's `Connection` field is
    // dropped (the host owns the connection, as native's core does). A field
    // the host refuses (one it manages) answers header-error and is left out.
    i.call(I_HTTP_FIELDS_NEW).local_set(L::FIELDS);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(L::CUR).local_get(L::END).i32_ge_u().br_if(1);
    cell(i);
    i.local_get(L::TMP).local_set(L::KEY_PTR);
    i.local_get(L::CUR).local_get(L::TMP).i32_sub().local_set(L::KEY_LEN);
    i.local_get(L::CUR).local_get(L::END).i32_ge_u().br_if(1);
    cell(i);
    // key == "connection", ASCII case-insensitively.
    i.local_get(L::KEY_LEN).i32_const(t.connection.len).i32_eq().if_(BlockType::Result(ValType::I32));
    i.i32_const(1);
    for k in 0..t.connection.len as u64 {
        i.local_get(L::KEY_PTR).i32_load8_u(mem8(k)).i32_const(0x20).i32_or();
        i.i32_const(t.connection.at).i32_load8_u(mem8(k)).i32_eq().i32_and();
    }
    i.else_().i32_const(0).end();
    i.i32_eqz().if_(BlockType::Empty);
    i.local_get(L::FIELDS).local_get(L::KEY_PTR).local_get(L::KEY_LEN);
    i.local_get(L::TMP).local_get(L::CUR).local_get(L::TMP).i32_sub();
    i.i32_const(ret).call(I_HTTP_FIELDS_APPEND);
    i.end();
    i.br(0).end().end();
    // Content-Length for a response with a body, as native writes it.
    i.local_get(L::NOBODY).i32_eqz().if_(BlockType::Empty);
    http_decimal_i32(i, t.digits, L::B_LEN, (L::TMP, L::DIGIT, L::CELL_LEN, L::K));
    i.local_get(L::FIELDS).i32_const(t.content_length.at).i32_const(t.content_length.len);
    i.i32_const(t.digits).local_get(L::DIGIT).i32_const(ret).call(I_HTTP_FIELDS_APPEND);
    i.end();
    // response.new(headers, contents?, trailers) → (response, result future).
    i.call(S_RESP_FNEW).local_set(L::S64);
    split_pair(i, L::F_TX, L::F_RX);
    i.i32_const(0).local_set(L::S_RX);
    i.local_get(L::NOBODY).i32_eqz().if_(BlockType::Empty);
    i.call(S_RESP_SNEW).local_set(L::S64);
    split_pair(i, L::S_TX, L::S_RX);
    i.end();
    i.local_get(L::FIELDS).local_get(L::NOBODY).i32_eqz().local_get(L::S_RX).local_get(L::F_RX).i32_const(ret);
    i.call(S_RESP_NEW);
    i.i32_const(ret).i32_load(mem(0)).local_set(L::RESP);
    i.i32_const(ret).i32_load(mem(4)).local_set(L::RFUT);
    i.local_get(L::RESP).local_get(L::STATUS).call(S_SET_STATUS).drop();
    // task.return(ok(response)): the host now reads the body.
    if abi.tr_params.len() == 1 {
        let buf = (g.park + SERVE_TRBUF) as i32;
        i.i32_const(buf).i32_const(0).i32_store8(mem8(0));
        i.i32_const(buf).local_get(L::RESP).i32_store(mem(abi.tr_payload));
        i.i32_const(buf).call(S_TASK_RETURN);
    } else {
        i.i32_const(0).local_get(L::RESP);
        if abi.tr_params[1] == ValType::I64 {
            i.i64_extend_i32_u();
        }
        for p in &abi.tr_params[2..] {
            match p {
                ValType::I64 => i.i64_const(0),
                ValType::F32 => i.f32_const(0.0f32.into()),
                ValType::F64 => i.f64_const(0.0f64.into()),
                _ => i.i32_const(0),
            };
        }
        i.call(S_TASK_RETURN);
    }
    // The body (each write through `$await`; a reader gone ends it), then
    // the trailers future `ok(none)`.
    i.local_get(L::NOBODY).i32_eqz().if_(BlockType::Empty);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(L::B_LEN).i32_eqz().br_if(1);
    i.local_get(L::S_TX);
    i.local_get(L::S_TX).local_get(L::B_PTR).local_get(L::B_LEN).call(S_RESP_SWRITE);
    i.call(g.f_await).local_set(L::N);
    i.local_get(L::B_PTR).local_get(L::N).i32_const(4).i32_shr_u().i32_add().local_set(L::B_PTR);
    i.local_get(L::B_LEN).local_get(L::N).i32_const(4).i32_shr_u().i32_sub().local_set(L::B_LEN);
    i.local_get(L::N).i32_const(15).i32_and().br_if(1);
    i.br(0).end().end();
    i.local_get(L::S_TX).call(S_RESP_SDROPW);
    i.end();
    i.i32_const(ret).i64_const(0).i64_store(mem64(16));
    i.local_get(L::F_TX);
    i.local_get(L::F_TX).i32_const(ret + 16).call(S_RESP_FWRITE);
    i.call(g.f_await).drop();
    i.local_get(L::F_TX).call(S_RESP_FDROPW);
    i.local_get(L::RFUT).call(S_RESP_RFUT_DROPR);
}

/// `n` in decimal at `at`, no padding; `DIGIT` = its length.
fn http_decimal_i32(i: &mut wasm_encoder::InstructionSink<'_>, at: i32, n: u32, (tmp, digit, rest, cur): (u32, u32, u32, u32)) {
    i.local_get(n).local_set(tmp);
    i.i32_const(1).local_set(digit);
    i.local_get(tmp).local_set(rest);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(rest).i32_const(10).i32_lt_u().br_if(1);
    i.local_get(rest).i32_const(10).i32_div_u().local_set(rest);
    i.local_get(digit).i32_const(1).i32_add().local_set(digit);
    i.br(0).end().end();
    i.local_get(digit).local_set(cur);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(cur).i32_eqz().br_if(1);
    i.local_get(cur).i32_const(1).i32_sub().local_set(cur);
    i.i32_const(at).local_get(cur).i32_add();
    i.local_get(tmp).i32_const(10).i32_rem_u().i32_const(48).i32_add();
    i.i32_store8(mem8(0));
    i.local_get(tmp).i32_const(10).i32_div_u().local_set(tmp);
    i.br(0).end().end();
}

/// `(request) -> status`: the async-lifted `wasi:http/handler.handle`.
fn shim_serve_handle(g: P3Globals, s: ServeGlobals, fns: ServeFns, (abi, t, arena): (&ServeAbi, &ServeTexts, &ServeArena), main: u32) -> Function {
    use HandleLocals as L;
    let mut f = Function::new([(L::I32S, ValType::I32), (1, ValType::I64), (ServeArena::LOCALS, ValType::I32)]);
    debug_assert_eq!(L::S64, L::I32S + 1);
    debug_assert_eq!(L::ARENA, L::S64 + 1);
    let mut i = f.instructions();
    // One handler in flight per instance: the guest is not re-entrant.
    i.call(S_BP_INC);
    arena.open(&mut i);
    serve_read_request(&mut i, g, abi, t);
    i.local_get(L::REJ_PTR).if_(BlockType::Empty);
    i.local_get(L::REJ_PTR).local_set(L::CUR);
    i.local_get(L::REJ_PTR).local_get(L::REJ_LEN).i32_add().local_set(L::END);
    i.else_();
    serve_run_guest(&mut i, g, s, fns, main);
    i.end();
    serve_respond(&mut i, g, abi, t);
    // Drain stdout and stderr: each completion future read once the stream
    // ends, so every line of this request reached the host; the next
    // request opens them afresh.
    for (g_tx, g_fut, drop_import, read_import) in [
        (g.g_out_tx, g.g_out_fut, I_OUT_DROP_TX, I_OUT_FUT_READ),
        (g.g_err_tx, g.g_err_fut, I_ERR_DROP_TX, I_ERR_FUT_READ),
    ] {
        i.global_get(g_tx).i32_const(0).i32_ge_s();
        i.if_(BlockType::Empty);
        i.global_get(g_tx).call(drop_import);
        i.global_get(g_fut);
        i.global_get(g_fut).i32_const((g.park + RET) as i32).call(read_import);
        i.call(g.f_await).drop();
        i.i32_const(-1).global_set(g_tx);
        i.i32_const(-1).global_set(g_fut);
        i.end();
    }
    arena.close(&mut i);
    i.call(S_BP_DEC);
    i.i32_const(0); // EXIT: the task is complete.
    i.end();
    f
}
