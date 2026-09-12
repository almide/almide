// `include!`d part of wasi.rs (codopsy max-lines split, mechanical text move —
// the p1 shims; shares the parent module's imports and items).

/// `(ptr, len) -> ()`: fd_write(fd, [(ptr,len),("\n",1)]).
fn shim_print(fd: i32, park: u64) -> Function {
    let (ptr, len) = (0u32, 1u32);
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.i32_const(park as i32).local_get(ptr).i32_store(mem(IOV));
    i.i32_const(park as i32).local_get(len).i32_store(mem(IOV + 4));
    i.i32_const(fd);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0); // fd_write: the payload
    i.drop();
    i.i32_const(park as i32).i32_const(0x0A).i32_store8(mem8(NL));
    i.i32_const(park as i32).i32_const((park + NL) as i32).i32_store(mem(IOV));
    i.i32_const(park as i32).i32_const(1).i32_store(mem(IOV + 4));
    i.i32_const(fd);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0); // fd_write: the newline
    i.drop();
    i.end();
    f
}

/// `(code) -> ()`: proc_exit never returns.
fn shim_exit() -> Function {
    let mut f = Function::new([]);
    f.instructions().local_get(0).call(1).unreachable().end();
    f
}

/// The almide `fs_call` contract over WASI: ops 26/29/30/31/32/34/35/36/37
/// supported (the environ/args trio routes to its own shims when they
/// ship, #1716/#1841), everything else takes the defined refusal
/// (stderr + exit 1).
fn shim_fs_call(
    park: u64,
    g_plen: u32,
    f_env_get: Option<u32>,
    f_env_set: Option<u32>,
    f_args: Option<u32>,
) -> Function {
    // params: 0=op 1=a_ptr 2=a_len 3=b_ptr 4=b_len; locals: 5=nread
    // 6=deadline (i64, op 36)
    let (op, a_len, b_ptr, b_len, nread) = (0u32, 2u32, 3u32, 4u32, 5u32);
    let deadline = 6u32;
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    let mut i = f.instructions();

    // ops 26/37/29: env.get / env.set / args — forwarded whole to the
    // service shim, when the op set shipped one (an absent service falls
    // through to the refusal, which the build-time op audit forecloses).
    let forwarded = [(26, f_env_get), (37, f_env_set), (29, f_args)];
    for (code, target) in forwarded.into_iter().filter_map(|(c, t)| t.map(|t| (c, t))) {
        i.local_get(op).i32_const(code).i32_eq().if_(BlockType::Empty);
        for p in 0..5u32 {
            i.local_get(p);
        }
        i.call(target).return_();
        i.end();
    }

    // op 30: raw stdout append.
    i.local_get(op).i32_const(30).i32_eq().if_(BlockType::Empty);
    i.i32_const(park as i32).local_get(b_ptr).i32_store(mem(IOV));
    i.i32_const(park as i32).local_get(b_len).i32_store(mem(IOV + 4));
    i.i32_const(1);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0).drop();
    i.i64_const(0).return_();
    i.end();

    // op 35: incremental stdin — ONE fd_read of up to min(a_len, 4096)
    // bytes into the park data region (the count rides in a_len, op 32's
    // b_len convention). Short reads are the contract ("up to n"): the
    // guest's read_line/read_byte loops ask byte-at-a-time, so one
    // fd_read per call is exactly the incumbent leg's cadence. An errno
    // or EOF answers 0 bytes.
    i.local_get(op).i32_const(35).i32_eq().if_(BlockType::Empty);
    i.i32_const(park as i32).i32_const((park + DATA) as i32).i32_store(mem(IOV));
    // len = clamp(a_len, 0..=4096) — unsigned min folds a negative count
    // into the 4096 arm, and 4096 stays inside the fixed park span.
    i.local_get(a_len).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.i32_const(0).local_set(a_len);
    i.end();
    i.local_get(a_len).i32_const(4096).i32_lt_u().if_(BlockType::Result(ValType::I32));
    i.local_get(a_len);
    i.else_();
    i.i32_const(4096);
    i.end();
    i.local_set(nread);
    i.i32_const(park as i32).local_get(nread).i32_store(mem(IOV + 4));
    i.i32_const(0);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(4); // fd_read
    i.if_(BlockType::Empty); // errno != 0 → 0 bytes
    i.i32_const(0).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();
    i.i32_const(park as i32).i32_load(mem(NREAD)).local_set(nread);
    i.local_get(nread).global_set(g_plen);
    i.local_get(nread).i64_extend_i32_u().return_();
    i.end();

    // op 32: entropy into the park data region (count rides in b_len).
    i.local_get(op).i32_const(32).i32_eq().if_(BlockType::Empty);
    i.i32_const((park + DATA) as i32).local_get(b_len).call(2).drop();
    i.local_get(b_len).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();

    // op 34: the wall clock, raw nanos.
    i.local_get(op).i32_const(34).i32_eq().if_(BlockType::Empty);
    i.i32_const(0).i64_const(1).i32_const(park as i32).call(3).drop();
    i.i32_const(park as i32).i64_load(mem(0)).return_();
    i.end();

    // op 36: env.sleep_ms — a MONOTONIC busy-wait over clock_time_get
    // (the ms count rides a_len, the op-35 scalar convention). WASI p1
    // has no sleep primitive short of poll_oneoff; the import-count
    // objection died with #1716 (elements re-encode through the Remap
    // now), so the spin survives only until someone wires poll_oneoff.
    // The embedded host and native sleep properly; the CPU burn is
    // confined to stock-runtime artifacts.
    i.local_get(op).i32_const(36).i32_eq().if_(BlockType::Empty);
    i.local_get(a_len).i32_const(0).i32_lt_s().if_(BlockType::Empty);
    i.i32_const(0).local_set(a_len);
    i.end();
    i.i32_const(1).i64_const(1).i32_const(park as i32).call(3).drop();
    i.i32_const(park as i32).i64_load(mem(0));
    i.local_get(a_len).i64_extend_i32_u().i64_const(1_000_000).i64_mul();
    i.i64_add().local_set(deadline);
    i.loop_(BlockType::Empty);
    i.i32_const(1).i64_const(1).i32_const(park as i32).call(3).drop();
    i.i32_const(park as i32).i64_load(mem(0));
    i.local_get(deadline).i64_lt_u().br_if(0);
    i.end();
    i.i64_const(0).return_();
    i.end();

    // Everything else: the defined refusal.
    i.i32_const(park as i32).i32_const((park + MSG) as i32).i32_store(mem(IOV));
    i.i32_const(park as i32).i32_const(UNSUPPORTED_MSG.len() as i32).i32_store(mem(IOV + 4));
    i.i32_const(2);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0).drop();
    i.i32_const(1).call(1); // proc_exit(1)
    i.unreachable();
    i.end();
    f
}

/// `(dst) -> ()`: copy the parked bytes into guest memory.
fn shim_host_read(park: u64, g_plen: u32) -> Function {
    let dst = 0u32;
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(dst);
    i.i32_const((park + DATA) as i32);
    i.global_get(g_plen);
    i.memory_copy(0, 0);
    i.end();
    f
}

/// op 37 (env.set): append `[klen u32][vlen u32][key][val]` to the overlay
/// log page. The log is append-only; op 26 scans it last-write-wins, so a
/// re-set key needs no in-place edit. A full page takes the defined
/// refusal — never a silent drop.
fn shim_env_set(park: u64, g_ovl: u32) -> Function {
    // params: 0=op 1=a_ptr 2=a_len 3=b_ptr 4=b_len; locals: 5=at
    let (a_ptr, a_len, b_ptr, b_len, at) = (1u32, 2u32, 3u32, 4u32, 5u32);
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const((park + OVL) as i32).global_get(g_ovl).i32_add().local_set(at);
    // Room check: entry must fit under the park end.
    i.local_get(at).i32_const(8).i32_add().local_get(a_len).i32_add().local_get(b_len).i32_add();
    i.i32_const((park + PARK_SPAN) as i32).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(park as i32).i32_const((park + MSG2) as i32).i32_store(mem(IOV));
    i.i32_const(park as i32).i32_const(ENV_FULL_MSG.len() as i32).i32_store(mem(IOV + 4));
    i.i32_const(2);
    i.i32_const((park + IOV) as i32);
    i.i32_const(1);
    i.i32_const((park + NREAD) as i32);
    i.call(0).drop();
    i.i32_const(1).call(1);
    i.unreachable();
    i.end();
    i.local_get(at).local_get(a_len).i32_store(mem(0));
    i.local_get(at).local_get(b_len).i32_store(mem(4));
    i.local_get(at).i32_const(8).i32_add().local_get(a_ptr).local_get(a_len).memory_copy(0, 0);
    i.local_get(at).i32_const(8).i32_add().local_get(a_len).i32_add();
    i.local_get(b_ptr).local_get(b_len).memory_copy(0, 0);
    i.global_get(g_ovl).i32_const(8).i32_add().local_get(a_len).i32_add().local_get(b_len).i32_add();
    i.global_set(g_ovl);
    i.i64_const(0).return_();
    i.unreachable();
    i.end();
    f
}

/// op 26 (env.get): scan the overlay log for the LAST entry whose key
/// matches (env.set wins over the process environ), else walk the real
/// environ via `environ_sizes_get`/`environ_get` ("KEY=VALUE\0" strings).
/// Found: value bytes stage at park+DATA (host_read's landing zone),
/// answer `pack(0, len)`. Absent: `pack(2, 0)` — the ok-none tag.
/// `i_sizes` / `i_get` are the import indices the environ pair landed on.
fn shim_env_get(park: u64, g_plen: u32, g_ovl: u32, i_sizes: u32, i_get: u32) -> Function {
    // params: 0=op 1=a_ptr 2=a_len 3=b_ptr 4=b_len
    // locals: 5=p 6=end 7=klen 8=vlen 9=best 10=j 11=s 12=count
    let (a_ptr, a_len) = (1u32, 2u32);
    let (p, endp, klen, vlen, best, j, s, count) = (5u32, 6u32, 7u32, 8u32, 9u32, 10u32, 11u32, 12u32);
    let mut f = Function::new([(8, ValType::I32)]);
    let mut i = f.instructions();

    // ── overlay scan, last match wins ──
    i.i32_const(0).local_set(best);
    i.i32_const((park + OVL) as i32).local_set(p);
    i.i32_const((park + OVL) as i32).global_get(g_ovl).i32_add().local_set(endp);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(endp).i32_ge_u().br_if(1);
    i.local_get(p).i32_load(mem(0)).local_set(klen);
    i.local_get(p).i32_load(mem(4)).local_set(vlen);
    i.local_get(klen).local_get(a_len).i32_eq().if_(BlockType::Empty);
    // byte compare key at p+8 vs a_ptr
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(klen).i32_ge_u().if_(BlockType::Empty);
    i.local_get(p).local_set(best); // full match
    i.br(2);
    i.end();
    i.local_get(p).i32_const(8).i32_add().local_get(j).i32_add().i32_load8_u(mem8(0));
    i.local_get(a_ptr).local_get(j).i32_add().i32_load8_u(mem8(0));
    i.i32_ne().br_if(1);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.end();
    i.local_get(p).i32_const(8).i32_add().local_get(klen).i32_add().local_get(vlen).i32_add().local_set(p);
    i.br(0).end().end();
    i.local_get(best).i32_const(0).i32_ne().if_(BlockType::Empty);
    i.local_get(best).i32_load(mem(4)).local_set(vlen);
    i.i32_const((park + DATA) as i32);
    i.local_get(best).i32_const(8).i32_add().local_get(best).i32_load(mem(0)).i32_add();
    i.local_get(vlen);
    i.memory_copy(0, 0);
    i.local_get(vlen).global_set(g_plen);
    i.local_get(vlen).i64_extend_i32_u().return_();
    i.end();

    // ── real environ fallthrough ──
    i.i32_const((park + NREAD) as i32).i32_const((park + NREAD + 4) as i32).call(i_sizes); // environ_sizes_get
    i.if_(BlockType::Empty); // errno → none
    i.i64_const(2).i64_const(32).i64_shl().return_();
    i.end();
    i.i32_const(park as i32).i32_load(mem(NREAD)).local_set(count);
    // Oversized environ (ptrs + buf past the overlay page's start) → none:
    // the staging area is DATA..OVL.
    i.i32_const((park + DATA) as i32).local_get(count).i32_const(4).i32_mul().i32_add();
    i.i32_const(park as i32).i32_load(mem(NREAD + 4)).i32_add();
    i.i32_const((park + OVL) as i32).i32_gt_u().if_(BlockType::Empty);
    i.i64_const(2).i64_const(32).i64_shl().return_();
    i.end();
    i.i32_const((park + DATA) as i32);
    i.i32_const((park + DATA) as i32).local_get(count).i32_const(4).i32_mul().i32_add();
    i.call(i_get); // environ_get(ptrs, buf)
    i.if_(BlockType::Empty);
    i.i64_const(2).i64_const(32).i64_shl().return_();
    i.end();
    // walk entries: match "KEY=" prefix.
    i.i32_const(0).local_set(p);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(p).local_get(count).i32_ge_u().br_if(1);
    i.i32_const((park + DATA) as i32).local_get(p).i32_const(4).i32_mul().i32_add().i32_load(mem(0)).local_set(s);
    // key bytes equal AND s[a_len] == '='
    i.i32_const(0).local_set(j);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(j).local_get(a_len).i32_ge_u().if_(BlockType::Empty);
    i.local_get(s).local_get(a_len).i32_add().i32_load8_u(mem8(0)).i32_const(61).i32_ne().br_if(2);
    // value = s+a_len+1 .. NUL
    i.local_get(s).local_get(a_len).i32_add().i32_const(1).i32_add().local_set(s);
    i.i32_const(0).local_set(vlen);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(s).local_get(vlen).i32_add().i32_load8_u(mem8(0)).i32_eqz().br_if(1);
    i.local_get(vlen).i32_const(1).i32_add().local_set(vlen);
    i.br(0).end().end();
    i.i32_const((park + DATA) as i32).local_get(s).local_get(vlen).memory_copy(0, 0);
    i.local_get(vlen).global_set(g_plen);
    i.local_get(vlen).i64_extend_i32_u().return_();
    i.end();
    i.local_get(s).local_get(j).i32_add().i32_load8_u(mem8(0));
    i.local_get(a_ptr).local_get(j).i32_add().i32_load8_u(mem8(0));
    i.i32_ne().br_if(1);
    i.local_get(j).i32_const(1).i32_add().local_set(j);
    i.br(0).end().end();
    i.local_get(p).i32_const(1).i32_add().local_set(p);
    i.br(0).end().end();
    i.i64_const(2).i64_const(32).i64_shl().return_();
    i.unreachable();
    i.end();
    f
}

/// op 29 (args): every argv entry from `args_sizes_get`/`args_get`
/// (argv0 included, matching the embedded host's non-empty-argv0
/// contract), re-framed as `[len u32][bytes]` per entry — the almide
/// frames encoding — staged at park+DATA. Answer `pack(0, total)`.
/// `i_sizes` / `i_get` are the import indices the args pair landed on.
fn shim_args(park: u64, g_plen: u32, i_sizes: u32, i_get: u32) -> Function {
    // params 0..4 unused beyond the ABI; locals: 5=argc 6=i 7=s 8=n 9=out
    // 10=frames_base
    let (argc, idx, s, n, out, frames_base) = (5u32, 6u32, 7u32, 8u32, 9u32, 10u32);
    let mut f = Function::new([(6, ValType::I32)]);
    let mut i = f.instructions();
    i.i32_const((park + NREAD) as i32).i32_const((park + NREAD + 4) as i32).call(i_sizes); // args_sizes_get
    i.if_(BlockType::Empty);
    i.i32_const(0).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();
    i.i32_const(park as i32).i32_load(mem(NREAD)).local_set(argc);
    // Staging: raw ptrs+buf at DATA, frames rebuilt behind them. The raw
    // area is argc*4 + bufsize; frames need at most bufsize + 4*argc more.
    // Both must sit under the overlay page.
    i.i32_const((park + DATA) as i32).local_get(argc).i32_const(8).i32_mul().i32_add();
    i.i32_const(park as i32).i32_load(mem(NREAD + 4)).i32_const(2).i32_mul().i32_add();
    i.i32_const((park + OVL) as i32).i32_gt_u().if_(BlockType::Empty);
    i.i32_const(0).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();
    i.i32_const((park + DATA) as i32);
    i.i32_const((park + DATA) as i32).local_get(argc).i32_const(4).i32_mul().i32_add();
    i.call(i_get); // args_get(ptrs, buf)
    i.if_(BlockType::Empty);
    i.i32_const(0).global_set(g_plen);
    i.i64_const(0).return_();
    i.end();
    // Build frames after the raw area: out = DATA + argc*4 + bufsize.
    i.i32_const((park + DATA) as i32).local_get(argc).i32_const(4).i32_mul().i32_add();
    i.i32_const(park as i32).i32_load(mem(NREAD + 4)).i32_add();
    i.local_tee(out).local_set(frames_base);
    i.i32_const(0).local_set(idx);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(idx).local_get(argc).i32_ge_u().br_if(1);
    i.i32_const((park + DATA) as i32).local_get(idx).i32_const(4).i32_mul().i32_add().i32_load(mem(0)).local_set(s);
    // n = strlen(s)
    i.i32_const(0).local_set(n);
    i.block(BlockType::Empty).loop_(BlockType::Empty);
    i.local_get(s).local_get(n).i32_add().i32_load8_u(mem8(0)).i32_eqz().br_if(1);
    i.local_get(n).i32_const(1).i32_add().local_set(n);
    i.br(0).end().end();
    i.local_get(out).local_get(n).i32_store(mem(0));
    i.local_get(out).i32_const(4).i32_add().local_get(s).local_get(n).memory_copy(0, 0);
    i.local_get(out).i32_const(4).i32_add().local_get(n).i32_add().local_set(out);
    i.local_get(idx).i32_const(1).i32_add().local_set(idx);
    i.br(0).end().end();
    // Slide the frames down to DATA (memmove semantics) and answer.
    i.local_get(out).local_get(frames_base).i32_sub().local_set(n); // total
    i.i32_const((park + DATA) as i32).local_get(frames_base).local_get(n).memory_copy(0, 0);
    i.local_get(n).global_set(g_plen);
    i.local_get(n).i64_extend_i32_u().return_();
    i.unreachable();
    i.end();
    f
}
