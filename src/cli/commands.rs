use crate::{out, err, err_no_nl};
use super::collect_test_files;
use super::test_scratch::TestScratch;
use super::worker_pool::{bounded_parallel, cpu_slots, guard_worker_panic};
use super::test_wasm::{
    compile_and_run_wasm_test, note_wasm_leg_unavailable, report_wasm_verdict_without_native, run_native_fallback_phase,
    run_wasm_test_phase, WasmTestOutcome,
};

pub fn cmd_init() {
    if std::path::Path::new("almide.toml").exists() {
        err(&format!("almide.toml already exists"));
        std::process::exit(1);
    }
    let dir_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "myapp".to_string());

    let toml = format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n", dir_name);

    if let Err(e) = std::fs::write("almide.toml", toml) {
        err(&format!("Failed to write almide.toml: {}", e));
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all("src") {
        err(&format!("Failed to create src/: {}", e));
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all("tests") {
        err(&format!("Failed to create tests/: {}", e));
        std::process::exit(1);
    }

    if !std::path::Path::new("src/main.almd").exists() {
        if let Err(e) = std::fs::write("src/main.almd", "effect fn main() -> Unit = {\n  println(\"Hello, Almide!\")\n}\n") {
            err(&format!("Failed to write src/main.almd: {}", e));
            std::process::exit(1);
        }
    }

    // Generate CLAUDE.md for AI-assisted development
    if !std::path::Path::new("CLAUDE.md").exists() {
        let claude_md = include_str!("../../docs/project/CLAUDE_TEMPLATE.md");
        if let Err(e) = std::fs::write("CLAUDE.md", claude_md) {
            err(&format!("Failed to write CLAUDE.md: {}", e));
            std::process::exit(1);
        }
    }

    err(&format!("Initialized project in ./"));
    err(&format!("  almide.toml"));
    err(&format!("  src/main.almd"));
    err(&format!("  tests/"));
    err(&format!("  CLAUDE.md"));
}

/// Print what the run actually executed, then decide the "nothing ran" verdict.
///
/// One rule covers every shape of zero (#2084): no test file discovered, a named
/// file with no `test` block, a directory of files that have none.
///
/// Two zeroes are NOT that verdict, and both would otherwise be caught here:
///
/// - a `--run` pattern that excluded everything — the caller's own narrowing,
///   which is why the check reads `filtered_out` and not just `ran`;
/// - a file this leg DECLINED (`// wasm:skip`, or a wall routing it to native).
///   A skip means "these tests exist and this leg cannot run them", the opposite
///   of "there were none", and `wasm_skip_marker_stays_a_green_skip` pins that a
///   genuine skip stays green.
pub(super) fn finish_test_run(counts: TestCounts, files: usize, declined: usize, allow_no_tests: bool) {
    err(&counts.summary(files));
    if counts.found_nothing() && declined == 0 && !allow_no_tests {
        err("no tests to run — pass --allow-no-tests if a run with no tests is expected");
        std::process::exit(NO_TESTS_EXIT);
    }
}

/// Shared "resolve `almide test [file]`'s target file list" logic — used by
/// `cmd_test`/`cmd_test_fast` (search `spec/` and `exercises/`, `.` fallback)
/// and `cmd_test_wasm` (search `.` directly, i.e. an empty `fallback_dirs`).
///
/// An empty result is reported but no longer exits here (#2084): "nothing to
/// run" is one verdict decided by [`finish_test_run`], so a discovery that
/// found no file and a named file that turned out to hold no `test` block get
/// the same exit code instead of 1 and 0 respectively. The "no almide.toml"
/// refusal below is a different thing — a misaimed command, not an empty one —
/// and keeps exiting 1.
pub(super) fn discover_test_files(file: &str, fallback_dirs: &[&str]) -> Vec<String> {
    if !file.is_empty() {
        let path = std::path::Path::new(file);
        if path.is_dir() {
            let mut files = collect_test_files(path);
            files.sort();
            if files.is_empty() {
                err(&format!("No .almd files with test blocks found in {}", file));
            }
            files
        } else {
            vec![file.to_string()]
        }
    } else {
        // Default: recursively find test files in the given standard
        // directories (e.g. spec/, exercises/); "." otherwise.
        let mut files = Vec::new();
        for dir in fallback_dirs {
            let path = std::path::Path::new(dir);
            if path.exists() {
                files.extend(collect_test_files(path));
            }
        }
        // Fallback: search the current directory if no standard dirs
        // found — but ONLY inside a project (#1928): without an
        // `almide.toml` here, `almide test` walked the whole CWD tree
        // (a workspace of twenty unrelated repos, from a reset shell)
        // where `almide check` refuses with a hint. Same refusal, same
        // hint, so the two commands agree on what "no file" means.
        if files.is_empty() {
            if !std::path::Path::new("almide.toml").exists() {
                err("No file specified and no almide.toml found.");
                err("Run 'almide init' to create a project, or specify a file or directory.");
                std::process::exit(1);
            }
            files = collect_test_files(std::path::Path::new("."));
        }
        files.sort();
        if files.is_empty() {
            err(&format!("No .almd files with test blocks found."));
        }
        files
    }
}

/// `cmd_test`'s Phase 1: compile every test file in parallel (bounded by
/// CPU count), each in its own scratch dir so cold rustc builds parallelize
/// instead of serializing on the shared dir's BUILD_LOCK. Extracted
/// verbatim.
fn compile_test_files_parallel(test_files: &[String], no_check: bool, scratch: &std::sync::Arc<TestScratch>) -> Vec<(String, Result<std::path::PathBuf, String>)> {
    let scratch = scratch.clone();
    let mut results = bounded_parallel(test_files.to_vec(), cpu_slots(), move |test_file: String| {
        // Per-file scratch dir so cold rustc builds parallelize instead
        // of serializing on the shared dir's BUILD_LOCK; keyed on the
        // absolute path (#1877).
        let worker_dir = scratch.native_worker_dir(&test_file);
        let result = guard_worker_panic(
            || super::run::compile_to_binary(&test_file, no_check, true, false, Some(&worker_dir)),
            Err,
        );
        (test_file, result)
    });
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

use super::test_report::{
    libtest_counts, report_passing_output, report_test_failure, report_test_failure_io, test_harness_args, TestCounts,
    TestRun, NO_TESTS_EXIT,
};

/// `cmd_test`'s Phase 2: execute every compiled test binary in parallel
/// (bounded by CPU count). Output is CAPTURED, not inherited: it feeds
/// [`report_test_failure`], and printing each file's output whole in sorted
/// order makes a parallel run's transcript deterministic.
fn run_test_binaries_parallel(compiled: Vec<(String, Result<std::path::PathBuf, String>)>, program_args: &std::sync::Arc<Vec<String>>) -> Vec<TestRun> {
    let args = program_args.clone();
    let mut results: Vec<TestRun> = bounded_parallel(compiled, cpu_slots(), move |(file, compile_result)| {
        let (code, stdout, stderr) = match compile_result {
            Ok(bin) => guard_worker_panic(
                || super::run::run_binary_captured_io(&bin, &args),
                |msg| (1, msg, String::new()),
            ),
            Err(e) => (1, format!("Compile error for {}:\n{}", file, e), String::new()),
        };
        (file, code, stdout, stderr)
    });
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

pub fn cmd_test(file: &str, no_check: bool, run_filter: Option<&str>, allow_no_tests: bool, show_output: bool) {
    let test_files: Vec<String> = discover_test_files(file, &["spec", "exercises"]);

    let program_args = test_harness_args(run_filter);
    let scratch = std::sync::Arc::new(TestScratch::new());

    // Phase 1: Compile all test files in parallel (bounded by CPU count)
    let compiled = compile_test_files_parallel(&test_files, no_check, &scratch);

    // Phase 2: Execute test binaries in parallel (bounded by CPU count)
    let results = run_test_binaries_parallel(compiled, &program_args);

    let mut failed = 0;
    let mut counts = TestCounts::default();
    for (file, code, stdout, stderr) in &results {
        counts.add(libtest_counts(&format!("{stdout}{stderr}")).unwrap_or_default());
        if *code != 0 {
            report_test_failure_io(file, stdout, stderr, show_output);
            failed += 1;
        } else if show_output {
            report_passing_output(file, stdout, stderr);
        }
    }
    err("");
    if failed > 0 {
        err(&counts.summary(test_files.len()));
        err(&format!("{}/{} test file(s) failed", failed, test_files.len()));
        scratch.finish();
        std::process::exit(1);
    }
    err(&format!("All {} test file(s) passed", test_files.len()));
    scratch.finish();
    finish_test_run(counts, test_files.len(), 0, allow_no_tests);
}


/// Default `almide test`: run each file on the fast rustc-free WASM path; for
/// any file the WASM path can't pass (emitter gap, wasm:skip, or a trap), fall
/// back to the native rustc path, which is authoritative. The common case (most
/// tests pass on WASM) is ~9x faster; the native fallback preserves correctness.
pub fn cmd_test_fast(file: &str, no_check: bool, run_filter: Option<&str>, allow_no_tests: bool, show_output: bool) {
    let test_files: Vec<String> = discover_test_files(file, &["spec", "exercises"]);

    let cpus = cpu_slots();
    let scratch = std::sync::Arc::new(TestScratch::new());

    // Phase 1: WASM (fast, rustc-free), parallel.
    let wasm_outcomes = run_wasm_test_phase(&test_files, &scratch, cpus, run_filter);
    note_wasm_leg_unavailable(&wasm_outcomes);

    let mut wasm_pass = 0usize;
    let mut fallback: Vec<String> = Vec::new();
    let mut trapped: Vec<(String, String)> = Vec::new();
    // What a failing wasm run printed, for the files in `trapped`.
    let mut wasm_verdicts: std::collections::HashMap<String, super::test_output::TestOutput> = std::collections::HashMap::new();
    // Counted per LEG, because a file that walls on wasm is re-run natively and
    // would otherwise be counted twice.
    let mut counts = TestCounts::default();
    for o in wasm_outcomes {
        match o {
            WasmTestOutcome::Pass { file, count, filtered_out, stdout, stderr, .. } => {
                if show_output {
                    err_no_nl(&super::test_output::TestOutput::wasm(&stdout, &stderr).render_passing(&file));
                }
                counts.add(TestCounts { ran: count, filtered_out });
                wasm_pass += 1
            }
            // Nothing to run on any leg: no native re-run would find a test
            // either, so it is claimed here with zero counts and the zero-test
            // verdict at the end fails the run as it does on every lane (#2204).
            WasmTestOutcome::Empty { .. } => wasm_pass += 1,
            // A `Fail` is DIFFERENT IN KIND from the benign fallback classes
            // (#1166): the wasm leg COMPILED the file, claimed it, and produced
            // a runtime failure. Whether that is a plain failing test or a
            // MISCOMPILE only the native re-run can say — so hold the detail
            // and judge after phase 2: native red → an ordinary FAILED (no
            // wasm-specific noise); native green → the divergence class the
            // walls exist to prevent, reported loudly below. Silently folding
            // it into "via native fallback" hid the #1165 `indirect call type
            // mismatch` for its whole life locally while CI's Test WASM failed
            // the PR.
            WasmTestOutcome::Fail { file, detail, printed, .. } => {
                trapped.push((file.clone(), detail));
                wasm_verdicts.insert(file.clone(), printed);
                fallback.push(file);
            }
            // CompileError routes to the fallback like everything else here:
            // the native leg re-runs it and reports the diagnostics
            // authoritatively (#862), turning it into a counted FAILED.
            WasmTestOutcome::CompileError { file, .. }
            | WasmTestOutcome::Skip { file, .. } => fallback.push(file),
        }
    }

    // Phase 2: native rustc fallback (authoritative) for everything the WASM
    // path didn't pass, parallel with per-file scratch dirs.
    let program_args = test_harness_args(run_filter);

    let (native_results, native_unbuilt) = run_native_fallback_phase(&fallback, &program_args, no_check, cpus, &scratch);
    let trap_detail: std::collections::HashMap<&String, &String> = trapped.iter().map(|(f, d)| (f, d)).collect();

    let mut failed = 0;
    for (file, code, stdout, stderr) in &native_results {
        counts.add(libtest_counts(&format!("{stdout}{stderr}")).unwrap_or_default());
        // A test that RAN on wasm and failed has its verdict already. The
        // native re-run is there to tell a failing test from a wasm
        // miscompile; when it cannot even build, it has no verdict to offer,
        // and its build error must not stand in for the failed assertion
        // (#3424). Report the wasm verdict, then why the re-run is missing.
        if let (true, Some(detail)) = (native_unbuilt.contains(file), trap_detail.get(file)) {
            report_wasm_verdict_without_native(file, detail, wasm_verdicts.get(file), stdout, show_output);
            failed += 1;
            continue;
        }
        if *code != 0 {
            report_test_failure_io(file, stdout, stderr, show_output);
            failed += 1;
        } else if show_output {
            report_passing_output(file, stdout, stderr);
        }
    }
    // The #1166 divergence class: the wasm leg compiled the file and failed at
    // runtime, but the AUTHORITATIVE native re-run passed — a wasm-only
    // miscompile signal, exactly what CI's Test WASM job reds a PR for. Report
    // each one loudly (still counted "via native fallback": the suite verdict
    // is native's). A trap whose native re-run ALSO failed is a plain FAILED
    // test — no wasm-specific noise for those.
    let native_code: std::collections::HashMap<&String, i32> =
        native_results.iter().map(|(f, c, _, _)| (f, *c)).collect();
    let diverged: Vec<&(String, String)> = trapped
        .iter()
        .filter(|(f, _)| native_code.get(f).copied() == Some(0))
        .collect();
    for (file, detail) in &diverged {
        err(&format!("WASM TRAP {} (compiled for wasm, failed at runtime; native re-run PASSED — a wasm-only miscompile)", file));
        err_no_nl(detail);
    }
    // The wasm COVERAGE ratchet's data feed (mission-critical arc): every
    // file the wasm leg did not pass, one per line, so
    // proofs/check-wasm-fallback.sh can diff the set against its shrink-only
    // baseline. Names only under the flag — the summary line stays stable.
    if almide_base::env::flag("ALMIDE_FALLBACK_NAMES") {
        let mut sorted = fallback.clone();
        sorted.sort();
        for f in &sorted {
            err(&format!("FALLBACK {}", f));
        }
    }
    let trap_note = if diverged.is_empty() {
        String::new()
    } else {
        format!(" ({} after a wasm TRAP)", diverged.len())
    };
    err(&format!("\n{} via WASM, {} via native fallback{}, {} failed (of {} files)",
        wasm_pass, fallback.len().saturating_sub(failed), trap_note, failed, test_files.len()));
    scratch.finish();
    if failed > 0 {
        std::process::exit(1);
    }
    // A diverged trap FAILS the run even though the native re-run passed: it is
    // a wasm-only miscompile signal, and the run was asked for the wasm target.
    // Before #2205 this needed ALMIDE_TEST_STRICT_WASM, which nothing in CI or
    // the scripts set — so this lane (the default `almide test`: wasm first,
    // native fallback) passed over a diverged leg everywhere. ALMIDE_TEST_LAX_WASM
    // is the documented opt-out (a gate bypass: announced on stderr when on).
    if !diverged.is_empty() {
        if almide_base::env::flag("ALMIDE_TEST_LAX_WASM") {
            err(&format!(
                "LAX WASM: {} file(s) trapped on the wasm leg (native re-run passed) — passing only because ALMIDE_TEST_LAX_WASM is set",
                diverged.len()
            ));
        } else {
            err(&format!(
                "{} file(s) trapped on the wasm leg (native re-run passed) — a wasm-only miscompile fails a --target wasm run; ALMIDE_TEST_LAX_WASM=1 to pass over it",
                diverged.len()
            ));
            std::process::exit(1);
        }
    }
    err(&format!("All {} test file(s) passed", test_files.len()));
    // Nothing is declined on this lane: a wasm wall routes the file to the
    // native leg, so every discovered file was actually run somewhere.
    finish_test_run(counts, test_files.len(), 0, allow_no_tests);
}

/// `almide test --update-snapshots` (#1314): the accept step. Each file runs
/// until it passes or fails for a reason other than snapshot drift; every run
/// that aborts on a `testing.assert_snapshot` mismatch has its expectation
/// literal rewritten in place (src/cli/snapshot.rs) and runs again. One
/// rewrite per run, because the abort is the program's exit — the loop is
/// what makes a file with several drifted snapshots converge. The lanes are
/// the default harness's: wasm first (rustc-free, so a round is cheap),
/// native when the wasm leg walls; `wasm_only` is `--target wasm`.
pub fn cmd_test_update_snapshots(file: &str, no_check: bool, run_filter: Option<&str>, wasm_only: bool) {
    let test_files: Vec<String> = discover_test_files(file, &["spec", "exercises"]);
    let program_args = test_harness_args(run_filter);
    let tmp_dir = std::env::temp_dir().join("almide-wasm-test");
    std::fs::create_dir_all(&tmp_dir).ok();

    let mut updated = 0usize;
    let mut failed = 0usize;
    for tf in &test_files {
        match accept_snapshots_in_file(tf, &tmp_dir, &program_args, no_check, wasm_only) {
            Ok(n) => updated += n,
            Err(AcceptFailure::Run(output)) => {
                report_test_failure(tf, &output);
                failed += 1;
            }
            Err(AcceptFailure::Reported) => failed += 1,
        }
    }
    err(&format!(
        "\n{} snapshot(s) updated, {} failed (of {} files)",
        updated, failed, test_files.len()
    ));
    if failed > 0 {
        std::process::exit(1);
    }
}

/// Rounds before the accept loop gives up on one file — a guard against a
/// snapshot whose value changes on every run, which can never converge.
const MAX_SNAPSHOT_ROUNDS: usize = 256;

/// Why one file's accept loop stopped short of green.
enum AcceptFailure {
    /// A run failed for a reason other than snapshot drift (or never ran):
    /// the transcript, for the ordinary structured report.
    Run(String),
    /// A drifted snapshot could not be written; the failure and the reason
    /// are already on stderr.
    Reported,
}

/// The per-file accept loop: `Ok(n)` after `n` rewrites and a green run.
fn accept_snapshots_in_file(
    file: &str,
    tmp_dir: &std::path::Path,
    program_args: &[String],
    no_check: bool,
    wasm_only: bool,
) -> Result<usize, AcceptFailure> {
    let mut n = 0usize;
    for _ in 0..MAX_SNAPSHOT_ROUNDS {
        let (code, output) =
            run_test_file_once(file, tmp_dir, program_args, no_check, wasm_only).map_err(AcceptFailure::Run)?;
        if code == 0 {
            return Ok(n);
        }
        let Some(m) = super::snapshot::parse_snapshot_mismatch(&output) else {
            return Err(AcceptFailure::Run(output));
        };
        match super::snapshot::rewrite_snapshot(file, &m) {
            Ok(rw) => {
                let what = if rw.was_new { "new snapshot written" } else { "snapshot rewritten" };
                err(&format!("{}:{}: {}", file, rw.line, what));
                n += 1;
            }
            Err(e) => {
                report_test_failure(file, &output);
                err(&format!("  update: the snapshot on line {} could not be updated — {e}", m.line));
                return Err(AcceptFailure::Reported);
            }
        }
    }
    Err(AcceptFailure::Run(format!(
        "{file}: snapshots did not converge after {MAX_SNAPSHOT_ROUNDS} rounds — is the value deterministic?\n"
    )))
}

/// One run of a test file on the harness's lanes: `Ok((exit code, stdout+stderr))`,
/// or `Err(transcript)` when the file does not compile (or walls under `wasm_only`).
fn run_test_file_once(
    file: &str,
    tmp_dir: &std::path::Path,
    program_args: &[String],
    no_check: bool,
    wasm_only: bool,
) -> Result<(i32, String), String> {
    // The scratch layout (#1877) hands the wasm module path to the runner;
    // the accept loop keeps its own per-invocation dir and mirrors the name.
    let wasm_path = tmp_dir.join(file.replace(['/', '.'], "_") + ".wasm");
    match compile_and_run_wasm_test(file, wasm_path, super::test_report::harness_filter(program_args)) {
        WasmTestOutcome::Pass { .. } | WasmTestOutcome::Empty { .. } => return Ok((0, String::new())),
        WasmTestOutcome::Fail { raw, .. } => return Ok((1, raw)),
        WasmTestOutcome::CompileError { detail, .. } => {
            return Err(format!("Compile error for {file}:\n{detail}"));
        }
        WasmTestOutcome::Skip { reason, .. } => {
            if wasm_only {
                return Err(format!("SKIP {file} ({reason}) — no wasm rendering, and --target wasm has no native fallback\n"));
            }
        }
    }
    let worker_dir = std::env::temp_dir()
        .join("almide-test")
        .join(file.replace(['/', '.'], "_"));
    match super::run::compile_to_binary(file, no_check, true, false, Some(&worker_dir)) {
        Ok(bin) => Ok(super::run::run_binary_captured(&bin, program_args)),
        Err(e) => Err(format!("Compile error for {file}:\n{e}")),
    }
}

pub fn cmd_test_json(file: &str, run_filter: Option<&str>, allow_no_tests: bool) {
    let test_files: Vec<String> = if !file.is_empty() {
        let path = std::path::Path::new(file);
        if path.is_dir() {
            let mut files = collect_test_files(path);
            files.sort();
            files
        } else {
            vec![file.to_string()]
        }
    } else {
        let mut files = collect_test_files(std::path::Path::new("."));
        files.sort();
        files
    };

    let program_args = test_harness_args(run_filter);
    let mut counts = TestCounts::default();
    let mut failed = 0usize;

    // JSONL, one line per file, in sorted file order — a run is diffable
    // against the next one. Each failing file also emits its per-assertion
    // records ({name, file, line, expected, found, diff}); that is the shape
    // the dojo harness reads, and the reason `--json` captures the child's
    // output instead of letting it stream to the terminal (#1313).
    for test_file in &test_files {
        let (code, output) = match super::run::compile_to_binary(test_file, false, true, false, None) {
            Ok(bin) => super::run::run_binary_captured(&bin, &program_args),
            Err(e) => (1, e),
        };
        let source = almide::source_overlay::read_to_string(test_file).unwrap_or_default();
        let failures = super::test_report::parse(test_file, &source, &output);
        let file_counts = libtest_counts(&output).unwrap_or_default();
        counts.add(file_counts);
        let status = if code == 0 { "pass" } else { "fail" };
        failed += usize::from(code != 0);
        out(&format!(
            r#"{{"file":{},"status":"{}","exit_code":{},"tests":{},"filtered_out":{},"failures":[{}]}}"#,
            serde_json::Value::from(test_file.as_str()),
            status,
            code,
            file_counts.ran,
            file_counts.filtered_out,
            failures.iter().map(|f| f.to_json()).collect::<Vec<_>>().join(","),
        ));
    }
    // No summary line on this lane — the records ARE the output — but the
    // verdict still applies, so a `--json` consumer sees the same exit code:
    // 1 when any file failed, exactly as `cmd_test` (#3449 — this lane used to
    // report `"status":"fail"` in the payload and then exit 0).
    if failed > 0 {
        std::process::exit(1);
    }
    if counts.found_nothing() && !allow_no_tests {
        std::process::exit(NO_TESTS_EXIT);
    }
}
