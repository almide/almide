//! `almide bench` (#1490 item 3): the perf suite's methodology as a
//! user-facing subcommand — verify before timing, interleave-free single
//! program, median headline.
//!
//! The methodology is the asset (research/benchmark/perf/bench.py):
//! 1. run once and take the stdout as the REFERENCE;
//! 2. every timed run's stdout must byte-match it — a workload whose
//!    output drifts between runs is measuring different work, and the
//!    bench refuses instead of averaging nonsense;
//! 3. one warmup run is discarded, then N timed runs (default 5), the
//!    headline is the MEDIAN (min/max shown beside it).
//!
//! THE BOUNDARY (#2980), the same on both legs: the headline times the
//! program's own run, from entering `main` to its return. The native leg
//! times it IN the process (a guard spliced into the generated `fn main`
//! writes its elapsed nanoseconds to `ALMIDE_BENCH_MAIN_NS`); the wasm leg
//! times the host's `main` call (`RunResult::main_secs`). Process spawn on
//! native and validate + compile + instantiate on wasm are outside it, so
//! the wasm/native ratio is steady-state code speed. Startup is not hidden:
//! a second, `cold start` column times the whole run as a user pays it —
//! spawn + run natively, compile + instantiate + run on the embedded host.
//! Before the split the headline WAS the cold figure, and on the ~2 ms rows
//! it measured Cranelift (2-3 ms a run) against process spawn (~1.2 ms),
//! not generated code.
//!
//! Legs: default = the native release binary (the cargo cache makes
//! repeat benches skip rustc); `--target wasm` = the embedded wasm host,
//! timed in-process. Program output is suppressed during timing — the
//! bench prints the measurement, not the workload.

use std::time::Instant;

use crate::err;

pub fn cmd_bench(file: &str, runs: u32, target: Option<&str>, args: &[String]) {
    let runs = runs.max(1);
    match target {
        None | Some("rust") | Some("native") => bench_native(file, runs, args),
        Some("wasm") | Some("wasm32") | Some("wasi") => bench_wasm(file, runs, args),
        Some(other) => {
            err(&format!("error: unknown bench target '{other}' (native, wasm)"));
            std::process::exit(2);
        }
    }
}

fn bench_native(file: &str, runs: u32, args: &[String]) {
    let rs_code = match crate::try_compile(file, false).map(|c| inject_main_timer(&c)) {
        Ok(Some(c)) => c,
        Ok(None) => {
            err("error: the generated program has no `fn main` to time");
            std::process::exit(1);
        }
        Err(e) => {
            err(&format!("Compile error:\n{e}"));
            std::process::exit(1);
        }
    };
    err("bench: building native release binary…");
    let bin = match super::run::build_native_cached(&rs_code, false, true, None, &[], None) {
        Ok(b) => b,
        Err(e) => {
            err(&format!("Compile error:\n{e}"));
            std::process::exit(1);
        }
    };
    let ns_file = std::env::temp_dir().join(format!("almide-bench-main-{}.ns", std::process::id()));
    let run_once = || -> Result<(Vec<u8>, Timing), String> {
        let _ = std::fs::remove_file(&ns_file);
        let started = Instant::now();
        let out = std::process::Command::new(&bin)
            .args(args)
            .env("ALMIDE_BENCH_MAIN_NS", &ns_file)
            .output()
            .map_err(|e| format!("execution failed: {e}"))?;
        let cold = started.elapsed().as_secs_f64();
        if !out.status.success() {
            return Err(format!(
                "workload exited {} — bench only times a clean run:\n{}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        // A missing file is a `main` that never returned normally (a
        // `process.exit` skips the guard): refuse rather than time spawn.
        let main_ns: u128 = std::fs::read_to_string(&ns_file)
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .ok_or("the program's `main` did not return normally, so its run cannot be timed (a `process.exit` skips the timer)")?;
        Ok((out.stdout, Timing { main: main_ns as f64 / 1e9, cold }))
    };
    run_bench(file, "native (release)", "spawn + run", runs, run_once);
    let _ = std::fs::remove_file(&ns_file);
}

/// Splice the in-process `main` timer (module header) as `fn main`'s FIRST
/// local, so it drops LAST: after the body, the stdout flush and every drop
/// the body owns. Items inside the function body survive the rlib fast
/// path's slimming (it keeps everything after the runtime boundary), so the
/// timed binary is the one `almide run` would build, plus one `Instant`.
fn inject_main_timer(rs_code: &str) -> Option<String> {
    const TIMER: &str = "    struct __AlmideBenchMain(std::time::Instant);\n    impl Drop for __AlmideBenchMain {\n        fn drop(&mut self) {\n            if let Some(p) = std::env::var_os(\"ALMIDE_BENCH_MAIN_NS\") {\n                let _ = std::fs::write(p, self.0.elapsed().as_nanos().to_string());\n            }\n        }\n    }\n    let __almide_bench_main = __AlmideBenchMain(std::time::Instant::now());\n";
    let at = rs_code.find("\nfn main() {\n")? + "\nfn main() {\n".len();
    Some(format!("{}{TIMER}{}", &rs_code[..at], &rs_code[at..]))
}

/// One timed run: `main` alone (the headline) and the whole run as a user
/// pays it (the `cold start` column), both in seconds.
struct Timing {
    main: f64,
    cold: f64,
}

fn bench_wasm(file: &str, runs: u32, args: &[String]) {
    let (bytes, _host_ops) = match super::build::compile_to_wasm_bytes(file, false, true, false, true) {
        Ok(b) => b,
        Err(()) => std::process::exit(1),
    };
    let args = args.to_vec();
    let run_once = move || -> Result<(Vec<u8>, Timing), String> {
        let started = Instant::now();
        // No 30 s epoch watchdog: its loop-header checks are the harness's
        // cost, not the program's (1.9x on mandelbrot, #2150).
        let r = almide_wasm_run::run_wasm_unbounded_args(&bytes, &args).map_err(|e| format!("embedded wasm host: {e}"))?;
        let cold = started.elapsed().as_secs_f64();
        if r.exit != 0 {
            return Err(format!("workload exited {} — bench only times a clean run:\n{}", r.exit, r.stderr));
        }
        Ok((r.stdout.into_bytes(), Timing { main: r.main_secs, cold }))
    };
    run_bench(file, "wasm (embedded host)", "compile + instantiate + run", runs, run_once);
}

fn run_bench<F>(file: &str, leg: &str, cold_what: &str, runs: u32, mut run_once: F)
where
    F: FnMut() -> Result<(Vec<u8>, Timing), String>,
{
    // Reference + warmup in one: the first run's output is the contract
    // every timed run must reproduce; its time is discarded.
    let (reference, _) = match run_once() {
        Ok(r) => r,
        Err(e) => {
            err(&format!("error: {e}"));
            std::process::exit(1);
        }
    };
    let mut mains: Vec<f64> = Vec::with_capacity(runs as usize);
    let mut colds: Vec<f64> = Vec::with_capacity(runs as usize);
    for i in 0..runs {
        match run_once() {
            Ok((stdout, t)) => {
                if stdout != reference {
                    err(&format!(
                        "error: run {} produced different output than the reference — the workload is nondeterministic, and a benchmark whose output changes is measuring different work. Pin the workload (seed / fixed input) and retry.",
                        i + 1
                    ));
                    std::process::exit(1);
                }
                mains.push(t.main);
                colds.push(t.cold);
            }
            Err(e) => {
                err(&format!("error: {e}"));
                std::process::exit(1);
            }
        }
    }
    for v in [&mut mains, &mut colds] {
        v.sort_by(|a, b| a.partial_cmp(b).expect("times are finite"));
    }
    let ms = |s: f64| s * 1000.0;
    // The headline first (the ledger gate parses the first `median … ms` and
    // the first `min …`), then the cold column under its own name.
    err(&format!(
        "bench {file} [{leg}]: median {:.2} ms (min {:.2}, max {:.2}, {} run(s) + 1 warmup, main() only, output verified identical across all runs); cold start median {:.2} ms (min {:.2}, {cold_what})",
        ms(mains[mains.len() / 2]),
        ms(mains[0]),
        ms(mains[mains.len() - 1]),
        runs,
        ms(colds[colds.len() / 2]),
        ms(colds[0]),
    ));
}
