//! Almide generative fuzzer (Stage 3).
//!
//! Continuously synthesizes / mutates well-typed Almide programs and runs
//! them through the oracle ladder, hunting observable divergences, codegen
//! failures, and hangs. Every program is reproducible from
//! `(seed, index, family)`.
//!
//! Two kinds of oracle live here. The DIFFERENTIAL one compares native
//! against wasm (with the interpreter as an abstaining third judge) and is
//! structurally blind to a bug the two legs share — the #1322 class. The
//! BY-CONSTRUCTION one (`--family identity`, #1332) generates programs
//! whose expected output is a literal in their own source, so a leg is
//! judged alone and unanimity is not a defense.
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
use generator::{Engine, Family};
use oracle::{run_ladder, FindingKind, Outcome, Rung, Toolchain};

/// Default per-program timeout. Generated programs are tiny and finite;
/// outrunning this budget makes a leg a SUSPECT, which one confirm re-run
/// at 10x then classifies: completed = Slow (perf-class), still over =
/// Hang (#1235 — the 0.57.0 release gate hit exactly this boundary).
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
        "ladder" => cmd_ladder(&args[2..]),
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
        "xtarget-fuzz — Almide generative fuzzer\n\n\
         USAGE:\n\
         \x20 xtarget-fuzz run    [--seed N] [--minutes M | --count N] [--jobs J] [--timeout S] [--family F] [--dump-walls DIR]\n\
         \x20 xtarget-fuzz replay --seed N --index I [--family F]\n\
         \x20 xtarget-fuzz ladder <file.almd> [--timeout S]\n\
         \x20 xtarget-fuzz gen    --seed N --index I [--family F]\n\
         \x20 xtarget-fuzz stats\n\n\
         --family all|identity|synthesis  (default: all)\n\
         \x20 `identity` runs ONLY the self-checking family (#1332): programs built\n\
         \x20 backwards from a known answer, so a leg is judged alone and a bug the\n\
         \x20 two backends SHARE is still convicted. It is part of the `all` mix too.\n\
         \x20 A (seed, index) pair only reproduces under the same --family, which is\n\
         \x20 why every finding's meta.txt records it.\n\n\
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

/// Read `--family`, defaulting to the full mix. An unknown value is a
/// hard error rather than a silent fallback: silently running `all` when
/// the operator asked for `identity` would misreport what a campaign
/// actually covered.
fn resolve_family(args: &[String]) -> Family {
    match flag_value(args, "--family") {
        None => Family::All,
        Some(s) => Family::parse(s).unwrap_or_else(|| {
            eprintln!("unknown --family {s:?} (expected: all, identity, synthesis)");
            std::process::exit(2);
        }),
    }
}

// ── run ──

fn cmd_run(args: &[String]) {
    let repo = resolve_repo(args);
    sweep_stale_scratch(&repo);
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

    // Wall-specimen dump (#1527): walls are aggregated as reasons, which is
    // the right ledger for a verdict but useless for BURN-DOWN — graduating a
    // wall family needs the walled programs themselves (to shrink into the
    // voting fixture the shrink-only rule demands). `--dump-walls DIR` writes
    // each walled program's source as `wall-<index>-<reason-slug>.almd`;
    // everything stays reproducible from (seed, index) regardless.
    let dump_walls: Option<PathBuf> = flag_value(args, "--dump-walls").map(PathBuf::from);
    if let Some(d) = &dump_walls {
        std::fs::create_dir_all(d).expect("create --dump-walls dir");
    }

    let family = resolve_family(args);

    eprintln!("xtarget-fuzz campaign");
    eprintln!("  repo     = {}", repo.display());
    eprintln!("  almide   = {}", almide.display());
    eprintln!("  seed     = {seed}");
    eprintln!("  family   = {}", family.label());
    eprintln!("  jobs     = {jobs}");
    eprintln!("  timeout  = {}s/program", timeout.as_secs());
    match (count, &budget) {
        (Some(c), _) => eprintln!("  budget   = {c} programs"),
        (_, Some(b)) => eprintln!("  budget   = {}s", b.as_secs()),
        _ => {}
    }

    let engine = Arc::new(Engine::with_family(&repo, family));
    eprintln!(
        "  catalogue= {} stdlib signatures, {} corpus programs\n",
        engine.catalogue_len(),
        engine.corpus_len()
    );

    let sink = Arc::new(
        FindingSink::new(out_dir.clone(), family.label())
            .expect("create findings dir"),
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
            dump_walls: dump_walls.clone(),
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
    let _ = std::fs::remove_dir_all(scratch_root(&repo));

    // Exit code carries the finding CLASS split (#1235): correctness
    // findings exit 1; a campaign whose only findings are perf-class Slow
    // exits 3, so a local caller can branch on it. (The nightly verdict
    // does its own split from the finding directory-name prefixes and
    // fails the night on correctness classes only.)
    let slow = sink.slow_count();
    if sink.count() > slow {
        std::process::exit(1);
    }
    if slow > 0 {
        std::process::exit(3);
    }
}

/// Per-worker campaign configuration (the parts that do not move).
#[derive(Clone)]
struct WorkerCfg {
    seed: u64,
    deadline: Option<Instant>,
    max_count: Option<u64>,
    /// `--dump-walls`: where walled programs' sources land, if anywhere.
    dump_walls: Option<PathBuf>,
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
        if gen.expected_stdout.is_some() {
            stats.self_checked.fetch_add(1, Ordering::Relaxed);
        }
        let outcome = run_ladder(
            &tc,
            &gen.source,
            &file,
            &wasm,
            Some(&reference),
            gen.expected_stdout.as_deref(),
        );

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
                if let Some(dir) = &cfg.dump_walls {
                    let slug: String = reason
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                        .take(64)
                        .collect();
                    let _ = std::fs::write(
                        dir.join(format!("wall-{index:05}-{slug}.almd")),
                        &gen.source,
                    );
                }
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
                // Minimize before recording so the artifact is small. The
                // recorded evidence is the MINIMIZED program's own run
                // whenever it shrank — `repro.almd` and `native.out` /
                // `wasm.out` must describe the same program, or a triager
                // reasons about output the repro never produced.
                //
                // Timeout-class findings are NOT minimized: with the #1235
                // confirm re-run, every shrink candidate that still blows
                // the budget costs (1 + 10)x the timeout — one pass would
                // eat the whole shard. Generated programs are small by
                // construction, so the original stands as the repro.
                //
                // An IDENTITY program is shrunk through its PLAN instead
                // (#1332): deleting a line from an identity program deletes
                // half of an inverse pair, which changes the value the
                // program is supposed to print — the text minimizer would
                // "reproduce" for the wrong reason and turn a miscompile
                // into a generator artifact.
                let minimized = if matches!(finding.kind, FindingKind::Hang | FindingKind::Slow) {
                    minimize::Minimized { source: gen.source.clone(), finding: None }
                } else if let Some(plan) = &gen.plan {
                    minimize::minimize_plan(&tc, plan, finding.kind, &work_dir, Some(&reference))
                } else {
                    minimize::minimize(&tc, &gen.source, finding.kind, &work_dir, Some(&reference))
                };
                let evidence = minimized.finding.as_ref().unwrap_or(&finding);
                let was_new = sink.record(
                    cfg.seed,
                    index,
                    &gen.origin,
                    &gen.source,
                    &minimized.source,
                    evidence,
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
        // Two-step native leg, same as the ladder's rung (c): a rustc-phase
        // timeout is toolchain noise (skip), only the program's own wall-clock
        // and observables count.
        let vbin = work_dir.join("prog_metamorph.nativebin");
        let build = tc.build_native(&vfile, &vbin);
        if !build.success() {
            continue; // build noise/timeout — no program observable to compare
        }
        let run = tc.run_native_bin(&vbin);
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

/// The invocation-unique scratch ROOT (#1532). Every path under `.scratch/`
/// is keyed by this process's pid, so CONCURRENT invocations — two ladders
/// in two terminals, a replay racing a campaign, parallel CI lanes — never
/// share a cargo build dir or a `replay.almd`. Shared scratch manufactured
/// BOGUS DIVERGENCES: one invocation's stale binary answered another's
/// fresh source, and the ladder minted a "finding" with no program
/// divergence in it (the 2026-08-18 sweep's parallel-safety note; the
/// attack list carried it as A2-1's second wedge). Stale pid dirs from
/// killed invocations are swept by [`sweep_stale_scratch`] on startup.
fn scratch_root(repo: &Path) -> PathBuf {
    repo.join(format!("tools/xtarget-fuzz/.scratch/pid-{}", std::process::id()))
}

/// Best-effort startup sweep: remove sibling `pid-*` dirs untouched for a
/// day — their owning process is long gone (a live campaign touches its
/// scratch constantly). Never touches the CURRENT pid's dir.
fn sweep_stale_scratch(repo: &Path) {
    let root = repo.join("tools/xtarget-fuzz/.scratch");
    let own = format!("pid-{}", std::process::id());
    let cutoff = std::time::SystemTime::now() - Duration::from_secs(24 * 3600);
    let Ok(entries) = std::fs::read_dir(&root) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with("pid-") || name == own {
            continue;
        }
        let stale = e
            .metadata()
            .and_then(|m| m.modified())
            .map(|t| t < cutoff)
            .unwrap_or(true);
        if stale {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// Per-program scratch dir for native cargo builds (isolated per worker
/// so the shared-`/tmp` build flock never serializes workers).
fn worker_scratch(repo: &Path, worker_id: usize) -> PathBuf {
    scratch_root(repo).join(format!("build-{worker_id}"))
}

/// Per-program source/artifact dir for a worker.
fn worker_work_dir(repo: &Path, worker_id: usize) -> PathBuf {
    scratch_root(repo).join(format!("work-{worker_id}"))
}

// ── replay ──

fn cmd_replay(args: &[String]) {
    let repo = resolve_repo(args);
    sweep_stale_scratch(&repo);
    let almide = resolve_almide(&repo, args);
    let seed: u64 = flag_value(args, "--seed")
        .and_then(|s| s.parse().ok())
        .expect("replay requires --seed");
    let index: u64 = flag_value(args, "--index")
        .and_then(|s| s.parse().ok())
        .expect("replay requires --index");

    let engine = Engine::with_family(&repo, resolve_family(args));
    let gen = engine.generate(seed, index);
    println!("// seed={seed} index={index} origin={:?}\n", gen.origin);
    println!("{}", gen.source);

    let work_dir = scratch_root(&repo).join("replay");
    let _ = std::fs::create_dir_all(&work_dir);
    let file = work_dir.join("replay.almd");
    let wasm = work_dir.join("replay.wasm");
    let _ = std::fs::write(&file, &gen.source);

    let reference = crate::oracle::InterpOracle::new();
    let tc = Toolchain {
        almide,
        wasmtime: resolve_wasmtime(),
        scratch: scratch_root(&repo).join("replay-build"),
        timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
    };
    let outcome = run_ladder(
        &tc,
        &gen.source,
        &file,
        &wasm,
        Some(&reference),
        gen.expected_stdout.as_deref(),
    );
    eprintln!("\n=== ladder outcome ===");
    print_outcome(&outcome);
    let _ = std::fs::remove_dir_all(scratch_root(&repo));
}

// ── ladder ──

/// Run the full oracle ladder on an EXISTING `.almd` file — the triage
/// instrument (#1235). A recorded finding's `repro.almd` can be re-judged
/// after a classifier change without going through `(seed, index)` replay,
/// which silently drifts whenever the mutation corpus changes underneath
/// the seed. Exits 1 on any finding, 0 otherwise.
fn cmd_ladder(args: &[String]) {
    let Some(file_arg) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: xtarget-fuzz ladder <file.almd> [--timeout S]");
        std::process::exit(2);
    };
    let source = match std::fs::read_to_string(file_arg) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {file_arg}: {e}");
            std::process::exit(2);
        }
    };
    let repo = resolve_repo(args);
    sweep_stale_scratch(&repo);
    let almide = resolve_almide(&repo, args);
    let timeout = flag_value(args, "--timeout")
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS);

    let work_dir = scratch_root(&repo).join("ladder");
    let _ = std::fs::create_dir_all(&work_dir);
    let file = work_dir.join("ladder.almd");
    let wasm = work_dir.join("ladder.wasm");
    let _ = std::fs::write(&file, &source);

    let reference = crate::oracle::InterpOracle::new();
    let tc = Toolchain {
        almide,
        wasmtime: resolve_wasmtime(),
        scratch: scratch_root(&repo).join("ladder-build"),
        timeout: Duration::from_secs(timeout),
    };
    // An identity-family repro carries its own oracle in `// @expect`
    // header lines, so `ladder <repro.almd>` re-judges it exactly as the
    // campaign did — no `(seed, index)` and no family flag needed.
    let expected = generator::identity::expected_from_source(&source);
    if expected.is_some() {
        eprintln!("(self-checking program: judging against its declared @expect output)");
    }
    let outcome = run_ladder(
        &tc,
        &source,
        &file,
        &wasm,
        Some(&reference),
        expected.as_deref(),
    );
    print_outcome(&outcome);
    let _ = std::fs::remove_dir_all(scratch_root(&repo));
    if matches!(outcome, Outcome::Finding(_)) {
        std::process::exit(1);
    }
}

// ── gen ──

fn cmd_gen(args: &[String]) {
    let repo = resolve_repo(args);
    sweep_stale_scratch(&repo);
    let seed: u64 = flag_value(args, "--seed")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let index: u64 = flag_value(args, "--index")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let engine = Engine::with_family(&repo, resolve_family(args));
    let gen = engine.generate(seed, index);
    print!("{}", gen.source);
}

// ── stats ──

fn cmd_stats() {
    let repo = resolve_repo(&[]);
    let engine = Engine::new(&repo);
    println!("repo            = {}", repo.display());
    println!("family          = {}", engine.family().label());
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
    /// Programs judged by the BY-CONSTRUCTION oracle (#1332) rather than
    /// only differentially. This is the campaign's honest answer to "how
    /// much of tonight could have caught a bug both backends share".
    self_checked: AtomicU64,
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
    let oracled = stats.self_checked.load(Ordering::Relaxed);
    eprintln!(
        "  self-checked     = {oracled} ({:.0}% — judged by the by-construction oracle, \
         so a shared-lowering bug is convictable)",
        if g == 0 { 0.0 } else { oracled as f64 / g as f64 * 100.0 }
    );
    let walls = stats.walled.load(Ordering::Relaxed);
    eprintln!("  walls (subset)   = {walls}");
    let slow = sink.slow_count();
    if slow > 0 {
        eprintln!(
            "  unique findings  = {} ({} correctness, {} perf-slow)",
            sink.count(),
            sink.count() - slow,
            slow
        );
    } else {
        eprintln!("  unique findings  = {}", sink.count());
    }
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
                eprintln!("--- native stdout ({:.1}s) ---\n{}", n.duration_secs, n.stdout);
            }
            if let Some(w) = &f.wasm {
                eprintln!("--- wasm stdout ({:.1}s) ---\n{}", w.duration_secs, w.stdout);
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
        Rung::SelfCheck => "self-check",
    }
}
