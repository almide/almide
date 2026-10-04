//! The instance-parallel fan offer on the embedded host (#3003, ADR-0011
//! §D2a). The guest hands over one pure scalar chunk map
//! (`crates/almide-wasm/src/fan_par_lower.rs` documents the request); this
//! host runs the elements on WORKERS — instances of the SAME compiled module,
//! one per OS thread, each with its own linear memory (nothing is shared, no
//! atomics exist) — and writes the results back in list order.
//!
//! Workers are pooled per run: instantiated once, reused by every later
//! offer of the run, from any fan site. The worker count is the available
//! parallelism (capped by `ALMIDE_FAN_THREADS`) and never more than the
//! elements; the elements are dealt dynamically (each worker takes the next
//! unclaimed index), so a worker slowed by a busy core does less of the work
//! instead of holding the offer back.
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

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{Host, StdinSource};

/// `fs_call` op of the offer (crates/almide-wasm `OP_FAN_PAR`).
pub(super) const OP_FAN_PAR: i32 = 74;

/// A worker whose linear memory grew past this is re-instantiated rather
/// than pooled (the reset invariant above).
const RESET_BYTES: usize = 64 << 20;

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
}

impl ParCtx {
    pub(super) fn new(
        (engine, module, linker): (wasmtime::Engine, wasmtime::Module, wasmtime::Linker<Host>),
        max_memory_bytes: Option<usize>,
        ticked: Option<Arc<AtomicBool>>,
        args: Vec<String>,
    ) -> Self {
        ParCtx { engine, module, linker, max_memory_bytes, ticked, args, pool: Mutex::new(Vec::new()) }
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
}

fn parse(a: &[u8]) -> Option<Request> {
    let slot = |j: usize| -> Option<i64> { Some(i64::from_le_bytes(a.get(8 * j..8 * j + 8)?.try_into().ok()?)) };
    let (k, n, m, r) = (slot(0)?, usize::try_from(slot(1)?).ok()?, usize::try_from(slot(2)?).ok()?, usize::try_from(slot(3)?).ok()?);
    if a.len() != 8 * (4 + 2 * r + m + n) {
        return None;
    }
    let fields = (0..r).map(|j| Some((slot(4 + 2 * j)?, u32::try_from(slot(5 + 2 * j)?).ok()?))).collect::<Option<Vec<_>>>()?;
    let caps = (0..m).map(|j| slot(4 + 2 * r + j)).collect::<Option<Vec<_>>>()?;
    let elems = (0..n).map(|j| slot(4 + 2 * r + m + j)).collect::<Option<Vec<_>>>()?;
    Some(Request { site: format!("__fan_site_{k}"), fields, caps, elems })
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
    let Some(results) = run_all(&ctx, &req) else {
        if almide_base::env::flag("ALMIDE_DBG_FAN") {
            eprintln!("[fan-dbg] {}: not served (a chunk failed), sequential", req.site);
        }
        return Ok(0);
    };
    if almide_base::env::flag("ALMIDE_DBG_FAN") {
        eprintln!("[fan-dbg] {}: served on separate instances ({} elements)", req.site, req.elems.len());
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

/// Every element's result slots, element-major — `None` if any chunk failed.
fn run_all(ctx: &ParCtx, req: &Request) -> Option<Vec<i64>> {
    let n = req.elems.len();
    if n == 0 {
        return Some(Vec::new());
    }
    let threads = worker_count(n);
    let mut taken = {
        let mut pool = ctx.pool.lock().ok()?;
        let keep = pool.len().saturating_sub(threads);
        pool.split_off(keep)
    };
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let deal = Deal { ctx, req, next: &next, failed: &failed };
    let parts: Vec<Option<(Worker, Vec<(usize, Vec<i64>)>)>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let w = taken.pop();
                let deal = &deal;
                s.spawn(move || deal.run(w))
            })
            .collect();
        handles.into_iter().map(|h| h.join().ok().flatten()).collect()
    });
    let width = req.fields.len().max(1);
    let mut out = vec![0i64; n * width];
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
    ok.then_some(out)
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
}

impl Deal<'_> {
    /// One thread: its worker (pooled, or instantiated now) takes the next
    /// unclaimed element until none is left or another chunk failed.
    fn run(&self, pooled: Option<Worker>) -> Option<(Worker, Vec<(usize, Vec<i64>)>)> {
        let got = self.work(pooled);
        if got.is_none() {
            self.failed.store(true, Ordering::SeqCst);
        }
        got
    }

    fn work(&self, pooled: Option<Worker>) -> Option<(Worker, Vec<(usize, Vec<i64>)>)> {
        let mut w = match pooled {
            Some(w) => w,
            None => instantiate(self.ctx).ok()?,
        };
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
        if params.len() != 1 + self.req.caps.len() || ty.results().len() != 1 {
            return None;
        }
        let mut done = Vec::new();
        loop {
            if self.failed.load(Ordering::SeqCst) {
                return None;
            }
            let i = self.next.fetch_add(1, Ordering::SeqCst);
            let Some(&x) = self.req.elems.get(i) else { break };
            let bits = std::iter::once(x).chain(self.req.caps.iter().copied());
            let args: Vec<wasmtime::Val> = params.iter().zip(bits).map(|(p, b)| to_val(p, b)).collect();
            let mut ret = [wasmtime::Val::I32(0)];
            func.call(&mut w.store, &args, &mut ret).ok()?;
            if w.store.data().exit.lock().map_or(true, |e| e.is_some()) {
                return None;
            }
            done.push((i, self.read_result(&w, &ret[0])?));
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
