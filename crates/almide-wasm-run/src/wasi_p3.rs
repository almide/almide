//! `to_p3` (#1628 stage 2, increment 1): rewrite an emitted almide module
//! into a WASI 0.3 COMPONENT — stdio over component-model streams. This
//! is the plumbing keystone for the fan lowering: guest-held streams,
//! sync stream/future builtins, and an async-lifted entry are exactly the
//! vocabulary fan arms will schedule on.
//!
//! Same doctrine as `to_p2` (#1588/#1628 stage 1) with the five-op host
//! surface (console out, exit codes, stdin, entropy, wall clock) PLUS the
//! filesystem READ surface (increment 2a: exists/is-dir/is-file via
//! stat-at, read_text/read_bytes via open-at + sync stream reads) AND
//! the WRITE surface (increment 2d: write/append/write_bytes through
//! write-via-stream with the completion-future durability handshake,
//! recursive mkdir_p, remove/remove_all via stat-then-unlink-or-rmdir —
//! a NON-EMPTY remove_all answers the honest not-empty error until the
//! recursive walk lands). Guest paths resolve against the FIRST preopen
//! (`wasmtime run --dir=.`).
//! Canonical-ABI facts (variant discriminants, payload offsets) are
//! DERIVED from the vendored WIT at emit time (`FsAbi`), never
//! hand-counted. fs writes, env and process keep the DEFINED refusal;
//! the fs-program route flip to this leg waits on the write surface
//! (until then it is exercised via ALMIDE_WASM_STRUCTURAL=1). The
//! transform is a post-pass — the emitter's verified envelope is
//! untouched.
//!
//! The 0.3 shapes used (as wasmtime 46+ implements them; vendored WIT
//! under `crates/almide-wasm-run/wit/p3/`):
//!   - stdout/stderr `write-via-stream: func(stream<u8>) ->
//!     future<result<_, error-code>>` — a SYNC call handing the host the
//!     readable end and answering a completion future. The guest opens
//!     the stream ONCE, keeps the writable end, and feeds it with SYNC
//!     `stream.write` (each write rendezvous-blocks until the host
//!     consumed the bytes — program order per stream by construction);
//!   - stdin `read-via-stream: func() -> tuple<stream<u8>, future<...>>`
//!     — sync with a retptr; SYNC `stream.read` lands bytes straight in
//!     guest memory (no cabi_realloc hop, unlike p2's blocking-read);
//!   - the run export (`run: async func() -> result`) is lifted `async`
//!     with a CALLBACK, but the body runs to completion in the initial
//!     call — sync builtins may block inside a callback-lifted task (the
//!     sync-streams doctrine), so the callback itself is unreachable:
//!     main → close streams → read each completion future → task.return
//!     (ok) → EXIT;
//!   - `wasi:clocks/system-clock.now() -> instant` (s64 seconds, u32
//!     nanos) and `get-random-bytes` are plain sync lowers, as on p2.
//!
//! The finale's `future.read` on each output stream's completion future
//! is the DETERMINISTIC drain handshake: it blocks until the host
//! acknowledges the whole stream, so "the program exited" implies "every
//! byte reached the host" — the property the cross-target byte-identity
//! contract stands on. Sync `stream.write`/`stream.read`/`future.read`
//! are the 🚝 builtins: the runtime needs
//! `component-model-more-async-builtins` (wasmtime: `-W ...`, on by
//! default in current releases' `-S p3` stacks).

use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, EntityType, ExportKind, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, MemArg, MemorySection, MemoryType,
    Module, TypeSection, ValType,
};

use crate::wasi::{
    mem, mem8, parse_module, reencode_body, type_index, Parsed, Remap, DATA, MSG, PARK_SPAN,
    UNSUPPORTED_MSG,
};

// Import indices (18 imports replace the 5 almide.* ones).
const I_EXIT: u32 = 0;
const I_OUT_CALL: u32 = 1; // write-via-stream (stdout): (rx) -> future
const I_OUT_NEW: u32 = 2;
const I_OUT_WRITE: u32 = 3;
const I_OUT_DROP_TX: u32 = 4;
const I_OUT_FUT_READ: u32 = 5;
const I_ERR_CALL: u32 = 6;
const I_ERR_NEW: u32 = 7;
const I_ERR_WRITE: u32 = 8;
const I_ERR_DROP_TX: u32 = 9;
const I_ERR_FUT_READ: u32 = 10;
const I_STDIN_OPEN: u32 = 11; // read-via-stream (sync, retptr)
const I_STDIN_READ: u32 = 12;
const I_STDIN_DROP_RX: u32 = 13;
const I_STDIN_DROP_FUT: u32 = 14;
const I_CLOCK_NOW: u32 = 15;
const I_RANDOM: u32 = 16;
const I_TASK_RETURN: u32 = 17;
// The filesystem READ surface (#1628 increment 2a): preopens + stat-at +
// open-at + read-via-stream, all SYNC lowers (an async-declared func may
// be lowered sync — the fiber blocks, same doctrine as the 🚝 builtins).
const I_FS_PRE: u32 = 18; // preopens.get-directories (retptr)
const I_FS_OPEN: u32 = 19; // [method]descriptor.open-at
const I_FS_STAT: u32 = 20; // [method]descriptor.stat-at
const I_FS_RVS: u32 = 21; // [method]descriptor.read-via-stream
const I_FS_SREAD: u32 = 22; // [stream-read-0] of read-via-stream
const I_FS_SDROP: u32 = 23; // [stream-drop-readable-0] of read-via-stream
const I_FS_FDROP: u32 = 24; // [future-drop-readable-1] of read-via-stream
const I_FS_RESDROP: u32 = 25; // [resource-drop]descriptor
// The fan prefetch machinery (#1628 increment 2b): an ASYNC-lowered
// open-at (>4 flats, so ONE argptr + retptr -> packed status) plus the
// $root waitable-set builtins the drain loop schedules on.
const I_FS_AOPEN: u32 = 26; // [async-lower][method]descriptor.open-at
const I_WS_NEW: u32 = 27; // [waitable-set-new]
const I_WS_JOIN: u32 = 28; // [waitable-join]
const I_WS_WAIT: u32 = 29; // [waitable-set-wait]
const I_SUBTASK_DROP: u32 = 30; // [subtask-drop]
const I_WS_DROP: u32 = 31; // [waitable-set-drop]
const I_SUBTASK_CANCEL: u32 = 32; // [subtask-cancel] — the loser-arm abandonment
// The filesystem WRITE surface (#1628 increment 2d): write/append streams
// (the guest keeps the writable end, sync stream.write feeds it, and the
// completion future.read is the durability handshake), plus the three
// path ops. All sync lowers, as the read surface.
const I_FS_WVS: u32 = 33; // [method]descriptor.write-via-stream
const I_FS_AVS: u32 = 34; // [method]descriptor.append-via-stream
const I_FS_WNEW: u32 = 35; // [stream-new-0] of write-via-stream
const I_FS_WWRITE: u32 = 36; // [stream-write-0] of write-via-stream
const I_FS_WDROP: u32 = 37; // [stream-drop-writable-0] of write-via-stream
const I_FS_WFUT: u32 = 38; // [future-read-1] of write-via-stream
const I_FS_MKDIR: u32 = 39; // [method]descriptor.create-directory-at
const I_FS_UNLINK: u32 = 40; // [method]descriptor.unlink-file-at
const I_FS_RMDIR: u32 = 41; // [method]descriptor.remove-directory-at
const IMPORTS: u32 = 42;

// ── The p3 http client import block (#1710 PR B) ────────────────────────
// Appended AFTER the fs table and included only when the module's op set
// reaches the http family (43..=47) — a non-http component must not demand
// `-S http=y` from its runtime. Builtin names follow wit-parser's mangling
// (`[stream-new-0][static]request.new`): the component encode validates
// them against the world, so a drifted name fails loudly at emit.
const I_HTTP_FIELDS_NEW: u32 = 42; // [constructor]fields () -> own<fields>
const I_HTTP_REQ_NEW: u32 = 43; // [static]request.new (retptr: request + sent-future)
const I_HTTP_REQ_SNEW: u32 = 44; // [stream-new-0] of request.new (contents)
const I_HTTP_REQ_SWRITE: u32 = 45; // [stream-write-0] of request.new
const I_HTTP_REQ_SDROPW: u32 = 46; // [stream-drop-writable-0] of request.new
const I_HTTP_REQ_FNEW: u32 = 47; // [future-new-1] of request.new (trailers)
const I_HTTP_REQ_FWRITE: u32 = 48; // [future-write-1] of request.new
const I_HTTP_REQ_FDROPW: u32 = 49; // [future-drop-writable-1] of request.new
const I_HTTP_REQ_SENTDROP: u32 = 50; // [future-drop-readable-2] of request.new
const I_HTTP_SET_METHOD: u32 = 51; // [method]request.set-method
const I_HTTP_SET_SCHEME: u32 = 52; // [method]request.set-scheme
const I_HTTP_SET_AUTH: u32 = 53; // [method]request.set-authority
const I_HTTP_SET_PATH: u32 = 54; // [method]request.set-path-with-query
const I_HTTP_SEND: u32 = 55; // client.send (sync lower, retptr)
const I_HTTP_STATUS: u32 = 56; // [method]response.get-status-code
const I_HTTP_CONSUME: u32 = 57; // [static]response.consume-body (retptr)
const I_HTTP_CB_FNEW: u32 = 58; // [future-new-0] of consume-body (handling result)
const I_HTTP_CB_FWRITE: u32 = 59; // [future-write-0] of consume-body
const I_HTTP_CB_FDROPW: u32 = 60; // [future-drop-writable-0] of consume-body
const I_HTTP_BODY_READ: u32 = 61; // [stream-read-1] of consume-body (the body)
const I_HTTP_BODY_DROPR: u32 = 62; // [stream-drop-readable-1] of consume-body
const I_HTTP_TRL_DROPR: u32 = 63; // [future-drop-readable-2] of consume-body
const I_HTTP_REQ_DROP: u32 = 64; // [resource-drop]request
const I_HTTP_RESP_DROP: u32 = 65; // [resource-drop]response
const I_HTTP_FIELDS_DROP: u32 = 66; // [resource-drop]fields
const IMPORTS_HTTP: u32 = 67;

// Park offsets past the shared ones: retptr / future-payload scratch.
const RET: u64 = 32;
// stat-at's result<descriptor-stat, error-code> needs 112 bytes — parked
// past MSG (64..109), before the fs message statics at 256.
const STATRET: u64 = 128;
// Static fs error messages (canonical-ABI error-code -> the SAME strings
// the native runtime's io::Error Display produces, so the common error
// legs stay byte-identical). Offsets within the park span.
const MSG_NOENT: u64 = 256;
const MSG_ACCES: u64 = 320;
const MSG_ISDIR: u64 = 384;
const MSG_GEN: u64 = 448;
const MSG_NOPRE: u64 = 512;
const E_NOENT: &[u8] = b"No such file or directory (os error 2)";
const E_ACCES: &[u8] = b"Permission denied (os error 13)";
const E_ISDIR: &[u8] = b"Is a directory (os error 21)";
const E_GEN: &[u8] = b"filesystem operation failed";
const E_NOPRE: &[u8] = b"no filesystem preopen (run with --dir)";
// The p3 http transport-error static (#1710 PR B): transport-error TEXT is
// host-specific by contract — the cross-lane fixtures assert err-ness, not
// the wording (the native legs' per-OS errno suffixes already force that).
const MSG_HTTP: u64 = 576;
const E_HTTP: &[u8] = b"http request failed (p3 transport)";

// Park layout, checked at COMPILE time: retptr spans and the message
// statics must not collide with each other or the stdin/entropy DATA
// span. (The stat result's WIT-derived footprint is checked at emit
// time where the resolve is in hand.)
const _: () = {
    assert!(RET + 32 <= MSG);
    assert!(MSG + UNSUPPORTED_MSG.len() as u64 <= STATRET);
    assert!(MSG_NOENT + E_NOENT.len() as u64 <= MSG_ACCES);
    assert!(MSG_ACCES + E_ACCES.len() as u64 <= MSG_ISDIR);
    assert!(MSG_ISDIR + E_ISDIR.len() as u64 <= MSG_GEN);
    assert!(MSG_GEN + E_GEN.len() as u64 <= MSG_NOPRE);
    assert!(MSG_NOPRE + E_NOPRE.len() as u64 <= MSG_HTTP);
    assert!(MSG_HTTP + E_HTTP.len() as u64 <= DATA);
};

// The fan prefetch slot table: SLOT_CAP slots of SLOT_STRIDE bytes on
// the bump heap (fresh memory is zero, and the bump never moves a
// handed-out range — argptr/retptr stay stable for the subtask's whole
// life, which the async ABI requires). Layout per slot:
//   args block @0..24 (self, path-flags, path ptr, path len,
//                      open-flags, descriptor-flags),
//   open result @24..44, state @48 (0 empty / 1 pending / 2 done),
//   subtask @52. Arms past SLOT_CAP simply stay sequential (the await
//   falls back to the sync op-1 path).
const SLOT_CAP: i32 = 1024;
const SLOT_STRIDE: i32 = 64;

/// Canonical-ABI facts the fs shim stores through — DERIVED from the
/// vendored WIT at emit time, never hand-counted (the wit-bindgen
/// doctrine: a case index or payload offset written as a literal drifts
/// silently when the WIT moves; a lookup by name fails loudly).
struct FsAbi {
    ec_no_entry: i32,      // error-code case index
    ec_access: i32,
    ec_not_permitted: i32,
    ec_is_directory: i32,
    ec_exist: i32,
    dt_directory: i32,     // descriptor-type case index
    dt_regular_file: i32,
    open_payload: u64,     // result<descriptor, error-code> payload offset
    unit_payload: u64,     // result<_, error-code> payload offset (the path ops)
    stat_payload: u64,     // result<descriptor-stat, error-code> payload offset
    stat_size: u64,        // full result size (park-span room check)
}

fn fs_abi(resolve: &wit_parser::Resolve) -> anyhow::Result<FsAbi> {
    use wit_parser::{Type, TypeDefKind};
    // Scope the lookups to wasi:filesystem's `types` interface — bare
    // name search collides (wasi:cli has its own `error-code`).
    let (_, fs_pkg) = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "wasi" && p.name.name == "filesystem")
        .ok_or_else(|| anyhow::anyhow!("wasi:filesystem package not in the resolve"))?;
    let iface_id = *fs_pkg
        .interfaces
        .get("types")
        .ok_or_else(|| anyhow::anyhow!("wasi:filesystem/types interface not found"))?;
    let iface = &resolve.interfaces[iface_id];
    let find = |name: &str| -> anyhow::Result<wit_parser::TypeId> {
        iface
            .types
            .get(name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("wit type {name} not found in wasi:filesystem/types"))
    };
    let case = |id: wit_parser::TypeId, name: &str| -> anyhow::Result<i32> {
        match &resolve.types[id].kind {
            TypeDefKind::Variant(v) => v
                .cases
                .iter()
                .position(|c| c.name == name)
                .map(|p| p as i32)
                .ok_or_else(|| anyhow::anyhow!("variant case {name} not found")),
            k => Err(anyhow::anyhow!("expected variant, got {k:?}")),
        }
    };
    let ec = find("error-code")?;
    let dt = find("descriptor-type")?;
    let stat = find("descriptor-stat")?;
    let mut sa = wit_parser::SizeAlign::default();
    sa.fill(resolve);
    // result<T, error-code> payload offset = discriminant (1 byte for
    // <=255 cases) aligned up to max(align(T), align(error-code)).
    let ec_align = sa.align(&Type::Id(ec)).align_wasm32() as u64;
    let stat_align = sa.align(&Type::Id(stat)).align_wasm32() as u64;
    let stat_sz = sa.size(&Type::Id(stat)).size_wasm32() as u64;
    let open_payload = 4u64.max(ec_align); // own<descriptor> aligns 4
    let unit_payload = ec_align; // result<_, error-code>: the err IS the payload
    let stat_payload = stat_align.max(ec_align);
    Ok(FsAbi {
        ec_no_entry: case(ec, "no-entry")?,
        ec_access: case(ec, "access")?,
        ec_not_permitted: case(ec, "not-permitted")?,
        ec_is_directory: case(ec, "is-directory")?,
        ec_exist: case(ec, "exist")?,
        dt_directory: case(dt, "directory")?,
        dt_regular_file: case(dt, "regular-file")?,
        open_payload,
        unit_payload,
        stat_payload,
        stat_size: stat_payload + stat_sz,
    })
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
    let mut f = Function::new([(23, ValType::I32), (1, ValType::I64), (4, ValType::I32)]);
    let mut i = f.instructions();

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
    i.i32_const(h.m_delete);
    i.end();
    i.end();
    i.end();
    i.end();
    i.local_set(n);
    i.local_get(request).local_get(n).i32_const(0).i32_const(0).call(I_HTTP_SET_METHOD).local_set(k);
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
    i.local_get(request).i32_const((park + RET + 8) as i32).call(I_HTTP_SEND).local_set(n);
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
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(8));
    i.if_(BlockType::Empty);
    fs_err(&mut i, g_ppos, g_plen, park, MSG_HTTP, E_HTTP.len());
    i.end();
    i.i32_const((park + RET) as i32).i32_load(mem(8 + h.send_payload)).local_set(response);

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

    // Error paths above leak the set + pending write-ends by design —
    // they are terminal transport failures.
    i.local_get(hws).i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.local_get(hws).call(I_WS_DROP);
    i.end();

    i.local_get(buf).global_set(g_ppos);
    i.local_get(total).global_set(g_plen);
    i.local_get(total).i64_extend_i32_u().return_();
    i.unreachable();
    i.end();
    f
}

/// Canonical-ABI facts the http shim stores through (#1710 PR B) — DERIVED
/// from the vendored WIT at emit time, never hand-counted (the fs_abi
/// doctrine: a case index or payload offset written as a literal drifts
/// silently when the WIT moves; a lookup by name fails loudly).
struct HttpAbi {
    m_get: i32, // method variant case indices
    m_post: i32,
    m_put: i32,
    m_patch: i32,
    m_delete: i32,
    sch_http: i32, // scheme variant case indices
    sch_https: i32,
    /// result<response, error-code> payload offset (send's retptr layout).
    send_payload: u64,
}

fn http_abi(resolve: &wit_parser::Resolve) -> anyhow::Result<HttpAbi> {
    use wit_parser::{Type, TypeDefKind};
    let (_, http_pkg) = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "wasi" && p.name.name == "http")
        .ok_or_else(|| anyhow::anyhow!("wasi:http package not in the resolve"))?;
    let iface_id = *http_pkg
        .interfaces
        .get("types")
        .ok_or_else(|| anyhow::anyhow!("wasi:http/types interface not found"))?;
    let iface = &resolve.interfaces[iface_id];
    let find = |name: &str| -> anyhow::Result<wit_parser::TypeId> {
        iface
            .types
            .get(name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("wit type {name} not found in wasi:http/types"))
    };
    let case = |id: wit_parser::TypeId, name: &str| -> anyhow::Result<i32> {
        match &resolve.types[id].kind {
            TypeDefKind::Variant(v) => v
                .cases
                .iter()
                .position(|c| c.name == name)
                .map(|p| p as i32)
                .ok_or_else(|| anyhow::anyhow!("variant case {name} not found")),
            k => Err(anyhow::anyhow!("expected variant, got {k:?}")),
        }
    };
    let method = find("method")?;
    let scheme = find("scheme")?;
    let ec = find("error-code")?;
    let mut sa = wit_parser::SizeAlign::default();
    sa.fill(resolve);
    let ec_align = sa.align(&Type::Id(ec)).align_wasm32() as u64;
    Ok(HttpAbi {
        m_get: case(method, "get")?,
        m_post: case(method, "post")?,
        m_put: case(method, "put")?,
        m_patch: case(method, "patch")?,
        m_delete: case(method, "delete")?,
        sch_http: case(scheme, "HTTP")?,
        sch_https: case(scheme, "HTTPS")?,
        // own<response> aligns 4; the discriminant byte rounds up to the
        // larger of that and error-code's alignment.
        send_payload: 4u64.max(ec_align),
    })
}

/// 8-byte MemArg.
fn mem64(offset: u64) -> MemArg {
    MemArg { offset, align: 3, memory_index: 0 }
}

pub fn to_p3(bytes: &[u8], wants_http: bool) -> anyhow::Result<Vec<u8>> {
    // The vendored WIT first: the fs shim's layout facts derive from it,
    // so a WIT/shim drift refuses to emit instead of corrupting stores.
    let mut resolve = wit_parser::Resolve::default();
    for (name, text) in [
        ("clocks.wit", include_str!("../wit/p3/deps/clocks/package.wit")),
        ("random.wit", include_str!("../wit/p3/deps/random/package.wit")),
        ("cli.wit", include_str!("../wit/p3/deps/cli/package.wit")),
        ("filesystem.wit", include_str!("../wit/p3/deps/filesystem/package.wit")),
        ("http.wit", include_str!("../wit/p3/deps/http/package.wit")),
    ] {
        resolve
            .push_str(name, text)
            .map_err(|e| anyhow::anyhow!("wit {name}: {e}"))?;
    }
    let pkg = resolve
        .push_str("world.wit", include_str!("../wit/p3/world.wit"))
        .map_err(|e| anyhow::anyhow!("wit world: {e}"))?;
    // The http-importing world only when the module's op set reaches the
    // http family — a non-http component must not demand `-S http=y`.
    let world_name = if wants_http { "p3-command-http" } else { "p3-command" };
    let world = resolve
        .select_world(&[pkg], Some(world_name))
        .map_err(|e| anyhow::anyhow!("world: {e}"))?;
    let abi = fs_abi(&resolve)?;
    let habi = if wants_http { Some(http_abi(&resolve)?) } else { None };
    // The stat result's WIT-derived footprint must fit its park slot.
    assert!(STATRET + abi.stat_size <= MSG_NOENT, "STATRET reaches the messages");

    let parsed = parse_module(bytes)?;
    let Parsed {
        mut types,
        func_types,
        tables,
        old_mem_min,
        old_mem_max,
        parsed_globals,
        global_count,
        heap_global,
        exports: _,
        main_index,
        elements,
        mut data,
        bodies,
    } = parsed;
    let main_index = main_index.ok_or_else(|| anyhow::anyhow!("no main export"))?;
    let heap_global = heap_global.ok_or_else(|| anyhow::anyhow!("no __heap export"))?;
    let n_funcs = func_types.len() as u32;
    let n_imports = if wants_http { IMPORTS_HTTP } else { IMPORTS };
    let shift = n_imports - 5;
    let shim_base = n_imports + n_funcs;
    // Shim order mirrors the almide.* import order (println, eprintln,
    // exit, fs_call, host_read), then cabi_realloc, run, callback.
    let f_realloc = shim_base + 5;
    let f_run = shim_base + 6;
    let f_callback = shim_base + 7;

    let heap_init = parsed_globals[heap_global as usize]
        .1
        .ok_or_else(|| anyhow::anyhow!("__heap init not i32"))? as u32 as u64;
    let park: u64 = heap_init;
    // Globals: originals, then plen, ppos, the stream state: stdout/stderr
    // writable ends + completion futures, stdin readable + its future.
    let (g_plen, g_ppos) = (global_count, global_count + 1);
    let (g_out_tx, g_out_fut) = (global_count + 2, global_count + 3);
    let (g_err_tx, g_err_fut) = (global_count + 4, global_count + 5);
    let (g_in_rx, g_in_fut) = (global_count + 6, global_count + 7);
    let g_pre = global_count + 8; // first preopen descriptor (lazy, -1 = unresolved)
    let g_wset = global_count + 9; // the ONE waitable set (lazy, -1)
    let g_slots = global_count + 10; // fan slot-table base (0 = unallocated)
    let g_slotn = global_count + 11; // slot high-water mark
    let mut globals = GlobalSection::new();
    for (idx, (gt, i32v, i64v, f64v)) in parsed_globals.iter().enumerate() {
        let init = if idx as u32 == heap_global {
            ConstExpr::i32_const((heap_init + PARK_SPAN) as i32)
        } else if let Some(v) = i32v {
            ConstExpr::i32_const(*v)
        } else if let Some(v) = i64v {
            ConstExpr::i64_const(*v)
        } else {
            ConstExpr::f64_const(f64::from_bits(f64v.expect("global init")).into())
        };
        globals.global(*gt, &init);
    }
    let mutable_i32 = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // plen
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // ppos
    for _ in 0..8 {
        globals.global(mutable_i32, &ConstExpr::i32_const(-1)); // stream state + g_pre + g_wset
    }
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // g_slots
    globals.global(mutable_i32, &ConstExpr::i32_const(0)); // g_slotn

    // Canonical-ABI core types.
    let t_exit = type_index(&mut types, &[ValType::I32], &[]);
    let t_call = type_index(&mut types, &[ValType::I32], &[ValType::I32]);
    let t_new = type_index(&mut types, &[], &[ValType::I64]);
    let t_rw = type_index(
        &mut types,
        &[ValType::I32, ValType::I32, ValType::I32],
        &[ValType::I32],
    );
    let t_fut_read = type_index(&mut types, &[ValType::I32, ValType::I32], &[ValType::I32]);
    let t_drop = type_index(&mut types, &[ValType::I32], &[]);
    let t_retptr = type_index(&mut types, &[ValType::I32], &[]);
    let t_random = type_index(&mut types, &[ValType::I64, ValType::I32], &[]);
    // Shim types (the almide.* signatures) + realloc + run + callback.
    let t_print = type_index(&mut types, &[ValType::I32, ValType::I32], &[]);
    let t_fs = type_index(&mut types, &[ValType::I32; 5], &[ValType::I64]);
    let t_hread = type_index(&mut types, &[ValType::I32], &[]);
    let t_realloc = type_index(&mut types, &[ValType::I32; 4], &[ValType::I32]);
    let t_status = type_index(&mut types, &[], &[ValType::I32]);
    let t_callback = type_index(&mut types, &[ValType::I32; 3], &[ValType::I32]);
    // fs read-surface core shapes (sync lowers).
    let t_open = type_index(&mut types, &[ValType::I32; 7], &[]);
    let t_stat = type_index(&mut types, &[ValType::I32; 5], &[]);
    let t_rvs = type_index(
        &mut types,
        &[ValType::I32, ValType::I64, ValType::I32],
        &[],
    );
    let t_aopen = type_index(&mut types, &[ValType::I32; 2], &[ValType::I32]);
    // write-via-stream: (self, stream, offset:i64) -> future — the future
    // returns DIRECTLY (1 flat result), as stdio's write-via-stream.
    let t_wvs = type_index(
        &mut types,
        &[ValType::I32, ValType::I32, ValType::I64],
        &[ValType::I32],
    );
    // append-via-stream: (self, stream) -> future.
    let t_avs = type_index(&mut types, &[ValType::I32; 2], &[ValType::I32]);
    // path ops: (self, ptr, len, retptr).
    let t_pathop = type_index(&mut types, &[ValType::I32; 4], &[]);
    let t_ws_new = type_index(&mut types, &[], &[ValType::I32]);
    let t_ws_join = type_index(&mut types, &[ValType::I32; 2], &[]);
    let t_ws_wait = type_index(&mut types, &[ValType::I32; 2], &[ValType::I32]);
    // http client core shapes (#1710 PR B).
    let t_set4 = type_index(&mut types, &[ValType::I32; 4], &[ValType::I32]);
    let t_set5 = type_index(&mut types, &[ValType::I32; 5], &[ValType::I32]);
    let t_consume = type_index(&mut types, &[ValType::I32; 3], &[]);

    let mut type_sec = TypeSection::new();
    for (p, r) in &types {
        type_sec.ty().function(p.iter().copied(), r.iter().copied());
    }

    // The import table, POSITION-CHECKED against the I_* constants: the
    // list is the single source of order, and a drifted constant fails
    // the build of every artifact instead of silently calling the wrong
    // host function.
    let cli_out = "wasi:cli/stdout@0.3.0";
    let cli_err = "wasi:cli/stderr@0.3.0";
    let cli_in = "wasi:cli/stdin@0.3.0";
    let fs_types = "wasi:filesystem/types@0.3.0";
    let import_list: &[(u32, &str, &str, u32)] = &[
        (I_EXIT, "wasi:cli/exit@0.3.0", "exit", t_exit),
        (I_OUT_CALL, cli_out, "write-via-stream", t_call),
        (I_OUT_NEW, cli_out, "[stream-new-0]write-via-stream", t_new),
        (I_OUT_WRITE, cli_out, "[stream-write-0]write-via-stream", t_rw),
        (I_OUT_DROP_TX, cli_out, "[stream-drop-writable-0]write-via-stream", t_drop),
        (I_OUT_FUT_READ, cli_out, "[future-read-1]write-via-stream", t_fut_read),
        (I_ERR_CALL, cli_err, "write-via-stream", t_call),
        (I_ERR_NEW, cli_err, "[stream-new-0]write-via-stream", t_new),
        (I_ERR_WRITE, cli_err, "[stream-write-0]write-via-stream", t_rw),
        (I_ERR_DROP_TX, cli_err, "[stream-drop-writable-0]write-via-stream", t_drop),
        (I_ERR_FUT_READ, cli_err, "[future-read-1]write-via-stream", t_fut_read),
        (I_STDIN_OPEN, cli_in, "read-via-stream", t_retptr),
        (I_STDIN_READ, cli_in, "[stream-read-0]read-via-stream", t_rw),
        (I_STDIN_DROP_RX, cli_in, "[stream-drop-readable-0]read-via-stream", t_drop),
        (I_STDIN_DROP_FUT, cli_in, "[future-drop-readable-1]read-via-stream", t_drop),
        (I_CLOCK_NOW, "wasi:clocks/system-clock@0.3.0", "now", t_retptr),
        (I_RANDOM, "wasi:random/random@0.3.0", "get-random-bytes", t_random),
        (I_TASK_RETURN, "[export]wasi:cli/run@0.3.0", "[task-return]run", t_exit),
        (I_FS_PRE, "wasi:filesystem/preopens@0.3.0", "get-directories", t_retptr),
        (I_FS_OPEN, fs_types, "[method]descriptor.open-at", t_open),
        (I_FS_STAT, fs_types, "[method]descriptor.stat-at", t_stat),
        (I_FS_RVS, fs_types, "[method]descriptor.read-via-stream", t_rvs),
        (I_FS_SREAD, fs_types, "[stream-read-0][method]descriptor.read-via-stream", t_rw),
        (I_FS_SDROP, fs_types, "[stream-drop-readable-0][method]descriptor.read-via-stream", t_drop),
        (I_FS_FDROP, fs_types, "[future-drop-readable-1][method]descriptor.read-via-stream", t_drop),
        (I_FS_RESDROP, fs_types, "[resource-drop]descriptor", t_drop),
        (I_FS_AOPEN, fs_types, "[async-lower][method]descriptor.open-at", t_aopen),
        (I_WS_NEW, "$root", "[waitable-set-new]", t_ws_new),
        (I_WS_JOIN, "$root", "[waitable-join]", t_ws_join),
        (I_WS_WAIT, "$root", "[waitable-set-wait]", t_ws_wait),
        (I_SUBTASK_DROP, "$root", "[subtask-drop]", t_drop),
        (I_WS_DROP, "$root", "[waitable-set-drop]", t_drop),
        (I_SUBTASK_CANCEL, "$root", "[subtask-cancel]", t_call),
        (I_FS_WVS, fs_types, "[method]descriptor.write-via-stream", t_wvs),
        (I_FS_AVS, fs_types, "[method]descriptor.append-via-stream", t_avs),
        (I_FS_WNEW, fs_types, "[stream-new-0][method]descriptor.write-via-stream", t_new),
        (I_FS_WWRITE, fs_types, "[stream-write-0][method]descriptor.write-via-stream", t_rw),
        (I_FS_WDROP, fs_types, "[stream-drop-writable-0][method]descriptor.write-via-stream", t_drop),
        (I_FS_WFUT, fs_types, "[future-read-1][method]descriptor.write-via-stream", t_fut_read),
        (I_FS_MKDIR, fs_types, "[method]descriptor.create-directory-at", t_pathop),
        (I_FS_UNLINK, fs_types, "[method]descriptor.unlink-file-at", t_pathop),
        (I_FS_RMDIR, fs_types, "[method]descriptor.remove-directory-at", t_pathop),
    ];
    assert_eq!(import_list.len() as u32, IMPORTS, "IMPORTS count drift");
    let http_types = "wasi:http/types@0.3.0";
    let http_client = "wasi:http/client@0.3.0";
    let http_import_list: &[(u32, &str, &str, u32)] = &[
        (I_HTTP_FIELDS_NEW, http_types, "[constructor]fields", t_ws_new),
        (I_HTTP_REQ_NEW, http_types, "[static]request.new", t_open),
        (I_HTTP_REQ_SNEW, http_types, "[stream-new-0][static]request.new", t_new),
        (
            I_HTTP_REQ_SWRITE,
            http_types,
            "[async-lower][stream-write-0][static]request.new",
            t_rw,
        ),
        (I_HTTP_REQ_SDROPW, http_types, "[stream-drop-writable-0][static]request.new", t_drop),
        (I_HTTP_REQ_FNEW, http_types, "[future-new-1][static]request.new", t_new),
        (
            I_HTTP_REQ_FWRITE,
            http_types,
            "[async-lower][future-write-1][static]request.new",
            t_fut_read,
        ),
        (I_HTTP_REQ_FDROPW, http_types, "[future-drop-writable-1][static]request.new", t_drop),
        (I_HTTP_REQ_SENTDROP, http_types, "[future-drop-readable-2][static]request.new", t_drop),
        (I_HTTP_SET_METHOD, http_types, "[method]request.set-method", t_set4),
        (I_HTTP_SET_SCHEME, http_types, "[method]request.set-scheme", t_set5),
        (I_HTTP_SET_AUTH, http_types, "[method]request.set-authority", t_set4),
        (I_HTTP_SET_PATH, http_types, "[method]request.set-path-with-query", t_set4),
        (I_HTTP_SEND, http_client, "[async-lower]send", t_fut_read),
        (I_HTTP_STATUS, http_types, "[method]response.get-status-code", t_call),
        (I_HTTP_CONSUME, http_types, "[static]response.consume-body", t_consume),
        (I_HTTP_CB_FNEW, http_types, "[future-new-0][static]response.consume-body", t_new),
        (I_HTTP_CB_FWRITE, http_types, "[future-write-0][static]response.consume-body", t_fut_read),
        (I_HTTP_CB_FDROPW, http_types, "[future-drop-writable-0][static]response.consume-body", t_drop),
        (I_HTTP_BODY_READ, http_types, "[stream-read-1][static]response.consume-body", t_rw),
        (I_HTTP_BODY_DROPR, http_types, "[stream-drop-readable-1][static]response.consume-body", t_drop),
        (I_HTTP_TRL_DROPR, http_types, "[future-drop-readable-2][static]response.consume-body", t_drop),
        (I_HTTP_REQ_DROP, http_types, "[resource-drop]request", t_drop),
        (I_HTTP_RESP_DROP, http_types, "[resource-drop]response", t_drop),
        (I_HTTP_FIELDS_DROP, http_types, "[resource-drop]fields", t_drop),
    ];
    assert_eq!(
        IMPORTS + http_import_list.len() as u32,
        IMPORTS_HTTP,
        "IMPORTS_HTTP count drift"
    );
    let mut imports = ImportSection::new();
    for (k, (want, m, n, t)) in import_list.iter().enumerate() {
        assert_eq!(k as u32, *want, "import order drift at {m}#{n}");
        imports.import(m, n, EntityType::Function(*t));
    }
    if wants_http {
        for (k, (want, m, n, t)) in http_import_list.iter().enumerate() {
            assert_eq!(IMPORTS + k as u32, *want, "http import order drift at {m}#{n}");
            imports.import(m, n, EntityType::Function(*t));
        }
    }

    let mut functions = FunctionSection::new();
    for ti in &func_types {
        functions.function(*ti);
    }
    for ti in [t_print, t_print, t_exit, t_fs, t_hread, t_realloc, t_status, t_callback] {
        functions.function(ti);
    }
    if wants_http {
        functions.function(t_fs); // shim_http (the almide fs_call ABI)
    }

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: old_mem_min + PARK_SPAN / 65536,
        // #1729: carry the heap-cap maximum through, span-shifted.
        maximum: old_mem_max.map(|m| m.max(old_mem_min) + PARK_SPAN / 65536),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let exports = {
        let mut e = wasm_encoder::ExportSection::new();
        e.export("memory", ExportKind::Memory, 0);
        e.export("[async-lift]wasi:cli/run@0.3.0#run", ExportKind::Func, f_run);
        e.export(
            "[callback][async-lift]wasi:cli/run@0.3.0#run",
            ExportKind::Func,
            f_callback,
        );
        e.export("cabi_realloc", ExportKind::Func, f_realloc);
        e
    };

    let mut code = CodeSection::new();
    let mut remap = Remap { shim_base, shift };
    for b in bodies {
        code.function(&reencode_body(&b, &mut remap, I_EXIT)?);
    }
    code.function(&shim_print(g_out_tx, g_out_fut, I_OUT_CALL, I_OUT_NEW, I_OUT_WRITE, park, true));
    code.function(&shim_print(g_err_tx, g_err_fut, I_ERR_CALL, I_ERR_NEW, I_ERR_WRITE, park, true));
    code.function(&shim_exit());
    let f_fs_self = shim_base + 3;
    let f_http = wants_http.then_some(shim_base + 8);
    code.function(&shim_fs_call(
        park, g_plen, g_ppos, g_in_rx, g_in_fut, g_out_tx, g_out_fut, g_err_tx, g_err_fut,
        g_pre, f_realloc, &abi, f_fs_self, g_wset, g_slots, g_slotn, f_http,
    ));
    code.function(&shim_host_read(g_plen, g_ppos));
    code.function(&shim_cabi_realloc(heap_global));
    code.function(&shim_run(
        main_index + shift,
        park,
        g_out_tx,
        g_out_fut,
        g_err_tx,
        g_err_fut,
        g_in_rx,
        g_in_fut,
        g_wset,
    ));
    code.function(&shim_callback());
    if let Some(h) = habi.as_ref() {
        code.function(&shim_http(park, g_plen, g_ppos, f_realloc, h));
    }

    // Elements re-encode through the Remap (#1716): the import shift must
    // move funcref table entries too (#1688's silent class).
    let mut element_sec = wasm_encoder::ElementSection::new();
    for e in elements {
        use wasm_encoder::reencode::Reencode as _;
        remap
            .parse_element(&mut element_sec, e)
            .map_err(|e| anyhow::anyhow!("element reencode: {e:?}"))?;
    }

    data.active(0, &ConstExpr::i32_const((park + MSG) as i32), UNSUPPORTED_MSG.iter().copied());
    for (off, msg) in [
        (MSG_NOENT, E_NOENT),
        (MSG_ACCES, E_ACCES),
        (MSG_ISDIR, E_ISDIR),
        (MSG_GEN, E_GEN),
        (MSG_NOPRE, E_NOPRE),
    ] {
        data.active(0, &ConstExpr::i32_const((park + off) as i32), msg.iter().copied());
    }
    if wants_http {
        data.active(0, &ConstExpr::i32_const((park + MSG_HTTP) as i32), E_HTTP.iter().copied());
    }

    let mut m = Module::new();
    m.section(&type_sec)
        .section(&imports)
        .section(&functions)
        .section(&tables)
        .section(&memories)
        .section(&globals)
        .section(&exports)
        .section(&element_sec)
        .section(&code)
        .section(&data);
    let mut core = m.finish();
    wasmparser::validate(&core)?;

    wit_component::embed_component_metadata(
        &mut core,
        &resolve,
        world,
        wit_component::StringEncoding::UTF8,
    )
    .map_err(|e| anyhow::anyhow!("embed: {e}"))?;
    let component = wit_component::ComponentEncoder::default()
        .module(&core)
        .map_err(|e| anyhow::anyhow!("module: {e}"))?
        .encode()
        .map_err(|e| anyhow::anyhow!("encode: {e}"))?;
    Ok(component)
}

/// Lazy stream open: `if g_tx < 0 { (rx,tx) = stream.new; g_tx = tx;
/// g_fut = write-via-stream(rx) }`. The host's read side starts
/// concurrently; every later sync write rendezvous-blocks against it.
fn open_stream(
    i: &mut wasm_encoder::InstructionSink<'_>,
    g_tx: u32,
    g_fut: u32,
    call_import: u32,
    new_import: u32,
    scratch64: u32,
) {
    i.global_get(g_tx).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    i.call(new_import).local_set(scratch64);
    // tx = high half, rx = low half (the sync-streams.wast packing)
    i.local_get(scratch64).i64_const(32).i64_shr_u().i32_wrap_i64().global_set(g_tx);
    i.local_get(scratch64).i32_wrap_i64();
    i.call(call_import).global_set(g_fut);
    i.end();
}

/// Sync write loop: `while len > 0 { r = stream.write(tx, ptr, len);
/// n = r >> 4; if n == 0 { break } ptr += n; len -= n }` — a DROPPED
/// status answers n=0 and the loop exits (output sunk, as p2/POSIX).
fn write_all(
    i: &mut wasm_encoder::InstructionSink<'_>,
    g_tx: u32,
    write_import: u32,
    ptr: u32,
    len: u32,
    n: u32,
) {
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(len).i32_eqz().br_if(1);
    i.global_get(g_tx);
    i.local_get(ptr).local_get(len);
    i.call(write_import);
    i.i32_const(4).i32_shr_u().local_set(n);
    i.local_get(n).i32_eqz().br_if(1);
    i.local_get(ptr).local_get(n).i32_add().local_set(ptr);
    i.local_get(len).local_get(n).i32_sub().local_set(len);
    i.br(0).end().end();
}

/// `(ptr, len) -> ()`: open-once, sync-write-all, then the newline.
fn shim_print(
    g_tx: u32,
    g_fut: u32,
    call_import: u32,
    new_import: u32,
    write_import: u32,
    park: u64,
    newline: bool,
) -> Function {
    let (ptr, len) = (0u32, 1u32);
    let n = 2u32;
    let s64 = 3u32;
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    // locals: 0 ptr, 1 len (params), 2 n (i32), 3 scratch (i64)
    let mut i = f.instructions();
    open_stream(&mut i, g_tx, g_fut, call_import, new_import, s64);
    write_all(&mut i, g_tx, write_import, ptr, len, n);
    if newline {
        i.i32_const(park as i32).i32_const(0x0A).i32_store8(mem8(0));
        i.i32_const(park as i32).local_set(ptr);
        i.i32_const(1).local_set(len);
        write_all(&mut i, g_tx, write_import, ptr, len, n);
    }
    i.end();
    f
}

/// `(code) -> ()`: exit(ok) for 0, exit(err) otherwise.
fn shim_exit() -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(0).i32_const(0).i32_ne();
    i.call(I_EXIT);
    i.unreachable();
    i.end();
    f
}

/// Lazy stdin open: `if g_rx < 0 { read-via-stream(retptr); g_rx =
/// mem[ret]; g_fut = mem[ret+4] }`.
fn open_stdin(i: &mut wasm_encoder::InstructionSink<'_>, g_rx: u32, g_fut: u32, park: u64) {
    i.global_get(g_rx).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32);
    i.call(I_STDIN_OPEN);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).global_set(g_rx);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).global_set(g_fut);
    i.end();
}

/// Lazy first-preopen resolve: `if g_pre < 0 { get-directories(retptr);
/// if len > 0 { g_pre = mem[listptr] } }` — increment 2a resolves guest
/// paths against the FIRST preopen verbatim (relative paths under
/// `wasmtime run --dir=.`); prefix matching over the full preopen list
/// is the follow-up noted in the module header.
fn fs_preopen(i: &mut wasm_encoder::InstructionSink<'_>, g_pre: u32, park: u64) {
    i.global_get(g_pre).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32).call(I_FS_PRE);
    i.i32_const((park + RET) as i32).i32_load(mem(4));
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).i32_load(mem(0)).global_set(g_pre);
    i.end();
    i.end();
}

/// Err return: park the static message, answer `pack(1, len)`.
fn fs_err(
    i: &mut wasm_encoder::InstructionSink<'_>,
    g_ppos: u32,
    g_plen: u32,
    park: u64,
    off: u64,
    len: usize,
) {
    i.i32_const((park + off) as i32).global_set(g_ppos);
    i.i32_const(len as i32).global_set(g_plen);
    i.i64_const((1i64 << 32) | len as i64).return_();
}

/// Error-code discriminant in local `n` -> the fs_err mapping (no-entry
/// / access / not-permitted / is-directory / generic). Always returns.
#[allow(clippy::too_many_arguments)]
fn fs_open_err_map(
    i: &mut wasm_encoder::InstructionSink<'_>,
    g_ppos: u32,
    g_plen: u32,
    park: u64,
    abi: &FsAbi,
    n: u32,
) {
    i.local_get(n).i32_const(abi.ec_no_entry).i32_eq();
    i.if_(BlockType::Empty);
    fs_err(i, g_ppos, g_plen, park, MSG_NOENT, E_NOENT.len());
    i.end();
    i.local_get(n).i32_const(abi.ec_access).i32_eq();
    i.local_get(n).i32_const(abi.ec_not_permitted).i32_eq().i32_or();
    i.if_(BlockType::Empty);
    fs_err(i, g_ppos, g_plen, park, MSG_ACCES, E_ACCES.len());
    i.end();
    i.local_get(n).i32_const(abi.ec_is_directory).i32_eq();
    i.if_(BlockType::Empty);
    fs_err(i, g_ppos, g_plen, park, MSG_ISDIR, E_ISDIR.len());
    i.end();
    fs_err(i, g_ppos, g_plen, park, MSG_GEN, E_GEN.len());
}

/// Open descriptor in local `d` -> read-via-stream, the doubling sync
/// read loop (DROPPED = EOF), handle drops, payload park, and the
/// `pack(0, total)` return.
#[allow(clippy::too_many_arguments)]
fn fs_read_tail(
    i: &mut wasm_encoder::InstructionSink<'_>,
    park: u64,
    f_realloc: u32,
    g_ppos: u32,
    g_plen: u32,
    d: u32,
    rx: u32,
    fut: u32,
    buf: u32,
    cap: u32,
    total: u32,
    n: u32,
) {
    i.local_get(d).i64_const(0).i32_const((park + RET) as i32).call(I_FS_RVS);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).local_set(rx);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).local_set(fut);
    i.i32_const(0).i32_const(0).i32_const(8).i32_const(65536).call(f_realloc).local_set(buf);
    i.i32_const(65536).local_set(cap);
    i.i32_const(0).local_set(total);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(total).local_get(cap).i32_ge_u();
    i.if_(BlockType::Empty);
    i.local_get(buf).local_get(cap).i32_const(8);
    i.local_get(cap).i32_const(1).i32_shl();
    i.call(f_realloc).local_set(buf);
    i.local_get(cap).i32_const(1).i32_shl().local_set(cap);
    i.end();
    i.local_get(rx);
    i.local_get(buf).local_get(total).i32_add();
    i.local_get(cap).local_get(total).i32_sub();
    i.call(I_FS_SREAD);
    i.i32_const(4).i32_shr_u().local_set(n);
    i.local_get(n).i32_eqz().br_if(1);
    i.local_get(total).local_get(n).i32_add().local_set(total);
    i.br(0).end().end();
    i.local_get(rx).call(I_FS_SDROP);
    i.local_get(fut).call(I_FS_FDROP);
    i.local_get(d).call(I_FS_RESDROP);
    i.local_get(buf).global_set(g_ppos);
    i.local_get(total).global_set(g_plen);
    i.local_get(total).i64_extend_i32_u().return_();
}

/// The five-op host contract over p3 (op codes shared with the embedded
/// host): 30 raw stdout, 31 stdin read-to-end, 35 stdin take-n, 32
/// entropy, 34 wall clock; anything else = the defined refusal.
#[allow(clippy::too_many_arguments)]
fn shim_fs_call(
    park: u64,
    g_plen: u32,
    g_ppos: u32,
    g_in_rx: u32,
    g_in_fut: u32,
    g_out_tx: u32,
    g_out_fut: u32,
    g_err_tx: u32,
    g_err_fut: u32,
    g_pre: u32,
    f_realloc: u32,
    abi: &FsAbi,
    f_self: u32,
    g_wset: u32,
    g_slots: u32,
    g_slotn: u32,
    f_http: Option<u32>,
) -> Function {
    let (op, a_ptr, a_len, b_ptr, b_len) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let total = 5u32;
    let n = 6u32;
    let s64 = 7u32;
    let (d, buf, cap, rx, fut) = (8u32, 9u32, 10u32, 11u32, 12u32);
    let (sl, j) = (13u32, 14u32);
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I64), (7, ValType::I32)]);
    let mut i = f.instructions();

    // ops 43..=47: the http string client (#1710 PR B) — forwarded whole
    // to the dedicated shim when the module's op set earned the imports.
    if let Some(h) = f_http {
        i.local_get(op).i32_const(43).i32_ge_s();
        i.local_get(op).i32_const(47).i32_le_s();
        i.i32_and().if_(BlockType::Empty);
        for pidx in 0..5u32 {
            i.local_get(pidx);
        }
        i.call(h).return_();
        i.end();
    }

    // op 30: raw stdout append (no newline) — b carries the bytes.
    i.local_get(op).i32_const(30).i32_eq().if_(BlockType::Empty);
    open_stream(&mut i, g_out_tx, g_out_fut, I_OUT_CALL, I_OUT_NEW, s64);
    write_all(&mut i, g_out_tx, I_OUT_WRITE, b_ptr, b_len, n);
    i.i64_const(0).return_();
    i.end();

    // op 35: stdin take up to a_len bytes — ONE sync read, straight into
    // the park data span; a DROPPED status (writer closed) answers 0.
    i.local_get(op).i32_const(35).i32_eq().if_(BlockType::Empty);
    open_stdin(&mut i, g_in_rx, g_in_fut, park);
    i.global_get(g_in_rx);
    i.i32_const((park + DATA) as i32);
    i.local_get(a_len);
    i.call(I_STDIN_READ);
    i.i32_const(4).i32_shr_u().global_set(g_plen);
    i.i32_const((park + DATA) as i32).global_set(g_ppos);
    i.global_get(g_plen).i64_extend_i32_u().return_();
    i.end();

    // op 31: stdin read-to-end — sync reads into the park data span until
    // DROPPED/empty; overflow takes the refusal.
    i.local_get(op).i32_const(31).i32_eq().if_(BlockType::Empty);
    open_stdin(&mut i, g_in_rx, g_in_fut, park);
    i.i32_const(0).local_set(total);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    // room check: DATA span is the park's tail.
    i.local_get(total).i32_const((PARK_SPAN - DATA) as i32).i32_ge_u();
    i.if_(BlockType::Empty);
    i.i32_const(1).call(I_EXIT).unreachable();
    i.end();
    i.global_get(g_in_rx);
    i.i32_const((park + DATA) as i32).local_get(total).i32_add();
    i.i32_const((PARK_SPAN - DATA) as i32).local_get(total).i32_sub();
    i.call(I_STDIN_READ);
    i.i32_const(4).i32_shr_u().local_set(n);
    i.local_get(n).i32_eqz().br_if(1);
    i.local_get(total).local_get(n).i32_add().local_set(total);
    i.br(0).end().end();
    i.i32_const((park + DATA) as i32).global_set(g_ppos);
    i.local_get(total).global_set(g_plen);
    i.local_get(total).i64_extend_i32_u().return_();
    i.end();

    // op 32: entropy — n rides b_len; the list lands via cabi_realloc.
    i.local_get(op).i32_const(32).i32_eq().if_(BlockType::Empty);
    i.local_get(b_len).i64_extend_i32_u();
    i.i32_const((park + RET) as i32);
    i.call(I_RANDOM);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).global_set(g_ppos);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).global_set(g_plen);
    i.global_get(g_plen).i64_extend_i32_u().return_();
    i.end();

    // op 34: wall clock, raw nanos (seconds * 1e9 + nanos).
    i.local_get(op).i32_const(34).i32_eq().if_(BlockType::Empty);
    i.i32_const((park + RET) as i32);
    i.call(I_CLOCK_NOW);
    i.i32_const((park + RET) as i32).i64_load(mem64(0));
    i.i64_const(1_000_000_000).i64_mul();
    i.i32_const((park + RET) as i32).i64_load32_u(mem(8));
    i.i64_add().return_();
    i.end();

    // ── The filesystem READ surface (#1628 increment 2a) ──────────────

    // ops 4/5/6: exists / is_dir / is_file — stat-at with symlink-follow
    // (the native `fs::metadata` behavior). The flag rides the len half;
    // these never err: any stat failure (including no preopen) is false.
    i.local_get(op).i32_const(4).i32_eq();
    i.local_get(op).i32_const(5).i32_eq().i32_or();
    i.local_get(op).i32_const(6).i32_eq().i32_or();
    i.if_(BlockType::Empty);
    fs_preopen(&mut i, g_pre, park);
    i.global_get(g_pre).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    i.i64_const(0).return_();
    i.end();
    i.global_get(g_pre);
    i.i32_const(1); // path-flags: symlink-follow
    i.local_get(a_ptr).local_get(a_len);
    i.i32_const((park + STATRET) as i32);
    i.call(I_FS_STAT);
    // result disc @0: nonzero = error-code → false.
    i.i32_const((park + STATRET) as i32).i32_load8_u(mem8(0));
    i.if_(BlockType::Empty);
    i.i64_const(0).return_();
    i.end();
    i.local_get(op).i32_const(4).i32_eq();
    i.if_(BlockType::Empty);
    i.i64_const(1).return_();
    i.end();
    // descriptor-stat's %type is its first field, so its discriminant
    // sits at the result payload offset (both WIT-derived).
    i.i32_const((park + STATRET) as i32).i32_load8_u(mem8(abi.stat_payload)).local_set(n);
    i.local_get(op).i32_const(5).i32_eq();
    i.if_(BlockType::Result(ValType::I32));
    i.local_get(n).i32_const(abi.dt_directory).i32_eq();
    i.else_();
    i.local_get(n).i32_const(abi.dt_regular_file).i32_eq();
    i.end();
    i.i64_extend_i32_u().return_();
    i.end();

    // ops 1/13/14: read_text / read_text_if_exists / read_bytes —
    // open-at(read) then a sync stream-read loop into a cabi_realloc'd
    // buffer (grown by doubling; the bump never frees). DROPPED (n=0)
    // is EOF: stream bytes arrive in order, so total is the whole file.
    i.local_get(op).i32_const(1).i32_eq();
    i.local_get(op).i32_const(13).i32_eq().i32_or();
    i.local_get(op).i32_const(14).i32_eq().i32_or();
    i.if_(BlockType::Empty);
    fs_preopen(&mut i, g_pre, park);
    i.global_get(g_pre).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    fs_err(&mut i, g_ppos, g_plen, park, MSG_NOPRE, E_NOPRE.len());
    i.end();
    i.global_get(g_pre);
    i.i32_const(1); // path-flags: symlink-follow
    i.local_get(a_ptr).local_get(a_len);
    i.i32_const(0); // open-flags: none
    i.i32_const(1); // descriptor-flags: read
    i.i32_const((park + RET) as i32);
    i.call(I_FS_OPEN);
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(0));
    i.if_(BlockType::Empty);
    // error-code discriminant at the result payload offset — case
    // indices WIT-derived, mapped to the native io::Error Display
    // strings so the error legs match.
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(abi.open_payload)).local_set(n);
    i.local_get(op).i32_const(13).i32_eq();
    i.local_get(n).i32_const(abi.ec_no_entry).i32_eq().i32_and();
    i.if_(BlockType::Empty);
    i.i64_const(2i64 << 32).return_(); // ok-none
    i.end();
    fs_open_err_map(&mut i, g_ppos, g_plen, park, abi, n);
    i.end();
    i.i32_const((park + RET) as i32).i32_load(mem(abi.open_payload)).local_set(d);
    fs_read_tail(&mut i, park, f_realloc, g_ppos, g_plen, d, rx, fut, buf, cap, total, n);
    i.end();

    // ── The fan prefetch pair (#1628 increment 2b) ────────────────────

    // op 40: START a slot — async-lower open-at for slot k (k rides
    // b_len; the path rides a). The subtask joins the ONE waitable set;
    // an immediate Returned (packed status 2, no subtask) marks the slot
    // done on the spot. No preopen / k past the cap: the slot stays
    // empty and the await falls back to the sync path.
    i.local_get(op).i32_const(40).i32_eq();
    i.if_(BlockType::Empty);
    fs_preopen(&mut i, g_pre, park);
    i.global_get(g_pre).i32_const(0).i32_ge_s();
    i.local_get(b_len).i32_const(SLOT_CAP).i32_lt_u().i32_and();
    i.if_(BlockType::Empty);
    i.global_get(g_slots).i32_eqz();
    i.if_(BlockType::Empty);
    i.i32_const(0).i32_const(0).i32_const(8);
    i.i32_const(SLOT_CAP * SLOT_STRIDE);
    i.call(f_realloc).global_set(g_slots);
    i.end();
    i.global_get(g_wset).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    i.call(I_WS_NEW).global_set(g_wset);
    i.end();
    i.global_get(g_slots).local_get(b_len).i32_const(SLOT_STRIDE).i32_mul().i32_add();
    i.local_set(sl);
    // hiwater
    i.local_get(b_len).i32_const(1).i32_add().global_get(g_slotn).i32_gt_u();
    i.if_(BlockType::Empty);
    i.local_get(b_len).i32_const(1).i32_add().global_set(g_slotn);
    i.end();
    // the argptr block (canonical layout of open-at's params)
    i.local_get(sl).global_get(g_pre).i32_store(mem(0));
    i.local_get(sl).i32_const(1).i32_store(mem(4)); // path-flags: symlink-follow
    i.local_get(sl).local_get(a_ptr).i32_store(mem(8));
    i.local_get(sl).local_get(a_len).i32_store(mem(12));
    i.local_get(sl).i32_const(0).i32_store(mem(16)); // open-flags: none
    i.local_get(sl).i32_const(1).i32_store(mem(20)); // descriptor-flags: read
    // packed = [async-lower]open-at(args, ret)
    i.local_get(sl);
    i.local_get(sl).i32_const(24).i32_add();
    i.call(I_FS_AOPEN).local_set(n);
    i.local_get(n).i32_const(15).i32_and().i32_const(2).i32_eq();
    i.if_(BlockType::Empty);
    i.local_get(sl).i32_const(2).i32_store(mem(48)); // returned already
    i.else_();
    i.local_get(n).i32_const(4).i32_shr_u().local_set(j);
    i.local_get(j).global_get(g_wset).call(I_WS_JOIN);
    i.local_get(sl).i32_const(1).i32_store(mem(48)); // pending
    i.local_get(sl).local_get(j).i32_store(mem(52));
    i.end();
    i.end();
    i.i64_const(0).return_();
    i.end();

    // op 41: AWAIT slot k in arm order (the path rides a again, so every
    // fallback is one recursive op-1 call). The drain loop is THE guest
    // scheduler: wait on the one set, mark each Returned subtask's slot
    // done, until slot k is done; then decode its parked open result and
    // run the same stream-read tail as the sync path.
    i.local_get(op).i32_const(41).i32_eq();
    i.if_(BlockType::Empty);
    i.global_get(g_slots).i32_eqz();
    i.local_get(b_len).i32_const(SLOT_CAP).i32_ge_u().i32_or();
    i.if_(BlockType::Empty);
    i.i32_const(1).local_get(a_ptr).local_get(a_len).i32_const(0).i32_const(0);
    i.call(f_self).return_();
    i.end();
    i.global_get(g_slots).local_get(b_len).i32_const(SLOT_STRIDE).i32_mul().i32_add();
    i.local_set(sl);
    i.local_get(sl).i32_load(mem(48)).i32_eqz();
    i.if_(BlockType::Empty);
    i.i32_const(1).local_get(a_ptr).local_get(a_len).i32_const(0).i32_const(0);
    i.call(f_self).return_();
    i.end();
    // drain until slot k reads done
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(sl).i32_load(mem(48)).i32_const(2).i32_eq().br_if(1);
    i.global_get(g_wset).i32_const((park + RET) as i32).call(I_WS_WAIT);
    i.i32_const(1).i32_eq(); // EVENT_SUBTASK
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).i32_const(2).i32_eq(); // Returned
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).local_set(n); // subtask handle
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).global_get(g_slotn).i32_ge_u().br_if(1);
    i.global_get(g_slots).local_get(j).i32_const(SLOT_STRIDE).i32_mul().i32_add();
    i.local_set(d);
    i.local_get(d).i32_load(mem(48)).i32_const(1).i32_eq();
    i.local_get(d).i32_load(mem(52)).local_get(n).i32_eq().i32_and();
    i.if_(BlockType::Empty);
    i.local_get(n).call(I_SUBTASK_DROP);
    i.local_get(d).i32_const(2).i32_store(mem(48));
    i.end();
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.end();
    i.end();
    i.br(0).end().end();
    // consume the slot; decode the parked open result
    i.local_get(sl).i32_const(0).i32_store(mem(48));
    i.local_get(sl).i32_load8_u(mem8(24));
    i.if_(BlockType::Empty);
    i.local_get(sl).i32_load8_u(mem8(24 + abi.open_payload)).local_set(n);
    fs_open_err_map(&mut i, g_ppos, g_plen, park, abi, n);
    i.end();
    i.local_get(sl).i32_load(mem(24 + abi.open_payload)).local_set(d);
    fs_read_tail(&mut i, park, f_realloc, g_ppos, g_plen, d, rx, fut, buf, cap, total, n);
    i.end();

    // ── The filesystem WRITE surface (#1628 increment 2d) ─────────────

    // ops 2/15 (write: create|truncate), 16 (append: create), 3
    // (write_bytes: the List[Int] payload packs to raw bytes first).
    // Shape: open-at(write) → stream.new → write/append-via-stream hands
    // the host the readable end → sync stream.write feeds the writable
    // end → drop writable → future.read is the DURABILITY handshake
    // (blocks until the host wrote every byte) → resource-drop → ok.
    i.local_get(op).i32_const(2).i32_eq();
    i.local_get(op).i32_const(15).i32_eq().i32_or();
    i.local_get(op).i32_const(16).i32_eq().i32_or();
    i.local_get(op).i32_const(3).i32_eq().i32_or();
    i.if_(BlockType::Empty);
    fs_preopen(&mut i, g_pre, park);
    i.global_get(g_pre).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    fs_err(&mut i, g_ppos, g_plen, park, MSG_NOPRE, E_NOPRE.len());
    i.end();
    // write_bytes: pack the 8-byte slots' low bytes into a compact buffer.
    i.local_get(op).i32_const(3).i32_eq();
    i.if_(BlockType::Empty);
    i.local_get(b_len).i32_const(3).i32_shr_u().local_set(cap);
    i.i32_const(0).i32_const(0).i32_const(8).local_get(cap).call(f_realloc).local_set(buf);
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(cap).i32_ge_u().br_if(1);
    i.local_get(buf).local_get(j).i32_add();
    i.local_get(b_ptr).local_get(j).i32_const(3).i32_shl().i32_add();
    i.i32_load8_u(mem8(0));
    i.i32_store8(mem8(0));
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.local_get(buf).local_set(b_ptr);
    i.local_get(cap).local_set(b_len);
    i.end();
    // open for write: append keeps the tail (create), write truncates.
    i.global_get(g_pre);
    i.i32_const(1); // path-flags: symlink-follow
    i.local_get(a_ptr).local_get(a_len);
    i.local_get(op).i32_const(16).i32_eq();
    i.if_(BlockType::Result(ValType::I32));
    i.i32_const(1); // open-flags: create
    i.else_();
    i.i32_const(9); // open-flags: create|truncate
    i.end();
    i.i32_const(2); // descriptor-flags: write
    i.i32_const((park + RET) as i32);
    i.call(I_FS_OPEN);
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(0));
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(abi.open_payload)).local_set(n);
    fs_open_err_map(&mut i, g_ppos, g_plen, park, abi, n);
    i.end();
    i.i32_const((park + RET) as i32).i32_load(mem(abi.open_payload)).local_set(d);
    // the write stream: tx = high half, rx = low half (the stdio packing).
    i.call(I_FS_WNEW).local_set(s64);
    i.local_get(s64).i64_const(32).i64_shr_u().i32_wrap_i64().local_set(rx); // rx local holds TX
    i.local_get(op).i32_const(16).i32_eq();
    i.if_(BlockType::Result(ValType::I32));
    i.local_get(d).local_get(s64).i32_wrap_i64().call(I_FS_AVS);
    i.else_();
    i.local_get(d).local_get(s64).i32_wrap_i64().i64_const(0).call(I_FS_WVS);
    i.end();
    i.local_set(fut);
    // sync write loop on the LOCAL writable end.
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(b_len).i32_eqz().br_if(1);
    i.local_get(rx);
    i.local_get(b_ptr).local_get(b_len);
    i.call(I_FS_WWRITE);
    i.i32_const(4).i32_shr_u().local_set(n);
    i.local_get(n).i32_eqz().br_if(1);
    i.local_get(b_ptr).local_get(n).i32_add().local_set(b_ptr);
    i.local_get(b_len).local_get(n).i32_sub().local_set(b_len);
    i.br(0).end().end();
    i.local_get(rx).call(I_FS_WDROP);
    i.local_get(fut).i32_const((park + RET) as i32).call(I_FS_WFUT).drop();
    i.local_get(d).call(I_FS_RESDROP);
    i.i64_const(0).return_();
    i.end();

    // op 7: mkdir_p — create-directory-at per '/'-prefix, exist ignored
    // (idempotent, the create_dir_all shape); the FULL path's non-exist
    // error surfaces through the shared map.
    i.local_get(op).i32_const(7).i32_eq();
    i.if_(BlockType::Empty);
    fs_preopen(&mut i, g_pre, park);
    i.global_get(g_pre).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    fs_err(&mut i, g_ppos, g_plen, park, MSG_NOPRE, E_NOPRE.len());
    i.end();
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(a_len).i32_gt_u().br_if(1);
    // segment boundary: end-of-path or '/'
    i.local_get(j).local_get(a_len).i32_eq();
    i.local_get(j).local_get(a_len).i32_lt_u();
    i.local_get(a_ptr).local_get(j).i32_add().i32_load8_u(mem8(0)).i32_const(47).i32_eq();
    i.i32_and().i32_or();
    i.local_get(j).i32_const(0).i32_gt_u().i32_and();
    i.if_(BlockType::Empty);
    i.global_get(g_pre);
    i.local_get(a_ptr).local_get(j);
    i.i32_const((park + RET) as i32);
    i.call(I_FS_MKDIR);
    // the FULL path's error decides; 'exist' is success (idempotent).
    i.local_get(j).local_get(a_len).i32_eq();
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(0)).i32_and();
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(abi.unit_payload)).local_set(n);
    i.local_get(n).i32_const(abi.ec_exist).i32_ne();
    i.if_(BlockType::Empty);
    fs_open_err_map(&mut i, g_ppos, g_plen, park, abi, n);
    i.end();
    i.end();
    i.end();
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.i64_const(0).return_();
    i.end();

    // ops 8/9: remove / remove_all — stat decides file vs directory (the
    // embedded host's `is_dir()` shape); a NON-EMPTY directory under op 9
    // answers the honest not-empty error (the recursive walk is a later
    // increment, not a refusal).
    i.local_get(op).i32_const(8).i32_eq();
    i.local_get(op).i32_const(9).i32_eq().i32_or();
    i.if_(BlockType::Empty);
    fs_preopen(&mut i, g_pre, park);
    i.global_get(g_pre).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    fs_err(&mut i, g_ppos, g_plen, park, MSG_NOPRE, E_NOPRE.len());
    i.end();
    i.global_get(g_pre);
    i.i32_const(1);
    i.local_get(a_ptr).local_get(a_len);
    i.i32_const((park + STATRET) as i32);
    i.call(I_FS_STAT);
    i.i32_const((park + STATRET) as i32).i32_load8_u(mem8(0));
    i.if_(BlockType::Empty);
    i.i32_const((park + STATRET) as i32).i32_load8_u(mem8(abi.stat_payload)).local_set(n);
    fs_open_err_map(&mut i, g_ppos, g_plen, park, abi, n);
    i.end();
    i.i32_const((park + STATRET) as i32).i32_load8_u(mem8(abi.stat_payload));
    i.i32_const(abi.dt_directory).i32_eq();
    i.if_(BlockType::Empty);
    i.global_get(g_pre).local_get(a_ptr).local_get(a_len).i32_const((park + RET) as i32);
    i.call(I_FS_RMDIR);
    i.else_();
    i.global_get(g_pre).local_get(a_ptr).local_get(a_len).i32_const((park + RET) as i32);
    i.call(I_FS_UNLINK);
    i.end();
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(0));
    i.if_(BlockType::Empty);
    i.i32_const((park + RET) as i32).i32_load8_u(mem8(abi.unit_payload)).local_set(n);
    fs_open_err_map(&mut i, g_ppos, g_plen, park, abi, n);
    i.end();
    i.i64_const(0).return_();
    i.end();

    // op 42: ABANDON slot k (k rides b_len; the path args are unused) —
    // fan.any's loser-arm handshake (#1628 increment 2c). A PENDING slot
    // gets subtask.cancel: if the open still RETURNED (packed status 2 —
    // cancel raced completion), its ok descriptor must be dropped; either
    // way the subtask handle drops. A DONE slot just drops its ok
    // descriptor. The slot clears to empty; never an error.
    i.local_get(op).i32_const(42).i32_eq();
    i.if_(BlockType::Empty);
    i.global_get(g_slots).i32_eqz();
    i.local_get(b_len).i32_const(SLOT_CAP).i32_ge_u().i32_or();
    i.if_(BlockType::Empty);
    i.i64_const(0).return_();
    i.end();
    i.global_get(g_slots).local_get(b_len).i32_const(SLOT_STRIDE).i32_mul().i32_add();
    i.local_set(sl);
    i.local_get(sl).i32_load(mem(48)).i32_const(1).i32_eq();
    i.if_(BlockType::Empty);
    // pending: cancel, then reap a raced completion.
    i.local_get(sl).i32_load(mem(52)).call(I_SUBTASK_CANCEL).local_set(n);
    i.local_get(n).i32_const(15).i32_and().i32_const(2).i32_eq();
    i.if_(BlockType::Empty);
    i.local_get(sl).i32_load8_u(mem8(24)).i32_eqz();
    i.if_(BlockType::Empty);
    i.local_get(sl).i32_load(mem(24 + abi.open_payload)).call(I_FS_RESDROP);
    i.end();
    i.end();
    i.local_get(sl).i32_load(mem(52)).call(I_SUBTASK_DROP);
    i.end();
    i.local_get(sl).i32_load(mem(48)).i32_const(2).i32_eq();
    i.if_(BlockType::Empty);
    // done: the parked open result holds an owned descriptor on ok.
    i.local_get(sl).i32_load8_u(mem8(24)).i32_eqz();
    i.if_(BlockType::Empty);
    i.local_get(sl).i32_load(mem(24 + abi.open_payload)).call(I_FS_RESDROP);
    i.end();
    i.end();
    i.local_get(sl).i32_const(0).i32_store(mem(48));
    i.i64_const(0).return_();
    i.end();

    // Everything else: the defined refusal — the message on stderr, exit 1.
    open_stream(&mut i, g_err_tx, g_err_fut, I_ERR_CALL, I_ERR_NEW, s64);
    i.i32_const((park + MSG) as i32).local_set(b_ptr);
    i.i32_const(UNSUPPORTED_MSG.len() as i32).local_set(b_len);
    write_all(&mut i, g_err_tx, I_ERR_WRITE, b_ptr, b_len, n);
    i.i32_const(1).call(I_EXIT);
    i.unreachable();
    i.end();
    f
}

/// `(dst) -> ()`: copy the parked payload to guest memory.
fn shim_host_read(g_plen: u32, g_ppos: u32) -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(0);
    i.global_get(g_ppos);
    i.global_get(g_plen);
    i.memory_copy(0, 0);
    i.end();
    f
}

/// `cabi_realloc(old_ptr, old_size, align, new_size) -> ptr`: bump the
/// module's own `__heap` frontier (8-aligned raw bytes, never freed),
/// growing memory on demand; a grow failure exits err.
fn shim_cabi_realloc(heap_global: u32) -> Function {
    let (old_ptr, old_size, _align, new_size) = (0u32, 1u32, 2u32, 3u32);
    let result = 4u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.global_get(heap_global).i32_const(7).i32_add().i32_const(-8).i32_and();
    i.local_set(result);
    i.local_get(result).local_get(new_size).i32_add();
    i.memory_size(0).i32_const(16).i32_shl();
    i.i32_gt_u();
    i.if_(BlockType::Empty);
    i.local_get(new_size).i32_const(0xFFFF).i32_add().i32_const(16).i32_shr_u();
    i.memory_grow(0);
    i.i32_const(-1).i32_eq();
    i.if_(BlockType::Empty);
    i.i32_const(1).call(I_EXIT).unreachable();
    i.end();
    i.end();
    i.local_get(result).local_get(new_size).i32_add().global_set(heap_global);
    i.local_get(old_ptr).i32_eqz().i32_eqz();
    i.if_(BlockType::Empty);
    i.local_get(result);
    i.local_get(old_ptr);
    i.local_get(old_size).local_get(new_size).i32_lt_u();
    i.if_(BlockType::Result(ValType::I32));
    i.local_get(old_size);
    i.else_();
    i.local_get(new_size);
    i.end();
    i.memory_copy(0, 0);
    i.end();
    i.local_get(result);
    i.end();
    f
}

/// `() -> status`: the async-lifted `wasi:cli/run.run` — call main, close
/// both output streams and BLOCK on each completion future (the
/// deterministic drain handshake), drop the stdin readables, task.return
/// (ok), answer EXIT (0). The callback is never reached: everything
/// happened in the initial call.
#[allow(clippy::too_many_arguments)]
fn shim_run(
    main_index: u32,
    park: u64,
    g_out_tx: u32,
    g_out_fut: u32,
    g_err_tx: u32,
    g_err_fut: u32,
    g_in_rx: u32,
    g_in_fut: u32,
    g_wset: u32,
) -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.call(main_index);

    // End both streams, then wait for the host's completion future: the
    // future.read blocks until every byte is drained host-side.
    for (g_tx, g_fut, drop_import, read_import) in [
        (g_out_tx, g_out_fut, I_OUT_DROP_TX, I_OUT_FUT_READ),
        (g_err_tx, g_err_fut, I_ERR_DROP_TX, I_ERR_FUT_READ),
    ] {
        i.global_get(g_tx).i32_const(0).i32_ge_s();
        i.if_(BlockType::Empty);
        i.global_get(g_tx).call(drop_import);
        i.global_get(g_fut).i32_const((park + RET) as i32).call(read_import);
        i.drop();
        i.end();
    }

    // Drop the stdin readables if they were opened.
    i.global_get(g_in_rx).i32_const(0).i32_ge_s();
    i.if_(BlockType::Empty);
    i.global_get(g_in_rx).call(I_STDIN_DROP_RX);
    i.global_get(g_in_fut).call(I_STDIN_DROP_FUT);
    i.end();

    // Drop the fan waitable set if one was created (every subtask was
    // dropped at its await, so the set is empty).
    i.global_get(g_wset).i32_const(0).i32_ge_s();
    i.if_(BlockType::Empty);
    i.global_get(g_wset).call(I_WS_DROP);
    i.end();

    // task.return(ok), then EXIT: the task is complete.
    i.i32_const(0).call(I_TASK_RETURN);
    i.i32_const(0);
    i.end();
    f
}

/// `(event, p1, p2) -> status`: never reached — the initial call runs to
/// completion (sync builtins block inside the task instead of yielding).
fn shim_callback() -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.unreachable();
    i.end();
    f
}

#[cfg(test)]
mod wit_tests {
    /// #1710 increment 2 foundation: the vendored wasi:http@0.3.0 package
    /// resolves, and BOTH worlds (the fs-only `p3-command` and the
    /// http-importing `p3-command-http`) select cleanly. A WIT drift —
    /// a trimmed interface the world still names, a dep the trim lost —
    /// fails here instead of at the first http emit.
    #[test]
    fn both_p3_worlds_resolve_with_the_vendored_http_package() {
        let mut resolve = wit_parser::Resolve::default();
        for (name, text) in [
            ("clocks.wit", include_str!("../wit/p3/deps/clocks/package.wit")),
            ("random.wit", include_str!("../wit/p3/deps/random/package.wit")),
            ("cli.wit", include_str!("../wit/p3/deps/cli/package.wit")),
            ("filesystem.wit", include_str!("../wit/p3/deps/filesystem/package.wit")),
            ("http.wit", include_str!("../wit/p3/deps/http/package.wit")),
        ] {
            resolve.push_str(name, text).unwrap_or_else(|e| panic!("wit {name}: {e}"));
        }
        let pkg = resolve
            .push_str("world.wit", include_str!("../wit/p3/world.wit"))
            .expect("world.wit");
        resolve.select_world(&[pkg], Some("p3-command")).expect("p3-command");
        let mut resolve2 = wit_parser::Resolve::default();
        for (name, text) in [
            ("clocks.wit", include_str!("../wit/p3/deps/clocks/package.wit")),
            ("random.wit", include_str!("../wit/p3/deps/random/package.wit")),
            ("cli.wit", include_str!("../wit/p3/deps/cli/package.wit")),
            ("filesystem.wit", include_str!("../wit/p3/deps/filesystem/package.wit")),
            ("http.wit", include_str!("../wit/p3/deps/http/package.wit")),
        ] {
            resolve2.push_str(name, text).unwrap_or_else(|e| panic!("wit {name}: {e}"));
        }
        let pkg2 = resolve2
            .push_str("world.wit", include_str!("../wit/p3/world.wit"))
            .expect("world.wit");
        resolve2
            .select_world(&[pkg2], Some("p3-command-http"))
            .expect("p3-command-http");
    }
}
