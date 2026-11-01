// `include!`d part of wasi_p3.rs (ADR-0023 §4.3, the call handle on the
// stock WASI 0.3 component): `http.start` / `poll` / `read_new` / `wait` /
// `cancel` and the drop glue — host ops 53..=59 but the retired streaming
// step 58 — over `wasi:http/client@0.3.0`'s async `send`, guest-enforced.
//
// The embedded lane runs native's call core (http_call_core.rs: a worker
// thread per call). A component has no thread: here a call is a per-call
// BLOCK holding the exchange's handles (the send subtask, the request's
// trailers future and body stream, then the response body stream), its own
// waitable set, the body received so far and the outcome. Every op on the
// call DRIVES it — `$hc_drive` waits on the call's set raced against an
// async `wait-for`:
//   - `wait` drives until the call ends, the timer at the nearer of the
//     `total_ms` deadline and the `idle_ms` gap since the last arrival;
//   - `poll` and `read_new` take one bounded TURN (HC_TURN_NS), so the
//     host makes progress on the exchange without the op blocking;
//   - `total_ms` is a wall clock from `start`, checked on every op as
//     native's `check_deadline` is; `idle_ms` fires only after a whole timer
//     window in which no byte arrived;
//   - `cancel` (and the drop glue) cancels the send subtask or the pending
//     body read and drops the body stream and the transmit future, so the
//     host closes the connection.
// Op 53 is `shim_http`'s: the request is built by the same code the framed
// family runs (URL, headers, userinfo, body framing), then `start` stores
// the in-flight exchange in a fresh block instead of draining it. The block
// owns copies of the URL (the error texts quote it) and of the request body
// (the pump feeds it after the op returns).
//
// The texts are native's (http_call_core.rs `http_call_total_msg`,
// `http_call_idle_msg`, `HTTP_CALL_CANCELLED`, `http_call_spawn`'s limits
// refusal); tests/p3_http_call_test.rs pins them against the core's answers.
// A body is decoded as `String::from_utf8_lossy` decodes it, and `read_new`
// holds an incomplete UTF-8 tail back as `http_call_take_new` does.

const OP_HC_OPEN: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::HTTP_CALL, 0);
const OP_HC_STATE: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::HTTP_CALL, 1);
const OP_HC_WAIT: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::HTTP_CALL, 2);
const OP_HC_READ: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::HTTP_CALL, 3);
const OP_HC_CANCEL: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::HTTP_CALL, 4);
const OP_HC_STEP: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::HTTP_CALL, 5);
const OP_HC_DROP: i32 = almide_wasm::host_ops::nth(almide_wasm::host_ops::HTTP_CALL, 6);

/// The call-handle ops this shim serves: the family but the streaming step
/// (op 58), which no stdlib surface emits since the streaming clients left.
pub(crate) fn http_call_served(op: i32) -> bool {
    almide_wasm::host_ops::HTTP_CALL.contains(&op) && op != OP_HC_STEP
}

/// One non-blocking turn: how long `poll` / `read_new` let the host run
/// the exchange (1 ms). A zero wait returns inline and gives the host no
/// turn at all.
const HC_TURN_NS: i64 = 1_000_000;
/// The body buffer's first size; it doubles when a read would find it full.
const HC_BODY_CHUNK: i32 = 65536;

// The per-call block (all offsets from its base).
const HC_STATE: u64 = 0; // 0 sending, 1 reading the body, 2 ended
const HC_ERR: u64 = 4; // 1 = ended with the err text below
const HC_ERR_PTR: u64 = 8;
const HC_ERR_LEN: u64 = 12;
const HC_WS: u64 = 16; // the call's waitable set
const HC_TRL_TX: u64 = 20; // the request trailers future's writable
const HC_TRL_PEND: u64 = 24;
const HC_STR_TX: u64 = 28; // the request body stream's writable (-1: none)
const HC_STR_PEND: u64 = 32;
const HC_B_PTR: u64 = 36; // the request body still to feed
const HC_B_LEN: u64 = 40;
const HC_SUB: u64 = 44; // the send subtask while it runs
const HC_SNDDONE: u64 = 48; // send returned: its result is at HC_SENDRET
const HC_SENTFUT: u64 = 52; // the transmit-result future's readable
const HC_STATUS: u64 = 56;
const HC_HDR_PTR: u64 = 60; // the response fields as frames (name, value)*
const HC_HDR_LEN: u64 = 64;
const HC_BODY_RX: u64 = 68;
const HC_TRLFUT: u64 = 72;
const HC_CB_TX: u64 = 76;
const HC_BUF: u64 = 80; // the body received so far
const HC_CAP: u64 = 84;
const HC_TOTAL: u64 = 88;
const HC_DELIV: u64 = 92; // what read_new has handed out
const HC_RPEND: u64 = 96; // a body read is in flight
const HC_URL_PTR: u64 = 100;
const HC_URL_LEN: u64 = 104;
const HC_DEADLINE: u64 = 112; // i64 monotonic ns, 0 = no total_ms
const HC_TOTAL_MS: u64 = 120; // i64, as the program gave it
const HC_IDLE_MS: u64 = 128; // i64
const HC_LAST: u64 = 136; // i64 monotonic ns of the last arrival (or start)
const HC_SENDRET: u64 = 144; // send's result<response, error-code>, 112 bytes
const HC_TRLBUF: u64 = 256; // the trailers future's ok(none), 16 bytes
const HC_EV: u64 = 272; // the call set's event record (waitable, payload)
const HC_BLOCK: i32 = 288;
const _: () = assert!(HC_SENDRET + 112 <= HC_TRLBUF && HC_EV + 8 <= HC_BLOCK as u64);

// Waitable-set event codes (the component model's `EventCode`).
const EV_SUBTASK: i32 = 1;
const EV_STREAM_READ: i32 = 2;
const EV_STREAM_WRITE: i32 = 3;
const EV_FUTURE_WRITE: i32 = 5;

/// The call shim's own imports (present when the op set names the call
/// handle): the sync cancels of the body read and of the request's two
/// writes, and the async `wait-for` the timer races — shared with the net
/// block when that ships (a core module may not import one name twice).
#[derive(Clone, Copy)]
struct HcImports {
    body_cancel: u32,
    trl_cancel: u32,
    str_cancel: u32,
    wait: u32,
    /// Whether this block carries the `wait-for` import itself.
    own_wait: bool,
    base: u32,
}

impl HcImports {
    fn plan(host_ops: &[i32], base: u32, net: Option<NetImports>) -> (Option<Self>, u32) {
        if !host_ops.iter().any(|op| http_call_served(*op)) {
            return (None, base);
        }
        let own_wait = net.is_none();
        let wait = match net {
            Some(n) => n.at(NI_T_WAIT),
            None => base + 3,
        };
        let imp = HcImports { body_cancel: base, trl_cancel: base + 1, str_cancel: base + 2, wait, own_wait, base };
        (Some(imp), base + 3 + u32::from(own_wait))
    }

    fn import_list(self, types: &mut Vec<(Vec<ValType>, Vec<ValType>)>) -> Vec<(u32, &'static str, &'static str, u32)> {
        let http_types = "wasi:http/types@0.3.0";
        let cancel = type_index(types, &[ValType::I32], &[ValType::I32]);
        let mut list = vec![
            (self.base, http_types, "[stream-cancel-read-1][static]response.consume-body", cancel),
            (self.base + 1, http_types, "[future-cancel-write-1][static]request.new", cancel),
            (self.base + 2, http_types, "[stream-cancel-write-0][static]request.new", cancel),
        ];
        if self.own_wait {
            let t = type_index(types, &[ValType::I64], &[ValType::I32]);
            list.push((self.base + 3, "wasi:clocks/monotonic-clock@0.3.0", "[async-lower]wait-for", t));
        }
        list
    }
}

/// The call table's globals: the block pointers (base, count, capacity).
#[derive(Clone, Copy)]
struct HcGlobals {
    tab: u32,
    n: u32,
    cap: u32,
}

impl HcGlobals {
    const COUNT: u32 = 3;

    fn at(first: u32) -> Self {
        HcGlobals { tab: first, n: first + 1, cap: first + 2 }
    }

    fn emit(globals: &mut GlobalSection) {
        let t = GlobalType { val_type: ValType::I32, mutable: true, shared: false };
        for _ in 0..Self::COUNT {
            globals.global(t, &ConstExpr::i32_const(0));
        }
    }
}

/// The call shim's functions: the op shim (fs_call ABI) and its helpers.
#[derive(Clone, Copy)]
struct HcFns {
    shim: u32,
    drive: u32,
    send_err: u32,
    teardown: u32,
    lossy: u32,
    dec: u32,
    slot_new: u32,
}

impl HcFns {
    const COUNT: u32 = 7;

    fn at(first: u32) -> Self {
        HcFns {
            shim: first,
            drive: first + 1,
            send_err: first + 2,
            teardown: first + 3,
            lossy: first + 4,
            dec: first + 5,
            slot_new: first + 6,
        }
    }

    /// The helpers' core types (registered before the type section is
    /// written), in `HcFns` order past the shim.
    fn types(types: &mut Vec<(Vec<ValType>, Vec<ValType>)>) -> Vec<u32> {
        use ValType::{I32, I64};
        [
            (&[I32, I32][..], &[I32][..]), // drive (blk, mode) -> ended
            (&[I32], &[I64]),              // send_err (blk) -> pack(1, len)
            (&[I32], &[]),                 // teardown (blk)
            (&[I32, I32, I32], &[I32]),    // lossy (dst, src, len) -> dst_end
            (&[I32, I64], &[I32]),         // dec (dst, v) -> dst_end
            (&[I32], &[I32]),              // slot_new (blk) -> id
        ]
        .into_iter()
        .map(|(p, r)| type_index(types, p, r))
        .collect()
    }

    /// The function-section entries, in index order.
    fn declare(functions: &mut FunctionSection, t_fs: u32, helper_types: &[u32]) {
        assert_eq!(helper_types.len() as u32 + 1, Self::COUNT);
        functions.function(t_fs);
        for t in helper_types {
            functions.function(*t);
        }
    }
}

/// The call texts (laid out with the http texts).
#[derive(Clone, Copy, Default)]
struct HcTexts {
    cancelled: Piece,
    total: [Piece; 2],
    idle: [Piece; 2],
    /// `invalid limits: … got total_ms ` and ` idle_ms ` around the numbers.
    invalid: [Piece; 2],
    unknown: Piece,
    one: Piece,
    zero: Piece,
}

/// native's call-core texts (http_call_core.rs), pinned against the core's
/// own answers by tests/p3_http_call_test.rs.
const HC_TEXT_CANCELLED: &str = "request cancelled";
const HC_TEXT_TOTAL: [&str; 2] = ["request timeout: total_ms ", " exceeded"];
const HC_TEXT_IDLE: [&str; 2] = ["request timeout: idle_ms ", " exceeded"];
const HC_TEXT_INVALID: [&str; 2] =
    ["invalid limits: total_ms and idle_ms must be >= 0 (0 = no limit), got total_ms ", " idle_ms "];
const HC_TEXT_UNKNOWN: &str = "unknown http call ";

impl HcTexts {
    fn cut(put: &mut dyn FnMut(&[u8]) -> Piece) -> Self {
        HcTexts {
            cancelled: put(HC_TEXT_CANCELLED.as_bytes()),
            total: HC_TEXT_TOTAL.map(|t| put(t.as_bytes())),
            idle: HC_TEXT_IDLE.map(|t| put(t.as_bytes())),
            invalid: HC_TEXT_INVALID.map(|t| put(t.as_bytes())),
            unknown: put(HC_TEXT_UNKNOWN.as_bytes()),
            one: put(b"1"),
            zero: put(b"0"),
        }
    }
}

/// What every call-shim function needs.
#[derive(Clone, Copy)]
struct HcCx<'a> {
    g: P3Globals,
    hg: HcGlobals,
    fns: HcFns,
    imp: HcImports,
    h: &'a HttpAbi,
    t: &'a HttpErrTexts,
    efns: HttpErrFns,
}

fn hc_ld(i: &mut Sink<'_>, blk: u32, off: u64) {
    i.local_get(blk).i32_load(mem(off));
}

fn hc_st(i: &mut Sink<'_>, blk: u32, off: u64, v: impl FnOnce(&mut Sink<'_>)) {
    i.local_get(blk);
    v(i);
    i.i32_store(mem(off));
}

fn hc_st_const(i: &mut Sink<'_>, blk: u32, off: u64, v: i32) {
    i.local_get(blk).i32_const(v).i32_store(mem(off));
}

/// `(blk) -> id`: the lowest free slot of the call table holds `blk`; the
/// id is the slot + 1 (0 never names a call).
fn hc_fn_slot_new(cx: HcCx<'_>) -> Function {
    let HcGlobals { tab, n, cap } = cx.hg;
    let (blk, k, nt) = (0u32, 1u32, 2u32);
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const(0).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).global_get(n).i32_ge_u().br_if(1);
    i.global_get(tab).local_get(k).i32_const(4).i32_mul().i32_add().i32_load(mem(0)).i32_eqz().br_if(1);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(k).global_get(n).i32_eq().if_(BlockType::Empty);
    i.global_get(n).global_get(cap).i32_eq().if_(BlockType::Empty);
    i.i32_const(8).global_get(cap).i32_const(1).i32_shl().global_get(cap).i32_const(8).i32_lt_u().select().local_set(nt);
    i.global_get(tab).global_get(cap).i32_const(4).i32_mul().i32_const(4);
    i.local_get(nt).i32_const(4).i32_mul().call(cx.g.f_alloc).global_set(tab);
    i.local_get(nt).global_set(cap);
    i.end();
    i.global_get(n).i32_const(1).i32_add().global_set(n);
    i.end();
    i.global_get(tab).local_get(k).i32_const(4).i32_mul().i32_add().local_get(blk).i32_store(mem(0));
    i.local_get(k).i32_const(1).i32_add();
    i.end();
    f
}

/// The block of call `id`, or 0 for an id the table never issued.
fn hc_slot_get(i: &mut Sink<'_>, hg: HcGlobals, id: u32, out: u32) {
    i.i32_const(0).local_set(out);
    i.local_get(id).i32_const(1).i32_ge_s().local_get(id).global_get(hg.n).i32_le_u().i32_and().if_(BlockType::Empty);
    i.global_get(hg.tab).local_get(id).i32_const(1).i32_sub().i32_const(4).i32_mul().i32_add().i32_load(mem(0)).local_set(out);
    i.end();
}

/// `(dst, v) -> dst_end`: `v` in decimal, a `-` first when negative.
fn hc_fn_dec() -> Function {
    let (dst, v) = (0u32, 1u32);
    let (u, d, k) = (2u32, 3u32, 4u32);
    let mut f = Function::new([(1, ValType::I64), (2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).local_set(u);
    i.local_get(v).i64_const(0).i64_lt_s().if_(BlockType::Empty);
    i.local_get(dst).i32_const(b'-' as i32).i32_store8(mem8(0));
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    i.i64_const(0).local_get(v).i64_sub().local_set(u);
    i.end();
    // The digit count, then the digits back to front (unsigned: i64::MIN's
    // magnitude survives the negation as a u64).
    i.i32_const(1).local_set(d);
    i.local_get(u).local_set(v);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(v).i64_const(10).i64_lt_u().br_if(1);
    i.local_get(v).i64_const(10).i64_div_u().local_set(v);
    i.local_get(d).i32_const(1).i32_add().local_set(d);
    i.br(0).end().end();
    i.local_get(d).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).i32_eqz().br_if(1);
    i.local_get(k).i32_const(1).i32_sub().local_set(k);
    i.local_get(dst).local_get(k).i32_add();
    i.local_get(u).i64_const(10).i64_rem_u().i32_wrap_i64().i32_const(48).i32_add().i32_store8(mem8(0));
    i.local_get(u).i64_const(10).i64_div_u().local_set(u);
    i.br(0).end().end();
    i.local_get(dst).local_get(d).i32_add();
    i.end();
    f
}

/// `(dst, src, len) -> dst_end`: `src[..len]` as `String::from_utf8_lossy`
/// decodes it — each maximal invalid subpart (the Unicode substitution of
/// maximal subparts, Rust's `Utf8Chunks`) becomes U+FFFD. `dst` holds at
/// least `3 * len` bytes.
fn hc_fn_lossy() -> Function {
    let (dst, src, len) = (0u32, 1u32, 2u32);
    let (p, b, need, lo, hi, j, k) = (3u32, 4u32, 5u32, 6u32, 7u32, 8u32, 9u32);
    let mut f = Function::new([(7, ValType::I32)]);
    let mut i = f.instructions();
    let fffd = |i: &mut Sink<'_>| {
        for (off, byte) in [0xEFu8, 0xBF, 0xBD].iter().enumerate() {
            i.local_get(dst).i32_const(*byte as i32).i32_store8(mem8(off as u64));
        }
        i.local_get(dst).i32_const(3).i32_add().local_set(dst);
    };
    i.i32_const(0).local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(len).i32_ge_u().br_if(1);
    i.local_get(src).local_get(p).i32_add().i32_load8_u(mem8(0)).local_set(b);
    // ASCII: copied.
    i.local_get(b).i32_const(0x80).i32_lt_u().if_(BlockType::Empty);
    i.local_get(dst).local_get(b).i32_store8(mem8(0));
    i.local_get(dst).i32_const(1).i32_add().local_set(dst);
    i.local_get(p).i32_const(1).i32_add().local_set(p);
    i.br(1);
    i.end();
    // The lead byte decides how many continuation bytes follow and the
    // range the FIRST of them must fall in (overlongs, surrogates and
    // past-U+10FFFF leads are refused at the second byte).
    i.i32_const(0).local_set(need);
    i.i32_const(0x80).local_set(lo);
    i.i32_const(0xBF).local_set(hi);
    let lead = |i: &mut Sink<'_>, from: i32, to: i32, n: i32, l: i32, h: i32| {
        i.local_get(b).i32_const(from).i32_ge_u().local_get(b).i32_const(to).i32_le_u().i32_and();
        i.if_(BlockType::Empty);
        i.i32_const(n).local_set(need);
        i.i32_const(l).local_set(lo);
        i.i32_const(h).local_set(hi);
        i.end();
    };
    lead(&mut i, 0xC2, 0xDF, 1, 0x80, 0xBF);
    lead(&mut i, 0xE0, 0xE0, 2, 0xA0, 0xBF);
    lead(&mut i, 0xE1, 0xEC, 2, 0x80, 0xBF);
    lead(&mut i, 0xED, 0xED, 2, 0x80, 0x9F);
    lead(&mut i, 0xEE, 0xEF, 2, 0x80, 0xBF);
    lead(&mut i, 0xF0, 0xF0, 3, 0x90, 0xBF);
    lead(&mut i, 0xF1, 0xF3, 3, 0x80, 0xBF);
    lead(&mut i, 0xF4, 0xF4, 3, 0x80, 0x8F);
    // No lead, or a second byte out of range: one byte becomes U+FFFD.
    i.local_get(p).i32_const(1).i32_add().local_set(j);
    i.local_get(need).i32_eqz();
    i.local_get(j).local_get(len).i32_ge_u().i32_or();
    i.if_(BlockType::Result(ValType::I32));
    i.i32_const(1);
    i.else_();
    i.local_get(src).local_get(j).i32_add().i32_load8_u(mem8(0)).local_set(k);
    i.local_get(k).local_get(lo).i32_lt_u().local_get(k).local_get(hi).i32_gt_u().i32_or();
    i.end();
    i.if_(BlockType::Empty);
    fffd(&mut i);
    i.local_get(p).i32_const(1).i32_add().local_set(p);
    i.br(1);
    i.end();
    // The rest must be continuation bytes; a sequence cut short is ONE
    // U+FFFD covering the bytes it had.
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.i32_const(1).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(need).i32_ge_u().br_if(1);
    i.local_get(j).local_get(len).i32_ge_u().br_if(1);
    i.local_get(src).local_get(j).i32_add().i32_load8_u(mem8(0)).i32_const(0xC0).i32_and().i32_const(0x80).i32_ne().br_if(1);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0).end().end();
    i.local_get(k).local_get(need).i32_lt_u().if_(BlockType::Empty);
    fffd(&mut i);
    i.local_get(j).local_set(p);
    i.br(1);
    i.end();
    i.local_get(dst).local_get(src).local_get(p).i32_add().local_get(j).local_get(p).i32_sub().memory_copy(0, 0);
    i.local_get(dst).local_get(j).local_get(p).i32_sub().i32_add().local_set(dst);
    i.local_get(j).local_set(p);
    i.br(0).end().end();
    i.local_get(dst);
    i.end();
    f
}

/// `(blk) -> i64`: send's err as C-328's classified text quoting the URL —
/// the framed family's renderer (`http_send_err`), over the block's copy of
/// the result and the URL (its locals 1 and 2 are the URL, as in shim_http).
fn hc_fn_send_err(cx: HcCx<'_>) -> Function {
    let park = cx.g.park;
    let blk = 0u32;
    let (url_ptr, url_len, ent) = (1u32, 2u32, 3u32);
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const((park + SENDRET) as i32);
    i.local_get(blk).i32_const(HC_SENDRET as i32).i32_add();
    i.i32_const(112).memory_copy(0, 0);
    hc_ld(&mut i, blk, HC_URL_PTR);
    i.local_set(url_ptr);
    hc_ld(&mut i, blk, HC_URL_LEN);
    i.local_set(url_len);
    let _ = (url_ptr, url_len);
    http_send_err(&mut i, park, cx.h, cx.t, cx.efns, ent);
    i.unreachable();
    i.end();
    f
}

/// `(blk)`: release what the call still holds of the exchange — the send
/// subtask (cancelled), the request's pending writes (cancelled), the body
/// read (cancelled: the bytes it already landed are kept), the body stream
/// and the transmit future — so the host closes the connection. The
/// state is left to the caller.
fn hc_fn_teardown(cx: HcCx<'_>) -> Function {
    let park = cx.g.park;
    let blk = 0u32;
    let (n, resp) = (1u32, 2u32);
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    // ── sending ──
    hc_ld(&mut i, blk, HC_STATE);
    i.i32_eqz().if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_SNDDONE);
    i.i32_eqz().if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_SUB);
    i.call(I_SUBTASK_CANCEL).local_set(n);
    hc_ld(&mut i, blk, HC_SUB);
    i.call(I_SUBTASK_DROP);
    // Returned before the cancel took: its result is in the block.
    i.local_get(n).i32_const(2).i32_eq().if_(BlockType::Empty);
    hc_st_const(&mut i, blk, HC_SNDDONE, 1);
    i.end();
    i.end();
    hc_ld(&mut i, blk, HC_SNDDONE);
    i.local_get(blk).i32_load8_u(mem8(HC_SENDRET)).i32_eqz().i32_and().if_(BlockType::Empty);
    i.local_get(blk).i32_load(mem(HC_SENDRET + cx.h.send_payload)).local_set(resp);
    i.local_get(resp).call(I_HTTP_RESP_DROP);
    i.end();
    hc_ld(&mut i, blk, HC_TRL_PEND);
    i.if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_TRL_TX);
    i.call(cx.imp.trl_cancel).drop();
    hc_ld(&mut i, blk, HC_TRL_TX);
    i.call(I_HTTP_REQ_FDROPW);
    hc_st_const(&mut i, blk, HC_TRL_PEND, 0);
    i.end();
    hc_ld(&mut i, blk, HC_STR_PEND);
    i.if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_STR_TX);
    i.call(cx.imp.str_cancel).drop();
    hc_ld(&mut i, blk, HC_STR_TX);
    i.call(I_HTTP_REQ_SDROPW);
    hc_st_const(&mut i, blk, HC_STR_PEND, 0);
    i.end();
    hc_ld(&mut i, blk, HC_SENTFUT);
    i.call(I_HTTP_REQ_SENTDROP);
    i.end();
    // ── reading the body ──
    hc_ld(&mut i, blk, HC_STATE);
    i.i32_const(1).i32_eq().if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_RPEND);
    i.if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_BODY_RX);
    i.call(cx.imp.body_cancel).local_set(n);
    hc_st(&mut i, blk, HC_TOTAL, |i| {
        i.local_get(blk).i32_load(mem(HC_TOTAL)).local_get(n).i32_const(4).i32_shr_u().i32_add();
    });
    hc_st_const(&mut i, blk, HC_RPEND, 0);
    i.end();
    hc_ld(&mut i, blk, HC_BODY_RX);
    i.call(I_HTTP_BODY_DROPR);
    hc_ld(&mut i, blk, HC_TRLFUT);
    i.call(I_HTTP_TRL_DROPR);
    hc_body_handled(&mut i, park, blk, cx.g.f_await);
    hc_ld(&mut i, blk, HC_SENTFUT);
    i.call(I_HTTP_REQ_SENTDROP);
    i.end();
    hc_ld(&mut i, blk, HC_WS);
    i.call(I_WS_DROP);
    i.end();
    f
}

/// Write the body's handling result `ok` into the kept writable and drop it
/// (consume-body's `res` future, as `http_body_retire` does).
fn hc_body_handled(i: &mut Sink<'_>, park: u64, blk: u32, f_await: u32) {
    i.i32_const((park + RET) as i32).i64_const(0).i64_store(mem64(16));
    hc_ld(i, blk, HC_CB_TX);
    hc_ld(i, blk, HC_CB_TX);
    i.i32_const((park + RET + 16) as i32).call(I_HTTP_CB_FWRITE);
    i.call(f_await).drop();
    hc_ld(i, blk, HC_CB_TX);
    i.call(I_HTTP_CB_FDROPW);
}

/// The drive's locals.
#[derive(Clone, Copy)]
struct HcDl {
    blk: u32,
    mode: u32,
    w: u32,
    p: u32,
    n: u32,
    k: u32,
    timer: u32,
    got: u32,
    code: u32,
    now: u32,
    t: u32,
    t2: u32,
    list: u32,
    cnt: u32,
    ent: u32,
    dst: u32,
}

/// Feed the request body: async writes until one blocks (the writable
/// joins the call's set, pending) or the body is fully accepted, when the
/// writable drops — the EOF the server waits for.
fn hc_pump(i: &mut Sink<'_>, l: HcDl) {
    let blk = l.blk;
    hc_ld(i, blk, HC_STR_TX);
    i.i32_const(0).i32_ge_s().if_(BlockType::Empty);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    hc_ld(i, blk, HC_B_LEN);
    i.i32_eqz().br_if(1);
    hc_ld(i, blk, HC_STR_TX);
    hc_ld(i, blk, HC_B_PTR);
    hc_ld(i, blk, HC_B_LEN);
    i.call(I_HTTP_REQ_SWRITE).local_set(l.n);
    i.local_get(l.n).i32_const(-1).i32_eq().if_(BlockType::Empty);
    hc_ld(i, blk, HC_STR_TX);
    hc_ld(i, blk, HC_WS);
    i.call(I_WS_JOIN);
    hc_st_const(i, blk, HC_STR_PEND, 1);
    i.br(2);
    i.end();
    hc_advance_body(i, l);
    i.br(0).end().end();
    // Fully accepted (or the reader dropped): EOF.
    hc_ld(i, blk, HC_STR_PEND);
    i.i32_eqz().if_(BlockType::Empty);
    hc_ld(i, blk, HC_STR_TX);
    i.call(I_HTTP_REQ_SDROPW);
    hc_st_const(i, blk, HC_STR_TX, -1);
    i.end();
    i.end();
}

/// A write's `count << 4 | status` in `l.n`: the body advances past what
/// was taken, and a DROPPED reader takes nothing more.
fn hc_advance_body(i: &mut Sink<'_>, l: HcDl) {
    let blk = l.blk;
    i.local_get(l.n).i32_const(4).i32_shr_u().local_set(l.k);
    hc_st(i, blk, HC_B_PTR, |i| {
        i.local_get(blk).i32_load(mem(HC_B_PTR)).local_get(l.k).i32_add();
    });
    hc_st(i, blk, HC_B_LEN, |i| {
        i.local_get(blk).i32_load(mem(HC_B_LEN)).local_get(l.k).i32_sub();
    });
    i.local_get(l.n).i32_const(15).i32_and().i32_const(1).i32_eq().if_(BlockType::Empty);
    hc_st_const(i, blk, HC_B_LEN, 0);
    i.end();
}

/// Issue body reads until one blocks (the readable joins the set) or the
/// body ends; a read never lands in a full buffer (it doubles first). On
/// the end the call is answered and `$hc_drive` returns 1.
fn hc_read_more(i: &mut Sink<'_>, cx: HcCx<'_>, l: HcDl) {
    let blk = l.blk;
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    hc_ld(i, blk, HC_TOTAL);
    hc_ld(i, blk, HC_CAP);
    i.i32_ge_u().if_(BlockType::Empty);
    hc_st(i, blk, HC_BUF, |i| {
        i.local_get(blk).i32_load(mem(HC_BUF)).local_get(blk).i32_load(mem(HC_CAP)).i32_const(1);
        i.local_get(blk).i32_load(mem(HC_CAP)).i32_const(1).i32_shl().call(cx.g.f_alloc);
    });
    hc_st(i, blk, HC_CAP, |i| {
        i.local_get(blk).i32_load(mem(HC_CAP)).i32_const(1).i32_shl();
    });
    i.end();
    hc_ld(i, blk, HC_BODY_RX);
    i.local_get(blk).i32_load(mem(HC_BUF)).local_get(blk).i32_load(mem(HC_TOTAL)).i32_add();
    i.local_get(blk).i32_load(mem(HC_CAP)).local_get(blk).i32_load(mem(HC_TOTAL)).i32_sub();
    i.call(I_HTTP_BODY_READ).local_set(l.n);
    i.local_get(l.n).i32_const(-1).i32_eq().if_(BlockType::Empty);
    hc_ld(i, blk, HC_BODY_RX);
    hc_ld(i, blk, HC_WS);
    i.call(I_WS_JOIN);
    hc_st_const(i, blk, HC_RPEND, 1);
    i.br(2);
    i.end();
    hc_landed(i, cx, l);
    i.br(0).end().end();
}

/// A body read's `count << 4 | status` in `l.n`: the bytes count as an
/// arrival; a nonzero status (the stream DROPPED — the body is whole) ends
/// the call `ok` and returns 1 from `$hc_drive`.
fn hc_landed(i: &mut Sink<'_>, cx: HcCx<'_>, l: HcDl) {
    let blk = l.blk;
    hc_st(i, blk, HC_TOTAL, |i| {
        i.local_get(blk).i32_load(mem(HC_TOTAL)).local_get(l.n).i32_const(4).i32_shr_u().i32_add();
    });
    i.local_get(l.n).i32_const(4).i32_shr_u().if_(BlockType::Empty);
    i.call(I_MONO_NOW).local_set(l.now);
    i.local_get(blk).local_get(l.now).i64_store(mem64(HC_LAST));
    i.i32_const(1).local_set(l.got);
    i.end();
    i.local_get(l.n).i32_const(15).i32_and().if_(BlockType::Empty);
    hc_ld(i, blk, HC_BODY_RX);
    i.call(I_HTTP_BODY_DROPR);
    hc_ld(i, blk, HC_TRLFUT);
    i.call(I_HTTP_TRL_DROPR);
    hc_body_handled(i, cx.g.park, blk, cx.g.f_await);
    hc_ld(i, blk, HC_SENTFUT);
    i.call(I_HTTP_REQ_SENTDROP);
    hc_end(i, l, false);
    i.end();
}

/// End the call (state 2) with what HC_ERR says, dropping the timer and the
/// set (`teardown` drops the set when it ran instead), and return 1.
fn hc_end(i: &mut Sink<'_>, l: HcDl, torn: bool) {
    let blk = l.blk;
    i.local_get(l.timer).i32_const(0).i32_gt_s().if_(BlockType::Empty);
    i.local_get(l.timer).i32_const(0).call(I_WS_JOIN);
    i.local_get(l.timer).call(I_SUBTASK_CANCEL).drop();
    i.local_get(l.timer).call(I_SUBTASK_DROP);
    i.i32_const(-1).local_set(l.timer);
    i.end();
    if !torn {
        hc_ld(i, blk, HC_WS);
        i.call(I_WS_DROP);
    }
    hc_st_const(i, blk, HC_STATE, 2);
    i.i32_const(1).return_();
}

/// End the call with an err whose text the call just parked (`g_ppos` /
/// `g_plen`), the exchange torn down first.
fn hc_fail(i: &mut Sink<'_>, cx: HcCx<'_>, l: HcDl) {
    let blk = l.blk;
    hc_st_const(i, blk, HC_ERR, 1);
    hc_st(i, blk, HC_ERR_PTR, |i| {
        i.global_get(cx.g.g_ppos);
    });
    hc_st(i, blk, HC_ERR_LEN, |i| {
        i.global_get(cx.g.g_plen);
    });
    // The timer leaves the set before the teardown drops it.
    i.local_get(l.timer).i32_const(0).i32_gt_s().if_(BlockType::Empty);
    i.local_get(l.timer).i32_const(0).call(I_WS_JOIN);
    i.local_get(l.timer).call(I_SUBTASK_CANCEL).drop();
    i.local_get(l.timer).call(I_SUBTASK_DROP);
    i.i32_const(-1).local_set(l.timer);
    i.end();
    i.local_get(blk).call(cx.fns.teardown);
    hc_end(i, l, true);
}

/// A limit's text `<p0><n><p1>` parked, the n from the block's i64 at `off`.
fn hc_limit_text(i: &mut Sink<'_>, cx: HcCx<'_>, l: HcDl, pieces: [Piece; 2], off: u64) {
    let scratch = (cx.g.park + CLEN_BUF) as i32;
    i.i32_const(scratch).local_get(l.blk).i64_load(mem64(off)).call(cx.fns.dec).local_set(l.dst);
    hc_err_text(i, cx.efns, pieces[0], Some((scratch, l.dst)), pieces[1]);
}

/// `p0 ++ [raw] ++ p1` parked as an err answer (`$http_err` with nothing
/// quoted); the raw span is `(start, end)` with `end` a local.
fn hc_err_text(i: &mut Sink<'_>, efns: HttpErrFns, p0: Piece, raw: Option<(i32, u32)>, p1: Piece) {
    i.i32_const(p0.at).i32_const(p0.len);
    i.i32_const(0).i32_const(-1);
    i.i32_const(0).i32_const(0);
    match raw {
        Some((at, end)) => {
            i.i32_const(at).local_get(end).i32_const(at).i32_sub();
        }
        None => {
            i.i32_const(0).i32_const(0);
        }
    }
    i.i32_const(p1.at).i32_const(p1.len);
    i.i32_const(0);
    i.call(efns.err).drop();
}

/// `send` returned: an err ends the call with C-328's text; a response is
/// read — its status, its fields as frames, then its body.
fn hc_on_response(i: &mut Sink<'_>, cx: HcCx<'_>, l: HcDl) {
    let blk = l.blk;
    let park = cx.g.park;
    i.local_get(blk).i32_load8_u(mem8(HC_SENDRET)).if_(BlockType::Empty);
    hc_ld(i, blk, HC_SENTFUT);
    i.call(I_HTTP_REQ_SENTDROP);
    i.local_get(blk).call(cx.fns.send_err).drop();
    hc_st_const(i, blk, HC_ERR, 1);
    hc_st(i, blk, HC_ERR_PTR, |i| {
        i.global_get(cx.g.g_ppos);
    });
    hc_st(i, blk, HC_ERR_LEN, |i| {
        i.global_get(cx.g.g_plen);
    });
    hc_end(i, l, false);
    i.end();
    i.local_get(blk).i32_load(mem(HC_SENDRET + cx.h.send_payload)).local_set(l.k);
    hc_st(i, blk, HC_STATUS, |i| {
        i.local_get(l.k).call(I_HTTP_STATUS).i32_const(0xFFFF).i32_and();
    });
    // The fields, before consume-body takes the response: frames of
    // (name, value), the names the client manages left out (C-370), each
    // value decoded lossily as native's head parse does.
    i.local_get(l.k).call(I_HTTP_RESP_HEADERS).local_set(l.p);
    i.local_get(l.p).i32_const((park + RET) as i32).call(I_HTTP_COPY_ALL);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).local_set(l.list);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).local_set(l.cnt);
    i.local_get(l.p).call(I_HTTP_FIELDS_DROP);
    // Room: 8 + name + 3 * value per entry.
    i.i32_const(0).local_set(l.n);
    i.i32_const(0).local_set(l.w);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.w).local_get(l.cnt).i32_ge_u().br_if(1);
    i.local_get(l.list).local_get(l.w).i32_const(16).i32_mul().i32_add().local_set(l.ent);
    i.local_get(l.n).i32_const(8).i32_add();
    i.local_get(l.ent).i32_load(mem(4)).i32_add();
    i.local_get(l.ent).i32_load(mem(12)).i32_const(3).i32_mul().i32_add().local_set(l.n);
    i.local_get(l.w).i32_const(1).i32_add().local_set(l.w);
    i.br(0).end().end();
    i.i32_const(0).i32_const(0).i32_const(4).local_get(l.n).call(cx.g.f_alloc).local_set(l.dst);
    hc_st(i, blk, HC_HDR_PTR, |i| {
        i.local_get(l.dst);
    });
    i.i32_const(0).local_set(l.w);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(l.w).local_get(l.cnt).i32_ge_u().br_if(1);
    i.local_get(l.list).local_get(l.w).i32_const(16).i32_mul().i32_add().local_set(l.ent);
    // A managed name (the header check answers 3) is left out.
    i.local_get(l.ent).i32_load(mem(0)).local_get(l.ent).i32_load(mem(4));
    i.local_get(l.ent).i32_load(mem(0)).i32_const(0).call(cx.efns.check);
    i.i32_const(3).i32_ne().if_(BlockType::Empty);
    i.local_get(l.dst).local_get(l.ent).i32_load(mem(4)).i32_store(mem(0));
    i.local_get(l.dst).i32_const(4).i32_add();
    i.local_get(l.ent).i32_load(mem(0)).local_get(l.ent).i32_load(mem(4)).memory_copy(0, 0);
    i.local_get(l.dst).i32_const(4).i32_add().local_get(l.ent).i32_load(mem(4)).i32_add().local_set(l.p);
    i.local_get(l.p).i32_const(4).i32_add();
    i.local_get(l.ent).i32_load(mem(8)).local_get(l.ent).i32_load(mem(12));
    i.call(cx.fns.lossy).local_set(l.n);
    i.local_get(l.p).local_get(l.n).local_get(l.p).i32_sub().i32_const(4).i32_sub().i32_store(mem(0));
    i.local_get(l.n).local_set(l.dst);
    i.end();
    i.local_get(l.w).i32_const(1).i32_add().local_set(l.w);
    i.br(0).end().end();
    hc_st(i, blk, HC_HDR_LEN, |i| {
        i.local_get(l.dst).local_get(blk).i32_load(mem(HC_HDR_PTR)).i32_sub();
    });
    // consume-body: the body stream, the trailers future, the kept writable
    // of the handling result.
    i.call(I_HTTP_CB_FNEW).local_set(l.t);
    hc_st(i, blk, HC_CB_TX, |i| {
        i.local_get(l.t).i64_const(32).i64_shr_u().i32_wrap_i64();
    });
    i.local_get(l.k);
    i.local_get(l.t).i32_wrap_i64();
    i.i32_const((park + RET) as i32).call(I_HTTP_CONSUME);
    hc_st(i, blk, HC_BODY_RX, |i| {
        i.i32_const((park + RET) as i32).i32_load(mem(0));
    });
    hc_st(i, blk, HC_TRLFUT, |i| {
        i.i32_const((park + RET) as i32).i32_load(mem(4));
    });
    hc_st(i, blk, HC_BUF, |i| {
        i.i32_const(0).i32_const(0).i32_const(8).i32_const(HC_BODY_CHUNK).call(cx.g.f_alloc);
    });
    hc_st_const(i, blk, HC_CAP, HC_BODY_CHUNK);
    hc_st_const(i, blk, HC_STATE, 1);
    hc_read_more(i, cx, l);
}

/// `(blk, mode) -> ended`: run the call. Mode 0 is one turn (`poll`,
/// `read_new`); mode 1 runs until the call ends (`wait`).
fn hc_fn_drive(cx: HcCx<'_>) -> Function {
    let l = HcDl {
        blk: 0,
        mode: 1,
        w: 2,
        p: 3,
        n: 4,
        k: 5,
        timer: 6,
        got: 7,
        code: 8,
        list: 9,
        cnt: 10,
        ent: 11,
        dst: 12,
        now: 13,
        t: 14,
        t2: 15,
    };
    let blk = l.blk;
    let mut f = Function::new([(11, ValType::I32), (3, ValType::I64)]);
    let mut i = f.instructions();
    i.i32_const(-1).local_set(l.timer);
    hc_ld(&mut i, blk, HC_STATE);
    i.i32_const(2).i32_eq().if_(BlockType::Empty);
    i.i32_const(1).return_();
    i.end();
    // total_ms is a wall clock: past it, the call ends whatever arrived.
    let total_check = |i: &mut Sink<'_>| {
        i.call(I_MONO_NOW).local_set(l.now);
        i.local_get(blk).i64_load(mem64(HC_DEADLINE)).i64_const(0).i64_ne();
        i.local_get(l.now).local_get(blk).i64_load(mem64(HC_DEADLINE)).i64_ge_s().i32_and();
        i.if_(BlockType::Empty);
        hc_limit_text(i, cx, l, cx.t.hc.total, HC_TOTAL_MS);
        hc_fail(i, cx, l);
        i.end();
    };
    total_check(&mut i);
    i.loop_(BlockType::Empty);
    i.i32_const(0).local_set(l.got);
    // The timer: one turn, or the nearer limit (at least a turn, so a stale
    // arrival clock never fires idle_ms before the set was looked at);
    // i64::MAX = no limit, no timer.
    i.i64_const(i64::MAX).local_set(l.t);
    i.local_get(l.mode).i32_eqz().if_(BlockType::Empty);
    i.i64_const(HC_TURN_NS).local_set(l.t);
    i.else_();
    i.local_get(blk).i64_load(mem64(HC_DEADLINE)).i64_const(0).i64_ne().if_(BlockType::Empty);
    i.local_get(blk).i64_load(mem64(HC_DEADLINE)).local_get(l.now).i64_sub().local_set(l.t);
    i.end();
    i.local_get(blk).i64_load(mem64(HC_IDLE_MS)).i64_const(0).i64_gt_s().if_(BlockType::Empty);
    i.local_get(blk).i64_load(mem64(HC_LAST));
    hc_ms_ns(&mut i, blk, HC_IDLE_MS);
    i.i64_add().local_get(l.now).i64_sub().local_set(l.t2);
    i.local_get(l.t2).local_get(l.t).i64_lt_s().if_(BlockType::Empty);
    i.local_get(l.t2).local_set(l.t);
    i.end();
    i.end();
    i.local_get(l.t).i64_const(HC_TURN_NS).i64_lt_s().if_(BlockType::Empty);
    i.i64_const(HC_TURN_NS).local_set(l.t);
    i.end();
    i.end();
    i.local_get(l.t).i64_const(i64::MAX).i64_ne().if_(BlockType::Empty);
    i.local_get(l.t).call(cx.imp.wait).local_set(l.n);
    i.local_get(l.n).i32_const(15).i32_and().i32_const(2).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).local_set(l.timer); // fired inline: no subtask
    i.else_();
    i.local_get(l.n).i32_const(4).i32_shr_u().local_set(l.timer);
    i.local_get(l.timer);
    hc_ld(&mut i, blk, HC_WS);
    i.call(I_WS_JOIN);
    i.end();
    i.end();
    // ── events until the timer returns ──
    i.block(BlockType::Empty);
    i.local_get(l.timer).i32_eqz().br_if(0);
    i.loop_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_WS);
    i.local_get(blk).i32_const(HC_EV as i32).i32_add().call(I_WS_WAIT).local_set(l.code);
    i.local_get(blk).i32_load(mem(HC_EV)).local_set(l.w);
    i.local_get(blk).i32_load(mem(HC_EV + 4)).local_set(l.p);
    // The timer returned.
    i.local_get(l.code).i32_const(EV_SUBTASK).i32_eq();
    i.local_get(l.w).local_get(l.timer).i32_eq().i32_and();
    i.local_get(l.p).i32_const(2).i32_eq().i32_and().if_(BlockType::Empty);
    i.local_get(l.timer).i32_const(0).call(I_WS_JOIN);
    i.local_get(l.timer).call(I_SUBTASK_DROP);
    i.i32_const(-1).local_set(l.timer);
    i.br(2);
    i.end();
    // The trailers future was taken.
    i.local_get(l.code).i32_const(EV_FUTURE_WRITE).i32_eq().if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_TRL_TX);
    i.call(I_HTTP_REQ_FDROPW);
    hc_st_const(&mut i, blk, HC_TRL_PEND, 0);
    i.end();
    // A body write completed: feed the rest.
    i.local_get(l.code).i32_const(EV_STREAM_WRITE).i32_eq().if_(BlockType::Empty);
    hc_st_const(&mut i, blk, HC_STR_PEND, 0);
    i.local_get(l.p).local_set(l.n);
    hc_advance_body(&mut i, l);
    hc_pump(&mut i, l);
    i.end();
    // send returned.
    i.local_get(l.code).i32_const(EV_SUBTASK).i32_eq();
    i.local_get(l.w).local_get(blk).i32_load(mem(HC_SUB)).i32_eq().i32_and();
    i.local_get(l.p).i32_const(2).i32_eq().i32_and().if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_SUB);
    i.call(I_SUBTASK_DROP);
    hc_st_const(&mut i, blk, HC_SNDDONE, 1);
    i.end();
    // Body bytes landed.
    i.local_get(l.code).i32_const(EV_STREAM_READ).i32_eq().if_(BlockType::Empty);
    hc_st_const(&mut i, blk, HC_RPEND, 0);
    i.local_get(l.p).local_set(l.n);
    hc_landed(&mut i, cx, l);
    hc_read_more(&mut i, cx, l);
    i.end();
    // The request is out and send answered: the response.
    hc_ld(&mut i, blk, HC_STATE);
    i.i32_eqz();
    hc_ld(&mut i, blk, HC_SNDDONE);
    i.i32_and();
    hc_ld(&mut i, blk, HC_TRL_PEND);
    i.i32_eqz().i32_and();
    hc_ld(&mut i, blk, HC_STR_PEND);
    i.i32_eqz().i32_and().if_(BlockType::Empty);
    hc_on_response(&mut i, cx, l);
    i.end();
    i.br(0);
    i.end();
    i.end();
    // ── the timer returned: the limits ──
    total_check(&mut i);
    i.local_get(blk).i64_load(mem64(HC_IDLE_MS)).i64_const(0).i64_gt_s();
    i.local_get(l.got).i32_eqz().i32_and();
    i.local_get(l.now).local_get(blk).i64_load(mem64(HC_LAST)).i64_sub();
    hc_ms_ns(&mut i, blk, HC_IDLE_MS);
    i.i64_ge_s().i32_and().if_(BlockType::Empty);
    hc_limit_text(&mut i, cx, l, cx.t.hc.idle, HC_IDLE_MS);
    hc_fail(&mut i, cx, l);
    i.end();
    i.local_get(l.mode).i32_eqz().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.br(0);
    i.end();
    i.unreachable();
    i.end();
    f
}

/// Push the block's i64 milliseconds at `off` as nanoseconds (clamped so
/// the product stays an i64).
fn hc_ms_ns(i: &mut Sink<'_>, blk: u32, off: u64) {
    const MAX_MS: i64 = 9_223_372_036_854;
    i.local_get(blk).i64_load(mem64(off)).i64_const(MAX_MS);
    i.local_get(blk).i64_load(mem64(off)).i64_const(MAX_MS).i64_lt_s().select();
    i.i64_const(1_000_000).i64_mul();
}

/// The call shim: ops 54..=59 on the call whose id rides `a_len`.
fn shim_http_call(cx: HcCx<'_>) -> Function {
    let park = cx.g.park;
    let (op, id) = (0u32, 2u32);
    let (blk, n, dst, buf, keep, k, b) = (5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32);
    let mut f = Function::new([(7, ValType::I32)]);
    let mut i = f.instructions();
    let ok_piece = |i: &mut Sink<'_>, p: Piece| {
        i.i32_const(p.at).global_set(cx.g.g_ppos);
        i.i32_const(p.len).global_set(cx.g.g_plen);
        i.i64_const(i64::from(p.len)).return_();
    };
    let ok_len = |i: &mut Sink<'_>, start: u32, end: u32| {
        i.local_get(start).global_set(cx.g.g_ppos);
        i.local_get(end).local_get(start).i32_sub().global_set(cx.g.g_plen);
        i.global_get(cx.g.g_plen).i64_extend_i32_u().return_();
    };
    // Op 58 (the retired streaming step) and anything else: refused.
    i.local_get(op).i32_const(OP_HC_STATE).i32_lt_s();
    i.local_get(op).i32_const(OP_HC_DROP).i32_gt_s().i32_or();
    i.local_get(op).i32_const(OP_HC_STEP).i32_eq().i32_or().if_(BlockType::Empty);
    fs_err(&mut i, cx.g.g_ppos, cx.g.g_plen, park, MSG, UNSUPPORTED_MSG.len());
    i.end();
    hc_slot_get(&mut i, cx.hg, id, blk);
    // The drop glue's op on a call already forgotten: nothing to do.
    i.local_get(op).i32_const(OP_HC_DROP).i32_eq().local_get(blk).i32_eqz().i32_and().if_(BlockType::Empty);
    i.i64_const(0).return_();
    i.end();
    i.local_get(blk).i32_eqz().if_(BlockType::Empty);
    i.i32_const((park + CLEN_BUF) as i32).local_get(id).i64_extend_i32_u().call(cx.fns.dec).local_set(dst);
    hc_err_text(&mut i, cx.efns, cx.t.hc.unknown, Some(((park + CLEN_BUF) as i32, dst)), Piece::default());
    i.i64_const(1).i64_const(32).i64_shl().global_get(cx.g.g_plen).i64_extend_i32_u().i64_or().return_();
    i.end();
    // ── cancel / drop: a running call ends `request cancelled`, its unread
    // bytes dropped; an ended one is left as it is ──
    i.local_get(op).i32_const(OP_HC_CANCEL).i32_eq().local_get(op).i32_const(OP_HC_DROP).i32_eq().i32_or();
    i.if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_STATE);
    i.i32_const(2).i32_ne().if_(BlockType::Empty);
    i.local_get(blk).call(cx.fns.teardown);
    hc_st_const(&mut i, blk, HC_ERR, 1);
    hc_st_const(&mut i, blk, HC_ERR_PTR, cx.t.hc.cancelled.at);
    hc_st_const(&mut i, blk, HC_ERR_LEN, cx.t.hc.cancelled.len);
    hc_st(&mut i, blk, HC_DELIV, |i| {
        i.local_get(blk).i32_load(mem(HC_TOTAL));
    });
    hc_st_const(&mut i, blk, HC_STATE, 2);
    i.end();
    i.local_get(op).i32_const(OP_HC_DROP).i32_eq().if_(BlockType::Empty);
    i.global_get(cx.hg.tab).local_get(id).i32_const(1).i32_sub().i32_const(4).i32_mul().i32_add();
    i.i32_const(0).i32_store(mem(0));
    i.end();
    i.i32_const(0).global_set(cx.g.g_plen);
    i.i64_const(0).return_();
    i.end();
    // ── poll: one turn, then "1" once the call ended ──
    i.local_get(op).i32_const(OP_HC_STATE).i32_eq().if_(BlockType::Empty);
    i.local_get(blk).i32_const(0).call(cx.fns.drive).if_(BlockType::Empty);
    ok_piece(&mut i, cx.t.hc.one);
    i.end();
    ok_piece(&mut i, cx.t.hc.zero);
    i.end();
    // ── read_new: one turn, then the body since the last read, an
    // incomplete UTF-8 tail held back while the call runs ──
    i.local_get(op).i32_const(OP_HC_READ).i32_eq().if_(BlockType::Empty);
    i.local_get(blk).i32_const(0).call(cx.fns.drive).drop();
    i.local_get(blk).i32_load(mem(HC_BUF)).local_get(blk).i32_load(mem(HC_DELIV)).i32_add().local_set(buf);
    i.local_get(blk).i32_load(mem(HC_TOTAL)).local_get(blk).i32_load(mem(HC_DELIV)).i32_sub().local_set(n);
    i.i32_const(0).local_set(keep);
    hc_ld(&mut i, blk, HC_STATE);
    i.i32_const(2).i32_ne().if_(BlockType::Empty);
    hc_utf8_tail(&mut i, (buf, n), (keep, k, b));
    i.end();
    i.local_get(n).local_get(keep).i32_sub().local_set(n);
    i.i32_const(0).i32_const(0).i32_const(1).local_get(n).i32_const(3).i32_mul().i32_const(1).i32_add();
    i.call(cx.g.f_alloc).local_set(dst);
    i.local_get(dst).local_get(buf).local_get(n).call(cx.fns.lossy).local_set(k);
    hc_st(&mut i, blk, HC_DELIV, |i| {
        i.local_get(blk).i32_load(mem(HC_DELIV)).local_get(n).i32_add();
    });
    ok_len(&mut i, dst, k);
    i.end();
    // ── wait: until the call ends; the response as frames
    // `[status, body, k1, v1, …]`, or the call's err ──
    i.local_get(blk).i32_const(1).call(cx.fns.drive).drop();
    hc_ld(&mut i, blk, HC_ERR);
    i.if_(BlockType::Empty);
    hc_ld(&mut i, blk, HC_ERR_PTR);
    i.global_set(cx.g.g_ppos);
    hc_ld(&mut i, blk, HC_ERR_LEN);
    i.global_set(cx.g.g_plen);
    i.i64_const(1).i64_const(32).i64_shl().global_get(cx.g.g_plen).i64_extend_i32_u().i64_or().return_();
    i.end();
    i.i32_const(0).i32_const(0).i32_const(4);
    i.i32_const(32).local_get(blk).i32_load(mem(HC_TOTAL)).i32_const(3).i32_mul().i32_add();
    i.local_get(blk).i32_load(mem(HC_HDR_LEN)).i32_add();
    i.call(cx.g.f_alloc).local_set(buf);
    // status
    i.local_get(buf).i32_const(4).i32_add().local_get(blk).i32_load(mem(HC_STATUS)).i64_extend_i32_u().call(cx.fns.dec).local_set(k);
    i.local_get(buf).local_get(k).local_get(buf).i32_sub().i32_const(4).i32_sub().i32_store(mem(0));
    // body
    i.local_get(k).i32_const(4).i32_add();
    i.local_get(blk).i32_load(mem(HC_BUF)).local_get(blk).i32_load(mem(HC_TOTAL));
    i.call(cx.fns.lossy).local_set(dst);
    i.local_get(k).local_get(dst).local_get(k).i32_sub().i32_const(4).i32_sub().i32_store(mem(0));
    // the fields
    i.local_get(dst).local_get(blk).i32_load(mem(HC_HDR_PTR)).local_get(blk).i32_load(mem(HC_HDR_LEN)).memory_copy(0, 0);
    i.local_get(dst).local_get(blk).i32_load(mem(HC_HDR_LEN)).i32_add().local_set(dst);
    ok_len(&mut i, buf, dst);
    i.end();
    f
}

/// `keep` = the length of an incomplete UTF-8 sequence at the end of
/// `[buf, buf + n)` (0..=3) — `http_stream_incomplete_utf8_tail`: the bytes
/// from the last lead byte on, when it announces more than follow it.
fn hc_utf8_tail(i: &mut Sink<'_>, (buf, n): (u32, u32), (keep, k, b): (u32, u32, u32)) {
    i.i32_const(0).local_set(keep);
    i.i32_const(1).local_set(k);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(k).local_get(n).i32_gt_u().br_if(1);
    i.local_get(k).i32_const(4).i32_gt_u().br_if(1);
    i.local_get(buf).local_get(n).i32_add().local_get(k).i32_sub().i32_load8_u(mem8(0)).local_set(b);
    i.local_get(b).i32_const(0xC0).i32_and().i32_const(0x80).i32_eq().if_(BlockType::Empty);
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(1);
    i.end();
    // want: 4 for F0..F7, 3 for E0..EF, 2 for C0..DF, else 1 — kept when
    // it is more than the bytes from the lead on.
    let in_range = |i: &mut Sink<'_>, lo: i32, hi: i32| {
        i.local_get(b).i32_const(lo).i32_ge_u().local_get(b).i32_const(hi).i32_le_u().i32_and();
    };
    i.i32_const(4);
    i.i32_const(3);
    i.i32_const(2);
    i.i32_const(1);
    in_range(i, 0xC0, 0xDF);
    i.select(); // C0..DF ? 2 : 1
    in_range(i, 0xE0, 0xEF);
    i.select(); // E0..EF ? 3 : that
    in_range(i, 0xF0, 0xF7);
    i.select(); // F0..F7 ? 4 : that
    i.local_get(k).i32_gt_u().if_(BlockType::Empty);
    i.local_get(k).local_set(keep);
    i.end();
    i.br(1);
    i.end().end();
}

// ── op 53 inside shim_http (its locals; `op` is local 0) ──

/// Past the method and body cells of `http.start`'s frame: the `total_ms`
/// and `idle_ms` cells (decimal, maybe negative) into two i64 locals, and
/// the limits refusal native's `http_call_spawn` gives before any request
/// exists.
fn hc_open_frame(
    i: &mut Sink<'_>,
    (g, t, efns, hf): (P3Globals, &HttpErrTexts, HttpErrFns, HcFns),
    (cur, cell_len, frame_end, tmp, digit): (u32, u32, u32, u32, u32),
    (lim_total, lim_idle): (u32, u32),
) {
    i.local_get(0).i32_const(OP_HC_OPEN).i32_eq().if_(BlockType::Empty);
    for lim in [lim_total, lim_idle] {
        http_frame_cell(i, cur, cell_len, frame_end, tmp, digit);
        // cell_len = the sign, digit = the byte cursor.
        i.i64_const(0).local_set(lim);
        i.i32_const(0).local_set(cell_len);
        i.local_get(tmp).local_set(digit);
        i.local_get(digit).local_get(cur).i32_lt_u().if_(BlockType::Empty);
        i.local_get(digit).i32_load8_u(mem8(0)).i32_const(b'-' as i32).i32_eq().if_(BlockType::Empty);
        i.i32_const(1).local_set(cell_len);
        i.local_get(digit).i32_const(1).i32_add().local_set(digit);
        i.end();
        i.end();
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(digit).local_get(cur).i32_ge_u().br_if(1);
        i.local_get(lim).i64_const(10).i64_mul();
        i.local_get(digit).i32_load8_u(mem8(0)).i32_const(48).i32_sub().i64_extend_i32_u().i64_add().local_set(lim);
        i.local_get(digit).i32_const(1).i32_add().local_set(digit);
        i.br(0).end().end();
        i.local_get(cell_len).if_(BlockType::Empty);
        i.i64_const(0).local_get(lim).i64_sub().local_set(lim);
        i.end();
    }
    i.local_get(lim_total).i64_const(0).i64_lt_s().local_get(lim_idle).i64_const(0).i64_lt_s().i32_or();
    i.if_(BlockType::Empty);
    let scratch = (g.park + CLEN_BUF) as i32;
    let mid = t.hc.invalid[1];
    i.i32_const(scratch).local_get(lim_total).call(hf.dec).local_set(digit);
    i.local_get(digit).i32_const(mid.at).i32_const(mid.len).memory_copy(0, 0);
    i.local_get(digit).i32_const(mid.len).i32_add().local_get(lim_idle).call(hf.dec).local_set(digit);
    hc_err_text(i, efns, t.hc.invalid[0], Some((scratch, digit)), Piece::default());
    i.i64_const(1).i64_const(32).i64_shl().global_get(g.g_plen).i64_extend_i32_u().i64_or().return_();
    i.end();
    i.end();
}

/// The call's block, zeroed, and the copies it owns: the URL (its error
/// texts quote it) and the request body (fed after this op returns); the
/// trailers value and send's result land in the block.
fn hc_open_block(
    i: &mut Sink<'_>,
    g: P3Globals,
    (a_ptr, a_len, b_ptr, b_len): (u32, u32, u32, u32),
    (blk, trl_buf, sret, tmp): (u32, u32, u32, u32),
) {
    i.local_get(0).i32_const(OP_HC_OPEN).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).i32_const(0).i32_const(8).i32_const(HC_BLOCK).call(g.f_alloc).local_set(blk);
    i.local_get(blk).i32_const(0).i32_const(HC_BLOCK).memory_fill(0);
    i.local_get(blk).i32_const(HC_TRLBUF as i32).i32_add().local_set(trl_buf);
    i.local_get(blk).i32_const(HC_SENDRET as i32).i32_add().local_set(sret);
    i.i32_const(0).i32_const(0).i32_const(1).local_get(a_len).call(g.f_alloc).local_set(tmp);
    i.local_get(tmp).local_get(a_ptr).local_get(a_len).memory_copy(0, 0);
    hc_st(i, blk, HC_URL_PTR, |i| {
        i.local_get(tmp);
    });
    hc_st(i, blk, HC_URL_LEN, |i| {
        i.local_get(a_len);
    });
    i.i32_const(0).i32_const(0).i32_const(1).local_get(b_len).call(g.f_alloc).local_set(tmp);
    i.local_get(tmp).local_get(b_ptr).local_get(b_len).memory_copy(0, 0);
    i.local_get(tmp).local_set(b_ptr);
    i.end();
}

/// shim_http's exchange locals at the moment `send` was called.
#[derive(Clone, Copy)]
struct HcOpen {
    trl_tx: u32,
    trl_pend: u32,
    str_tx: u32,
    str_pend: u32,
    b_ptr: u32,
    b_len: u32,
    sentfut: u32,
    hws: u32,
    /// send's async-lower answer `(subtask << 4) | state`.
    n: u32,
}

/// `start` returns at once: the in-flight exchange moves into the block,
/// the clocks start, and the call's id is the answer.
fn hc_open_tail(i: &mut Sink<'_>, (g, hf): (P3Globals, HcFns), ex: HcOpen, (blk, (lim_total, lim_idle, now)): (u32, (u32, u32, u32))) {
    i.local_get(ex.hws).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.call(I_WS_NEW).local_set(ex.hws);
    i.end();
    hc_st(i, blk, HC_WS, |i| {
        i.local_get(ex.hws);
    });
    i.local_get(ex.n).i32_const(15).i32_and().i32_const(2).i32_eq().if_(BlockType::Empty);
    hc_st_const(i, blk, HC_SNDDONE, 1);
    i.else_();
    hc_st(i, blk, HC_SUB, |i| {
        i.local_get(ex.n).i32_const(4).i32_shr_u();
    });
    i.local_get(ex.n).i32_const(4).i32_shr_u().local_get(ex.hws).call(I_WS_JOIN);
    i.end();
    hc_st(i, blk, HC_TRL_TX, |i| {
        i.local_get(ex.trl_tx);
    });
    hc_st(i, blk, HC_TRL_PEND, |i| {
        i.local_get(ex.trl_pend);
    });
    hc_st(i, blk, HC_STR_TX, |i| {
        i.local_get(ex.str_tx).i32_const(-1).local_get(ex.str_pend).select();
    });
    hc_st(i, blk, HC_STR_PEND, |i| {
        i.local_get(ex.str_pend);
    });
    hc_st(i, blk, HC_B_PTR, |i| {
        i.local_get(ex.b_ptr);
    });
    hc_st(i, blk, HC_B_LEN, |i| {
        i.local_get(ex.b_len);
    });
    hc_st(i, blk, HC_SENTFUT, |i| {
        i.local_get(ex.sentfut);
    });
    i.call(I_MONO_NOW).local_set(now);
    i.local_get(blk).local_get(lim_total).i64_store(mem64(HC_TOTAL_MS));
    i.local_get(blk).local_get(lim_idle).i64_store(mem64(HC_IDLE_MS));
    i.local_get(blk).local_get(now).i64_store(mem64(HC_LAST));
    i.local_get(lim_total).i64_const(0).i64_gt_s().if_(BlockType::Empty);
    i.local_get(blk).local_get(now);
    hc_ms_ns(i, blk, HC_TOTAL_MS);
    i.i64_add().i64_store(mem64(HC_DEADLINE));
    i.end();
    i.i32_const(0).global_set(g.g_plen);
    i.local_get(blk).call(hf.slot_new).i64_extend_i32_u().return_();
}

/// The call shim and its helpers, in `HcFns` order.
fn push_http_call_shims(code: &mut CodeSection, cx: HcCx<'_>) {
    code.function(&shim_http_call(cx));
    code.function(&hc_fn_drive(cx));
    code.function(&hc_fn_send_err(cx));
    code.function(&hc_fn_teardown(cx));
    code.function(&hc_fn_lossy());
    code.function(&hc_fn_dec());
    code.function(&hc_fn_slot_new(cx));
}
