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

/// One wasm run's cross-target observables: stdout, stderr, exit code.
/// A trap WITHOUT a recorded `almide.exit` code is a runtime abort
/// (unreachable / div-by-zero / OOB) — exit 1, the native abort contract,
/// and (#1826) ONE stderr line naming it, `Error: wasm trap: <reason>`,
/// never a silent exit 1. Not every gate reads every field (the run
/// manifest hashes stdout only).
#[allow(dead_code)]
pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
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
    out: Arc<Mutex<String>>,
    err: Arc<Mutex<String>>,
    exit: Arc<Mutex<Option<i32>>>,
    /// The fs result parking buffer (host_read copies it to the guest).
    fs_buf: Arc<Mutex<Vec<u8>>>,
    /// The stdin stream (op 35 takes chunks off its cursor); tests run
    /// with a fixed buffer, the runner reads lazily.
    stdin: Arc<Mutex<StdinSource>>,
    /// Program args for op 29 (#1716): framed as [argv0, args...] — the
    /// guest's args arm skips the first frame, matching native argv[1..].
    args: Vec<String>,
    /// Linear-memory budget (heap-budget gates); unlimited by default.
    limits: wasmtime::StoreLimits,
    /// The run's live http calls (#2633) — cancelled when the run ends.
    calls: Arc<Mutex<crate::http_call_host::HttpCalls>>,
    /// `http.serve`'s listener and pending connection (ops 70..=72, #2650).
    serve: Arc<Mutex<crate::host_serve::ServeState>>,
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
}

/// Is the process's stdout a terminal (native flushes per write there).
fn stdout_is_terminal() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdout().is_terminal()
}

/// Append program output: into the capture buffer, or — live — to the
/// real stream with native's buffering rule.
fn emit_out(host: &Host, text: &str) {
    match &host.live_out {
        Some(w) => {
            use std::io::Write as _;
            let mut w = w.lock().expect("live stdout");
            let _ = w.write_all(text.as_bytes());
            // Line-buffered, native's rule (#3417): a line end flushes on any
            // stdout, every write on a terminal.
            if text.contains('\n') || stdout_is_terminal() {
                let _ = w.flush();
            }
        }
        None => host.out.lock().expect("test harness invariant").push_str(text),
    }
}

fn emit_err_line(host: &Host, line: &str) {
    *host.err_last.lock().expect("err last") = line.to_string();
    if host.live_out.is_some() {
        eprintln!("{line}");
    } else {
        let mut o = host.err.lock().expect("test harness invariant");
        o.push_str(line);
        o.push('\n');
    }
}

/// Append to stderr verbatim — op 73, the newline-free twin of
/// [`emit_err_line`].
fn emit_err_raw(host: &Host, text: &str) {
    *host.err_last.lock().expect("err last") = text.to_string();
    if host.live_out.is_some() {
        eprint!("{text}");
    } else {
        host.err.lock().expect("test harness invariant").push_str(text);
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
                use std::io::Read;
                let mut buf = vec![0u8; n];
                match std::io::stdin().read(&mut buf) {
                    Ok(got) => {
                        buf.truncate(got);
                        buf
                    }
                    Err(_) => Vec::new(),
                }
            }
        }
    }
}

include!("host_fs.rs");

fn append_line(
    caller: &mut wasmtime::Caller<'_, Host>,
    sink: fn(&Host) -> &Arc<Mutex<String>>,
    ptr: i32,
    len: i32,
) {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .expect("exported memory");
    let mut buf = vec![0u8; len as u32 as usize];
    if let Err(e) = mem.read(&caller, ptr as u32 as usize, &mut buf) {
        panic!("in-bounds read: {e:?} ptr={ptr} len={len} memsize={}", mem.data_size(&caller));
    }
    let text = String::from_utf8_lossy(&buf);
    if std::ptr::eq(sink(caller.data()), &caller.data().err) {
        emit_err_line(caller.data(), &text);
    } else {
        emit_out(caller.data(), &format!("{text}\n"));
    }
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
    let out = Arc::new(Mutex::new(String::new()));
    let err = Arc::new(Mutex::new(String::new()));
    let exit = Arc::new(Mutex::new(None));
    let fs_buf = Arc::new(Mutex::new(Vec::new()));
    let stdin_buf = Arc::new(Mutex::new(stdin));
    let limits = match max_memory_bytes {
        Some(cap) => wasmtime::StoreLimitsBuilder::new().memory_size(cap).build(),
        None => wasmtime::StoreLimits::default(),
    };
    let mut store = wasmtime::Store::new(
        &engine,
        Host {
            out: out.clone(),
            err: err.clone(),
            exit: exit.clone(),
            fs_buf: fs_buf.clone(),
            stdin: stdin_buf.clone(),
            args: args.to_vec(),
            limits,
            calls: Arc::default(),
            serve: Arc::new(Mutex::new(crate::host_serve::ServeState::default())),
            live_out: live.then(|| Arc::new(Mutex::new(std::io::BufWriter::with_capacity(65536, std::io::stdout())))),
            err_last: Arc::new(Mutex::new(String::new())),
            par: None,
        },
    );
    store.limiter(|h| &mut h.limits);
    let linker = host_linker(&engine)?;
    // #3003 (ADR-0011 §D2a): the instance-parallel fan offer (op 74) runs a
    // chunk on fresh instances of THIS module, through THIS import set.
    let ticked = watchdog.map(|_| Arc::new(std::sync::atomic::AtomicBool::new(false)));
    store.data_mut().par = Some(Arc::new(fan::ParCtx::new(
        (engine.clone(), module.clone(), linker.clone()),
        max_memory_bytes,
        ticked.clone(),
        args.to_vec(),
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
                emit_err_line(store.data(), trap_line(e).trim_end_matches('\n'));
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
    Ok(RunResult {
        stdout: out.lock().expect("test harness invariant").clone(),
        stderr: err.lock().expect("test harness invariant").clone(),
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
        |mut caller: wasmtime::Caller<'_, Host>, ptr: i32, len: i32| {
            append_line(&mut caller, |h| &h.out, ptr, len);
        },
    )?;
    linker.func_wrap(
        "almide",
        "eprintln",
        |mut caller: wasmtime::Caller<'_, Host>, ptr: i32, len: i32| {
            append_line(&mut caller, |h| &h.err, ptr, len);
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
            let mem = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("exported memory");
            let mut a = vec![0u8; a_len as u32 as usize];
            mem.read(&caller, a_ptr as u32 as usize, &mut a)?;
            let mut b = vec![0u8; b_len as u32 as usize];
            mem.read(&caller, b_ptr as u32 as usize, &mut b)?;
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
                emit_out(caller.data(), &String::from_utf8_lossy(&b));
                return Ok(0);
            }
            // op 73 = raw stderr append (`panic`'s line, #2769): no newline.
            if op == 73 {
                emit_err_raw(caller.data(), &String::from_utf8_lossy(&b));
                return Ok(0);
            }
            // http.serve (#2650): the listener and the held connection
            // live in the run's own state — one per run, like native's.
            if (crate::host_serve::OP_SERVE_BIND..=crate::host_serve::OP_SERVE_REPLY).contains(&op) {
                let (ret, buf) = crate::host_serve::dispatch(&caller.data().serve, caller.data().live_out.as_ref(), op, &a, frames, parse_http_frame);
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
            // op 53 = open an http call (#2633): url in a, the start frame in b.
            let (ret, buf) = if op == 53 {
                crate::http_call_host::open(&caller.data().calls.clone(), &a, &b)
            } else {
                fs_dispatch(op, &a, &b)
            };
            *caller.data().fs_buf.lock().expect("fs buf") = buf;
            Ok(ret)
        },
    )?;
    linker.func_wrap(
        "almide",
        "host_read",
        |mut caller: wasmtime::Caller<'_, Host>, dst: i32| -> wasmtime::Result<()> {
            let mem = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("exported memory");
            let buf = caller.data().fs_buf.lock().expect("fs buf").clone();
            mem.write(&mut caller, dst as u32 as usize, &buf)?;
            Ok(())
        },
    )?;
    Ok(linker)
}

/// The `unreachable` trap — the instruction the die lowering ends on.
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
}
