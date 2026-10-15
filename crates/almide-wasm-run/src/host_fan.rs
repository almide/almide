//! The instance-parallel fan offer on the embedded host (#3003, ADR-0011
//! §D2a). The guest hands over one pure scalar chunk map
//! (`crates/almide-wasm/src/fan_par_lower.rs` documents the request); this
//! host runs the elements on WORKERS — instances of the SAME compiled module,
//! one per OS thread, each with its own linear memory (nothing is shared, no
//! atomics exist) — and writes the results back in list order.
//!
//! Workers are pooled per run: instantiated once, reused by every later
//! offer of the run, from any fan site. The cost model (#3341, [`plan`] —
//! the native runtime's `almide_rt_fan_plan` with the same constants, pinned
//! to docs/benchmarks/fan-cost-model.txt) picks the worker count from the
//! site's last measured compute time per element — a site's first offer
//! goes parallel — capped by the available parallelism and
//! `ALMIDE_FAN_THREADS`; when a parallel offer would cost more than it
//! saves, one worker runs the whole map on the calling thread. Parallel
//! elements are dealt dynamically (each worker takes the next unclaimed
//! index), so a worker slowed by a busy core does less of the work instead
//! of holding the offer back.
//!
//! **The reset invariant.** A pooled worker carries ONLY its heap from one
//! chunk to the next — the allocator state and whatever blocks earlier
//! chunks left behind. That is unobservable because a chunk qualifies only
//! when it is pure (`crates/almide-wasm/src/fan_par.rs`): it reads no global
//! and no top-level let, performs no effect, and returns scalars — so its
//! result is a function of its arguments alone, whatever the heap holds. The
//! sequential run relies on the same fact: there every chunk runs in one
//! instance on top of `main`'s own allocations. A result is copied out
//! (tuple fields read) before the worker takes its next chunk, so no later
//! allocation can overwrite it. A worker is DROPPED, never reused, when
//! - its chunk trapped, aborted or exited (a trap can stop the allocator
//!   mid-update; the exit or abort text sits in its capture), or
//! - its capture is non-empty, or its memory grew past [`RESET_BYTES`] —
//!   leaked heap must not accumulate toward the run's memory cap across
//!   offers; the next offer instantiates a fresh worker (30-150 µs).
//!
//! Every failure answers 0 ("not served") and the guest runs the map
//! sequentially: a chunk that traps, aborts or exits, an instantiation
//! error, a malformed request. The chunk is pure, so whatever a worker did
//! is unobservable — its stdout, stderr and exit are captured in its own
//! `Host` and dropped with it — and the sequential run then produces exactly
//! the observation it always did.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{Host, StdinSource};

/// `fs_call` op of the offer (crates/almide-wasm `OP_FAN_PAR`).
pub(super) const OP_FAN_PAR: i32 = 74;

/// A worker whose linear memory grew past this is re-instantiated rather
/// than pooled (the reset invariant above).
const RESET_BYTES: usize = 64 << 20;

/// The rc a list capture's block is built with (#3331): far above any count
/// the chunks' retains and releases can reach, so the block is never freed
/// and never unique — a chunk that writes to it copies it first, exactly as
/// it would a list the caller still holds.
const PINNED_RC: u32 = 1 << 30;

/// The fan cost model (#3341) — the SAME formula and constants as the native
/// runtime's `almide_rt_fan_plan` (runtime/rs/src/list.rs), pinned to
/// docs/benchmarks/fan-cost-model.txt by tests/fan_cost_model_test.rs. A
/// parallel offer of `n` elements on W workers costs `OFFER + W·WORKER` and
/// saves `t·(n − ⌈n/W⌉)`, `t` the site's last measured time per element;
/// the largest positive net saving wins.
const FAN_OFFER_NS: u128 = 30_000;
const FAN_WORKER_NS: u128 = 9_000;

/// The calling thread's CPU time in ns — the clock an offer's elements are
/// measured on, the native runtime's `almide_fan_cpu_ns` (list.rs). Wall
/// time would also count the time a worker sat descheduled: on a loaded
/// machine ONE preempted call of a trivial body read 4.6 ms and kept its
/// site parallel. Elsewhere: a monotonic wall clock.
#[cfg(unix)]
fn cpu_ns() -> u64 {
    let ts = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    (ts.tv_sec as u64).wrapping_mul(1_000_000_000).wrapping_add(ts.tv_nsec as u64)
}
#[cfg(not(unix))]
fn cpu_ns() -> u64 {
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    T0.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64
}

/// The worker count for an offer of `n` elements whose site last measured
/// `per_elem` ns per element (`None` = its first offer): >= 2 = parallel on
/// that many workers, 1 = stay sequential.
fn plan(n: usize, per_elem: Option<u128>) -> usize {
    let off = almide_base::env::flag("ALMIDE_FAN_COST_OFF");
    let Some(t) = per_elem.filter(|_| !off) else { return worker_count(n) };
    let mut best = (1usize, 0u128);
    for w in 2..=worker_count(n) {
        let saved = t.saturating_mul((n - n.div_ceil(w)) as u128);
        let cost = FAN_OFFER_NS + FAN_WORKER_NS * w as u128;
        if saved > cost && saved - cost > best.1 {
            best = (w, saved - cost);
        }
    }
    best.0
}

/// What a worker needs: the run's engine, compiled module and import set,
/// the run's own resource bounds — and the pooled workers.
pub(super) struct ParCtx {
    engine: wasmtime::Engine,
    module: wasmtime::Module,
    linker: wasmtime::Linker<Host>,
    max_memory_bytes: Option<usize>,
    /// The run has a watchdog: set once its epoch tick fired. A worker
    /// traps on the same tick; after it, nothing is offered (the guest's
    /// sequential run meets its own deadline).
    ticked: Option<Arc<AtomicBool>>,
    args: Vec<String>,
    pool: Mutex<Vec<Worker>>,
    /// Per site: the last measured compute time per element (ns).
    history: Mutex<std::collections::HashMap<String, u128>>,
}

impl ParCtx {
    pub(super) fn new(
        (engine, module, linker): (wasmtime::Engine, wasmtime::Module, wasmtime::Linker<Host>),
        max_memory_bytes: Option<usize>,
        ticked: Option<Arc<AtomicBool>>,
        args: Vec<String>,
    ) -> Self {
        ParCtx { engine, module, linker, max_memory_bytes, ticked, args, pool: Mutex::new(Vec::new()), history: Mutex::default() }
    }
}

/// One pooled instance and the handles its reuse check reads.
struct Worker {
    store: wasmtime::Store<Host>,
    instance: wasmtime::Instance,
    memory: wasmtime::Memory,
}

/// The parsed request.
struct Request {
    site: String,
    /// `(kind, absolute offset)` per tuple field; empty = a scalar result.
    fields: Vec<(i64, u32)>,
    caps: Vec<i64>,
    elems: Vec<i64>,
    /// #3331: `(capture index, payload bytes)` of every list capture — each
    /// worker gets its own copy, and the capture's argument is that copy.
    lists: Vec<(usize, Vec<u8>)>,
}

fn parse(a: &[u8]) -> Option<Request> {
    let slot = |j: usize| -> Option<i64> { Some(i64::from_le_bytes(a.get(8 * j..8 * j + 8)?.try_into().ok()?)) };
    let (k, n, m, r) = (slot(0)?, usize::try_from(slot(1)?).ok()?, usize::try_from(slot(2)?).ok()?, usize::try_from(slot(3)?).ok()?);
    let base = 8 * (4 + 2 * r + m + n);
    if a.len() < base {
        return None;
    }
    let fields = (0..r).map(|j| Some((slot(4 + 2 * j)?, u32::try_from(slot(5 + 2 * j)?).ok()?))).collect::<Option<Vec<_>>>()?;
    let caps = (0..m).map(|j| slot(4 + 2 * r + j)).collect::<Option<Vec<_>>>()?;
    let elems = (0..n).map(|j| slot(4 + 2 * r + m + j)).collect::<Option<Vec<_>>>()?;
    let lists = if a.len() == base { Vec::new() } else { parse_tail(a, base, &caps)? };
    Some(Request { site: format!("__fan_site_{k}"), fields, caps, elems, lists })
}

/// The tail after the elements (`fan_par_lower.rs`): one kind per capture,
/// then each list's payload padded to 8. It must account for every byte.
fn parse_tail(a: &[u8], base: usize, caps: &[i64]) -> Option<Vec<(usize, Vec<u8>)>> {
    let mut at = base + 8 * caps.len();
    let mut lists = Vec::new();
    for (j, &cap) in caps.iter().enumerate() {
        let kind = i64::from_le_bytes(a.get(base + 8 * j..base + 8 * j + 8)?.try_into().ok()?);
        match kind {
            0 => {}
            1 => {
                let len = usize::try_from(cap).ok()?;
                lists.push((j, a.get(at..at + len)?.to_vec()));
                at += len.div_ceil(8) * 8;
            }
            _ => return None,
        }
    }
    (at == a.len() && !lists.is_empty()).then_some(lists)
}

/// Serve the offer: 1 = the answer room holds every element's result.
pub(super) fn serve(
    caller: &mut wasmtime::Caller<'_, Host>,
    (a_ptr, a_len): (i32, i32),
    (b_ptr, b_len): (i32, i32),
) -> wasmtime::Result<i64> {
    let Some(ctx) = caller.data().par.clone() else { return Ok(0) };
    let mem = caller.get_export("memory").and_then(|e| e.into_memory()).expect("exported memory");
    let mut a = vec![0u8; a_len as u32 as usize];
    mem.read(&*caller, a_ptr as u32 as usize, &mut a)?;
    let Some(req) = parse(&a) else { return Ok(0) };
    let width = req.fields.len().max(1);
    if b_len as u32 as usize != 8 * width * req.elems.len() {
        return Ok(0);
    }
    let Some((results, workers)) = run_all(&ctx, &req) else {
        if almide_base::env::flag("ALMIDE_DBG_FAN") {
            eprintln!("[fan-dbg] {}: not served (a chunk failed), sequential", req.site);
        }
        return Ok(0);
    };
    if almide_base::env::flag("ALMIDE_DBG_FAN") {
        if workers >= 2 {
            eprintln!("[fan-dbg] {}: served on separate instances ({} elements, {workers} workers)", req.site, req.elems.len());
        } else {
            eprintln!("[fan-dbg] {}: served on one instance, the cost model kept it sequential ({} elements)", req.site, req.elems.len());
        }
    }
    let mut b = Vec::with_capacity(b_len as u32 as usize);
    for v in results {
        b.extend_from_slice(&v.to_le_bytes());
    }
    mem.write(&mut *caller, b_ptr as u32 as usize, &b)?;
    Ok(1)
}

/// The worker count for `n` elements: the available parallelism, capped by
/// `ALMIDE_FAN_THREADS` (the native runtime reads the same switch), never
/// more than `n`.
fn worker_count(n: usize) -> usize {
    let cap = almide_base::env::var("ALMIDE_FAN_THREADS").and_then(|v| v.parse::<usize>().ok()).filter(|&c| c > 0);
    let cpus = std::thread::available_parallelism().map_or(1, |p| p.get());
    cpus.min(cap.unwrap_or(usize::MAX)).min(n).max(1)
}

/// Every element's result slots, element-major, and the worker count — `None`
/// if any chunk failed.
fn run_all(ctx: &ParCtx, req: &Request) -> Option<(Vec<i64>, usize)> {
    let n = req.elems.len();
    let width = req.fields.len().max(1);
    let mut out = vec![0i64; n * width];
    if n == 0 {
        return Some((out, 0));
    }
    let estimate = ctx.history.lock().ok().and_then(|h| h.get(&req.site).copied());
    let workers = plan(n, estimate);
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let (warm, cold) = ([AtomicU64::new(0), AtomicU64::new(0)], [AtomicU64::new(0), AtomicU64::new(0)]);
    let deal = Deal { ctx, req, next: &next, failed: &failed, warm: &warm, cold: &cold };
    let mut taken = take(ctx, workers);
    let parts: Vec<Option<(Worker, Vec<(usize, Vec<i64>)>)>> = if workers < 2 {
        vec![deal.run(taken.pop())]
    } else {
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..workers)
                .map(|_| {
                    let w = taken.pop();
                    let deal = &deal;
                    // #3435: a fan worker calls into a guest of the same
                    // EMBEDDED_WASM_STACK engine, so it needs the same stack.
                    crate::host::spawn_guest_thread(s, move || deal.run(w))
                })
                .collect();
            handles.into_iter().map(|h| h.join().ok().flatten()).collect()
        })
    };
    let mut ok = !failed.load(Ordering::SeqCst);
    let mut back = Vec::new();
    for part in parts {
        let Some((w, done)) = part else {
            ok = false;
            continue;
        };
        for (i, vals) in done {
            out[i * width..i * width + width].copy_from_slice(&vals);
        }
        if reusable(&w) {
            back.push(w);
        }
    }
    if let Ok(mut pool) = ctx.pool.lock() {
        pool.extend(back);
    }
    // The warm elements' mean; the warm-ups' when no worker ran a second one.
    let [w, wn] = warm.map(AtomicU64::into_inner);
    let [c, cn] = cold.map(AtomicU64::into_inner);
    let (ns, k) = if wn > 0 { (w, wn) } else { (c, cn) };
    if almide_base::env::flag("ALMIDE_DBG_FAN") {
        eprintln!("[fan-dbg] {}: measured {} ns per element (warm {w} ns / {wn}, warm-up {c} ns / {cn})", req.site, ns / k.max(1));
    }
    if ok && k > 0 && let Ok(mut h) = ctx.history.lock() {
        h.insert(req.site.clone(), u128::from(ns) / u128::from(k));
    }
    ok.then_some((out, workers))
}

/// Up to `k` pooled workers (fewer when the pool is short).
fn take(ctx: &ParCtx, k: usize) -> Vec<Worker> {
    let Ok(mut pool) = ctx.pool.lock() else { return Vec::new() };
    let keep = pool.len().saturating_sub(k);
    pool.split_off(keep)
}

/// A finished worker goes back to the pool only when nothing of its run is
/// left that a later chunk could notice (the reset invariant).
fn reusable(w: &Worker) -> bool {
    let h = w.store.data();
    let quiet = |m: &Arc<Mutex<String>>| m.lock().is_ok_and(|s| s.is_empty());
    let exited = h.exit.lock().map_or(true, |e| e.is_some());
    !exited && quiet(&h.out) && quiet(&h.err) && w.memory.data_size(&w.store) <= RESET_BYTES
}

/// The shared state of one offer: elements are dealt by index.
struct Deal<'a> {
    ctx: &'a ParCtx,
    req: &'a Request,
    next: &'a AtomicUsize,
    failed: &'a AtomicBool,
    /// `[ns, count]` of the elements' compute (thread CPU time), split as
    /// the native runtime's `AlmideFanClock` splits it: a worker's FIRST
    /// element is its warm-up (a fresh thread's and instance's first call —
    /// that cost belongs to the offer's `WORKER` term, not to `t`) and goes
    /// to `cold`; the rest go to `warm`.
    warm: &'a [AtomicU64; 2],
    cold: &'a [AtomicU64; 2],
}

impl Deal<'_> {
    /// One thread: its worker (pooled, or instantiated now) takes the next
    /// unclaimed element until none is left or another chunk failed.
    fn run(&self, pooled: Option<Worker>) -> Option<(Worker, Vec<(usize, Vec<i64>)>)> {
        let got = match pooled {
            Some(w) => Some(w),
            None => instantiate(self.ctx).ok(),
        }
        .and_then(|mut w| {
            let (func, params) = self.arm(&mut w)?;
            let caps = self.place_lists(&mut w)?;
            self.drain(w, func, &params, &caps)
        });
        if got.is_none() {
            self.failed.store(true, Ordering::SeqCst);
        }
        got
    }

    /// Ready a worker for this offer: its deadline, the site's export and
    /// its parameter types — `None` when the watchdog already fired or the
    /// export does not have the offer's shape.
    fn arm(&self, w: &mut Worker) -> Option<(wasmtime::Func, Vec<wasmtime::ValType>)> {
        if let Some(t) = &self.ctx.ticked {
            // deadline first, then the check: a tick after the check still
            // lands on this deadline
            w.store.set_epoch_deadline(1);
            if t.load(Ordering::SeqCst) {
                return None;
            }
        }
        let func = w.instance.get_func(&mut w.store, &self.req.site)?;
        let ty = func.ty(&w.store);
        let params: Vec<wasmtime::ValType> = ty.params().collect();
        (params.len() == 1 + self.req.caps.len() && ty.results().len() == 1).then_some((func, params))
    }

    /// This worker's capture arguments: the request's, with every list
    /// capture (#3331) replaced by a block built in the worker's own memory —
    /// allocated by the module's exported allocator, the payload copied in,
    /// and its rc set to [`PINNED_RC`] so no chunk's release ever frees it
    /// and any write to it copies first. It is never freed: the reset
    /// invariant's memory bound drops a worker whose heap these grew.
    fn place_lists(&self, w: &mut Worker) -> Option<Vec<i64>> {
        let mut caps = self.req.caps.clone();
        if self.req.lists.is_empty() {
            return Some(caps);
        }
        let alloc = w.instance.get_typed_func::<i32, i32>(&mut w.store, "__alloc").ok()?;
        for (j, bytes) in &self.req.lists {
            let block = alloc.call(&mut w.store, i32::try_from(bytes.len()).ok()?).ok()? as u32 as usize;
            let data = w.memory.data_mut(&mut w.store);
            let payload = block + almide_layout::PAYLOAD as usize;
            data.get_mut(payload..payload + bytes.len())?.copy_from_slice(bytes);
            let rc = block + almide_layout::RC.offset as usize;
            data.get_mut(rc..rc + 4)?.copy_from_slice(&PINNED_RC.to_le_bytes());
            caps[*j] = block as i64;
        }
        Some(caps)
    }

    /// Element `i` on worker `w`: its result slots, read out before the
    /// worker's next chunk allocates.
    fn one(&self, w: &mut Worker, (func, params): (wasmtime::Func, &[wasmtime::ValType]), caps: &[i64], i: usize) -> Option<Vec<i64>> {
        let bits = std::iter::once(self.req.elems[i]).chain(caps.iter().copied());
        let args: Vec<wasmtime::Val> = params.iter().zip(bits).map(|(p, b)| to_val(p, b)).collect();
        let mut ret = [wasmtime::Val::I32(0)];
        func.call(&mut w.store, &args, &mut ret).ok()?;
        if w.store.data().exit.lock().map_or(true, |e| e.is_some()) {
            return None;
        }
        self.read_result(w, &ret[0])
    }

    /// Take unclaimed elements on `w` until none is left (or a chunk failed).
    fn drain(&self, mut w: Worker, func: wasmtime::Func, params: &[wasmtime::ValType], caps: &[i64]) -> Option<(Worker, Vec<(usize, Vec<i64>)>)> {
        let mut done = Vec::new();
        let mut t0 = cpu_ns();
        loop {
            if self.failed.load(Ordering::SeqCst) {
                return None;
            }
            let i = self.next.fetch_add(1, Ordering::SeqCst);
            if i >= self.req.elems.len() {
                break;
            }
            let vals = self.one(&mut w, (func, params), caps, i)?;
            done.push((i, vals));
            if done.len() == 1 {
                let t1 = cpu_ns();
                add(self.cold, t1.wrapping_sub(t0), 1);
                t0 = t1;
            }
        }
        if done.len() > 1 {
            add(self.warm, cpu_ns().wrapping_sub(t0), done.len() as u64 - 1);
        }
        Some((w, done))
    }

    /// The element's result slots: the scalar itself, or the tuple's fields
    /// read out of the worker's memory now — before its next chunk allocates.
    fn read_result(&self, w: &Worker, ret: &wasmtime::Val) -> Option<Vec<i64>> {
        if self.req.fields.is_empty() {
            return from_val(ret).map(|v| vec![v]);
        }
        let base = ret.i32()? as u32 as usize;
        let data = w.memory.data(&w.store);
        self.req
            .fields
            .iter()
            .map(|&(kind, off)| {
                let at = base + off as usize;
                if kind == 2 {
                    Some(i64::from(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?)))
                } else {
                    Some(i64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?))
                }
            })
            .collect()
    }
}

/// Add `ns` over `n` elements to a `[ns, count]` cell.
fn add(cell: &[AtomicU64; 2], ns: u64, n: u64) {
    cell[0].fetch_add(ns, Ordering::Relaxed);
    cell[1].fetch_add(n, Ordering::Relaxed);
}

/// A fresh worker: a new store with the run's bounds and its own captures.
fn instantiate(ctx: &ParCtx) -> anyhow::Result<Worker> {
    let limits = match ctx.max_memory_bytes {
        Some(cap) => wasmtime::StoreLimitsBuilder::new().memory_size(cap).build(),
        None => wasmtime::StoreLimits::default(),
    };
    let host = Host {
        out: Arc::default(),
        err: Arc::default(),
        exit: Arc::new(Mutex::new(None)),
        fs_buf: Arc::default(),
        stdin: Arc::new(Mutex::new(StdinSource::Buf(Vec::new()))),
        args: ctx.args.clone(),
        limits,
        calls: Arc::default(),
        serve: Arc::default(),
        net: Arc::default(),
        live_out: None,
        err_last: Arc::default(),
        par: None,
    };
    let mut store = wasmtime::Store::new(&ctx.engine, host);
    store.limiter(|h| &mut h.limits);
    let instance = ctx.linker.instantiate(&mut store, &ctx.module)?;
    let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| anyhow::anyhow!("no memory"))?;
    Ok(Worker { store, instance, memory })
}

fn to_val(ty: &wasmtime::ValType, bits: i64) -> wasmtime::Val {
    match ty {
        wasmtime::ValType::I64 => wasmtime::Val::I64(bits),
        wasmtime::ValType::F64 => wasmtime::Val::F64(bits as u64),
        _ => wasmtime::Val::I32(bits as i32),
    }
}

fn from_val(v: &wasmtime::Val) -> Option<i64> {
    match v {
        wasmtime::Val::I64(x) => Some(*x),
        wasmtime::Val::F64(x) => Some(*x as i64),
        wasmtime::Val::I32(x) => Some(i64::from(*x as u32)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(v: &[i64]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// `[K, n, m, r, cap × m, elem × n]`, then the tail when given.
    fn request(caps: &[i64], elems: &[i64], tail: &[u8]) -> Vec<u8> {
        let mut a = slots(&[7, elems.len() as i64, caps.len() as i64, 0]);
        a.extend(slots(caps));
        a.extend(slots(elems));
        a.extend_from_slice(tail);
        a
    }

    #[test]
    fn a_scalar_request_has_no_tail() {
        let req = parse(&request(&[5], &[1, 2], &[])).expect("parses");
        assert_eq!((req.site.as_str(), req.caps, req.elems), ("__fan_site_7", vec![5], vec![1, 2]));
        assert!(req.lists.is_empty());
    }

    #[test]
    fn a_list_capture_rides_in_the_tail_padded_to_8() {
        // capture 0 a scalar, capture 1 a 12-byte list (padded to 16)
        let mut tail = slots(&[0, 1]);
        tail.extend((1..=12).collect::<Vec<u8>>());
        tail.extend([0u8; 4]);
        let req = parse(&request(&[9, 12], &[3], &tail)).expect("parses");
        assert_eq!(req.lists, vec![(1, (1..=12).collect::<Vec<u8>>())]);
    }

    #[test]
    fn a_tail_that_does_not_account_for_every_byte_is_not_served() {
        let short = parse(&request(&[12], &[3], &slots(&[1]))); // kind says 12 bytes, none follow
        assert!(short.is_none());
        let mut extra = slots(&[1]);
        extra.extend([0u8; 24]); // 12 bytes padded to 16, plus 8 more
        assert!(parse(&request(&[12], &[3], &extra)).is_none());
        assert!(parse(&request(&[12], &[3], &slots(&[2]))).is_none(), "unknown kind");
        assert!(parse(&request(&[4], &[3], &slots(&[0]))).is_none(), "a tail with no list is malformed");
    }
}
