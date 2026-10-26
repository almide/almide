//! The greenfield wasm HOST — the `almide.*` import surface
//! (println/eprintln/exit/fs_call/host_read) over wasmtime. ONE
//! implementation serves both the product runner (src/main.rs) and the
//! almide-wasm test harness (which delegates here), so the host the
//! gates verify IS the host that ships. Every module passes the
//! wasmparser wall before instantiation; `almide.exit` records the
//! process exit code and unwinds — ABORT parity (exit code +
//! stdout-before-abort, the C-153 family) is a first-class observable
//! of every run, never an opaque Err.

use std::sync::{Arc, Mutex};

#[path = "host_fan.rs"]
mod fan;

#[path = "host_budget.rs"]
mod budget;

/// One wasm run's cross-target observables: stdout, stderr, exit code.
/// A trap WITHOUT a recorded `almide.exit` code is a runtime abort
/// (unreachable / div-by-zero / OOB) — exit 1, the native abort contract,
/// and (#1826) ONE stderr line naming it, `Error: wasm trap: <reason>`,
/// never a silent exit 1. Not every gate reads every field (the run
/// manifest hashes stdout only).
#[allow(dead_code)]
pub struct RunResult {
    /// The captured stdout as text — [`RunResult::stdout_bytes`] decoded
    /// lossily, for the harness gates that compare text.
    pub stdout: String,
    pub stderr: String,
    /// The captured stdout EXACTLY as the guest wrote it (C-399): a byte
    /// writer's non-UTF-8 bytes are kept here, never replaced.
    pub stdout_bytes: Vec<u8>,
    pub stderr_bytes: Vec<u8>,
    pub exit: i32,
    /// The bump-heap watermark (exported `__heap` global) after the run:
    /// total bytes the module ever allocated plus the fixed base — the
    /// allocation-ledger observable (#1586). None if the module predates
    /// the export.
    pub heap_end: Option<u64>,
    /// The allocation counters (#2407), read from the `__alloc_count` /
    /// `__alloc_reused` / `__alloc_bytes` / `__free_count` /
    /// `__region_reclaimed` globals a module
    /// emitted under the `ALMIDE_WASM_ALLOC_COUNT` switch carries. None for
    /// a shipped (unarmed) module — the counters are absent, not zero.
    pub alloc_count: Option<AllocCount>,
    /// Wall time of the `main` call alone, in seconds (#2980): entering the
    /// guest's `main` to its return (or trap) — no validation, compile,
    /// instantiate or linking. The steady-state boundary `almide bench`
    /// draws on both legs (the native leg times its own `main` in-process).
    pub main_secs: f64,
}

/// What the structural leg's allocator did during one run (#2407): the
/// churn the `__heap` watermark cannot show. `allocs` is every allocation
/// (a `$alloc` call or a fixed-size constructor's inlined bump, #2318),
/// `reused` the ones a size-class free-list pop served (the rest bumped
/// the heap), `bytes` the payload bytes requested in total, `frees`
/// every `$free` call, and `reclaimed` the blocks a region window's restore
/// took back wholesale without a `$free` (#1961).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocCount {
    pub allocs: u64,
    pub reused: u64,
    pub bytes: u64,
    pub frees: u64,
    pub reclaimed: u64,
}

impl AllocCount {
    /// The heap blocks still live when the run ended: `allocs − frees −
    /// reclaimed`. An armed `main` releases its top-let globals before it
    /// returns (almide_wasm::alloc_count), so after a normal exit a
    /// non-zero value is a leak; negative would be a double release.
    pub fn live(&self) -> i64 {
        self.allocs as i64 - self.frees as i64 - self.reclaimed as i64
    }
}

impl std::fmt::Display for AllocCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "allocs={} reused={} bytes={} frees={} reclaimed={} live={}",
            self.allocs,
            self.reused,
            self.bytes,
            self.frees,
            self.reclaimed,
            self.live()
        )
    }
}

struct Host {
    /// The captured streams (C-399): raw bytes, as the guest wrote them.
    out: Arc<Mutex<Vec<u8>>>,
    err: Arc<Mutex<Vec<u8>>>,
    exit: Arc<Mutex<Option<i32>>>,
    /// The fs result parking buffer (host_read copies it to the guest).
    fs_buf: Arc<Mutex<Vec<u8>>>,
    /// The stdin stream (op 35 takes chunks off its cursor); tests run
    /// with a fixed buffer, the runner reads lazily.
    stdin: Arc<Mutex<StdinSource>>,
    /// Program args for op 29 (#1716): framed as [argv0, args...] — the
    /// guest's args arm skips the first frame, matching native argv[1..].
    args: Vec<String>,
    /// This store's share of the run's ONE linear-memory budget (#4135):
    /// every store of a run — its own, its fan workers', an abort-site
    /// child's — meters against the same cap (`host_budget.rs`).
    meter: budget::Meter,
    /// The run's live http calls (#2633) — cancelled when the run ends.
    calls: Arc<Mutex<crate::http_call_host::HttpCalls>>,
    /// `http.serve`'s listener and pending connection (ops 70..=72, #2650).
    serve: Arc<Mutex<crate::host_serve::ServeState>>,
    /// The run's tcp handles and unix / shm descriptors (ops 91..=116,
    /// #3620) — closed when the run ends.
    net: Arc<Mutex<crate::net_host::NetRun>>,
    /// The product runner's LIVE streams (#2650): stdout through a 64 KiB
    /// buffer flushed at every line end, and per write on a terminal —
    /// native's rule (#3417) — and stderr
    /// straight through, so a program that never returns (a server) shows
    /// its output as it runs. None = the buffered harness capture.
    live_out: Option<Arc<Mutex<std::io::BufWriter<std::io::Stdout>>>>,
    /// The last stderr line written, in either mode: the die convention
    /// (#1912) reads it after a trap.
    err_last: Arc<Mutex<String>>,
    /// The instance-parallel fan context (#3003): set on a run's own store,
    /// `None` on a chunk's — a chunk's nested offer runs sequentially.
    par: Option<Arc<fan::ParCtx>>,
    /// The `testing.assert_aborts` site this instance was started to run
    /// (op 118, ADR-0029), or -1: the run's own instance runs its tests.
    abort_site: i64,
    /// The `fs_call` ops this module was built with (#4102): the constant op
    /// of every call site. The in-process module imports the whole gateway
    /// whatever its effects; an op outside this set traps, so a miscompile
    /// cannot steer the gateway into a capability the program never had. A
    /// site whose op is not a visible constant adds nothing: the list fails
    /// closed there (row 19), so such a site reaches only an op another site
    /// names. There is no module without a list.
    allowed_ops: AllowedOps,
    /// The contained serve loop's stderr hold (#4203, op 75).
    contain: Arc<Mutex<Contain>>,
}

/// While op 75 runs the guest's serve loop, an abort's own stderr write —
/// the die line `Error: <msg>` (or `PANIC: <msg>`) before its `unreachable`,
/// or `panic`'s raw `PANIC: <msg>` before its exit 1 — must not reach stderr
/// when the abort is contained: native prints only the `handler aborted`
/// line then. So a write that could be such a line is HELD, and released by
/// the next host call or the loop's return; it is consumed only when the
/// trap that follows it is that abort.
#[derive(Default)]
struct Contain {
    active: bool,
    /// The held write and whether it was a line (eprintln) or raw (op 73).
    held: Option<(Vec<u8>, bool)>,
    /// `process.exit` sent its mark (op 76): the call ends by a deliberate
    /// exit, which is never contained.
    exit_marked: bool,
    /// A linear-memory growth of this store was refused while the call ran
    /// (the store limiter below): the call is running out of memory, which
    /// is never contained (row 1 of the security table, C-434). The guest's
    /// allocator answers the refused `memory.grow` with C-197's abort, so the
    /// kind is read from the refusal itself, never from the abort's text.
    oom: bool,
}

/// `process.exit`'s mark (crates/almide-wasm fs_meta.rs `OP_EXIT_MARK`).
const OP_EXIT_MARK: i32 = 76;

enum Handles {
    Begin,
    End,
    Abort,
}

/// The handles an `http.serve` handler opens (#4203): recorded from the
/// request's hand-out to its reply, and closed if its handler aborts — the
/// tcp and unix tables and the http calls.
fn request_handles(host: &Host, h: Handles) -> usize {
    let mut net = host.net.lock().expect("net");
    let mut calls = host.calls.lock().expect("calls");
    match h {
        Handles::Begin => {
            net.request_begin();
            calls.request_begin();
            0
        }
        Handles::End => {
            net.request_end();
            calls.request_end();
            0
        }
        Handles::Abort => net.request_abort() + calls.request_abort(),
    }
}

/// See [`Host::allowed_ops`].
pub(crate) type AllowedOps = Arc<std::collections::BTreeSet<i32>>;

/// The `fs_call` op set of `bytes` (#4102), shared by every instance of it:
/// the visible constants only, so a computed op fails closed (row 19).
fn allowed_ops_of(bytes: &[u8]) -> anyhow::Result<AllowedOps> {
    let ops = almide_wasm::cert_project::fs_call_ops_visible(bytes).map_err(anyhow::Error::msg)?;
    Ok(Arc::new(ops))
}

/// Refuse an `fs_call` op the module was not built with (#4102).
fn op_allowed(host: &Host, op: i32) -> wasmtime::Result<()> {
    if host.allowed_ops.contains(&op) {
        return Ok(());
    }
    Err(wasmtime::Error::msg(format!("almide.fs_call: op {op} is not one this module was built with")))
}

/// Record that a growth of `host`'s memory was refused (see [`Contain::oom`]).
fn mark_oom(host: &Host) {
    host.contain.lock().expect("contain").oom = true;
}

/// Is the process's stdout a terminal (native flushes per write there).
fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdout().is_terminal()
}

/// The store limiter of every store of a run: linear-memory growth is
/// metered against the run's shared budget (#4135); tables, instance and
/// memory counts keep wasmtime's defaults, as `StoreLimits` did.
impl wasmtime::ResourceLimiter for Host {
    fn memory_growing(&mut self, current: usize, desired: usize, maximum: Option<usize>) -> wasmtime::Result<bool> {
        if maximum.is_some_and(|m| desired > m) {
            mark_oom(self);
            return Ok(false);
        }
        let delta = desired.saturating_sub(current);
        if self.meter.reserve(delta) {
            return Ok(true);
        }
        // The run's own instance takes back what its idle pooled fan
        // workers hold before it is refused: alone, it reaches the whole cap.
        let granted = match &self.par {
            Some(par) => {
                par.release_pool();
                self.meter.reserve(delta)
            }
            None => false,
        };
        if !granted {
            mark_oom(self);
        }
        Ok(granted)
    }

    fn memory_grow_failed(&mut self, _error: wasmtime::Error) -> wasmtime::Result<()> {
        self.meter.undo_last();
        mark_oom(self);
        Ok(())
    }

    fn table_growing(&mut self, _current: usize, desired: usize, maximum: Option<usize>) -> wasmtime::Result<bool> {
        Ok(maximum.is_none_or(|m| desired <= m))
    }
}


/// Append program output: into the capture buffer, or — live — to the
/// real stream with native's buffering rule. The bytes go through exactly
/// as the guest wrote them (C-399, #4138): `io.write` / `io.write_bytes`
/// are byte writers, and a `String`'s bytes are written as native writes
/// them — no UTF-8 decoding, so no U+FFFD substitution, on any path.
fn emit_out(host: &Host, bytes: &[u8]) {
    match &host.live_out {
        Some(w) => {
            use std::io::Write as _;
            let mut w = w.lock().expect("live stdout");
            let _ = w.write_all(bytes);
            // Line-buffered, native's rule (#3417): a line end flushes on any
            // stdout, every write on a terminal.
            if bytes.contains(&b'\n') || stdout_is_terminal() {
                let _ = w.flush();
            }
        }
        None => host.out.lock().expect("test harness invariant").extend_from_slice(bytes),
    }
}

/// Write to the real stderr unbuffered, as native's `eprint!` does — but
/// the bytes, not a decoded text.
fn stderr_write(parts: &[&[u8]]) {
    use std::io::Write as _;
    let mut e = std::io::stderr().lock();
    for p in parts {
        let _ = e.write_all(p);
    }
}

fn emit_err_line(host: &Host, line: &[u8]) {
    if hold_err(host, line, true) {
        return;
    }
    emit_err_line_now(host, line);
}

/// Hold `bytes` when the contained loop runs and they could be an abort's
/// line (#4203); otherwise release any held write first. True = held.
fn hold_err(host: &Host, bytes: &[u8], line: bool) -> bool {
    release_held(host);
    let mut c = host.contain.lock().expect("contain");
    let abort_shaped = bytes.starts_with(b"PANIC: ") || (line && bytes.starts_with(b"Error: "));
    if c.active && abort_shaped {
        *host.err_last.lock().expect("err last") = String::from_utf8_lossy(bytes).into_owned();
        c.held = Some((bytes.to_vec(), line));
        return true;
    }
    false
}

/// Write a held stderr write out: what followed it was not its abort.
fn release_held(host: &Host) {
    let held = host.contain.lock().expect("contain").held.take();
    match held {
        Some((b, true)) => emit_err_line_now(host, &b),
        Some((b, false)) => emit_err_raw_now(host, &b),
        None => {}
    }
}

fn emit_err_line_now(host: &Host, line: &[u8]) {
    *host.err_last.lock().expect("err last") = String::from_utf8_lossy(line).into_owned();
    if host.live_out.is_some() {
        stderr_write(&[line, b"\n"]);
    } else {
        let mut o = host.err.lock().expect("test harness invariant");
        o.extend_from_slice(line);
        o.push(b'\n');
    }
}

/// Append to stderr verbatim — op 73, the newline-free twin of
/// [`emit_err_line`].
fn emit_err_raw(host: &Host, bytes: &[u8]) {
    if hold_err(host, bytes, false) {
        return;
    }
    emit_err_raw_now(host, bytes);
}

fn emit_err_raw_now(host: &Host, bytes: &[u8]) {
    *host.err_last.lock().expect("err last") = String::from_utf8_lossy(bytes).into_owned();
    if host.live_out.is_some() {
        stderr_write(&[bytes]);
    } else {
        host.err.lock().expect("test harness invariant").extend_from_slice(bytes);
    }
}

/// Where op 35 gets its bytes: a fixed buffer (tests, piped runs), or
/// the process's real stdin read at the FIRST guest read — so a program
/// that never touches stdin never blocks on an open terminal.
pub enum StdinSource {
    Buf(Vec<u8>),
    RealOnce,
}

impl StdinSource {
    /// Take UP TO `n` bytes off the stream's cursor (op 35 — the only
    /// stdin op since #2116 retired the op-31 drain). A fixed buffer
    /// serves its front; the real stream reads lazily, so a terminal
    /// program keeps native's line-at-a-time interleaving (a
    /// line-buffered read blocks until Enter, not until EOF).
    fn take(&mut self, n: usize) -> Vec<u8> {
        if n == 0 {
            return Vec::new();
        }
        match self {
            StdinSource::Buf(b) => {
                let k = n.min(b.len());
                b.drain(..k).collect()
            }
            StdinSource::RealOnce => {
                // Read in bounded chunks, so what the host holds grows with
                // the bytes that ARRIVE, never with the guest's word (#4135:
                // a forged count must not reserve up to 2 GiB). A chunk that
                // comes back short is all the stream has ready (a terminal
                // line, EOF), so the read stops there; a full one may have
                // more behind it, so it reads on until the request is filled
                // (native's io.read_n_bytes answers min(n, what stdin has)).
                use std::io::Read;
                const CHUNK: usize = 64 * 1024;
                let mut out = Vec::new();
                let mut stdin = std::io::stdin().lock();
                while out.len() < n {
                    let want = (n - out.len()).min(CHUNK);
                    let start = out.len();
                    out.resize(start + want, 0);
                    let got = stdin.read(&mut out[start..]).unwrap_or(0);
                    out.truncate(start + got);
                    if got < want {
                        break;
                    }
                }
                out
            }
        }
    }
}

include!("host_fs.rs");

/// The guest's exported memory — an error (a trap of the call), never a
/// panic, for a module that exports none (#4135).
pub(crate) fn guest_memory<T>(caller: &mut wasmtime::Caller<'_, T>) -> wasmtime::Result<wasmtime::Memory> {
    caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("the module exports no memory"))
}

/// Copy the guest span `[ptr, ptr + len)` out of `mem` (both i32s read
/// unsigned, the wasm32 address rule). The span is checked against the
/// memory's CURRENT size BEFORE anything is allocated (#4135): a forged
/// length can neither make the host reserve up to 4 GiB nor panic it — it is
/// the guest's own out-of-bounds trap, `Error: wasm trap: out of bounds
/// memory access`, exit 1, exactly what a guest load past the end gives.
pub(crate) fn read_guest<T>(caller: &wasmtime::Caller<'_, T>, mem: wasmtime::Memory, ptr: i32, len: i32) -> wasmtime::Result<Vec<u8>> {
    let data = mem.data(caller);
    let start = ptr as u32 as usize;
    match start.checked_add(len as u32 as usize) {
        Some(end) if end <= data.len() => Ok(data[start..end].to_vec()),
        _ => Err(wasmtime::Trap::MemoryOutOfBounds.into()),
    }
}

/// Is the guest span `[ptr, ptr + len)` inside `mem` (the answer room a host
/// op writes back into)?
pub(crate) fn guest_span_fits<T>(caller: &wasmtime::Caller<'_, T>, mem: wasmtime::Memory, ptr: i32, len: usize) -> bool {
    (ptr as u32 as usize).checked_add(len).is_some_and(|end| end <= mem.data_size(caller))
}

/// `println` / `eprintln`: the guest's bytes plus a line end, written as
/// they are (a `String` is already UTF-8; nothing is decoded).
fn append_line(caller: &mut wasmtime::Caller<'_, Host>, to_err: bool, ptr: i32, len: i32) -> wasmtime::Result<()> {
    let mem = guest_memory(caller)?;
    let mut line = read_guest(caller, mem, ptr, len)?;
    if to_err {
        emit_err_line(caller.data(), &line);
    } else {
        release_held(caller.data());
        line.push(b'\n');
        emit_out(caller.data(), &line);
    }
    Ok(())
}

pub fn run_wasm(bytes: &[u8]) -> anyhow::Result<RunResult> {
    run_wasm_with(bytes, &[])
}

/// Run with a fixed stdin buffer (tests; piped byte streams).
pub fn run_wasm_with(bytes: &[u8], stdin: &[u8]) -> anyhow::Result<RunResult> {
    run_wasm_src(bytes, StdinSource::Buf(stdin.to_vec()), None, &[], Some(harness_watchdog()), false)
}

/// The in-process TEST runner's epoch watchdog: a fixture (or a MUTANT under
/// a gate) that diverges must FAIL the run, never hang the suite. 30 s of
/// wall time is orders beyond any fixture; `ALMIDE_WASM_WATCHDOG_SECS`
/// shortens or lengthens it. It is test-harness equipment and nothing else
/// arms it: the product runner (`almide run --target wasm`) and the timing
/// runner (`almide bench --target wasm`) run to completion, exactly as the
/// native binary does, which has no time limit (#2615).
fn harness_watchdog() -> std::time::Duration {
    let secs = almide_base::env::var("ALMIDE_WASM_WATCHDOG_SECS")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(30);
    std::time::Duration::from_secs(secs)
}

/// `run_wasm` WITHOUT the epoch watchdog — the timing runner
/// (`almide bench --target wasm`, #2150) and the wasm leg of `almide test`
/// (#3046), which runs a test file to completion as its native twin does. The watchdog is test-harness
/// equipment, and it is not free: epoch interruption makes wasmtime check
/// the epoch at every loop header and function entry, which measured 1.9x
/// on mandelbrot's inner loop and 1.2x on fft (same module, `wasmtime run`
/// with and without `-W timeout`, 2026-09-24). A bench must time the
/// emitted program, as the native leg's bench does and as a stock runtime
/// runs it. A bench of a diverging program hangs, exactly as it does natively.
pub fn run_wasm_unbounded(bytes: &[u8]) -> anyhow::Result<RunResult> {
    run_wasm_src(bytes, StdinSource::Buf(Vec::new()), None, &[], None, false)
}

/// [`run_wasm_unbounded`] with program arguments (the bench's workload
/// size, #2980), exactly as `almide run --target wasm -- args` passes them.
pub fn run_wasm_unbounded_args(bytes: &[u8], args: &[String]) -> anyhow::Result<RunResult> {
    run_wasm_src(bytes, StdinSource::Buf(Vec::new()), None, args, None, false)
}

/// Run under a hard linear-memory budget (bytes). Growth past the cap
/// fails, which the emitted allocator turns into the DEFINED
/// "Error: out of memory" + exit 1 (C-197) — the heap-budget
/// acceptance-gate observable (W-8; the RC arc's floor).
pub fn run_wasm_capped(bytes: &[u8], max_memory_bytes: usize) -> anyhow::Result<RunResult> {
    run_wasm_src(bytes, StdinSource::Buf(Vec::new()), Some(max_memory_bytes), &[], Some(harness_watchdog()), false)
}

/// Run with the process's real stdin, read lazily on first guest read
/// (the product runner — never blocks for programs that skip stdin). No
/// time limit, as native has none (#2615).
pub fn run_wasm_real_stdin(bytes: &[u8]) -> anyhow::Result<RunResult> {
    run_wasm_src(bytes, StdinSource::RealOnce, None, &[], None, false)
}

/// The product runner with program args (#1716), streaming LIVE (#2650):
/// output reaches the real stdout/stderr as the program runs, with native's
/// buffering rule, so a program that never returns — an `http.serve`
/// server — is observable while it runs. op 29 answers
/// [argv0, args...] and the guest's frame walk skips argv0. No time limit:
/// `almide run --target wasm` runs a program to completion exactly as the
/// native binary does (#2615 — it used to arm the test harness's 30 s
/// watchdog, so a program native finished in 36 s trapped with `interrupt`
/// on wasm, and every loop header paid the epoch check).
pub fn run_wasm_real_stdin_args(bytes: &[u8], args: &[String]) -> anyhow::Result<RunResult> {
    run_wasm_src(bytes, StdinSource::RealOnce, None, args, None, true)
}

/// The embedded host's wasm call-stack budget (#3435), set as wasmtime's
/// `Config::max_wasm_stack` on the one engine every run (and every fan
/// instance of that run) uses. wasmtime's default is 512 KiB, which made a
/// plain non-tail recursion (a tree walk, a type checker's `infer`) trap
/// with `call stack exhausted` 5–8x shallower than the native binary, whose
/// main thread has an 8 MiB stack. 8 MiB matches native's budget; measured
/// on 2026-10-06, the embedded lane then reaches at least native's depth on
/// a non-tail tree walk and on a recursion with heap locals per frame (a
/// wasm frame is smaller than its native twin). Exhaustion stays the
/// resource limit C-196 names; this only sets where it is. Stock runtimes
/// (`wasmtime run`, browsers) keep their own limits.
pub const EMBEDDED_WASM_STACK: usize = 8 * 1024 * 1024;

/// The native stack of the host thread a guest runs on. wasmtime requires
/// it to exceed [`EMBEDDED_WASM_STACK`] plus the host frames below and
/// between guest frames (host imports, the trap handler, and Cranelift
/// compiling the module on this thread), or a deep guest overflows the
/// thread's guard page instead of trapping. The size is a virtual
/// reservation, committed lazily, so a shallow program pays nothing.
pub(crate) const EMBEDDED_HOST_THREAD_STACK: usize = EMBEDDED_WASM_STACK + 56 * 1024 * 1024;

/// Spawn a scoped thread with [`EMBEDDED_HOST_THREAD_STACK`] — every host
/// thread that calls into a guest of an [`EMBEDDED_WASM_STACK`] engine.
pub(crate) fn spawn_guest_thread<'scope, 'env, T: Send + 'scope>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    f: impl FnOnce() -> T + Send + 'scope,
) -> std::thread::ScopedJoinHandle<'scope, T> {
    std::thread::Builder::new()
        .name("almide-wasm-guest".to_string())
        .stack_size(EMBEDDED_HOST_THREAD_STACK)
        .spawn_scoped(scope, f)
        .expect("failed to spawn the embedded wasm guest thread")
}

/// [`spawn_guest_thread`] for a thread that outlives the call spawning it —
/// the fan's resident threads (`host_fan.rs` `Crew`). A refused spawn is the
/// caller's to handle.
pub(crate) fn spawn_resident_guest_thread(f: impl FnOnce() + Send + 'static) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new().name("almide-wasm-fan".to_string()).stack_size(EMBEDDED_HOST_THREAD_STACK).spawn(f)
}

/// Run the guest on its own host thread sized for [`EMBEDDED_WASM_STACK`]
/// (the caller's thread may be any size: a test harness worker, the CLI's
/// driver). Everything a run touches — stdout/stderr, the exit code, the
/// serve loop and its signal handling — is per-run state or process-wide,
/// so moving the run off the caller's thread changes nothing observable; a
/// host panic is re-raised on the caller's thread.
fn run_wasm_src(
    bytes: &[u8],
    stdin: StdinSource,
    max_memory_bytes: Option<usize>,
    args: &[String],
    watchdog: Option<std::time::Duration>,
    live: bool,
) -> anyhow::Result<RunResult> {
    std::thread::scope(|s| {
        spawn_guest_thread(s, || run_wasm_src_here(bytes, stdin, max_memory_bytes, args, watchdog, live))
            .join()
            .unwrap_or_else(|p| std::panic::resume_unwind(p))
    })
}

fn run_wasm_src_here(
    bytes: &[u8],
    stdin: StdinSource,
    max_memory_bytes: Option<usize>,
    args: &[String],
    watchdog: Option<std::time::Duration>,
    live: bool,
) -> anyhow::Result<RunResult> {
    wasmparser::validate(bytes)?; // the wall: never instantiate an invalid module
    with_env_overlay(|o| o.clear()); // a fresh environ per run, as per process
    // Epoch deadline (test harness only, see `harness_watchdog`): the
    // deadline maps to a plain trap.
    let mut cfg = wasmtime::Config::new();
    cfg.epoch_interruption(watchdog.is_some());
    cfg.max_wasm_stack(EMBEDDED_WASM_STACK);
    // wasmtime refuses a `max_wasm_stack` above `async_stack_size` (2 MiB by
    // default) even when, as here, nothing runs async: no fiber is ever
    // allocated, so this only satisfies the engine's config check.
    cfg.async_stack_size(EMBEDDED_WASM_STACK + 1024 * 1024);
    let engine = wasmtime::Engine::new(&cfg)?;
    let module = wasmtime::Module::new(&engine, bytes)?;
    let allowed_ops = allowed_ops_of(bytes)?;
    let budget = budget::Budget::new(max_memory_bytes);
    run_compiled(&engine, &module, Launch { stdin, budget, args, watchdog, live, abort_site: -1, allowed_ops })
}

/// How one instance of a compiled module is started: the run's own, or an
/// abort-site child of it (`testing.assert_aborts`, ADR-0029).
struct Launch<'a> {
    stdin: StdinSource,
    /// The run's linear-memory budget — an abort-site child shares its
    /// parent's (#4135).
    budget: Arc<budget::Budget>,
    args: &'a [String],
    watchdog: Option<std::time::Duration>,
    live: bool,
    /// The abort site this instance runs (op 118), or -1 for the tests.
    abort_site: i64,
    /// The module's `fs_call` op set (#4102).
    allowed_ops: AllowedOps,
}

/// Instantiate `module` once and run its `main` to completion.
fn run_compiled(engine: &wasmtime::Engine, module: &wasmtime::Module, launch: Launch<'_>) -> anyhow::Result<RunResult> {
    let Launch { stdin, budget, args, watchdog, live, abort_site, allowed_ops } = launch;
    let (engine, module) = (engine.clone(), module.clone());
    let out = Arc::new(Mutex::new(Vec::new()));
    let err = Arc::new(Mutex::new(Vec::new()));
    let exit = Arc::new(Mutex::new(None));
    let fs_buf = Arc::new(Mutex::new(Vec::new()));
    let stdin_buf = Arc::new(Mutex::new(stdin));
    let mut store = wasmtime::Store::new(
        &engine,
        Host {
            out: out.clone(),
            err: err.clone(),
            exit: exit.clone(),
            fs_buf: fs_buf.clone(),
            stdin: stdin_buf.clone(),
            args: args.to_vec(),
            meter: budget::Meter::new(budget.clone()),
            calls: Arc::default(),
            serve: Arc::new(Mutex::new(crate::host_serve::ServeState::default())),
            net: Arc::default(),
            live_out: live.then(|| Arc::new(Mutex::new(std::io::BufWriter::with_capacity(65536, std::io::stdout())))),
            err_last: Arc::new(Mutex::new(String::new())),
            par: None,
            abort_site,
            allowed_ops: allowed_ops.clone(),
            contain: Arc::default(),
        },
    );
    store.limiter(|h| h as &mut dyn wasmtime::ResourceLimiter);
    let linker = host_linker(&engine)?;
    // #3003 (ADR-0011 §D2a): the instance-parallel fan offer (op 74) runs a
    // chunk on fresh instances of THIS module, through THIS import set.
    let ticked = watchdog.map(|_| Arc::new(std::sync::atomic::AtomicBool::new(false)));
    store.data_mut().par = Some(Arc::new(fan::ParCtx::new(
        (engine.clone(), module.clone(), linker.clone()),
        budget,
        ticked.clone(),
        args.to_vec(),
        allowed_ops,
    )));
    let ticker = watchdog.map(|after| {
        store.set_epoch_deadline(1);
        let eng = engine.clone();
        let ticked = ticked.clone();
        std::thread::spawn(move || {
            std::thread::sleep(after);
            if let Some(t) = ticked {
                t.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            eng.increment_epoch();
        })
    });
    let instance = linker.instantiate(&mut store, &module)?;
    let main = instance.get_typed_func::<(), ()>(&mut store, "main")?;
    let main_started = std::time::Instant::now();
    let call = main.call(&mut store, ());
    let main_secs = main_started.elapsed().as_secs_f64();
    let recorded = exit.lock().expect("test harness invariant").take();
    let exit_code = match (&call, recorded) {
        (Ok(()), None) => 0,
        (Err(_), Some(code)) => code,
        (Err(e), None) => {
            if almide_base::env::flag("ALMIDE_DBG_TRAP") {
                eprintln!("TRAP: {e:?}");
            }
            // A genuine trap is a runtime abort: exit 1, and (#1826) the
            // abort NAMES itself on stderr in the `Error: ` form native's
            // aborts use. Native never has this case (its aborts are all
            // `Error: <msg>` from a defined guard), so the `wasm trap:`
            // prefix is this leg's own spelling — a fuzz finding or a
            // user is never left with an empty stderr and a bare 1.
            // EXCEPT the die convention (#1912): a defined guard's
            // `prim.die` prints its `Error: <msg>` line and then executes
            // `unreachable` — the trap IS the exit, already named. The stock
            // wasmtime lane and native show that one line; adding
            // `Error: wasm trap: unreachable…` after it made the embedded
            // lane the odd one out.
            let named_die = is_unreachable_trap(e)
                && store.data().err_last.lock().expect("err last").starts_with("Error: ");
            if !named_die {
                emit_err_line(store.data(), trap_line(e).trim_end_matches('\n').as_bytes());
            }
            1
        }
        (Ok(()), Some(_)) => {
            anyhow::bail!("almide.exit recorded a code but the run returned normally")
        }
    };
    drop(ticker);
    if let Some(w) = &store.data().live_out {
        use std::io::Write as _;
        let _ = w.lock().expect("live stdout").flush();
    }
    let read_global = |store: &mut wasmtime::Store<_>, name: &str| {
        instance.get_global(&mut *store, name).map(|g| match g.get(&mut *store) {
            wasmtime::Val::I32(v) => v as u32 as u64,
            wasmtime::Val::I64(v) => v as u64,
            _ => 0,
        })
    };
    // A region window (#1961) rewinds `__heap`; the allocation total is
    // the peak, kept in `__heap_high` when the module has windows.
    let heap_end = read_global(&mut store, "__heap");
    let heap_end = match (heap_end, read_global(&mut store, "__heap_high")) {
        (Some(h), Some(hi)) => Some(h.max(hi)),
        (h, _) => h,
    };
    // #2407: the counters ride five i64 globals an armed build exports;
    // all five or none — a module missing any is a shipped one.
    let alloc_count = match (
        read_global(&mut store, "__alloc_count"),
        read_global(&mut store, "__alloc_reused"),
        read_global(&mut store, "__alloc_bytes"),
        read_global(&mut store, "__free_count"),
        read_global(&mut store, "__region_reclaimed"),
    ) {
        (Some(allocs), Some(reused), Some(bytes), Some(frees), Some(reclaimed)) => {
            Some(AllocCount { allocs, reused, bytes, frees, reclaimed })
        }
        _ => None,
    };
    let stdout_bytes = out.lock().expect("test harness invariant").clone();
    let stderr_bytes = err.lock().expect("test harness invariant").clone();
    Ok(RunResult {
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        stdout_bytes,
        stderr_bytes,
        exit: exit_code,
        heap_end,
        alloc_count,
        main_secs,
    })
}

/// The `almide.*` import set every instance of a run links against — the
/// run's own and, for the instance-parallel fan offer (#3003), its chunks'.
fn host_linker(engine: &wasmtime::Engine) -> anyhow::Result<wasmtime::Linker<Host>> {
    let mut linker = wasmtime::Linker::new(engine);
    linker.func_wrap(
        "almide",
        "println",
        |mut caller: wasmtime::Caller<'_, Host>, ptr: i32, len: i32| -> wasmtime::Result<()> {
            append_line(&mut caller, false, ptr, len)
        },
    )?;
    linker.func_wrap(
        "almide",
        "eprintln",
        |mut caller: wasmtime::Caller<'_, Host>, ptr: i32, len: i32| -> wasmtime::Result<()> {
            append_line(&mut caller, true, ptr, len)
        },
    )?;
    // A fn item (not a closure): a return-type-annotated closure is not
    // higher-ranked over the Caller lifetime and fails IntoFunc.
    fn exit_host(caller: wasmtime::Caller<'_, Host>, code: i32) -> wasmtime::Result<()> {
        *caller.data().exit.lock().expect("test harness invariant") = Some(code);
        // Unwind: the emitter guarantees an `unreachable` follows the
        // call, so returning an error here is the ONLY way out — no
        // instruction after `process.exit` ever executes.
        Err(wasmtime::Error::msg("almide.exit"))
    }
    linker.func_wrap("almide", "exit", exit_host)?;
    // The private subprocess import (#2589): the canonical form a stock
    // artifact carries, served here with the same core as ops 80..=90.
    crate::host_process::link_spawn_import(&mut linker)?;
    linker.func_wrap(
        "almide",
        "fs_call",
        |mut caller: wasmtime::Caller<'_, Host>,
         op: i32,
         a_ptr: i32,
         a_len: i32,
         b_ptr: i32,
         b_len: i32|
         -> wasmtime::Result<i64> {
            op_allowed(caller.data(), op)?;
            release_held(caller.data());
            // op 75 = the contained serve loop (#4203): the loop closure's
            // env rides a_len (scalar, null a_ptr).
            if op == crate::host_serve::OP_SERVE_RUN {
                return serve_contained(&mut caller, a_len);
            }
            // op 76 = `process.exit`'s mark (#4203): the exit that follows is
            // the program's own, never a contained abort.
            if op == OP_EXIT_MARK {
                caller.data().contain.lock().expect("contain").exit_marked = true;
                return Ok(0);
            }
            // op 74 = the instance-parallel fan offer (#3003): the request
            // and the answer room are raw i64 slots, never text.
            if op == fan::OP_FAN_PAR {
                return fan::serve(&mut caller, (a_ptr, a_len), (b_ptr, b_len));
            }
            // op 35 = incremental stdin (up to a_len bytes off the cursor).
            // Handled BEFORE the a/b buffer reads: the count rides in
            // a_len with a null a_ptr, and materializing it as a guest
            // buffer would read a_len bytes of guest memory (the 4 GiB
            // trap the op-31 comment in the emitter records).
            if op == 35 {
                // The answer is min(a_len, what stdin has) — NOT capped at the
                // guest's current memory size: the guest allocates the answer
                // AFTER this op (growing its memory), then host_read copies it
                // in, so a cap at the size before the read truncated a 1 MB
                // request to the 512 KiB the memory then had. What bounds the
                // host's reservation is `take` reading in chunks (#4135).
                let n = i64::from(a_len).max(0) as usize;
                let got = caller.data().stdin.lock().expect("stdin").take(n);
                let len = got.len();
                *caller.data().fs_buf.lock().expect("fs buf") = got;
                return Ok((len as i64) & 0xFFFF_FFFF);
            }
            // op 36 = env.sleep_ms: the count rides a_len (scalar, null
            // a_ptr — the op-35 discipline). No observable value.
            if op == 36 {
                let ms = i64::from(a_len).max(0) as u64;
                std::thread::sleep(std::time::Duration::from_millis(ms));
                return Ok(0);
            }
            // op 60 = the monotonic clock (datetime.monotonic_ns): raw
            // nanos since the run's first read, native's own origin rule
            // (a process-wide OnceLock<Instant>). No args, no buffer.
            if op == 60 {
                static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
                let start = ORIGIN.get_or_init(std::time::Instant::now);
                return Ok(start.elapsed().as_nanos() as i64);
            }
            // ops 54..=59 = the http call handle on call `id` (#2633): the
            // id rides a_len (scalar, null a_ptr — the op-35 discipline).
            if (54..=59).contains(&op) {
                let calls = caller.data().calls.clone();
                let (ret, buf) = crate::http_call_host::by_id(&calls, op, a_len as u32);
                *caller.data().fs_buf.lock().expect("fs buf") = buf;
                return Ok(ret);
            }
            // op 29 = args (#1716): argv0 + the run's program args; the
            // guest skips frame 0 (native argv[1..] semantics).
            if op == 29 {
                let mut names = vec!["wasm-harness".to_string()];
                names.extend(caller.data().args.iter().cloned());
                let buf = frames(&names);
                let len = buf.len();
                *caller.data().fs_buf.lock().expect("fs buf") = buf;
                return Ok((len as i64) & 0xFFFF_FFFF);
            }
            // ops 117 / 118 = `testing.assert_aborts`'s abort sites
            // (ADR-0029): scalars in a_len, no guest buffers.
            if op == 117 || op == 118 {
                return Ok(abort_site_op(&caller, op, a_len));
            }
            let mem = guest_memory(&mut caller)?;
            let a = read_guest(&caller, mem, a_ptr, a_len)?;
            let b = read_guest(&caller, mem, b_ptr, b_len)?;
            let a = String::from_utf8_lossy(&a).to_string();
            // op 30 = raw stdout append (io.write / io.write_bytes):
            // PROGRAM order with println is the C-contract, so it goes
            // straight into the same sink, no trailing newline.
            // op 34 = wall clock (nanos, RAW i64 — no status packing).
            if op == 34 {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as i64;
                return Ok(now);
            }
            if op == 30 {
                emit_out(caller.data(), &b);
                return Ok(0);
            }
            // op 73 = raw stderr append (`panic`'s line, #2769): no newline.
            if op == 73 {
                emit_err_raw(caller.data(), &b);
                return Ok(0);
            }
            // http.serve (#2650): the listener and the held connection
            // live in the run's own state — one per run, like native's.
            if (crate::host_serve::OP_SERVE_BIND..=crate::host_serve::OP_SERVE_REPLY).contains(&op) {
                let (ret, buf) = crate::host_serve::dispatch(&caller.data().serve, caller.data().live_out.as_ref(), op, &a, frames, parse_http_frame);
                // #4203: a request handed out starts its handler's handle
                // record; its reply ends it.
                if op == crate::host_serve::OP_SERVE_NEXT && !buf.is_empty() {
                    request_handles(caller.data(), Handles::Begin);
                } else if op == crate::host_serve::OP_SERVE_REPLY {
                    request_handles(caller.data(), Handles::End);
                }
                *caller.data().fs_buf.lock().expect("fs buf") = buf;
                return Ok(ret);
            }
            // ops 80..=90 = almide:process/spawn (#2589, ADR-0025): the
            // subprocess core native runs; stdout flushed before a child
            // that shares it.
            if (crate::host_process::OP_FIRST..=crate::host_process::OP_LAST).contains(&op) {
                let live = caller.data().live_out.clone();
                let flush = move || {
                    if let Some(w) = &live {
                        use std::io::Write as _;
                        let _ = w.lock().expect("live stdout").flush();
                    }
                };
                let (ret, buf) = crate::host_process::dispatch(op, &a, &b, &flush);
                *caller.data().fs_buf.lock().expect("fs buf") = buf;
                return Ok(ret);
            }
            // ops 91..=116 = the tcp family and the interim unix / shm ops
            // (#3620): the net cores native runs, on the run's own tables.
            if (crate::net_host::OP_FIRST..=crate::net_host::OP_LAST).contains(&op) {
                let net = caller.data().net.clone();
                let (ret, buf) = crate::net_host::dispatch(&net, op, &a, &b);
                *caller.data().fs_buf.lock().expect("fs buf") = buf;
                return Ok(ret);
            }
            // op 53 = open an http call (#2633): url in a, the start frame in b.
            let (ret, buf) = if op == 53 {
                crate::http_call_host::open(&caller.data().calls.clone(), &a, &b)
            } else {
                // `[permissions] fs` (#4022): a path outside the granted roots is
                // refused before the host touches it, with native's bytes.
                // The op itself runs beneath the granted root (#4084); a
                // symlink it meets there answers the same refusal.
                crate::host_fs_scope::refuse(op, &a, &b)
                    .unwrap_or_else(|| crate::host_fs_scope::refused_by_call(op, &a, &b, fs_dispatch(op, &a, &b)))
            };
            *caller.data().fs_buf.lock().expect("fs buf") = buf;
            Ok(ret)
        },
    )?;
    linker.func_wrap(
        "almide",
        "host_read",
        |mut caller: wasmtime::Caller<'_, Host>, dst: i32| -> wasmtime::Result<()> {
            let mem = guest_memory(&mut caller)?;
            let buf = caller.data().fs_buf.lock().expect("fs buf").clone();
            mem.write(&mut caller, dst as u32 as usize, &buf)?;
            Ok(())
        },
    )?;
    Ok(linker)
}

/// The `unreachable` trap — the instruction the die lowering ends on.
/// `testing.assert_aborts` (ADR-0029). Op 118 answers the site this instance
/// runs (-1: the tests). Op 117 runs site `a_len` on a FRESH instance of the
/// run's own compiled module — fresh globals, an empty stdin, the run's args
/// and bounds — on its own guest thread, and parks the frames [status,
/// stdout, stderr]; the parent instance continues after it. A store without
/// the run context (a fan chunk) cannot start one and answers status -1.
fn abort_site_op(caller: &wasmtime::Caller<'_, Host>, op: i32, a_len: i32) -> i64 {
    if op == 118 {
        return caller.data().abort_site;
    }
    // A child never starts a grandchild (E099 refuses the nesting at check
    // time; this bounds a direct call of the private op).
    let par = if caller.data().abort_site >= 0 { None } else { caller.data().par.clone() };
    let child = match par {
        Some(par) => {
            // the child meters against the run's own budget (#4135); the
            // parent's idle fan workers give their memory back first, as the
            // parent's own growth would make them
            par.release_pool();
            let (engine, module, budget, args, allowed_ops) = par.compiled();
            std::thread::scope(|s| {
                spawn_guest_thread(s, || {
                    let launch =
                        Launch {
                        stdin: StdinSource::Buf(Vec::new()),
                        budget,
                        args,
                        watchdog: None,
                        live: false,
                        abort_site: i64::from(a_len),
                        allowed_ops: allowed_ops.clone(),
                    };
                    run_compiled(engine, module, launch)
                })
                .join()
                .unwrap_or_else(|p| std::panic::resume_unwind(p))
            })
        }
        None => Err(anyhow::anyhow!("an abort site cannot start from a fan element or from another abort site")),
    };
    let answer = match child {
        Ok(r) => vec![r.exit.to_string(), r.stdout, r.stderr],
        Err(e) => vec!["-1".to_string(), String::new(), format!("the abort-site child did not start: {e:#}")],
    };
    let buf = frames(&answer);
    let len = buf.len();
    *caller.data().fs_buf.lock().expect("fs buf") = buf;
    (len as i64) & 0xFFFF_FFFF
}

/// op 75 (#4203): run the guest's serve loop — the closure whose env block
/// is `env` — so that a runtime abort inside a handler fails only its
/// request: the request gets native's C-409 500, stderr gets native's one
/// `handler aborted` line, and the loop is called again. Answers 0 when the
/// loop returned (a shutdown), or `1 << 32` (err, "not served") for a module
/// without the exports, and the guest then runs the loop itself.
///
/// State soundness. Before the first call the host copies the instance's
/// whole linear memory and every mutable global (a module naming op 75
/// exports its globals and table, assembly.rs). An abort unwinds only the
/// call made here: wasmtime hands the trap back to this host function, and
/// the guest frames below it (`http.serve`'s, `main`'s) are untouched, since
/// a trap ends only the frames the trapping call pushed. The host then writes
/// the copy back, so memory and globals are bit for bit what they were when
/// `http.serve` called op 75: whatever the handler left half done (an
/// allocator mid-update, a region window still open, refcounts raised and
/// never lowered, a partly built line-buffer string) is gone rather than
/// carried into the next request. Memory grown since is zeroed, as fresh
/// pages are. The frames below hold only values that existed at the copy
/// (the loop closure and the app), so they stay valid. That point is the
/// state a fresh instance of the app would start serving from, and C-367's
/// app is instance-closed (how many instances evaluate it is not
/// observable), so a later request sees nothing of the aborted one but its
/// effects on the world: output already written, files, sockets.
///
/// Only the two defined aborts are contained, the ones native's serve loop
/// catches (`almide_abort`, `almide_panic_abort`): the line `Error: <m>`
/// (or `PANIC: <m>`, or `panic`'s raw `PANIC: <m>`) written just before the
/// call ends on its `unreachable` (the die convention) or its exit 1. A
/// `process.exit` sends its own mark (op 76) before its exit import, so a
/// program that prints `Error: x` and then calls `process.exit(1)` is told
/// apart exactly: it ends the run, as on native. Any other end of the call
/// (another exit code, stack exhaustion, another trap, an abort outside a
/// request) stays the run's too. A contained abort also closes the tcp
/// sockets, unix descriptors and http calls the aborted handler opened and
/// left open (`request_handles`) — native's unwinding drops what owned
/// them; the guest's own drop paths were rolled back with its state.
fn serve_contained(caller: &mut wasmtime::Caller<'_, Host>, env: i32) -> wasmtime::Result<i64> {
    const NOT_SERVED: i64 = 1 << 32;
    let mem = guest_memory(caller)?;
    let Some(table) = caller.get_export("__almide_table").and_then(|e| e.into_table()) else {
        return Ok(NOT_SERVED);
    };
    let mut globals = Vec::new();
    for g in 0.. {
        let Some(gl) = caller.get_export(&format!("__almide_g{g}")).and_then(|e| e.into_global()) else {
            break;
        };
        if gl.ty(&*caller).mutability() == wasmtime::Mutability::Var {
            let v = gl.get(&mut *caller);
            globals.push((gl, v));
        }
    }
    let mut slot = [0u8; 4];
    mem.read(&*caller, (env as u32 as usize).saturating_add(almide_layout::PAYLOAD as usize), &mut slot)?;
    let Some(func) = table.get(&mut *caller, u64::from(u32::from_le_bytes(slot))).and_then(|r| r.as_func().flatten().copied()) else {
        return Ok(NOT_SERVED);
    };
    let snapshot = mem.data(&*caller).to_vec();
    let serve = caller.data().serve.clone();
    let mut results = vec![wasmtime::Val::I32(0); func.ty(&*caller).results().len()];
    loop {
        {
            let mut c = caller.data().contain.lock().expect("contain");
            c.active = true;
            c.exit_marked = false;
            c.oom = false;
        }
        let ran = func.call(&mut *caller, &[wasmtime::Val::I32(env)], &mut results);
        let (held, exit_marked, oom) = {
            let mut c = caller.data().contain.lock().expect("contain");
            c.active = false;
            (c.held.take(), c.exit_marked, c.oom)
        };
        let put_back = |host: &Host, held: Option<(Vec<u8>, bool)>| match held {
            Some((b, true)) => emit_err_line_now(host, &b),
            Some((b, false)) => emit_err_raw_now(host, &b),
            None => {}
        };
        let e = match ran {
            Ok(()) => {
                put_back(caller.data(), held);
                return Ok(0);
            }
            Err(e) => e,
        };
        let exit = *caller.data().exit.lock().expect("exit");
        // The abort's own end: its `unreachable` (the die convention) or
        // its exit 1 (the emitter's guards and `panic`).
        // An out-of-memory end is never contained: its growth was refused.
        let ended = !exit_marked && !oom && (matches!(exit, Some(1)) || (exit.is_none() && is_unreachable_trap(&e)));
        let text = match &held {
            Some((h, line)) if ended => match h.strip_prefix(b"Error: ").filter(|_| *line) {
                Some(m) => Some(m.to_vec()),
                None => h.starts_with(b"PANIC: ").then(|| h.clone()),
            },
            _ => None,
        };
        let line = text.and_then(|t| crate::host_serve::abort_pending(&serve, &String::from_utf8_lossy(&t)));
        let Some(line) = line else {
            put_back(caller.data(), held);
            return Err(e);
        };
        *caller.data().exit.lock().expect("exit") = None;
        request_handles(caller.data(), Handles::Abort);
        emit_err_line_now(caller.data(), line.as_bytes());
        let data = mem.data_mut(&mut *caller);
        data[..snapshot.len()].copy_from_slice(&snapshot);
        data[snapshot.len()..].fill(0);
        for (g, v) in &globals {
            g.set(&mut *caller, *v)?;
        }
    }
}

fn is_unreachable_trap(e: &wasmtime::Error) -> bool {
    matches!(e.downcast_ref::<wasmtime::Trap>(), Some(wasmtime::Trap::UnreachableCodeReached))
}

/// The one stderr line a trapped run reports (#1826), in the `Error: `
/// abort form: `Error: stack overflow` for call-stack exhaustion (C-196),
/// otherwise wasmtime's own `Trap` Display — already spelled
/// `wasm trap: <reason>` ("out of bounds memory access", "wasm
/// `unreachable` instruction executed", …) — when the error is a trap,
/// else the chain's root cause under the same prefix. Never the
/// multi-line backtrace.
fn trap_line(e: &wasmtime::Error) -> String {
    match e.downcast_ref::<wasmtime::Trap>() {
        // C-196: call-stack exhaustion is ALS-T6's defined abort, spelled as
        // the native leg spells it (prelude_stack.rs), not as wasmtime's
        // `wasm trap: call stack exhausted`. The depth it happens at stays
        // this host's own ([`EMBEDDED_WASM_STACK`]).
        Some(wasmtime::Trap::StackOverflow) => "Error: stack overflow\n".to_string(),
        Some(t) => format!("Error: {t}\n"),
        None => format!("Error: wasm trap: {}\n", e.root_cause()),
    }
}

#[cfg(test)]
mod tests {
    use super::run_wasm;
    use wasm_encoder::{
        CodeSection, ExportKind, ExportSection, Function, FunctionSection, Instruction,
        MemArg, MemorySection, MemoryType, Module, TypeSection,
    };

    /// A one-function module whose exported `main` runs `body` — the
    /// smallest thing the host will instantiate (no `almide.*` imports).
    fn module(body: &[Instruction<'_>]) -> Vec<u8> {
        let mut types = TypeSection::new();
        types.ty().function([], []);
        let mut funcs = FunctionSection::new();
        funcs.function(0);
        let mut mems = MemorySection::new();
        mems.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);
        exports.export("main", ExportKind::Func, 0);
        let mut code = CodeSection::new();
        let mut f = Function::new([]);
        for op in body {
            f.instruction(op);
        }
        f.instruction(&Instruction::End);
        code.function(&f);
        let mut m = Module::new();
        m.section(&types).section(&funcs).section(&mems).section(&exports).section(&code);
        m.finish()
    }

    /// #1826 defect 2: a trap that reaches the host with no recorded
    /// `almide.exit` is exit 1 AND one stderr line naming the reason —
    /// never a silent 1.
    #[test]
    fn a_trap_names_itself_on_stderr_and_exits_1() {
        let r = run_wasm(&module(&[Instruction::Unreachable])).expect("engine runs the module");
        assert_eq!(r.exit, 1);
        assert_eq!(r.stdout, "");
        assert_eq!(r.stderr, "Error: wasm trap: wasm `unreachable` instruction executed\n");
    }

    #[test]
    fn an_out_of_bounds_access_names_the_memory_trap() {
        let oob = [
            Instruction::I32Const(-1),
            Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }),
            Instruction::Drop,
        ];
        let r = run_wasm(&module(&oob)).expect("engine runs the module");
        assert_eq!(r.exit, 1);
        assert_eq!(r.stderr, "Error: wasm trap: out of bounds memory access\n");
    }

    /// C-196: an unbounded non-tail self-call exhausts the wasm stack, and the
    /// run ends with ALMIDE's abort line for it — the native leg's spelling —
    /// not wasmtime's `wasm trap: call stack exhausted`.
    #[test]
    fn stack_exhaustion_is_the_defined_abort() {
        let r = run_wasm(&module(&[Instruction::Call(0)])).expect("engine runs the module");
        assert_eq!(r.exit, 1);
        assert_eq!(r.stdout, "");
        assert_eq!(r.stderr, "Error: stack overflow\n");
    }

    /// A runaway program is STOPPED, not hung (#2955): the test harness's
    /// bound on execution is the epoch watchdog (the embedded host arms no
    /// wasmtime fuel), and it must still interrupt a diverging `main` on
    /// the pinned wasmtime. Two shapes, because wasmtime checks the epoch
    /// at two places: a loop header (`loop br 0`) and a function entry (a
    /// self `return_call`, the tail-recursion lowering, which has no loop).
    #[test]
    fn a_runaway_program_is_interrupted_by_the_watchdog() {
        let watchdog = Some(std::time::Duration::from_millis(200));
        let shapes: [(&str, &[Instruction<'_>]); 2] = [
            ("loop", &[Instruction::Loop(wasm_encoder::BlockType::Empty), Instruction::Br(0), Instruction::End]),
            ("return_call", &[Instruction::ReturnCall(0)]),
        ];
        for (name, body) in shapes {
            let started = std::time::Instant::now();
            let r = super::run_wasm_src(&module(body), super::StdinSource::Buf(Vec::new()), None, &[], watchdog, false)
                .expect("engine runs the module");
            assert_eq!(r.exit, 1, "{name}: a runaway must abort");
            assert_eq!(r.stderr, "Error: wasm trap: interrupt\n", "{name}");
            assert!(started.elapsed() < std::time::Duration::from_secs(20), "{name}: stopped late");
        }
    }

    /// The happy path is untouched: a clean return is exit 0, empty stderr.
    #[test]
    fn a_clean_return_stays_silent() {
        let r = run_wasm(&module(&[Instruction::Nop])).expect("engine runs the module");
        assert_eq!(r.exit, 0);
        assert_eq!(r.stderr, "");
    }

    /// A hand-made module that imports `almide.<name>` — `(i32, i32) -> ()`
    /// for println/eprintln, `(i32 × 5) -> i64` for fs_call — with `data`
    /// at address 16 of a one-page memory (exported only when `export_memory`),
    /// and a `main` running `body`. The shapes compiler output never makes:
    /// forged pointers and lengths.
    fn calling(name: &str, data: &[u8], export_memory: bool, body: &[Instruction<'_>]) -> Vec<u8> {
        use wasm_encoder::{ConstExpr, DataSection, EntityType, ImportSection, ValType};
        let mut types = TypeSection::new();
        types.ty().function([], []);
        if name == "fs_call" {
            types.ty().function([ValType::I32; 5], [ValType::I64]);
        } else {
            types.ty().function([ValType::I32; 2], []);
        }
        let mut imports = ImportSection::new();
        imports.import("almide", name, EntityType::Function(1));
        let mut funcs = FunctionSection::new();
        funcs.function(0);
        let mut mems = MemorySection::new();
        mems.memory(MemoryType { minimum: 1, maximum: None, memory64: false, shared: false, page_size_log2: None });
        let mut exports = ExportSection::new();
        if export_memory {
            exports.export("memory", ExportKind::Memory, 0);
        }
        exports.export("main", ExportKind::Func, 1);
        let mut code = CodeSection::new();
        let mut f = Function::new([]);
        for op in body {
            f.instruction(op);
        }
        f.instruction(&Instruction::End);
        code.function(&f);
        let mut datas = DataSection::new();
        datas.active(0, &ConstExpr::i32_const(16), data.iter().copied());
        let mut m = Module::new();
        m.section(&types).section(&imports).section(&funcs).section(&mems).section(&exports).section(&code).section(&datas);
        m.finish()
    }

    /// `[i32.const a ...]` then `call 0` (the import), dropping an fs_call's i64.
    fn call0(args: &[i32], drop_result: bool) -> Vec<Instruction<'static>> {
        let mut body: Vec<Instruction<'static>> = args.iter().map(|&a| Instruction::I32Const(a)).collect();
        body.push(Instruction::Call(0));
        if drop_result {
            body.push(Instruction::Drop);
        }
        body
    }

    /// C-399 (#4138): op 30 — `io.write` / `io.write_bytes` — writes the
    /// guest's bytes as they are. Not UTF-8 here: a lone 0xFF, a NUL, 0x80,
    /// a PNG signature's 0x89 'P' 'N' 'G'; the lossy decode this replaced
    /// wrote EF BF BD for each invalid byte.
    #[test]
    fn a_byte_write_reaches_stdout_byte_for_byte() {
        let bytes = [0xFF, 0x00, 0x80, 0x89, b'P', b'N', b'G', 0xFF, 0xFE, b'\n'];
        let body = call0(&[30, 0, 0, 16, bytes.len() as i32], true);
        let r = run_wasm(&calling("fs_call", &bytes, true, &body)).expect("engine runs the module");
        assert_eq!(r.exit, 0, "stderr: {}", r.stderr);
        assert_eq!(r.stdout_bytes, bytes);
        // the stderr twin (op 73) is the same rule
        let body = call0(&[73, 0, 0, 16, bytes.len() as i32], true);
        let r = run_wasm(&calling("fs_call", &bytes, true, &body)).expect("engine runs the module");
        assert_eq!(r.stderr_bytes, bytes);
    }

    /// #4135: a guest pointer or length past the end of memory is the
    /// guest's own out-of-bounds trap — `Error: wasm trap: out of bounds
    /// memory access`, exit 1, as a guest load past the end gives — never a
    /// host panic, and never a host allocation sized by the forged length.
    /// Negative control (2026-10-26): with `read_guest`'s bounds check
    /// removed (the span sliced unchecked) the guest thread panicked at the
    /// slice ("panicked at crates/almide-wasm-run/src/host.rs:417:12") and
    /// this test failed.
    #[test]
    fn a_forged_span_traps_instead_of_panicking_the_host() {
        let oob = "Error: wasm trap: out of bounds memory access\n";
        let page = 65536;
        let cases: [(&str, Vec<Instruction<'static>>); 6] = [
            // println / eprintln: a span crossing the end, a 4 GiB length, a
            // pointer past the end
            ("println", call0(&[page - 2, 100], false)),
            ("println", call0(&[0, -1], false)),
            ("eprintln", call0(&[-16, 4], false)),
            // fs_call: the a-span and the b-span, each forged
            ("fs_call", call0(&[30, -1, 8, 0, 0], true)),
            ("fs_call", call0(&[30, 0, 0, 16, -1], true)),
            ("fs_call", call0(&[1, 0, i32::MAX, 0, 0], true)),
        ];
        for (name, body) in cases {
            let r = run_wasm(&calling(name, b"x", true, &body)).expect("engine runs the module");
            assert_eq!((r.exit, r.stderr.as_str()), (1, oob), "{name} {body:?}");
        }
    }

    /// `[i32.const 29, i32.const 1, i32.add]` then `b`'s span: an op-30 call
    /// (`io.write` of `len` bytes at 16) whose op the operand stack does not
    /// show as a constant.
    fn computed_op_30(len: i32) -> Vec<Instruction<'static>> {
        vec![
            Instruction::I32Const(29),
            Instruction::I32Const(1),
            Instruction::I32Add,
            Instruction::I32Const(0),
            Instruction::I32Const(0),
            Instruction::I32Const(16),
            Instruction::I32Const(len),
            Instruction::Call(0),
            Instruction::Drop,
        ]
    }

    /// Row 19 (#4102): the host serves only the `fs_call` ops the module was
    /// built with. This module's one constant op is 73 (a stderr write); its
    /// second site computes op 30, a stdout write the set does not hold, and
    /// the call traps before anything reaches stdout. Negative control: with
    /// `op_allowed` answering `Ok(())` the write lands and the run exits 0.
    #[test]
    fn an_op_outside_the_modules_set_traps() {
        let mut body = call0(&[73, 0, 0, 16, 1], true);
        body.extend(computed_op_30(1));
        let r = run_wasm(&calling("fs_call", b"x", true, &body)).expect("engine runs the module");
        assert_eq!(r.exit, 1, "stdout: {:?} stderr: {:?}", r.stdout, r.stderr);
        assert_eq!(r.stdout, "", "the refused op wrote");
        assert!(r.stderr.starts_with('x'), "the allowed op 73 ran first: {:?}", r.stderr);
        assert!(r.stderr.contains("almide.fs_call: op 30 is not one this module was built with"), "{:?}", r.stderr);
    }

    /// Row 19: a module whose only `fs_call` op is not a visible constant
    /// gets the empty list, never no list — it fails closed. Negative
    /// control: with `allowed_ops_of` back on `fs_call_ops`' `None` (no list),
    /// op 30 is served and the run exits 0 with `x` on stdout.
    #[test]
    fn a_module_whose_op_is_not_a_constant_fails_closed() {
        let r = run_wasm(&calling("fs_call", b"x", true, &computed_op_30(1))).expect("engine runs the module");
        assert_eq!(r.exit, 1, "stdout: {:?} stderr: {:?}", r.stdout, r.stderr);
        assert_eq!(r.stdout, "", "a computed op was served");
        assert!(r.stderr.contains("almide.fs_call: op 30 is not one this module was built with"), "{:?}", r.stderr);
    }

    /// A module that exports no memory and calls a host op that reads it is
    /// an error of the run, not a host panic.
    #[test]
    fn a_host_op_without_an_exported_memory_is_an_error_not_a_panic() {
        let r = run_wasm(&calling("println", b"x", false, &call0(&[16, 1], false))).expect("engine runs the module");
        assert_eq!(r.exit, 1);
        assert_eq!(r.stderr, "Error: wasm trap: the module exports no memory\n");
    }
}
