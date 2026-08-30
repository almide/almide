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
//! Legs: default = the native release binary (the cargo cache makes
//! repeat benches skip rustc); `--target wasm` = the embedded wasm host,
//! timed in-process. Program output is suppressed during timing — the
//! bench prints the measurement, not the workload.

use std::time::Instant;

use crate::err;

pub fn cmd_bench(file: &str, runs: u32, target: Option<&str>) {
    let runs = runs.max(1);
    match target {
        None | Some("rust") | Some("native") => bench_native(file, runs),
        Some("wasm") | Some("wasm32") | Some("wasi") => bench_wasm(file, runs),
        Some(other) => {
            err(&format!("error: unknown bench target '{other}' (native, wasm)"));
            std::process::exit(2);
        }
    }
}

fn bench_native(file: &str, runs: u32) {
    let rs_code = match crate::try_compile(file, false) {
        Ok(c) => c,
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
    let run_once = || -> Result<(Vec<u8>, f64), String> {
        let started = Instant::now();
        let out = std::process::Command::new(&bin)
            .output()
            .map_err(|e| format!("execution failed: {e}"))?;
        let secs = started.elapsed().as_secs_f64();
        if !out.status.success() {
            return Err(format!(
                "workload exited {} — bench only times a clean run:\n{}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok((out.stdout, secs))
    };
    run_bench(file, "native (release)", runs, run_once);
}

fn bench_wasm(file: &str, runs: u32) {
    let (bytes, structural) = match super::build::compile_to_wasm_bytes(file, false, true, false) {
        Ok(b) => b,
        Err(()) => std::process::exit(1),
    };
    if !structural {
        err("error: bench --target wasm needs the structural leg (this program routed to the incumbent artifact, which runs on an external wasmtime — time it there)");
        std::process::exit(1);
    }
    let run_once = move || -> Result<(Vec<u8>, f64), String> {
        let started = Instant::now();
        let r = almide_wasm_run::run_wasm(&bytes).map_err(|e| format!("embedded wasm host: {e}"))?;
        let secs = started.elapsed().as_secs_f64();
        if r.exit != 0 {
            return Err(format!("workload exited {} — bench only times a clean run:\n{}", r.exit, r.stderr));
        }
        Ok((r.stdout.into_bytes(), secs))
    };
    run_bench(file, "wasm (embedded host)", runs, run_once);
}

fn run_bench<F>(file: &str, leg: &str, runs: u32, mut run_once: F)
where
    F: FnMut() -> Result<(Vec<u8>, f64), String>,
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
    let mut times: Vec<f64> = Vec::with_capacity(runs as usize);
    for i in 0..runs {
        match run_once() {
            Ok((stdout, secs)) => {
                if stdout != reference {
                    err(&format!(
                        "error: run {} produced different output than the reference — the workload is nondeterministic, and a benchmark whose output changes is measuring different work. Pin the workload (seed its randomness, drop wall-clock reads) and re-run.",
                        i + 1
                    ));
                    std::process::exit(1);
                }
                times.push(secs);
            }
            Err(e) => {
                err(&format!("error: {e}"));
                std::process::exit(1);
            }
        }
    }
    times.sort_by(|a, b| a.partial_cmp(b).expect("times are finite"));
    let ms = |s: f64| s * 1000.0;
    let median = times[times.len() / 2];
    err(&format!(
        "bench {file} [{leg}]: median {:.2} ms (min {:.2}, max {:.2}, {} run(s) + 1 warmup, output verified identical across all runs)",
        ms(median),
        ms(times[0]),
        ms(times[times.len() - 1]),
        runs
    ));
}
