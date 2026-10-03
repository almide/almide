//! `to_p3` (#1628 stage 2, increment 1): rewrite an emitted almide module
//! into a WASI 0.3 COMPONENT — stdio over component-model streams. This
//! is the plumbing keystone for the fan lowering: guest-held streams,
//! async stream/future builtins, and an async-lifted entry are exactly the
//! vocabulary fan arms will schedule on.
//!
//! Same doctrine as `to_p2` (#1588/#1628 stage 1) with the five-op host
//! surface (console out, exit codes, stdin, entropy, wall clock) PLUS the
//! whole filesystem surface: since #3140 the component carries the p1 fs
//! service itself (`fs_service.wat`, the code the stock-p1 artifact runs)
//! over a preview-1 adapter on wasi:filesystem@0.3 (`p3_fs_adapter.wat`,
//! `wasi_p3_fs_service.rs`) — path resolution against the full preopen
//! table and the cwd, every fs op, env.os / env.temp_dir / env.cwd, and
//! native's `fs.<call>("<path>")` error heads. The fan prefetch triple
//! (ops 40..=42) stays in this shim: its opens are async-lowered subtasks
//! (increment 2b), on the descriptor the service resolves. `env.get`, the
//! program arguments and `env.sleep_ms` ride `wasi:cli/environment` and
//! `monotonic-clock.wait-for` (ADR-0023 step 3, `wasi_p3_env.rs`), each
//! import shipped only when the op set names its op.
//! Canonical-ABI facts (variant discriminants, payload offsets) are
//! DERIVED from the vendored WIT at emit time (`FsAbi`), never
//! hand-counted. Requested p3 filesystem programs route here without an
//! ALMIDE_WASM_SKIP_STOCK_AUDIT override. Env and process operations outside
//! this world's imports retain the defined refusal. The
//! transform is a post-pass — the emitter's verified envelope is
//! untouched.
//!
//! The 0.3 shapes used (as wasmtime 46+ implements them; vendored WIT
//! under `crates/almide-wasm-run/wit/p3/`):
//!   - stdout/stderr `write-via-stream: func(stream<u8>) ->
//!     future<result<_, error-code>>` — a SYNC call handing the host the
//!     readable end and answering a completion future. The guest opens
//!     the stream ONCE, keeps the writable end, and feeds it with
//!     `stream.write` (each write completes before the next is issued —
//!     program order per stream by construction);
//!   - stdin `read-via-stream: func() -> tuple<stream<u8>, future<...>>`
//!     — sync with a retptr; `stream.read` lands bytes straight in
//!     guest memory (no cabi_realloc hop, unlike p2's blocking-read);
//!   - the run export (`run: async func() -> result`) is lifted `async`
//!     with a CALLBACK, but the body runs to completion in the initial
//!     call — it blocks in `waitable-set.wait` instead of yielding, so the
//!     callback itself is unreachable: main → close streams → read each
//!     completion future → task.return (ok) → EXIT;
//!   - `wasi:clocks/system-clock.now() -> instant` (s64 seconds, u32
//!     nanos) and `get-random-bytes` are plain sync lowers, as on p2.
//!
//! The finale's `future.read` on each output stream's completion future
//! is the DETERMINISTIC drain handshake: it completes only once the host
//! acknowledges the whole stream, so "the program exited" implies "every
//! byte reached the host" — the property the cross-target byte-identity
//! contract stands on.
//!
//! NO SYNCHRONOUS stream/future builtin (ADR-0023 step 1). The sync
//! `stream.read/write` / `future.read/write` are the 🚝
//! `component-model-more-async-builtins` builtins: wasmtime 47 refuses a
//! component that uses them unless `-W component-model-more-async-builtins`
//! is passed, and they are outside the WASI 0.3 baseline. Every stream and
//! future copy here is the `[async-lower]` form; a BLOCKED answer goes
//! through `$await` (`wasi_p3_env.rs`), which joins the one end to a fresh
//! waitable set, waits for its copy event and answers the event's payload
//! — the same `count<<4 | status` word the sync form returns, so every
//! loop over it is unchanged. The baseline builtins (`waitable-set.*`,
//! `waitable.join`, `subtask.*`, the async copies) need only
//! `component-model-async`, on by default since wasmtime 46: an artifact
//! runs as `wasmtime run app.wasm` (`-S http` for an http client).

use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, EntityType, ExportKind, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, MemArg, MemorySection, MemoryType,
    Module, TypeSection, ValType,
};

use crate::component_alloc::{shim_cabi_realloc, shim_realloc_checked, shim_reserve};
use crate::wasi::{
    mem, mem8, parse_module, reencode_body, type_index, Parsed, Remap, DATA, MSG, OOM_MSG,
    PARK_SPAN, UNSUPPORTED_MSG,
};

// Import indices (18 imports replace the 5 almide.* ones).
const I_EXIT: u32 = 0;
const I_OUT_CALL: u32 = 1; // write-via-stream (stdout): (rx) -> future
const I_OUT_NEW: u32 = 2;
const I_OUT_WRITE: u32 = 3; // [async-lower][stream-write-0], via $await
const I_OUT_DROP_TX: u32 = 4;
const I_OUT_FUT_READ: u32 = 5; // [async-lower][future-read-1], via $await
const I_ERR_CALL: u32 = 6;
const I_ERR_NEW: u32 = 7;
const I_ERR_WRITE: u32 = 8; // async, as I_OUT_WRITE
const I_ERR_DROP_TX: u32 = 9;
const I_ERR_FUT_READ: u32 = 10; // async, as I_OUT_FUT_READ
const I_STDIN_OPEN: u32 = 11; // read-via-stream (sync, retptr)
const I_STDIN_READ: u32 = 12; // [async-lower][stream-read-0], via $await
const I_STDIN_DROP_RX: u32 = 13;
const I_STDIN_DROP_FUT: u32 = 14;
const I_CLOCK_NOW: u32 = 15;
const I_RANDOM: u32 = 16;
const I_TASK_RETURN: u32 = 17;
// The filesystem READ surface (#1628 increment 2a): preopens + stat-at +
// open-at + read-via-stream, all SYNC lowers of the funcs (an
// async-declared func may be lowered sync — the task blocks; that is
// baseline `component-model-async`, not the 🚝 gate). The stream READ is
// the async builtin behind `$await`.
const I_FS_PRE: u32 = 18; // preopens.get-directories (retptr)
const I_FS_OPEN: u32 = 19; // [method]descriptor.open-at
const I_FS_STAT: u32 = 20; // [method]descriptor.stat-at
const I_FS_RVS: u32 = 21; // [method]descriptor.read-via-stream
const I_FS_SREAD: u32 = 22; // [async-lower][stream-read-0] of read-via-stream
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
// (the guest keeps the writable end, stream.write feeds it, and the
// completion future.read is the durability handshake — both the async
// builtins behind `$await`), plus the three path ops (sync lowers).
const I_FS_WVS: u32 = 33; // [method]descriptor.write-via-stream
const I_FS_AVS: u32 = 34; // [method]descriptor.append-via-stream
const I_FS_WNEW: u32 = 35; // [stream-new-0] of write-via-stream
const I_FS_WWRITE: u32 = 36; // [async-lower][stream-write-0] of write-via-stream
const I_FS_WDROP: u32 = 37; // [stream-drop-writable-0] of write-via-stream
const I_FS_WFUT: u32 = 38; // [async-lower][future-read-1] of write-via-stream
const I_FS_MKDIR: u32 = 39; // [method]descriptor.create-directory-at
const I_FS_UNLINK: u32 = 40; // [method]descriptor.unlink-file-at
const I_FS_RMDIR: u32 = 41; // [method]descriptor.remove-directory-at
const I_MONO_NOW: u32 = 42; // monotonic-clock.now (datetime.monotonic_ns, op 60)
const IMPORTS: u32 = 43;

// ── The p3 http client import block (#1710 PR B) ────────────────────────
// Appended AFTER the fs table and included only when the module's op set
// reaches the http family (43..=47) — a non-http component must not demand
// `-S http=y` from its runtime. Builtin names follow wit-parser's mangling
// (`[stream-new-0][static]request.new`): the component encode validates
// them against the world, so a drifted name fails loudly at emit.
const I_HTTP_FIELDS_NEW: u32 = 43; // [constructor]fields () -> own<fields>
const I_HTTP_REQ_NEW: u32 = 44; // [static]request.new (retptr: request + sent-future)
const I_HTTP_REQ_SNEW: u32 = 45; // [stream-new-0] of request.new (contents)
const I_HTTP_REQ_SWRITE: u32 = 46; // [stream-write-0] of request.new
const I_HTTP_REQ_SDROPW: u32 = 47; // [stream-drop-writable-0] of request.new
const I_HTTP_REQ_FNEW: u32 = 48; // [future-new-1] of request.new (trailers)
const I_HTTP_REQ_FWRITE: u32 = 49; // [future-write-1] of request.new
const I_HTTP_REQ_FDROPW: u32 = 50; // [future-drop-writable-1] of request.new
const I_HTTP_REQ_SENTDROP: u32 = 51; // [future-drop-readable-2] of request.new
const I_HTTP_SET_METHOD: u32 = 52; // [method]request.set-method
const I_HTTP_SET_SCHEME: u32 = 53; // [method]request.set-scheme
const I_HTTP_SET_AUTH: u32 = 54; // [method]request.set-authority
const I_HTTP_SET_PATH: u32 = 55; // [method]request.set-path-with-query
const I_HTTP_SEND: u32 = 56; // [async-lower] client.send (request, retptr)
const I_HTTP_STATUS: u32 = 57; // [method]response.get-status-code
const I_HTTP_CONSUME: u32 = 58; // [static]response.consume-body (retptr)
const I_HTTP_CB_FNEW: u32 = 59; // [future-new-0] of consume-body (handling result)
const I_HTTP_CB_FWRITE: u32 = 60; // [async-lower][future-write-0] of consume-body
const I_HTTP_CB_FDROPW: u32 = 61; // [future-drop-writable-0] of consume-body
const I_HTTP_BODY_READ: u32 = 62; // [async-lower][stream-read-1] of consume-body (the body)
const I_HTTP_BODY_DROPR: u32 = 63; // [stream-drop-readable-1] of consume-body
const I_HTTP_TRL_DROPR: u32 = 64; // [future-drop-readable-2] of consume-body
const I_HTTP_REQ_DROP: u32 = 65; // [resource-drop]request
const I_HTTP_RESP_DROP: u32 = 66; // [resource-drop]response
const I_HTTP_FIELDS_DROP: u32 = 67; // [resource-drop]fields
const I_HTTP_FIELDS_APPEND: u32 = 68; // [method]fields.append (the framed family's headers, #1710)
// The request options (ADR-0023 step 2): ALMIDE_HTTP_TIMEOUT_SECS as the
// connect / first-byte / between-bytes timeouts.
const I_HTTP_OPT_NEW: u32 = 69; // [constructor]request-options
const I_HTTP_OPT_CONNECT: u32 = 70; // [method]request-options.set-connect-timeout (retptr)
const I_HTTP_OPT_FIRST: u32 = 71; // [method]request-options.set-first-byte-timeout (retptr)
const I_HTTP_OPT_BETWEEN: u32 = 72; // [method]request-options.set-between-bytes-timeout (retptr)
const IMPORTS_HTTP: u32 = 73;

// Park offsets past the shared ones: retptr / future-payload scratch.
const RET: u64 = 32;
// stat-at's result<descriptor-stat, error-code> needs 112 bytes — parked
// past MSG (64..109), before the fs message statics at 256.
const STATRET: u64 = 128;
// send's result<response, error-code> retptr (#1924): the SAME 112-byte
// slot as stat-at's (the fs and http shims never nest), NOT the RET
// scratch. An `err` carries a payload (dns-error-payload's option<string>
// ptr/len, tls-alert-received's, …) that reaches past RET+8+8 — over the
// trailers future's parked ok(none) buffer at RET+16, whose write was
// still pending. The host lifted the scribbled buffer as the trailers
// value on the NEXT exchange: `failed to read result … unknown handle
// index <the rcode string's address>`.
const SENDRET: u64 = STATRET;
// The fs error texts are the spliced fs service's (#3140): its
// `$errno_text` statics are `almide_base::fs_errno`'s rows (#2206), with the
// `fs.<call>("<path>"): ` head native carries — the park holds none of them.
// The p3 http bring-up static (#1710 PR B): only the ALMIDE_P3_HTTP_STOP
// bisect knob answers it now — every real failure is classified and
// rendered from rt-core's texts (ADR-0023 step 2, wasi_p3_http_err.rs).
const MSG_HTTP: u64 = 576;
const E_HTTP: &[u8] = b"http request failed (p3 transport)";
// The `content-length` header name (#1924 B) and the decimal scratch its
// value is rendered into: a body sent WITHOUT it goes out
// `transfer-encoding: chunked` (the host's default for a stream of
// unknown length), which a stock HTTP/1.1 server without chunked
// support reads as an EMPTY body — the native lane's client always
// sends a content-length for its String/Bytes body, so the wire shape
// is the same on both lanes.
const MSG_CLEN: u64 = 640;
const E_CLEN: &[u8] = b"content-length";
const CLEN_BUF: u64 = 704;
// C-197's line, written by `$reserve` when a grow is refused (#2119).
const MSG_OOM: u64 = 768;
// `$await`'s waitable-set event record (waitable @0, payload @4): its own
// slot, because the copy it waits on may be a future.read whose payload
// lands at RET, or the trailers future's parked value at RET+16.
const AWAIT_EV: u64 = 800;

// Park layout, checked at COMPILE time: retptr spans and the message
// statics must not collide with each other or the stdin/entropy DATA
// span. (The stat result's WIT-derived footprint is checked at emit
// time where the resolve is in hand.)
const _: () = {
    assert!(RET + 32 <= MSG);
    assert!(MSG + UNSUPPORTED_MSG.len() as u64 <= STATRET);
    assert!(MSG_HTTP + E_HTTP.len() as u64 <= MSG_CLEN);
    assert!(MSG_CLEN + E_CLEN.len() as u64 <= CLEN_BUF);
    assert!(CLEN_BUF + 20 <= MSG_OOM);
    assert!(MSG_OOM + OOM_MSG.len() as u64 <= AWAIT_EV);
    assert!(AWAIT_EV + 8 <= DATA);
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
/// The declared ceiling reserved before `get-directories` lowers the
/// preopen table (#2119): 64 KiB of descriptors and path bytes, which no
/// real host approaches. See `component_alloc`'s header for what the
/// reservation buys and what remains outside it.
const PREOPEN_RESERVE: i32 = 65536;

const SLOT_CAP: i32 = 1024;
const SLOT_STRIDE: i32 = 64;

/// Canonical-ABI facts the fs shim and the service's adapter store through
/// — DERIVED from the vendored WIT at emit time, never hand-counted (the
/// wit-bindgen doctrine: a case index or payload offset written as a
/// literal drifts silently when the WIT moves; a lookup by name fails
/// loudly).
struct FsAbi {
    open_payload: u64,     // result<descriptor, error-code> payload offset
    unit_payload: u64,     // result<_, error-code> payload offset (the path ops)
    stat_payload: u64,     // result<descriptor-stat, error-code> payload offset
    stat_size: u64,        // full result size (park-span room check)
}

/// A type of wasi:filesystem's `types` interface, by name — scoped to that
/// interface, because a bare name search collides (wasi:cli has its own
/// `error-code`).
fn fs_type(resolve: &wit_parser::Resolve, name: &str) -> anyhow::Result<wit_parser::TypeId> {
    let (_, fs_pkg) = resolve
        .packages
        .iter()
        .find(|(_, p)| p.name.namespace == "wasi" && p.name.name == "filesystem")
        .ok_or_else(|| anyhow::anyhow!("wasi:filesystem package not in the resolve"))?;
    let iface_id = *fs_pkg
        .interfaces
        .get("types")
        .ok_or_else(|| anyhow::anyhow!("wasi:filesystem/types interface not found"))?;
    resolve.interfaces[iface_id]
        .types
        .get(name)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("wit type {name} not found in wasi:filesystem/types"))
}

fn fs_abi(resolve: &wit_parser::Resolve) -> anyhow::Result<FsAbi> {
    use wit_parser::Type;
    let find = |name: &str| fs_type(resolve, name);
    let ec = find("error-code")?;
    let stat = find("descriptor-stat")?;
    let mut sa = wit_parser::SizeAlign::default();
    // 0.259 made `fill` fallible: a type whose size it cannot compute leaves the
    // map short, and every `sa.size`/`sa.align` below would then read a stale
    // entry. The offsets are what we lay the host's records out on, so the
    // failure has to travel, not be dropped.
    sa.fill(resolve)?;
    // result<T, error-code> payload offset = discriminant (1 byte for
    // <=255 cases) aligned up to max(align(T), align(error-code)).
    let ec_align = sa.align(&Type::Id(ec)).align_wasm32() as u64;
    let stat_align = sa.align(&Type::Id(stat)).align_wasm32() as u64;
    let stat_sz = sa.size(&Type::Id(stat)).size_wasm32() as u64;
    let open_payload = 4u64.max(ec_align); // own<descriptor> aligns 4
    let unit_payload = ec_align; // result<_, error-code>: the err IS the payload
    let stat_payload = stat_align.max(ec_align);
    Ok(FsAbi {
        open_payload,
        unit_payload,
        stat_payload,
        stat_size: stat_payload + stat_sz,
    })
}


/// The p3 module's shared globals + park + realloc, bundled once in `to_p3`
/// and destructured at each shim's entry (codopsy max-params: the shims took
/// 7–17 positional u32s; a struct field per global is also harder to swap by
/// accident than a positional list).
#[derive(Clone, Copy)]
struct P3Globals {
    park: u64,
    /// The shims' own allocator (`$reserve` + the bump), NOT the ABI-facing
    /// `cabi_realloc`: guest-side allocation must report C-197 (#2119).
    f_alloc: u32,
    g_plen: u32,
    g_ppos: u32,
    g_in_rx: u32,
    g_in_fut: u32,
    g_out_tx: u32,
    g_out_fut: u32,
    g_err_tx: u32,
    g_err_fut: u32,
    g_wset: u32,
    g_slots: u32,
    g_slotn: u32,
    f_reserve: u32,
    /// `$await` (ADR-0023 step 1): every async stream/future copy answers
    /// through it, so no synchronous builtin is ever imported.
    f_await: u32,
    /// The cached `get-environment` list (ptr, -1 = not fetched; count).
    g_env: u32,
    g_envn: u32,
}

/// One output port for `shim_print`: the stream/future globals and the
/// three imports (call / stream-new / stream-write) that serve it.
#[derive(Clone, Copy)]
struct PrintPort {
    g_tx: u32,
    g_fut: u32,
    call_import: u32,
    new_import: u32,
    write_import: u32,
}

/// `fs_read_tail`'s scratch locals (descriptor, stream rx, future, buffer,
/// cap, total, chunk count) — allocated by the caller, named once here.
#[derive(Clone, Copy)]
struct ReadLocals {
    d: u32,
    rx: u32,
    fut: u32,
    buf: u32,
    cap: u32,
    total: u32,
    n: u32,
}

// The fs service splice and its p3 adapter (#3140): wasi_p3_fs_service.rs.
include!("wasi_p3_fs_service.rs");

// The http shim (shim_http + its frame/body helpers): wasi_p3_http.rs.
include!("wasi_p3_http.rs");

// Its error classes, texts and helper functions (ADR-0023 step 2).
include!("wasi_p3_http_err.rs");

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
    m_head: i32,
    m_connect: i32,
    m_options: i32,
    m_trace: i32,
    /// `other(string)`: the framed family's method is any string.
    m_other: i32,
    sch_http: i32, // scheme variant case indices
    sch_https: i32,
    /// result<response, error-code> payload offset (send's retptr layout).
    send_payload: u64,
    /// error-code's case names in discriminant order (ADR-0023 step 2: the
    /// shim classifies by NAME, and names an unclassified case by it).
    ec_names: Vec<String>,
    /// Offset of an error-code case's payload from the discriminant.
    ec_payload: u64,
    /// `internal-error(option<string>)`: its case index, and the offset of
    /// the string's (ptr, len) within the option.
    ec_internal: i32,
    ec_opt_str: u64,
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
    sa.fill(resolve)?;
    let ec_align = sa.align(&Type::Id(ec)).align_wasm32() as u64;
    let TypeDefKind::Variant(ecv) = &resolve.types[ec].kind else {
        anyhow::bail!("wasi:http error-code is not a variant");
    };
    let ec_names: Vec<String> = ecv.cases.iter().map(|c| c.name.clone()).collect();
    let ec_payload = sa.payload_offset(ecv.tag(), ecv.cases.iter().map(|c| c.ty.as_ref())).size_wasm32() as u64;
    let ec_opt_str =
        sa.payload_offset(wit_parser::Int::U8, [None, Some(&Type::String)]).size_wasm32() as u64;
    Ok(HttpAbi {
        m_get: case(method, "get")?,
        m_post: case(method, "post")?,
        m_put: case(method, "put")?,
        m_patch: case(method, "patch")?,
        m_delete: case(method, "delete")?,
        m_head: case(method, "head")?,
        m_connect: case(method, "connect")?,
        m_options: case(method, "options")?,
        m_trace: case(method, "trace")?,
        m_other: case(method, "other")?,
        sch_http: case(scheme, "HTTP")?,
        sch_https: case(scheme, "HTTPS")?,
        // own<response> aligns 4; the discriminant byte rounds up to the
        // larger of that and error-code's alignment.
        send_payload: 4u64.max(ec_align),
        ec_internal: case(ec, "internal-error")?,
        ec_names,
        ec_payload,
        ec_opt_str,
    })
}

/// 8-byte MemArg.
fn mem64(offset: u64) -> MemArg {
    MemArg { offset, align: 3, memory_index: 0 }
}

// The p3 transform itself (`to_p3`): wasi_p3_emit.rs.
include!("wasi_p3_emit.rs");

/// Lazy stream open: `if g_tx < 0 { (rx,tx) = stream.new; g_tx = tx;
/// g_fut = write-via-stream(rx) }`. The host's read side starts
/// concurrently; every later write completes (through `$await`) against it.
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

/// Write loop: `while len > 0 { r = await(tx, stream.write(tx, ptr, len));
/// n = r >> 4; if n == 0 { break } ptr += n; len -= n }` — a DROPPED
/// status answers n=0 and the loop exits (output sunk, as p2/POSIX).
/// `l` = the (ptr, len, n) locals.
fn write_all(
    i: &mut wasm_encoder::InstructionSink<'_>,
    g_tx: u32,
    write_import: u32,
    f_await: u32,
    l: (u32, u32, u32),
) {
    let (ptr, len, n) = l;
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(len).i32_eqz().br_if(1);
    i.global_get(g_tx);
    i.global_get(g_tx);
    i.local_get(ptr).local_get(len);
    i.call(write_import);
    // #2955: `n` = the raw `(count << 4) | status`. A COMPLETED write may
    // accept 0 items (write again); only DROPPED/CANCELLED ends the loop.
    i.call(f_await).local_set(n);
    i.local_get(ptr).local_get(n).i32_const(4).i32_shr_u().i32_add().local_set(ptr);
    i.local_get(len).local_get(n).i32_const(4).i32_shr_u().i32_sub().local_set(len);
    i.local_get(n).i32_const(15).i32_and().br_if(1);
    i.br(0).end().end();
}

// The print/exit/stdin/fs shims: wasi_p3_fs.rs.
include!("wasi_p3_fs.rs");

// `$await` and the env/args/sleep service: wasi_p3_env.rs.
include!("wasi_p3_env.rs");

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

/// `() -> status`: the async-lifted `wasi:cli/run.run` — call main, close
/// both output streams and BLOCK on each completion future (the
/// deterministic drain handshake), drop the stdin readables, task.return
/// (ok), answer EXIT (0). The callback is never reached: everything
/// happened in the initial call.
fn shim_run(main_index: u32, g: P3Globals) -> Function {
    let P3Globals { park, g_out_tx, g_out_fut, g_err_tx, g_err_fut, g_in_rx, g_in_fut, g_wset, f_await, .. } = g;
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.call(main_index);

    // End both streams, then wait for the host's completion future: the
    // future.read completes only once every byte is drained host-side.
    for (g_tx, g_fut, drop_import, read_import) in [
        (g_out_tx, g_out_fut, I_OUT_DROP_TX, I_OUT_FUT_READ),
        (g_err_tx, g_err_fut, I_ERR_DROP_TX, I_ERR_FUT_READ),
    ] {
        i.global_get(g_tx).i32_const(0).i32_ge_s();
        i.if_(BlockType::Empty);
        i.global_get(g_tx).call(drop_import);
        i.global_get(g_fut);
        i.global_get(g_fut).i32_const((park + RET) as i32).call(read_import);
        i.call(f_await).drop();
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
/// completion (`$await` blocks inside the task instead of yielding).
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
