// `include!`d part of wasi_p3.rs (codopsy max-lines split, mechanical text move —
// shares the parent module's imports and items; nothing here is pub beyond the parent).

/// `(ptr, len) -> ()`: open-once, write-all, then the newline.
fn shim_print(port: PrintPort, park: u64, f_await: u32, newline: bool) -> Function {
    let PrintPort { g_tx, g_fut, call_import, new_import, write_import } = port;
    let (ptr, len) = (0u32, 1u32);
    let n = 2u32;
    let s64 = 3u32;
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    // locals: 0 ptr, 1 len (params), 2 n (i32), 3 scratch (i64)
    let mut i = f.instructions();
    open_stream(&mut i, g_tx, g_fut, call_import, new_import, s64);
    write_all(&mut i, g_tx, write_import, f_await, (ptr, len, n));
    if newline {
        i.i32_const(park as i32).i32_const(0x0A).i32_store8(mem8(0));
        i.i32_const(park as i32).local_set(ptr);
        i.i32_const(1).local_set(len);
        write_all(&mut i, g_tx, write_import, f_await, (ptr, len, n));
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

/// Err return: park the static message, answer `pack(1, len)` (the http
/// shim's static refusals).
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

/// The fan await's read of a PREFETCHED descriptor in local `d`:
/// read-via-stream, the doubling read loop (DROPPED = EOF, each read through
/// `$await`) and the handle drops, leaving the bytes in `buf` / `total` for
/// the caller to vet (#3140: anything the sync read would answer
/// differently goes back through it).
fn fs_read_prefetched(i: &mut wasm_encoder::InstructionSink<'_>, g: P3Globals, l: ReadLocals) {
    let P3Globals { park, f_alloc, f_await, .. } = g;
    let ReadLocals { d, rx, fut, buf, cap, total, n } = l;
    i.local_get(d).i64_const(0).i32_const((park + RET) as i32).call(I_FS_RVS);
    i.i32_const((park + RET) as i32).i32_load(mem(0)).local_set(rx);
    i.i32_const((park + RET) as i32).i32_load(mem(4)).local_set(fut);
    i.i32_const(0).i32_const(0).i32_const(8).i32_const(65536).call(f_alloc).local_set(buf);
    i.i32_const(65536).local_set(cap);
    i.i32_const(0).local_set(total);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(total).local_get(cap).i32_ge_u();
    i.if_(BlockType::Empty);
    i.local_get(buf).local_get(cap).i32_const(8);
    i.local_get(cap).i32_const(1).i32_shl();
    i.call(f_alloc).local_set(buf);
    i.local_get(cap).i32_const(1).i32_shl().local_set(cap);
    i.end();
    i.local_get(rx);
    i.local_get(rx);
    i.local_get(buf).local_get(total).i32_add();
    i.local_get(cap).local_get(total).i32_sub();
    i.call(I_FS_SREAD);
    // #2955: the raw `(count << 4) | status`; EOF is the status (DROPPED /
    // CANCELLED), not a zero count — a COMPLETED read may carry 0 items.
    i.call(f_await).local_set(n);
    i.local_get(total).local_get(n).i32_const(4).i32_shr_u().i32_add().local_set(total);
    i.local_get(n).i32_const(15).i32_and().br_if(1);
    i.br(0).end().end();
    i.local_get(rx).call(I_FS_SDROP);
    i.local_get(fut).call(I_FS_FDROP);
    i.local_get(d).call(I_FS_RESDROP);
}

/// `return fs_call(1, a, al, 0, 0)`: the sequential read_text, which the
/// fs service answers — every fan-await path that cannot vouch for its
/// prefetched answer ends here, so the observable answer is the sync one.
fn fs_read_sync(i: &mut wasm_encoder::InstructionSink<'_>, f_self: u32, a_ptr: u32, a_len: u32) {
    i.i32_const(1).local_get(a_ptr).local_get(a_len).i32_const(0).i32_const(0);
    i.call(f_self).return_();
}

/// The p3 fs service (#3140): the spliced p1 service over its p3 adapter —
/// its dispatcher's index and the ops `shim_fs_call` forwards to it.
struct FsService {
    f: u32,
    ops: Vec<i32>,
}

/// The host contract over p3 (op codes shared with the embedded host): 30
/// raw stdout, 31 stdin read-to-end, 35 stdin take-n, 32 entropy, 34 wall
/// clock, 60 monotonic clock, plus the fs (through the spliced service), http
/// and env (26 / 29 / 36 / 37) families; anything else = the defined refusal.
fn shim_fs_call(g: P3Globals, abi: &FsAbi, f_self: u32, f_http: Option<u32>, f_env: Option<(u32, &[i32])>, svc: Option<&FsService>) -> Function {
    let P3Globals { park, g_plen, g_ppos, g_in_rx, g_in_fut, g_out_tx, g_out_fut, g_err_tx, g_err_fut, f_reserve, f_await, .. } = g;
    let (op, a_len, b_ptr, b_len) = (0u32, 2u32, 3u32, 4u32);
    let n = 6u32;
    let s64 = 7u32;
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I64), (7, ValType::I32)]);
    let mut i = f.instructions();

    // ops 43..=47 (the string client) and 48..=50 (the framed family) —
    // forwarded whole to the http shim when the module's op set earned the
    // imports (#1710 PR B).
    if let Some(h) = f_http {
        i.local_get(op).i32_const(43).i32_ge_s();
        i.local_get(op).i32_const(50).i32_le_s();
        i.i32_and().if_(BlockType::Empty);
        for pidx in 0..5u32 {
            i.local_get(pidx);
        }
        i.call(h).return_();
        i.end();
    }

    // ops 26 / 29 / 36 / 37 (env.get, the program arguments, env.sleep_ms,
    // env.set) — the ones the op set names, forwarded whole to the env
    // service (ADR-0023 step 3, #3223).
    if let Some((e, ops)) = f_env.filter(|(_, ops)| !ops.is_empty()) {
        for (k, o) in ops.iter().enumerate() {
            i.local_get(op).i32_const(*o).i32_eq();
            if k > 0 {
                i.i32_or();
            }
        }
        i.if_(BlockType::Empty);
        for pidx in 0..5u32 {
            i.local_get(pidx);
        }
        i.call(e).return_();
        i.end();
    }

    // op 30: raw stdout append (no newline) — b carries the bytes; op 73:
    // the same on stderr (`panic`'s line, #2769).
    let ports = [
        (30, g_out_tx, g_out_fut, I_OUT_CALL, I_OUT_NEW, I_OUT_WRITE),
        (73, g_err_tx, g_err_fut, I_ERR_CALL, I_ERR_NEW, I_ERR_WRITE),
    ];
    for (code, g_tx, g_fut, call, new, write) in ports {
        i.local_get(op).i32_const(code).i32_eq().if_(BlockType::Empty);
        open_stream(&mut i, g_tx, g_fut, call, new, s64);
        write_all(&mut i, g_tx, write, f_await, (b_ptr, b_len, n));
        i.i64_const(0).return_();
        i.end();
    }

    // op 35: stdin take up to a_len bytes — reads (through `$await`)
    // straight into the park data span until one delivers bytes; a DROPPED
    // status (writer closed) answers 0. A COMPLETED read of 0 items is not
    // EOF (#2955) and reads again.
    i.local_get(op).i32_const(35).i32_eq().if_(BlockType::Empty);
    open_stdin(&mut i, g_in_rx, g_in_fut, park);
    i.i32_const(0).local_set(n);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(a_len).i32_eqz().br_if(1);
    i.global_get(g_in_rx);
    i.global_get(g_in_rx);
    i.i32_const((park + DATA) as i32);
    i.local_get(a_len);
    i.call(I_STDIN_READ);
    i.call(f_await).local_set(n);
    i.local_get(n).i32_const(4).i32_shr_u().br_if(1);
    i.local_get(n).i32_const(15).i32_and().br_if(1);
    i.br(0).end().end();
    i.local_get(n).i32_const(4).i32_shr_u().global_set(g_plen);
    i.i32_const((park + DATA) as i32).global_set(g_ppos);
    i.global_get(g_plen).i64_extend_i32_u().return_();
    i.end();

    // op 32: entropy — n rides b_len; the list lands via cabi_realloc,
    // reserved first for exactly the length asked for (#2119), so the
    // landing cannot need a grow the ABI would leave unreported.
    i.local_get(op).i32_const(32).i32_eq().if_(BlockType::Empty);
    i.local_get(b_len).call(f_reserve);
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

    // op 60: the monotonic clock — `monotonic-clock.now() -> mark` (u64
    // nanos) lowers to a bare `() -> i64`, answered as is.
    i.local_get(op).i32_const(60).i32_eq().if_(BlockType::Empty);
    i.call(I_MONO_NOW).return_();
    i.end();

    // ── The filesystem (#3140): the spliced service ───────────────────
    // Every fs op but the fan prefetch triple goes to the p1 fs service,
    // spliced over its p3 adapter — the same code the stock-p1 artifact
    // runs, so the two answer alike by construction.
    if let Some(s) = svc.filter(|s| !s.ops.is_empty()) {
        for (k, o) in s.ops.iter().enumerate() {
            i.local_get(op).i32_const(*o).i32_eq();
            if k > 0 {
                i.i32_or();
            }
        }
        i.if_(BlockType::Empty);
        for pidx in 0..5u32 {
            i.local_get(pidx);
        }
        i.call(s.f).return_();
        i.end();
        fan_prefetch_arms(&mut i, g, abi, f_self, s.f);
    }

    // Everything else: the defined refusal — the message on stderr, exit 1.
    open_stream(&mut i, g_err_tx, g_err_fut, I_ERR_CALL, I_ERR_NEW, s64);
    i.i32_const((park + MSG) as i32).local_set(b_ptr);
    i.i32_const(UNSUPPORTED_MSG.len() as i32).local_set(b_len);
    write_all(&mut i, g_err_tx, I_ERR_WRITE, f_await, (b_ptr, b_len, n));
    i.i32_const(1).call(I_EXIT);
    i.unreachable();
    i.end();
    f
}

/// The fan prefetch triple (#1628 increments 2b/2c) — START (40), AWAIT (41)
/// and ABANDON (42) a slot. The path a slot opens is the one the fs service
/// RESOLVES the guest path to (pseudo-op -1: its preopen descriptor and
/// relative remainder), so a prefetched read and a sequential one name the
/// same file (#3140).
fn fan_prefetch_arms(i: &mut wasm_encoder::InstructionSink<'_>, g: P3Globals, abi: &FsAbi, f_self: u32, f_svc: u32) {
    let P3Globals { park, g_plen, g_ppos, f_alloc, g_wset, g_slots, g_slotn, .. } = g;
    let (op, a_ptr, a_len, b_len) = (0u32, 1u32, 2u32, 4u32);
    let total = 5u32;
    let n = 6u32;
    let s64 = 7u32;
    let (d, buf, cap, rx, fut) = (8u32, 9u32, 10u32, 11u32, 12u32);
    let (sl, j) = (13u32, 14u32);

    // op 40: START a slot — async-lower open-at for slot k (k rides b_len;
    // the path rides a). The subtask joins the ONE waitable set; an
    // immediate Returned (packed status 2, no subtask) marks the slot done
    // on the spot. A path the service cannot resolve / k past the cap: the
    // slot stays empty and the await falls back to the sync path.
    i.local_get(op).i32_const(40).i32_eq();
    i.if_(BlockType::Empty);
    i.i32_const(-1).local_get(a_ptr).local_get(a_len).i32_const(0).i32_const(0);
    i.call(f_svc).local_set(s64);
    i.local_get(s64).i64_const(0).i64_ge_s();
    i.local_get(b_len).i32_const(SLOT_CAP).i32_lt_u().i32_and();
    i.if_(BlockType::Empty);
    i.global_get(g_slots).i32_eqz();
    i.if_(BlockType::Empty);
    i.i32_const(0).i32_const(0).i32_const(8);
    i.i32_const(SLOT_CAP * SLOT_STRIDE);
    i.call(f_alloc).global_set(g_slots);
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
    // the argptr block (canonical layout of open-at's params): the resolved
    // descriptor and remainder (ppos / plen) from the service.
    i.local_get(sl).local_get(s64).i32_wrap_i64().i32_store(mem(0));
    i.local_get(sl).i32_const(1).i32_store(mem(4)); // path-flags: symlink-follow
    i.local_get(sl).global_get(g_ppos).i32_store(mem(8));
    i.local_get(sl).global_get(g_plen).i32_store(mem(12));
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
    // done, until slot k is done; then read the parked descriptor. A failed
    // open, a descriptor that is not a regular file (a directory) or bytes
    // that are not UTF-8 go back through the sync read, whose error text and
    // checks are the service's — the prefetch never answers what it would not.
    i.local_get(op).i32_const(41).i32_eq();
    i.if_(BlockType::Empty);
    i.global_get(g_slots).i32_eqz();
    i.local_get(b_len).i32_const(SLOT_CAP).i32_ge_u().i32_or();
    i.if_(BlockType::Empty);
    fs_read_sync(i, f_self, a_ptr, a_len);
    i.end();
    i.global_get(g_slots).local_get(b_len).i32_const(SLOT_STRIDE).i32_mul().i32_add();
    i.local_set(sl);
    i.local_get(sl).i32_load(mem(48)).i32_eqz();
    i.if_(BlockType::Empty);
    fs_read_sync(i, f_self, a_ptr, a_len);
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
    // consume the slot; a failed open re-reads synchronously
    i.local_get(sl).i32_const(0).i32_store(mem(48));
    i.local_get(sl).i32_load8_u(mem8(24));
    i.if_(BlockType::Empty);
    fs_read_sync(i, f_self, a_ptr, a_len);
    i.end();
    i.local_get(sl).i32_load(mem(24 + abi.open_payload)).local_set(d);
    // only a regular file is streamed (read-via-stream traps on a directory)
    i.i32_const(-3).local_get(d).i32_const(0).i32_const(0).i32_const(0);
    i.call(f_svc).i64_eqz();
    i.if_(BlockType::Empty);
    i.local_get(d).call(I_FS_RESDROP);
    fs_read_sync(i, f_self, a_ptr, a_len);
    i.end();
    fs_read_prefetched(i, g, ReadLocals { d, rx, fut, buf, cap, total, n });
    i.i32_const(-2).local_get(buf).local_get(total).i32_const(0).i32_const(0);
    i.call(f_svc).i64_eqz();
    i.if_(BlockType::Empty);
    fs_read_sync(i, f_self, a_ptr, a_len);
    i.end();
    i.local_get(buf).global_set(g_ppos);
    i.local_get(total).global_set(g_plen);
    i.local_get(total).i64_extend_i32_u().return_();
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
}
