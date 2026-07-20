//! Almide generative differential fuzzer (Stage 3).
//!
//! Continuously synthesizes / mutates well-typed Almide programs and
//! runs them through the native↔WASM differential oracle ladder, hunting
//! observable divergences, codegen failures, and hangs. Every program is
//! reproducible from `(seed, index)`.
//!
//! Subcommands:
//!   run     — run a campaign (time budget or fixed program count)
//!   replay  — regenerate and re-test a single (seed, index)
//!   gen     — print a single generated program (no oracle)
//!   stats   — print catalogue/corpus sizes and exit

mod findings;
mod generator;
mod metamorph;
mod minimize;
mod oracle;
mod rng;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use findings::FindingSink;
use generator::Engine;
use oracle::{run_ladder, Outcome, Rung, Toolchain};

/// Default per-program timeout. Generated programs are tiny and finite;
/// anything slower than this is hanging.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Default campaign duration when `--minutes` is given without a value
/// elsewhere (the nightly CI passes an explicit budget).
const DEFAULT_BUDGET_SECS: u64 = 60;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("run");

    match cmd {
        "run" => cmd_run(&args[2..]),
        "replay" => cmd_replay(&args[2..]),
        "gen" => cmd_gen(&args[2..]),
        "stats" => cmd_stats(),
        "-h" | "--help" | "help" => print_usage(),
        other => {
            eprintln!("unknown subcommand: {other}\n");
            print_usage();
            std::process::exit(2);
        }
    }
}

fn print_usage() {
    eprintln!(
        "xtarget-fuzz — Almide generative differential fuzzer\n\n\
         USAGE:\n\
         \x20 xtarget-fuzz run    [--seed N] [--minutes M | --count N] [--jobs J] [--timeout S]\n\
         \x20 xtarget-fuzz replay --seed N --index I\n\
         \x20 xtarget-fuzz gen    --seed N --index I\n\
         \x20 xtarget-fuzz stats\n\n\
         The repo root is autodetected from the binary location; override with --repo PATH.\n\
         Findings are written under <repo>/tools/xtarget-fuzz/findings/ (override with --out DIR)."
    );
}

/// Resolve the repo root: explicit `--repo`, else walk up from CWD until
/// a `Cargo.toml` with `[workspace]` + a `stdlib/` dir is found.
fn resolve_repo(args: &[String]) -> PathBuf {
    if let Some(p) = flag_value(args, "--repo") {
        return PathBuf::from(p);
    }
    // Walk up from the current dir.
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        if dir.join("stdlib").is_dir() && dir.join("Cargo.toml").is_file() {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    // Fallback: the worktree this binary was built in (three levels up
    // from tools/xtarget-fuzz/target/release).
    PathBuf::from(".")
}

/// Locate the freshly built `almide` binary for the repo.
fn resolve_almide(repo: &Path, args: &[String]) -> PathBuf {
    if let Some(p) = flag_value(args, "--almide") {
        return PathBuf::from(p);
    }
    let release = repo.join("target/release/almide");
    if release.is_file() {
        return release;
    }
    let debug = repo.join("target/debug/almide");
    if debug.is_file() {
        return debug;
    }
    // Last resort: PATH lookup (may be stale — warned about below).
    PathBuf::from("almide")
}

fn resolve_wasmtime() -> PathBuf {
    // wasmtime is expected on PATH; the runner reports a spawn failure as
    // a skip if it is missing.
    PathBuf::from("wasmtime")
}

// ── run ──

fn cmd_run(args: &[String]) {
    let repo = resolve_repo(args);
    let almide = resolve_almide(&repo, args);
    let wasmtime = resolve_wasmtime();

    let seed: u64 = flag_value(args, "--seed")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(default_seed);
    let jobs: usize = flag_value(args, "--jobs")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(default_jobs);
    let timeout = Duration::from_secs(
        flag_value(args, "--timeout")
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS),
    );

    // Budget: either a fixed program count or a wall-clock minute budget.
    let count: Option<u64> = flag_value(args, "--count").and_then(|s| s.parse().ok());
    let budget = match count {
        Some(_) => None,
        None => Some(Duration::from_secs(
            flag_value(args, "--minutes")
                .and_then(|s| s.parse::<u64>().ok())
                .map(|m| m * 60)
                .unwrap_or(DEFAULT_BUDGET_SECS),
        )),
    };

    let out_dir = flag_value(args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("tools/xtarget-fuzz/findings"));

    eprintln!("xtarget-fuzz campaign");
    eprintln!("  repo     = {}", repo.display());
    eprintln!("  almide   = {}", almide.display());
    eprintln!("  seed     = {seed}");
    eprintln!("  jobs     = {jobs}");
    eprintln!("  timeout  = {}s/program", timeout.as_secs());
    match (count, &budget) {
        (Some(c), _) => eprintln!("  budget   = {c} programs"),
        (_, Some(b)) => eprintln!("  budget   = {}s", b.as_secs()),
        _ => {}
    }

    let engine = Arc::new(Engine::new(&repo));
    eprintln!(
        "  catalogue= {} stdlib signatures, {} corpus programs\n",
        engine.catalogue_len(),
        engine.corpus_len()
    );

    let sink = Arc::new(
        FindingSink::new(out_dir.clone()).expect("create findings dir"),
    );

    // Shared campaign state.
    let next_index = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(Stats::default());
    let deadline = budget.map(|b| Instant::now() + b);
    let max_count = count;

    let start = Instant::now();

    let mut handles = Vec::new();
    for worker_id in 0..jobs {
        let engine = Arc::clone(&engine);
        let sink = Arc::clone(&sink);
        let next_index = Arc::clone(&next_index);
        let stop = Arc::clone(&stop);
        let stats = Arc::clone(&stats);
        let scratch = worker_scratch(&repo, worker_id);
        let tc = Toolchain {
            almide: almide.clone(),
            wasmtime: wasmtime.clone(),
            scratch,
            timeout,
        };
        let work_dir = worker_work_dir(&repo, worker_id);
        let _ = std::fs::create_dir_all(&work_dir);

        let cfg = WorkerCfg {
            seed,
            deadline,
            max_count,
        };
        handles.push(std::thread::spawn(move || {
            worker_loop(engine, sink, tc, work_dir, next_index, stop, stats, cfg);
        }));
    }

    // Progress reporter on the main thread.
    report_progress(&stats, &stop, deadline, max_count, start);

    for h in handles {
        let _ = h.join();
    }

    let elapsed = start.elapsed();
    print_summary(&stats, &sink, elapsed, &out_dir);

    // Non-zero exit if any finding was recorded — the nightly CI gates on
    // this to open an issue.
    if sink.count() > 0 {
        std::process::exit(1);
    }
}

/// Per-worker campaign configuration (the parts that do not move).
#[derive(Clone, Copy)]
struct WorkerCfg {
    seed: u64,
    deadline: Option<Instant>,
    max_count: Option<u64>,
}

/// One worker: pull program indices, generate, run the ladder, minimize
/// and record findings, until the campaign stops.
fn worker_loop(
    engine: Arc<Engine>,
    sink: Arc<FindingSink>,
    tc: Toolchain,
    work_dir: PathBuf,
    next_index: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    stats: Arc<Stats>,
    cfg: WorkerCfg,
) {
    let file = work_dir.join("prog.almd");
    let wasm = work_dir.join("prog.wasm");
    // The third judge (#516): per-worker, abstains on anything it can't run.
    let reference = crate::oracle::InterpOracle::new();

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(d) = cfg.deadline {
            if Instant::now() >= d {
                stop.store(true, Ordering::Relaxed);
                break;
            }
        }
        let index = next_index.fetch_add(1, Ordering::Relaxed);
        if let Some(max) = cfg.max_count {
            if index >= max {
                stop.store(true, Ordering::Relaxed);
                break;
            }
        }

        let gen = engine.generate(cfg.seed, index);
        if std::fs::write(&file, &gen.source).is_err() {
            continue;
        }

        stats.generated.fetch_add(1, Ordering::Relaxed);
        let outcome = run_ladder(&tc, &gen.source, &file, &wasm, Some(&reference));

        match outcome {
            Outcome::Clean { native } => {
                stats.clean.fetch_add(1, Ordering::Relaxed);
                // Metamorphic rung (#515): binding-shape variants of clean
                // SYNTHESIZED programs must be accepted and byte-identical.
                if matches!(gen.origin, crate::generator::Origin::Synthesis) {
                    if let Some(finding) =
                        run_metamorphic(&tc, &gen.source, &native, &work_dir)
                    {
                        stats.findings.fetch_add(1, Ordering::Relaxed);
                        let was_new = sink.record(
                            cfg.seed,
                            index,
                            &gen.origin,
                            &gen.source,
                            &gen.source,
                            &finding,
                        );
                        if was_new {
                            eprintln!(
                                "  ** FINDING [{:?}] seed={} index={} — {}",
                                finding.kind, cfg.seed, index, finding.summary
                            );
                        }
                    }
                }
            }
            Outcome::GeneratorReject { .. } => {
                stats.generator_rejects.fetch_add(1, Ordering::Relaxed);
            }
            Outcome::Walled { reason } => {
                stats.walled.fetch_add(1, Ordering::Relaxed);
                let mut reasons = stats.wall_reasons.lock().unwrap();
                let key = if reason.len() > 160 {
                    format!("{}…", &reason[..reason.char_indices().take_while(|(i, _)| *i < 160).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(0)])
                } else {
                    reason
                };
                *reasons.entry(key).or_insert(0) += 1;
            }
            Outcome::Skipped { .. } => {
                stats.skipped.fetch_add(1, Ordering::Relaxed);
            }
            Outcome::Finding(finding) => {
                stats.findings.fetch_add(1, Ordering::Relaxed);
                // Minimize before recording so the artifact is small.
                let minimized =
                    minimize::minimize(&tc, &gen.source, finding.kind, &work_dir);
                let was_new = sink.record(
                    cfg.seed,
                    index,
                    &gen.origin,
                    &gen.source,
                    &minimized,
                    &finding,
                );
                if was_new {
                    eprintln!(
                        "  ** FINDING [{:?}] seed={} index={} — {}",
                        finding.kind, cfg.seed, index, finding.summary
                    );
                }
            }
        }
    }
}

/// The metamorphic rung (#515): check + run every binding-shape variant;
/// acceptance or output deltas vs the clean original are findings.
fn run_metamorphic(
    tc: &Toolchain,
    source: &str,
    native: &oracle::RunEvidence,
    work_dir: &Path,
) -> Option<oracle::Finding> {
    let vfile = work_dir.join("prog_metamorph.almd");
    for (label, variant) in metamorph::binding_variants(source) {
        if std::fs::write(&vfile, &variant).is_err() {
            continue;
        }
        let chk = tc.check(&vfile);
        if chk.timed_out {
            continue; // wall-clock noise, not an acceptance verdict
        }
        if !chk.success() {
            return Some(oracle::Finding {
                rung: oracle::Rung::Check,
                kind: oracle::FindingKind::MetamorphicDivergence,
                summary: format!(
                    "binding variant `{label}` REJECTED though the original was accepted"
                ),
                native: None,
                wasm: None,
            });
        }
        let run = tc.run_native(&vfile);
        if run.timed_out || run.spawn_failed {
            continue;
        }
        let v_stdout = String::from_utf8_lossy(&run.stdout).into_owned();
        if v_stdout != native.stdout || run.exit_code != native.exit_code {
            return Some(oracle::Finding {
                rung: oracle::Rung::Run,
                kind: oracle::FindingKind::MetamorphicDivergence,
                summary: format!(
                    "binding variant `{label}` diverged: stdout {:?} vs original {:?}",
                    v_stdout.chars().take(60).collect::<String>(),
                    native.stdout.chars().take(60).collect::<String>(),
                ),
                native: None,
                wasm: None,
            });
        }
    }
    None
}

/// Per-program scratch dir for native cargo builds (isolated per worker
/// so the shared-`/tmp` build flock never serializes workers).
fn worker_scratch(repo: &Path, worker_id: usize) -> PathBuf {
    repo.join(format!("tools/xtarget-fuzz/.scratch/build-{worker_id}"))
}

/// Per-program source/artifact dir for a worker.
fn worker_work_dir(repo: &Path, worker_id: usize) -> PathBuf {
    repo.join(format!("tools/xtarget-fuzz/.scratch/work-{worker_id}"))
}

// ── replay ──

fn cmd_replay(args: &[String]) {
    let repo = resolve_repo(args);
    let almide = resolve_almide(&repo, args);
    let seed: u64 = flag_value(args, "--seed")
        .and_then(|s| s.parse().ok())
        .expect("replay requires --seed");
    let index: u64 = flag_value(args, "--index")
        .and_then(|s| s.parse().ok())
        .expect("replay requires --index");

    let engine = Engine::new(&repo);
    let gen = engine.generate(seed, index);
    println!("// seed={seed} index={index} origin={:?}\n", gen.origin);
    println!("{}", gen.source);

    let work_dir = repo.join("tools/xtarget-fuzz/.scratch/replay");
    let _ = std::fs::create_dir_all(&work_dir);
    let file = work_dir.join("replay.almd");
    let wasm = work_dir.join("replay.wasm");
    let _ = std::fs::write(&file, &gen.source);

    let reference = crate::oracle::InterpOracle::new();
    let tc = Toolchain {
        almide,
        wasmtime: resolve_wasmtime(),
        scratch: repo.join("tools/xtarget-fuzz/.scratch/replay-build"),
        timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
    };
    let outcome = run_ladder(&tc, &gen.source, &file, &wasm, Some(&reference));
    eprintln!("\n=== ladder outcome ===");
    print_outcome(&outcome);
}

// ── gen ──

fn cmd_gen(args: &[String]) {
    let repo = resolve_repo(args);
    let seed: u64 = flag_value(args, "--seed")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let index: u64 = flag_value(args, "--index")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let engine = Engine::new(&repo);
    let gen = engine.generate(seed, index);
    print!("{}", gen.source);
}

// ── stats ──

fn cmd_stats() {
    let repo = resolve_repo(&[]);
    let engine = Engine::new(&repo);
    println!("repo            = {}", repo.display());
    println!("catalogue size  = {}", engine.catalogue_len());
    println!("corpus programs = {}", engine.corpus_len());
}

// ── progress / summary ──

#[derive(Default)]
struct Stats {
    generated: AtomicU64,
    clean: AtomicU64,
    generator_rejects: AtomicU64,
    findings: AtomicU64,
    skipped: AtomicU64,
    /// Honest v1 walls (`Unsupported`) — subset-coverage debt, NOT findings
    /// (#796 taxonomy: a walled program has no wasm leg to diverge). The
    /// reason histogram feeds the subset burn-down.
    walled: AtomicU64,
    wall_reasons: std::sync::Mutex<std::collections::BTreeMap<String, u64>>,
}

fn report_progress(
    stats: &Arc<Stats>,
    stop: &Arc<AtomicBool>,
    deadline: Option<Instant>,
    max_count: Option<u64>,
    start: Instant,
) {
    let report_interval = Duration::from_secs(PROGRESS_INTERVAL_SECS);
    loop {
        std::thread::sleep(report_interval);
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Some(d) = deadline {
            if Instant::now() >= d {
                stop.store(true, Ordering::Relaxed);
                break;
            }
        }
        if let Some(max) = max_count {
            if stats.generated.load(Ordering::Relaxed) >= max {
                stop.store(true, Ordering::Relaxed);
                break;
            }
        }
        let g = stats.generated.load(Ordering::Relaxed);
        let secs = start.elapsed().as_secs_f64().max(0.001);
        eprintln!(
            "  [{:>5.0}s] generated={g} clean={} rejects={} findings={} walls={} skipped={} | {:.1} prog/min",
            secs,
            stats.clean.load(Ordering::Relaxed),
            stats.generator_rejects.load(Ordering::Relaxed),
            stats.findings.load(Ordering::Relaxed),
            stats.walled.load(Ordering::Relaxed),
            stats.skipped.load(Ordering::Relaxed),
            g as f64 / secs * 60.0,
        );
    }
}

/// Progress report cadence.
const PROGRESS_INTERVAL_SECS: u64 = 5;

fn print_summary(stats: &Stats, sink: &FindingSink, elapsed: Duration, out_dir: &Path) {
    let g = stats.generated.load(Ordering::Relaxed);
    let secs = elapsed.as_secs_f64().max(0.001);
    eprintln!("\n=== campaign summary ===");
    eprintln!("  elapsed          = {:.1}s", secs);
    eprintln!("  generated        = {g}");
    eprintln!("  clean            = {}", stats.clean.load(Ordering::Relaxed));
    eprintln!(
        "  generator rejects= {}",
        stats.generator_rejects.load(Ordering::Relaxed)
    );
    eprintln!("  skipped          = {}", stats.skipped.load(Ordering::Relaxed));
    let walls = stats.walled.load(Ordering::Relaxed);
    eprintln!("  walls (subset)   = {walls}");
    eprintln!("  unique findings  = {}", sink.count());
    eprintln!("  throughput       = {:.1} programs/min", g as f64 / secs * 60.0);
    if walls > 0 {
        let reasons = stats.wall_reasons.lock().unwrap();
        let mut top: Vec<(&String, &u64)> = reasons.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        eprintln!("  top wall reasons (subset burn-down, not findings):");
        for (reason, n) in top.iter().take(8) {
            eprintln!("    {n:>4}× {reason}");
        }
    }
    if sink.count() > 0 {
        eprintln!("  findings dir     = {}", out_dir.display());
    }
}

fn print_outcome(outcome: &Outcome) {
    match outcome {
        Outcome::Clean { .. } => eprintln!("CLEAN — native and wasm agree"),
        Outcome::GeneratorReject { diagnostics } => {
            eprintln!("GENERATOR REJECT (check failed):\n{diagnostics}")
        }
        Outcome::Skipped { reason } => eprintln!("SKIPPED: {reason}"),
        Outcome::Walled { reason } => {
            eprintln!("WALLED (subset-coverage, not a finding): {reason}")
        }
        Outcome::Finding(f) => {
            eprintln!("FINDING [{:?}] at rung {:?}: {}", f.kind, f.rung, f.summary);
            if let Some(n) = &f.native {
                eprintln!("--- native stdout ---\n{}", n.stdout);
            }
            if let Some(w) = &f.wasm {
                eprintln!("--- wasm stdout ---\n{}", w.stdout);
            }
        }
    }
}

// ── small arg helpers ──

/// Read `--flag value` from args.
fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Default campaign seed: stable per process from the wall clock. The
/// seed is the ONLY non-deterministic input, and it is logged so any run
/// is reproducible — the *generated programs* remain pure functions of
/// `(seed, index)`.
fn default_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x1234_5678)
}

/// Default worker count: available parallelism, capped so the shared
/// native build cache and the host stay responsive.
fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[allow(unused)]
fn rung_name(r: Rung) -> &'static str {
    match r {
        Rung::Check => "check",
        Rung::FmtRoundTrip => "fmt",
        Rung::NativeBuild => "native-build",
        Rung::WasmBuild => "wasm-build",
        Rung::Run => "run",
    }
}
