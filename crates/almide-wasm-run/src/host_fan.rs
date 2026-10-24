//! The instance-parallel fan offer on the embedded host (#3003, ADR-0011
//! §D2a). The guest hands over one pure scalar chunk map
//! (`crates/almide-wasm/src/fan_par_lower.rs` documents the request); this
//! host runs the elements on WORKERS — instances of the SAME compiled module,
//! one per OS thread, each with its own linear memory (nothing is shared, no
//! atomics exist) — and writes the results back in list order.
//!
//! Workers are pooled per run: instantiated once, reused by every later
//! offer of the run, from any fan site. So are the threads they run on (the
//! [`Crew`]): spawned by the first offer that needs them, parked between
//! offers, and joined when the run ends — spawning a thread per worker per
//! offer cost about 200 µs of an offer's fixed cost on eight workers. The cost model (#3341, [`plan`] —
//! the native runtime's `almide_rt_fan_plan` formula with this leg's own constants, pinned
//! to docs/benchmarks/fan-cost-model.txt) picks the worker count from the
//! site's last measured compute time per element — a site's first offer
//! goes parallel — capped by the available parallelism and
//! `ALMIDE_FAN_THREADS`; when a parallel offer would cost more than it
//! saves, one worker runs the whole map on the calling thread. Parallel
//! elements are dealt dynamically in BLOCKS of consecutive indices (about
//! four per worker), so a worker slowed by a busy core does less of the work
//! instead of holding the offer back. A block is ONE call into the worker:
//! its elements are built there as a sub-list and the site maps over it
//! (#3331) — a call per element made every worker take wasmtime's
//! engine-wide type lock (`Func::call` → `load_ty`), and the workers
//! serialized on it (1.56 µs per element on one worker, 4.99 µs on eight).
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
use std::sync::{Arc, Mutex, mpsc};

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

/// The fan cost model (#3341) — the SAME formula as the native runtime's
/// `almide_rt_fan_plan` (runtime/rs/src/list.rs), with the wasm leg's own
/// constants (`wasm_*`), pinned to docs/benchmarks/fan-cost-model.txt by
/// tests/fan_cost_model_test.rs. A
/// parallel offer of `n` elements on W workers costs `OFFER + W·WORKER` and
/// saves `t·(n − ⌈n/W⌉)`, `t` the site's last measured time per element;
/// the largest positive net saving wins.
const FAN_OFFER_NS: u128 = 30_000;
const FAN_WORKER_NS: u128 = 5_000;

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
    /// The run module's `fs_call` op set (#4102): a chunk instance is held
    /// to the same ops as the run's own.
    allowed_ops: super::AllowedOps,
    pool: Mutex<Vec<Worker>>,
    crew: Mutex<Crew>,
    /// Per site: the last measured compute time per element (ns).
    history: Mutex<std::collections::HashMap<String, u128>>,
}

impl ParCtx {
    pub(super) fn new(
        (engine, module, linker): (wasmtime::Engine, wasmtime::Module, wasmtime::Linker<Host>),
        max_memory_bytes: Option<usize>,
        ticked: Option<Arc<AtomicBool>>,
        args: Vec<String>,
        allowed_ops: super::AllowedOps,
    ) -> Self {
        ParCtx { engine, module, linker, max_memory_bytes, ticked, args, allowed_ops, pool: Mutex::new(Vec::new()), crew: Mutex::default(), history: Mutex::default() }
    }

    /// The run's compiled module and bounds — what an abort-site child
    /// (`testing.assert_aborts`, ADR-0029) is started from.
    pub(super) fn compiled(&self) -> (&wasmtime::Engine, &wasmtime::Module, Option<usize>, &[String], &super::AllowedOps) {
        (&self.engine, &self.module, self.max_memory_bytes, &self.args, &self.allowed_ops)
    }
}

/// One pooled instance and the handles its reuse check reads.
struct Worker {
    store: wasmtime::Store<Host>,
    instance: wasmtime::Instance,
    memory: wasmtime::Memory,
}

/// What one worker of an offer hands back: the worker, for the pool, and
/// its elements' results — `None` when its chunk failed.
type Part = Option<(Worker, Vec<(usize, Vec<i64>)>)>;

/// One offer's work for one resident thread: the shared deal, the pooled
/// worker it starts with (or none: it instantiates one), and where its part
/// goes.
type Job = (Arc<Deal>, Option<Worker>, mpsc::Sender<Part>);

/// The run's resident fan threads. Each waits on its own channel; an offer
/// sends one [`Job`] to each of the first `w` threads. Dropping the crew (the
/// run's [`ParCtx`] going with its store) closes every channel, and each
/// thread, parked between offers, exits and is joined.
#[derive(Default)]
struct Crew {
    hands: Vec<(mpsc::Sender<Job>, std::thread::JoinHandle<()>)>,
}

impl Crew {
    /// Send every job to its own thread, spawning threads the crew is short
    /// of. A job no thread took is handed back — the offer counts it as a
    /// failed part.
    fn dispatch(&mut self, jobs: Vec<Job>) -> Vec<Job> {
        let mut refused = Vec::new();
        for (i, job) in jobs.into_iter().enumerate() {
            let job = match self.hands.get(i) {
                Some((tx, _)) => match tx.send(job) {
                    Ok(()) => continue,
                    // the thread is gone (it cannot be: a part's panic is
                    // caught); replace it below
                    Err(mpsc::SendError(job)) => job,
                },
                None => job,
            };
            let (tx, rx) = mpsc::channel::<Job>();
            match crate::host::spawn_resident_guest_thread(move || hand(&rx)) {
                Ok(handle) => {
                    let _ = tx.send(job);
                    if i < self.hands.len() {
                        self.hands[i] = (tx, handle);
                    } else {
                        self.hands.push((tx, handle));
                    }
                }
                Err(_) => refused.push(job),
            }
        }
        refused
    }
}

impl Drop for Crew {
    fn drop(&mut self) {
        let me = std::thread::current().id();
        for (tx, handle) in self.hands.drain(..) {
            drop(tx);
            if handle.thread().id() != me {
                let _ = handle.join();
            }
        }
    }
}

/// A resident thread: run each job's deal on its worker and send the part
/// back. The deal is dropped BEFORE the part is sent, so when the offer has
/// every part no thread still holds the run's context.
fn hand(rx: &mpsc::Receiver<Job>) {
    while let Ok((deal, worker, back)) = rx.recv() {
        let part = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| deal.run(worker))).ok().flatten();
        if part.is_none() {
            deal.failed.store(true, Ordering::SeqCst);
        }
        drop(deal);
        let _ = back.send(part);
    }
}

/// The block-form request's first slot (crates/almide-wasm `FAN_MAGIC`).
const FAN_MAGIC: i64 = 0x4641_4e32;

/// The parsed request (`crates/almide-wasm/src/fan_par_lower.rs` documents
/// the layout).
struct Request {
    site: String,
    /// The element count, and each element's width in the list payload.
    n: usize,
    estride: usize,
    /// The result element's kind (3 = a tuple pointer) and width.
    rkind: i64,
    rstride: usize,
    /// `(kind, absolute offset)` per tuple field; empty = a scalar result.
    fields: Vec<(i64, u32)>,
    caps: Vec<i64>,
    /// The list's payload: `n × estride` bytes.
    elems: Vec<u8>,
    /// #3331: `(capture index, payload bytes)` of every list capture — each
    /// worker gets its own copy, and the capture's argument is that copy.
    lists: Vec<(usize, Vec<u8>)>,
}

fn parse(a: &[u8]) -> Option<Request> {
    let slot = |j: usize| -> Option<i64> { Some(i64::from_le_bytes(a.get(8 * j..8 * j + 8)?.try_into().ok()?)) };
    let size = |j: usize| -> Option<usize> { usize::try_from(slot(j)?).ok() };
    if slot(0)? != FAN_MAGIC {
        return None;
    }
    let (k, n, m, r) = (slot(1)?, size(2)?, size(3)?, size(4)?);
    let (estride, rkind, rstride) = (size(6)?, slot(7)?, size(8)?);
    if !matches!(estride, 4 | 8) || !matches!((rkind, rstride), (0 | 1, 8) | (2 | 3, 4)) || (rkind == 3) != (r > 0) {
        return None;
    }
    let fields = (0..r).map(|j| Some((slot(9 + 2 * j)?, u32::try_from(slot(10 + 2 * j)?).ok()?))).collect::<Option<Vec<_>>>()?;
    let pairs = (0..m).map(|j| Some((slot(9 + 2 * r + 2 * j)?, slot(10 + 2 * r + 2 * j)?))).collect::<Option<Vec<_>>>()?;
    // the blobs: the list's payload, then every list capture's, each padded to 8
    let mut at = 8 * (9 + 2 * r + 2 * m);
    let mut blob = |len: usize| -> Option<Vec<u8>> {
        let bytes = a.get(at..at + len)?.to_vec();
        at += len.div_ceil(8) * 8;
        Some(bytes)
    };
    let elems = blob(n.checked_mul(estride)?)?;
    let mut caps = Vec::with_capacity(m);
    let mut lists = Vec::new();
    for (j, (kind, cap)) in pairs.into_iter().enumerate() {
        match kind {
            0 => {}
            1 => lists.push((j, blob(usize::try_from(cap).ok()?)?)),
            _ => return None,
        }
        caps.push(cap);
    }
    (at == a.len()).then_some(Request { site: format!("__fan_site_{k}"), n, estride, rkind, rstride, fields, caps, elems, lists })
}

/// Serve the offer: 1 = the answer room holds every element's result.
pub(super) fn serve(
    caller: &mut wasmtime::Caller<'_, Host>,
    (a_ptr, a_len): (i32, i32),
    (b_ptr, b_len): (i32, i32),
) -> wasmtime::Result<i64> {
    let Some(ctx) = caller.data().par.clone() else { return Ok(0) };
    let mem = super::guest_memory(caller)?;
    let a = super::read_guest(caller, mem, a_ptr, a_len)?;
    let Some(req) = parse(&a) else { return Ok(0) };
    let width = req.fields.len().max(1);
    // the answer room is checked BEFORE the offer runs (#4135): its size is
    // what the result buffer is sized from, so a forged one cannot make the
    // host reserve memory the guest does not have
    let room = 8usize.checked_mul(width).and_then(|w| w.checked_mul(req.n));
    if room != Some(b_len as u32 as usize) {
        return Ok(0);
    }
    if !super::guest_span_fits(caller, mem, b_ptr, b_len as u32 as usize) {
        return Err(wasmtime::Trap::MemoryOutOfBounds.into());
    }
    let site = req.site.clone();
    let n = req.n;
    let Some((results, workers)) = run_all(&ctx, req) else {
        if almide_base::env::flag("ALMIDE_DBG_FAN") {
            eprintln!("[fan-dbg] {site}: not served (a chunk failed), sequential");
        }
        return Ok(0);
    };
    if almide_base::env::flag("ALMIDE_DBG_FAN") {
        if workers >= 2 {
            eprintln!("[fan-dbg] {site}: served on separate instances ({n} elements, {workers} workers)");
        } else {
            eprintln!("[fan-dbg] {site}: served on one instance, the cost model kept it sequential ({n} elements)");
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
    // read once: on Linux it reads the cgroup quota files on every call (the
    // native runtime's `almide_rt_list_par_workers` caches it the same way)
    static CPUS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let cpus = *CPUS.get_or_init(|| std::thread::available_parallelism().map_or(1, |p| p.get()));
    cpus.min(cap.unwrap_or(usize::MAX)).min(n).max(1)
}

/// Every element's result slots, element-major, and the worker count — `None`
/// if any chunk failed.
fn run_all(ctx: &Arc<ParCtx>, req: Request) -> Option<(Vec<i64>, usize)> {
    let n = req.n;
    let width = req.fields.len().max(1);
    let mut out = vec![0i64; n * width];
    if n == 0 {
        return Some((out, 0));
    }
    let estimate = ctx.history.lock().ok().and_then(|h| h.get(&req.site).copied());
    let workers = plan(n, estimate);
    // a few blocks per worker, so a worker slowed by a busy core does less
    let block = if workers < 2 { n } else { n.div_ceil(workers * 4).max(1) };
    let deal = Arc::new(Deal {
        ctx: Arc::clone(ctx),
        req,
        block,
        next: AtomicUsize::new(0),
        failed: AtomicBool::new(false),
        warm: [AtomicU64::new(0), AtomicU64::new(0)],
        cold: [AtomicU64::new(0), AtomicU64::new(0)],
    });
    let mut taken = take(ctx, workers);
    let mut parts: Vec<Part> = Vec::with_capacity(workers);
    if workers < 2 {
        parts.push(deal.run(taken.pop()));
    } else {
        let (back, parts_rx) = mpsc::channel();
        let jobs = (0..workers).map(|_| (Arc::clone(&deal), taken.pop(), back.clone())).collect();
        drop(back);
        // #3435: a fan worker calls into a guest of the same
        // EMBEDDED_WASM_STACK engine, so its thread has the same stack.
        let refused = ctx.crew.lock().map_or_else(|_| Vec::new(), |mut crew| crew.dispatch(jobs));
        if !refused.is_empty() {
            deal.failed.store(true, Ordering::SeqCst);
        }
        drop(refused);
        // every sender is a job's: the iteration ends once each job's thread
        // sent its part (or a job was dropped unrun)
        parts.extend(parts_rx.iter());
        if parts.len() < workers {
            deal.failed.store(true, Ordering::SeqCst);
        }
    }
    let Deal { failed, warm, cold, req, .. } = Arc::into_inner(deal)?;
    let mut ok = !failed.into_inner();
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
        eprintln!("[fan-dbg] {}: {workers} workers, measured {} ns per element (warm {w} ns / {wn}, warm-up {c} ns / {cn})", req.site, ns / k.max(1));
    }
    if ok && k > 0 && let Ok(mut h) = ctx.history.lock() {
        h.insert(req.site, u128::from(ns) / u128::from(k));
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
    let quiet = |m: &Arc<Mutex<Vec<u8>>>| m.lock().is_ok_and(|s| s.is_empty());
    let exited = h.exit.lock().map_or(true, |e| e.is_some());
    !exited && quiet(&h.out) && quiet(&h.err) && w.memory.data_size(&w.store) <= RESET_BYTES
}

/// The shared state of one offer: elements are dealt in blocks of `block`
/// consecutive indices.
struct Deal {
    ctx: Arc<ParCtx>,
    req: Request,
    block: usize,
    next: AtomicUsize,
    failed: AtomicBool,
    /// `[ns, count]` of the elements' compute (thread CPU time), split as
    /// the native runtime's `AlmideFanClock` splits it: a worker's FIRST
    /// block is its warm-up (a fresh thread's and instance's first call —
    /// that cost belongs to the offer's `WORKER` term, not to `t`) and goes
    /// to `cold`; the rest go to `warm`.
    warm: [AtomicU64; 2],
    cold: [AtomicU64; 2],
}

impl Deal {
    /// One thread: its worker (pooled, or instantiated now) takes the next
    /// unclaimed block until none is left or another block failed.
    fn run(&self, pooled: Option<Worker>) -> Part {
        let got = match pooled {
            Some(w) => Some(w),
            None => instantiate(&self.ctx).ok(),
        }
        .and_then(|mut w| {
            let func = self.arm(&mut w)?;
            let caps = self.place_lists(&mut w)?;
            self.drain(w, func, &caps)
        });
        if got.is_none() {
            self.failed.store(true, Ordering::SeqCst);
        }
        got
    }

    /// Ready a worker for this offer: its deadline and the site's export —
    /// `None` when the watchdog already fired or the export does not have
    /// the offer's shape (the list, then the captures; one list back).
    fn arm(&self, w: &mut Worker) -> Option<wasmtime::Func> {
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
        let shape = ty.params().len() == 1 + self.req.caps.len()
            && ty.params().next().is_some_and(|p| matches!(p, wasmtime::ValType::I32))
            && matches!(ty.results().collect::<Vec<_>>().as_slice(), [wasmtime::ValType::I32]);
        shape.then_some(func)
    }

    /// A list block in the worker's memory holding `bytes` as its payload,
    /// allocated by the module's exported allocator, with its rc set to
    /// [`PINNED_RC`] so no release frees it and any write copies it first.
    /// It is never freed: the reset invariant's memory bound drops a worker
    /// whose heap these grew.
    fn place(w: &mut Worker, alloc: &wasmtime::TypedFunc<i32, i32>, bytes: &[u8]) -> Option<i32> {
        let block = alloc.call(&mut w.store, i32::try_from(bytes.len()).ok()?).ok()?;
        let at = block as u32 as usize;
        let data = w.memory.data_mut(&mut w.store);
        let payload = at + almide_layout::PAYLOAD as usize;
        data.get_mut(payload..payload + bytes.len())?.copy_from_slice(bytes);
        let rc = at + almide_layout::RC.offset as usize;
        data.get_mut(rc..rc + 4)?.copy_from_slice(&PINNED_RC.to_le_bytes());
        Some(block)
    }

    fn alloc(w: &mut Worker) -> Option<wasmtime::TypedFunc<i32, i32>> {
        w.instance.get_typed_func::<i32, i32>(&mut w.store, "__alloc").ok()
    }

    /// This worker's capture arguments: the request's, with every list
    /// capture (#3331) replaced by its block in the worker — built once per
    /// offer, read by every block of elements.
    fn place_lists(&self, w: &mut Worker) -> Option<Vec<i64>> {
        let mut caps = self.req.caps.clone();
        if self.req.lists.is_empty() {
            return Some(caps);
        }
        let alloc = Self::alloc(w)?;
        for (j, bytes) in &self.req.lists {
            caps[*j] = i64::from(Self::place(w, &alloc, bytes)? as u32);
        }
        Some(caps)
    }

    /// Elements `lo..hi` on worker `w` in ONE call: their sub-list built in
    /// the worker, the site mapped over it, and each result's slots read
    /// out of the returned list before the worker's next block allocates.
    fn block_call(&self, w: &mut Worker, func: wasmtime::Func, caps: &[i64], (lo, hi): (usize, usize)) -> Option<Vec<Vec<i64>>> {
        let alloc = Self::alloc(w)?;
        let xs = Self::place(w, &alloc, self.req.elems.get(lo * self.req.estride..hi * self.req.estride)?)?;
        let ty = func.ty(&w.store);
        let args: Vec<wasmtime::Val> =
            std::iter::once(wasmtime::Val::I32(xs)).chain(ty.params().skip(1).zip(caps).map(|(p, &b)| to_val(&p, b))).collect();
        let mut ret = [wasmtime::Val::I32(0)];
        func.call(&mut w.store, &args, &mut ret).ok()?;
        if w.store.data().exit.lock().map_or(true, |e| e.is_some()) {
            return None;
        }
        let list = ret[0].i32()? as u32 as usize;
        let data = w.memory.data(&w.store);
        let word = |at: usize| -> Option<u32> { Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?)) };
        let len = word(list + almide_layout::LEN.offset as usize)? as usize;
        if len != (hi - lo) * self.req.rstride {
            return None;
        }
        let payload = list + almide_layout::PAYLOAD as usize;
        (0..hi - lo).map(|e| self.read_result(data, payload + e * self.req.rstride)).collect()
    }

    /// Take unclaimed blocks on `w` until none is left (or a block failed).
    fn drain(&self, mut w: Worker, func: wasmtime::Func, caps: &[i64]) -> Part {
        let mut done = Vec::new();
        let mut first = true;
        loop {
            if self.failed.load(Ordering::SeqCst) {
                return None;
            }
            let lo = self.next.fetch_add(self.block, Ordering::SeqCst);
            if lo >= self.req.n {
                break;
            }
            let hi = (lo + self.block).min(self.req.n);
            let t0 = cpu_ns();
            let vals = self.block_call(&mut w, func, caps, (lo, hi))?;
            add(if first { &self.cold } else { &self.warm }, cpu_ns().wrapping_sub(t0), (hi - lo) as u64);
            first = false;
            done.extend((lo..hi).zip(vals));
        }
        Some((w, done))
    }

    /// One result element's slots, from its slot in the returned list: the
    /// scalar itself, or the tuple's fields read through its pointer.
    fn read_result(&self, data: &[u8], at: usize) -> Option<Vec<i64>> {
        let word = |at: usize| -> Option<i64> { Some(i64::from(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))) };
        let dword = |at: usize| -> Option<i64> { Some(i64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?)) };
        match self.req.rkind {
            0 | 1 => Some(vec![dword(at)?]),
            2 => Some(vec![word(at)?]),
            _ => {
                let base = word(at)? as usize;
                self.req.fields.iter().map(|&(kind, off)| if kind == 2 { word(base + off as usize) } else { dword(base + off as usize) }).collect()
            }
        }
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
        abort_site: -1,
        allowed_ops: ctx.allowed_ops.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(v: &[i64]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// A block-form request over Int elements with a Float result:
    /// `[MAGIC, K, n, m, r=0, ekind, estride, rkind, rstride, (capkind, cap) × m]`
    /// then the blobs given.
    fn request(n: i64, caps: &[(i64, i64)], blobs: &[u8]) -> Vec<u8> {
        let mut a = slots(&[FAN_MAGIC, 7, n, caps.len() as i64, 0, 0, 8, 1, 8]);
        for &(k, c) in caps {
            a.extend(slots(&[k, c]));
        }
        a.extend_from_slice(blobs);
        a
    }

    #[test]
    fn the_elements_and_a_list_capture_ride_as_blobs_padded_to_8() {
        let mut blobs = slots(&[10, 20]); // the two elements' payload
        blobs.extend((1..=12).collect::<Vec<u8>>()); // capture 1: a 12-byte list
        blobs.extend([0u8; 4]);
        let req = parse(&request(2, &[(0, 9), (1, 12)], &blobs)).expect("parses");
        assert_eq!((req.site.as_str(), req.n, req.caps.clone()), ("__fan_site_7", 2, vec![9, 12]));
        assert_eq!(req.elems, slots(&[10, 20]));
        assert_eq!(req.lists, vec![(1, (1..=12).collect::<Vec<u8>>())]);
    }

    #[test]
    fn a_request_that_does_not_account_for_every_byte_is_not_served() {
        assert!(parse(&request(2, &[], &slots(&[10]))).is_none(), "short elements");
        assert!(parse(&request(2, &[], &slots(&[10, 20, 30]))).is_none(), "extra bytes");
        assert!(parse(&request(1, &[(2, 0)], &slots(&[10]))).is_none(), "unknown capture kind");
        let mut stale = request(1, &[], &slots(&[10]));
        stale[0..8].copy_from_slice(&3i64.to_le_bytes());
        assert!(parse(&stale).is_none(), "the per-element form (no magic)");
    }
}
