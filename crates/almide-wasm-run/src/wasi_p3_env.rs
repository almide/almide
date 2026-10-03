// `include!`d part of wasi_p3.rs — shares the parent module's imports and
// items; nothing here is pub beyond the parent.

/// `(waitable, rc) -> rc` (ADR-0023 step 1): complete an `[async-lower]`
/// stream/future copy the way the synchronous builtin would have. `rc` is
/// the async call's answer: anything but BLOCKED (-1) is already the
/// `count<<4 | status` word (or a future's status) and returns as is.
/// BLOCKED joins the one end to a FRESH waitable set — never the fan's
/// `g_wset` or the http exchange's set, whose subtask and copy events belong
/// to their own drain loops — waits for its copy event, unjoins, drops the
/// set and answers the event's payload, which is the same word the
/// synchronous builtin returns. So every caller keeps its loop over the
/// word unchanged, and the artifact imports no 🚝 builtin.
fn shim_await(park: u64) -> Function {
    let (h, rc, ws) = (0u32, 1u32, 2u32);
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(rc).i32_const(-1).i32_ne().if_(BlockType::Empty);
    i.local_get(rc).return_();
    i.end();
    i.call(I_WS_NEW).local_set(ws);
    i.local_get(h).local_get(ws).call(I_WS_JOIN);
    i.local_get(ws).i32_const((park + AWAIT_EV) as i32).call(I_WS_WAIT).drop();
    i.local_get(h).i32_const(0).call(I_WS_JOIN);
    i.local_get(ws).call(I_WS_DROP);
    i.i32_const((park + AWAIT_EV) as i32).i32_load(mem(4));
    i.end();
    f
}

/// The env service's import indices (ADR-0023 step 3). Each import ships
/// only when the op set names its op — the p1 transform's reachability
/// gate (#1841): a program that never reads its environment does not ask
/// the runtime for `wasi:cli/environment`.
#[derive(Clone, Copy, Default)]
struct EnvImports {
    /// `wasi:cli/environment.get-environment` (op 26), retptr.
    get_env: Option<u32>,
    /// `wasi:cli/environment.get-arguments` (op 29), retptr.
    get_args: Option<u32>,
    /// `wasi:clocks/monotonic-clock.wait-for` (op 36), a sync lower of the
    /// async func: the task blocks for the duration.
    wait_for: Option<u32>,
    /// `env.set` (op 37, #3223): the overlay log's append, which imports
    /// nothing — as on p1.
    set: bool,
}

impl EnvImports {
    /// Assign indices from `base` in the fixed order env, args, sleep,
    /// taking only the ops `host_ops` names — and `get-environment` for an
    /// http program too, whose timeout and size limit are read from it
    /// (ADR-0023 step 2).
    fn plan(host_ops: &[i32], base: u32, wants_http: bool) -> (Self, u32) {
        let mut next = base;
        let mut take = |wanted: bool| {
            wanted.then(|| {
                next += 1;
                next - 1
            })
        };
        let e = EnvImports {
            get_env: take(wants_http || host_ops.contains(&26)),
            get_args: take(host_ops.contains(&29)),
            wait_for: take(host_ops.contains(&36)),
            set: host_ops.contains(&37),
        };
        (e, next)
    }

    fn any(self) -> bool {
        self.get_env.is_some() || self.get_args.is_some() || self.wait_for.is_some() || self.set
    }

    /// The ops the service answers — what `shim_fs_call` forwards to it.
    fn ops(self) -> Vec<i32> {
        [(self.get_env.is_some(), 26), (self.get_args.is_some(), 29), (self.wait_for.is_some(), 36), (self.set, 37)]
            .into_iter()
            .filter_map(|(on, op)| on.then_some(op))
            .collect()
    }

    /// The block's import-section entries, in index order.
    fn import_list(self, t_retptr: u32, t_wait: u32) -> Vec<(u32, &'static str, &'static str, u32)> {
        [
            (self.get_env, "wasi:cli/environment@0.3.0", "get-environment", t_retptr),
            (self.get_args, "wasi:cli/environment@0.3.0", "get-arguments", t_retptr),
            (self.wait_for, "wasi:clocks/monotonic-clock@0.3.0", "wait-for", t_wait),
        ]
        .into_iter()
        .filter_map(|(at, m, n, t)| at.map(|at| (at, m, n, t)))
        .collect()
    }
}

/// The declared ceiling reserved before `get-environment` / `get-arguments`
/// lower their lists (#2119, the preopen discipline): the host chooses the
/// length, so the reservation is a ceiling, not an exact bound.
const ENV_RESERVE: i32 = 262_144;

/// The env.set overlay on p3 (#3223): the p1 log (`env_overlay`) on its own
/// page past the park and the fs page, and the eprintln shim its refusal
/// prints through. Present exactly when the op set names op 37.
#[derive(Clone, Copy)]
struct P3Overlay {
    log: crate::wasi::env_overlay::OverlayLog,
    f_eprintln: u32,
}

/// The env service over the fs_call ABI `(op, a_ptr, a_len, b_ptr, b_len)
/// -> i64` — the native semantics the embedded host and the p1 shim serve:
///
/// - op 37 `env.set(key, value)` (#3223): appends to the overlay log — the
///   p1 shim's own emitter, so a set is local to the instance and never
///   reaches the host, as with wasi-libc's `setenv` over its copy of the
///   environment. A full log prints `ENV_FULL_MSG` and exits 1, as on p1.
/// - op 26 `env.get(key)`: the overlay first (last write wins), then the
///   FIRST `(name, value)` of `get-environment`
///   whose name equals the key byte for byte answers `pack(0, len)` with
///   the value parked (it already sits where the canonical ABI lowered it);
///   no match answers `pack(2, 0)`, the ok-none tag. The list is fetched
///   once and cached (`g_env` / `g_envn`): the environment of a WASI 0.3
///   command is fixed for the run, and a re-fetch per call would leak the
///   lowered list each time. The runtime decides what the environment is
///   (`wasmtime run --env K=V`, `-S inherit-env`), exactly as for p1.
/// - op 29 args: `get-arguments` (argv0 first, as `args_get` on p1),
///   re-framed as `[len u32][bytes]` per entry — the almide frames
///   encoding the guest skips frame 0 of.
/// - op 36 `env.sleep_ms(ms)`: the count rides `a_len` (negative = 0);
///   `wait-for(ms * 1_000_000)` on the monotonic clock. The task blocks —
///   no busy-wait (the p1 shim's spin has no p3 counterpart).
fn shim_env(g: P3Globals, e: EnvImports, ovl: Option<P3Overlay>) -> Function {
    use crate::wasi::env_overlay::{emit_append, emit_scan, ScanLocals};
    let P3Globals { park, g_plen, g_ppos, f_alloc, f_reserve, g_env, g_envn, .. } = g;
    let (op, a_ptr, a_len) = (0u32, 1u32, 2u32);
    let (p, endp, j, total, buf, out) = (5u32, 6u32, 7u32, 8u32, 9u32, 10u32);
    let (klen, vlen, best) = (11u32, 12u32, 13u32);
    let mut f = Function::new([(9, ValType::I32)]);
    let mut i = f.instructions();

    if let Some(P3Overlay { log, f_eprintln }) = ovl {
        i.local_get(op).i32_const(37).i32_eq().if_(BlockType::Empty);
        emit_append(&mut i, log, p, |i| {
            i.i32_const((park + MSG2) as i32).i32_const(ENV_FULL_MSG.len() as i32 - 1).call(f_eprintln);
            i.i32_const(1).call(I_EXIT);
            i.unreachable();
        });
        i.end();
    }

    if let Some(wait_for) = e.wait_for {
        i.local_get(op).i32_const(36).i32_eq().if_(BlockType::Empty);
        i.local_get(a_len).i32_const(0).i32_lt_s().if_(BlockType::Empty);
        i.i32_const(0).local_set(a_len);
        i.end();
        i.local_get(a_len).i64_extend_i32_u().i64_const(1_000_000).i64_mul();
        i.call(wait_for);
        i.i64_const(0).return_();
        i.end();
    }

    if let Some(get_env) = e.get_env {
        i.local_get(op).i32_const(26).i32_eq().if_(BlockType::Empty);
        if let Some(P3Overlay { log, .. }) = ovl {
            emit_scan(&mut i, log, (g_ppos, g_plen), ScanLocals { p, endp, klen, vlen, best, j });
        }
        // The snapshot, taken once.
        i.global_get(g_env).i32_const(0).i32_lt_s().if_(BlockType::Empty);
        i.i32_const(ENV_RESERVE).call(f_reserve);
        i.i32_const((park + RET) as i32).call(get_env);
        i.i32_const((park + RET) as i32).i32_load(mem(0)).global_set(g_env);
        i.i32_const((park + RET) as i32).i32_load(mem(4)).global_set(g_envn);
        i.end();
        // entries are tuple<string, string>: name ptr/len @0/4, value @8/12.
        i.global_get(g_env).local_set(p);
        i.global_get(g_env).global_get(g_envn).i32_const(16).i32_mul().i32_add().local_set(endp);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(p).local_get(endp).i32_ge_u().br_if(1);
        i.local_get(p).i32_load(mem(4)).local_get(a_len).i32_eq().if_(BlockType::Empty);
        i.i32_const(0).local_set(j);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(j).local_get(a_len).i32_ge_u().if_(BlockType::Empty);
        i.local_get(p).i32_load(mem(8)).global_set(g_ppos);
        i.local_get(p).i32_load(mem(12)).global_set(g_plen);
        i.global_get(g_plen).i64_extend_i32_u().return_();
        i.end();
        i.local_get(p).i32_load(mem(0)).local_get(j).i32_add().i32_load8_u(mem8(0));
        i.local_get(a_ptr).local_get(j).i32_add().i32_load8_u(mem8(0));
        i.i32_ne().br_if(1);
        i.local_get(j).i32_const(1).i32_add().local_set(j);
        i.br(0).end().end();
        i.end();
        i.local_get(p).i32_const(16).i32_add().local_set(p);
        i.br(0).end().end();
        i.i64_const(2).i64_const(32).i64_shl().return_();
        i.end();
    }

    if let Some(get_args) = e.get_args {
        i.local_get(op).i32_const(29).i32_eq().if_(BlockType::Empty);
        i.i32_const(ENV_RESERVE).call(f_reserve);
        i.i32_const((park + RET) as i32).call(get_args);
        // entries are strings: ptr/len @0/4.
        i.i32_const((park + RET) as i32).i32_load(mem(0)).local_set(p);
        i.local_get(p);
        i.i32_const((park + RET) as i32).i32_load(mem(4)).i32_const(8).i32_mul();
        i.i32_add().local_set(endp);
        // total = Σ (4 + len)
        i.i32_const(0).local_set(total);
        i.local_get(p).local_set(j);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(j).local_get(endp).i32_ge_u().br_if(1);
        i.local_get(total).i32_const(4).i32_add().local_get(j).i32_load(mem(4)).i32_add().local_set(total);
        i.local_get(j).i32_const(8).i32_add().local_set(j);
        i.br(0).end().end();
        i.i32_const(0).i32_const(0).i32_const(4).local_get(total).call(f_alloc).local_tee(buf).local_set(out);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(p).local_get(endp).i32_ge_u().br_if(1);
        i.local_get(out).local_get(p).i32_load(mem(4)).i32_store(mem(0));
        i.local_get(out).i32_const(4).i32_add();
        i.local_get(p).i32_load(mem(0));
        i.local_get(p).i32_load(mem(4));
        i.memory_copy(0, 0);
        i.local_get(out).i32_const(4).i32_add().local_get(p).i32_load(mem(4)).i32_add().local_set(out);
        i.local_get(p).i32_const(8).i32_add().local_set(p);
        i.br(0).end().end();
        i.local_get(buf).global_set(g_ppos);
        i.local_get(total).global_set(g_plen);
        i.local_get(total).i64_extend_i32_u().return_();
        i.end();
    }

    // Unreachable by construction: shim_fs_call forwards exactly the ops
    // whose imports shipped.
    i.unreachable();
    i.end();
    f
}
