// `include!`d part of wasi_p3.rs (codopsy max-lines split, mechanical text move —
// shares the parent module's imports and items; nothing here is pub beyond the parent).

/// `(ptr, len) -> ()`: open-once, sync-write-all, then the newline.
fn shim_print(port: PrintPort, park: u64, newline: bool) -> Function {
    let PrintPort { g_tx, g_fut, call_import, new_import, write_import } = port;
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
fn fs_preopen(
    i: &mut wasm_encoder::InstructionSink<'_>,
    g_pre: u32,
    park: u64,
    f_reserve: u32,
) {
    i.global_get(g_pre).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    // The preopen list's descriptors and path strings land via
    // cabi_realloc at a length the HOST chooses, so the reservation is a
    // declared ceiling rather than an exact bound (#2119): every real
    // preopen table is orders of magnitude under it, and a table past it
    // is the residual this shim ledgers.
    i.i32_const(PREOPEN_RESERVE).call(f_reserve);
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
fn fs_read_tail(i: &mut wasm_encoder::InstructionSink<'_>, g: P3Globals, l: ReadLocals) {
    let P3Globals { park, f_alloc, g_ppos, g_plen, .. } = g;
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
fn shim_fs_call(g: P3Globals, abi: &FsAbi, f_self: u32, f_http: Option<u32>) -> Function {
    let P3Globals { park, g_plen, g_ppos, g_in_rx, g_in_fut, g_out_tx, g_out_fut, g_err_tx, g_err_fut, g_pre, f_alloc, g_wset, g_slots, g_slotn, f_reserve } = g;
    let (op, a_ptr, a_len, b_ptr, b_len) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let total = 5u32;
    let n = 6u32;
    let s64 = 7u32;
    let (d, buf, cap, rx, fut) = (8u32, 9u32, 10u32, 11u32, 12u32);
    let (sl, j) = (13u32, 14u32);
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

    // ── The filesystem READ surface (#1628 increment 2a) ──────────────

    // ops 4/5/6: exists / is_dir / is_file — stat-at with symlink-follow
    // (the native `fs::metadata` behavior). The flag rides the len half;
    // these never err: any stat failure (including no preopen) is false.
    i.local_get(op).i32_const(4).i32_eq();
    i.local_get(op).i32_const(5).i32_eq().i32_or();
    i.local_get(op).i32_const(6).i32_eq().i32_or();
    i.if_(BlockType::Empty);
    fs_preopen(&mut i, g_pre, park, f_reserve);
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
    fs_preopen(&mut i, g_pre, park, f_reserve);
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
    fs_read_tail(&mut i, g, ReadLocals { d, rx, fut, buf, cap, total, n });
    i.end();

    // ── The fan prefetch pair (#1628 increment 2b) ────────────────────

    // op 40: START a slot — async-lower open-at for slot k (k rides
    // b_len; the path rides a). The subtask joins the ONE waitable set;
    // an immediate Returned (packed status 2, no subtask) marks the slot
    // done on the spot. No preopen / k past the cap: the slot stays
    // empty and the await falls back to the sync path.
    i.local_get(op).i32_const(40).i32_eq();
    i.if_(BlockType::Empty);
    fs_preopen(&mut i, g_pre, park, f_reserve);
    i.global_get(g_pre).i32_const(0).i32_ge_s();
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
    fs_read_tail(&mut i, g, ReadLocals { d, rx, fut, buf, cap, total, n });
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
    fs_preopen(&mut i, g_pre, park, f_reserve);
    i.global_get(g_pre).i32_const(0).i32_lt_s();
    i.if_(BlockType::Empty);
    fs_err(&mut i, g_ppos, g_plen, park, MSG_NOPRE, E_NOPRE.len());
    i.end();
    // write_bytes: pack the 8-byte slots' low bytes into a compact buffer.
    i.local_get(op).i32_const(3).i32_eq();
    i.if_(BlockType::Empty);
    i.local_get(b_len).i32_const(3).i32_shr_u().local_set(cap);
    i.i32_const(0).i32_const(0).i32_const(8).local_get(cap).call(f_alloc).local_set(buf);
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
    fs_preopen(&mut i, g_pre, park, f_reserve);
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
    fs_preopen(&mut i, g_pre, park, f_reserve);
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
