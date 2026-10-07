//! The wasm leg of `almide test`: compile one file through the product's
//! wasm leg, run it on the embedded host (or the opt-in wasmtime CLI), and
//! classify what happened — plus the standalone `--target wasm` harness and
//! the two parallel phases `cmd_test_fast` drives.

use crate::{parse_file, project, project_fetch, resolve, canonicalize, check, diagnostic, err, err_no_nl};
use super::commands::{discover_test_files, finish_test_run};
use super::test_report::{TestCounts, TestRun};
use super::test_scratch::TestScratch;
use super::worker_pool::{bounded_parallel, cpu_slots, guard_worker_panic};

pub(super) enum WasmTestOutcome {
    /// `stdout`/`stderr` are what the run printed — `--show-output` shows them.
    Pass { file: String, count: usize, filtered_out: usize, bytes: usize, stdout: String, stderr: String },
    /// `raw` is the run's whole stdout+stderr (the same concatenation the
    /// native capture makes) — the accept step reads the snapshot block out
    /// of it (#1314); `detail` is the two-line summary the harness prints;
    /// `printed` is what the program printed, per test where the stream
    /// allows (#2538 — `test_output`).
    Fail { file: String, detail: String, raw: String, printed: super::test_output::TestOutput },
    /// The file does not compile on ANY target: resolve/type errors in the
    /// entry file or an imported module. Distinct from `Skip` — a SKIP means
    /// "correct program outside the verified renderer's subset", and the skip
    /// ledger must never absorb diagnostics (#957). The default harness
    /// routes these to the native fallback (which reports them
    /// authoritatively); the standalone `--target wasm` harness counts them
    /// FAILED, matching the default harness's verdict on the same file.
    CompileError { file: String, detail: String },
    Skip { file: String, reason: String, kind: SkipKind },
    /// No `main` and no `test` block: nothing for ANY leg to run. Not a wall
    /// (no renderer declined it) and not a skip (nothing was declined), so it
    /// contributes zero to the counts and lets `finish_test_run` reach the same
    /// exit-5 verdict native does on the same file (#2204).
    Empty { file: String },
}

/// WHY a file's tests did not run on wasm. The distinction is the whole point
/// of #2121: a skip the author DECLARED and a skip a RENDERER decided are not
/// the same verdict, and reporting both as "skipped" let `almide test --target
/// wasm` exit 0 having run nothing on the target the caller asked for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SkipKind {
    /// `// wasm:skip` in the first three lines — the author said so, with a
    /// reason a reader can audit. Benign.
    Declared,
    /// The host cannot run the lane at all (the opt-in `wasmtime` CLI route
    /// with no `wasmtime` on PATH, an unwritable scratch path). Not a
    /// statement about the program. Benign — but never silent: each cause is
    /// named once per run (`note_wasm_leg_unavailable`, #3046).
    Environment,
    /// A RENDERER declined this program. The caller asked for wasm and this
    /// file's tests did not run there — so the summary says exactly that,
    /// separately from the skips the author declared. `// wasm:skip` is NOT
    /// the place to park one: that marker means "wasm cannot do this", and a
    /// wall means "this leg has not lowered this shape yet"
    /// (tests/wasm_skip_ledger_test.rs, #812). This repository's own walls are
    /// registered in proofs/wasm-test-walls.txt and gated shrink-only by
    /// scripts/check-wasm-test-walls.sh.
    Wall,
}

/// Compile one `.almd` file to WASM and run it under wasmtime. Pure per-file
/// work (no shared mutable state) so it runs in parallel — the WASM path takes
/// no rustc/cargo, so there's no global build lock to serialize on.
/// `compile_and_run_wasm_test`'s independent pre-flight gates: `// wasm:skip`
/// marker, parse errors (a real failure, not a skip — see the comment at the
/// call site), and the main+test co-presence gap in the v1 test-mode runner.
/// Extracted verbatim — each check only reads its parameters; whichever
/// fires first determines the outcome, matching the original code's
/// early-return order exactly.
fn wasm_test_preflight_outcome(
    test_file: &str,
    _program: &almide_lang::ast::Program,
    source_text: &str,
    parse_errors: &[crate::diagnostic::Diagnostic],
) -> Option<WasmTestOutcome> {
    if source_text.lines().take(3).any(|line| line.contains("// wasm:skip")) {
        return Some(WasmTestOutcome::Skip {
            file: test_file.to_string(),
            reason: "wasm:skip".to_string(),
            kind: SkipKind::Declared,
        });
    }
    if parse_errors.iter().any(|d| d.level == crate::diagnostic::Level::Error) {
        let mut detail = String::new();
        for d in parse_errors.iter().filter(|d| d.level == crate::diagnostic::Level::Error).take(3) {
            detail.push_str(&format!("  parse error: {}\n", d.message));
        }
        return Some(WasmTestOutcome::Fail { file: test_file.to_string(), raw: detail.clone(), detail, printed: Default::default() });
    }
    None
}

/// Push an `ALMIDE_PROFILE` timing mark when profiling is enabled — guards
/// `compile_and_run_wasm_test`'s repeated `if prof { marks.push(...) }`
/// call sites behind one named function instead of six inline branches.
fn mark(prof: bool, marks: &mut Vec<(&'static str, std::time::Instant)>, label: &'static str) {
    if prof {
        marks.push((label, std::time::Instant::now()));
    }
}

/// Print the `ALMIDE_PROFILE` per-phase timing breakdown for one test file.
/// Extracted verbatim from `compile_and_run_wasm_test`'s trailing profiling
/// block.
fn print_wasm_test_profile(test_file: &str, marks: &[(&'static str, std::time::Instant)]) {
    let (Some(first), Some(last)) = (marks.first(), marks.last()) else {
        return;
    };
    let total = last.1.duration_since(first.1).as_secs_f64();
    let mut line = format!("[prof] {} total={:.3}s", test_file, total);
    for w in marks.windows(2) {
        line.push_str(&format!(" | {}={:.3}", w[1].0, w[1].1.duration_since(w[0].1).as_secs_f64()));
    }
    err(&format!("{}", line));
}

/// `compile_and_run_wasm_test`'s dependency-fetch + import-resolution
/// phase. Extracted verbatim.
fn resolve_wasm_test_deps(test_file: &str, program: &almide_lang::ast::Program) -> Result<resolve::ResolvedModules, String> {
    resolve::resolve_imports_with_deps(test_file, program, &wasm_test_dep_paths())
}

/// The dependency table the test runner resolves against — the cwd
/// package's fetched deps, or nothing outside a package.
fn wasm_test_dep_paths() -> Vec<(project::PkgId, std::path::PathBuf)> {
    if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            return project_fetch::fetch_all_deps(&proj)
                .unwrap_or_else(|_| vec![])
                .into_iter()
                .map(|fd| (fd.pkg_id, fd.source_dir))
                .collect();
        }
    }
    vec![]
}

/// The test lane's rendering of a test file (#2179): the product's wasm
/// leg, so the lane and `almide build --target wasm` are one compiler. A
/// main-only file runs its `main`; a file that declares tests gets the
/// shared `__test_runner` synthesis. Validated and audited against the p1
/// host surface wasmtime serves; a decline at any stage is a wall, and the
/// file routes to the authoritative native leg (#2752: there is no second
/// wasm renderer to hand it to).
/// The test module, as the embedded host runs it (`almide.*` imports) and as
/// the stock-runtime artifact `almide build --target wasm` ships (`to_wasi`).
/// Both are produced so the wall verdict stays the build's own; which one RUNS
/// is `WasmTestRunner`'s choice.
struct TestModule {
    almide: Vec<u8>,
    wasi: Vec<u8>,
}

fn structural_test_render(test_file: &str, source_text: &str, ir_program: &almide::ir::IrProgram, declared_tests: usize, run_filter: Option<&str>, explain: bool) -> Option<TestModule> {
    let has_main = ir_program.functions.iter().any(|f| f.name.as_str() == "main");
    if !has_main && declared_tests == 0 {
        return None;
    }
    let wall = |stage: &str, detail: String| {
        if explain { err(&format!("[wall] {}: structural {}: {}", test_file, stage, detail)); }
    };
    let lowered = if declared_tests > 0 {
        almide::wasm_leg::lower_to_ir_tests_with_deps(test_file, source_text, &wasm_test_dep_paths(), run_filter)
    } else {
        almide::wasm_leg::lower_to_ir_with_deps(test_file, source_text, &wasm_test_dep_paths())
    };
    let ir = match lowered {
        Ok(ir) => ir,
        Err(e) => { wall("lower", e); return None; }
    };
    let (bytes, host_ops) = match almide_wasm::emit_program_with_ops(&ir) {
        Ok(x) => x,
        Err(e) => { wall("emit", format!("{e:?}")); return None; }
    };
    if let Err(e) = wasmparser::validate(&bytes) {
        wall("validate", e.to_string());
        return None;
    }
    if let Some(op) = host_ops.iter().find(|op| !almide_wasm_run::wasi::P1_SERVED_OPS.contains(op)) {
        wall("host audit", format!("op {op} is not served by the p1 shim"));
        return None;
    }
    // The structural module imports `almide.*` (the embedded host's surface,
    // which runs it by default); the same `to_wasi` rewrite the build ships
    // is a wall stage here too, and is what the opt-in stock-runtime route runs.
    let ops: Vec<i32> = host_ops.iter().copied().collect();
    let wasi = match almide_wasm_run::wasi::to_wasi(&bytes, &ops) {
        Ok(w) => w,
        Err(e) => { wall("to_wasi", e.to_string()); return None; }
    };
    if explain {
        err(&format!("[route] {}: structural leg rendered the test module ({} bytes)", test_file, wasi.len()));
    }
    Some(TestModule { almide: bytes, wasi })
}

/// Which runtime executes the wasm leg of `almide test` (#3046).
///
/// The EMBEDDED host is the default: the same wasmtime `almide run --target
/// wasm` uses, compiled into every `almide` binary, so the lane needs nothing
/// on PATH. It used to spawn the `wasmtime` CLI, and where that binary was
/// missing (a fresh CI runner, a container) every file fell to the native
/// fallback with nothing but the count to say so — for a package with heavy
/// `[native-deps]` that read as a hang.
///
/// `ALMIDE_TEST_WASM_RUNNER=wasmtime` keeps the CLI route: it runs the stock
/// `to_wasi` artifact the build ships under a stock runtime, the comparison
/// the embedded default is measured against.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WasmTestRunner {
    Embedded,
    WasmtimeCli,
}

impl WasmTestRunner {
    fn from_env() -> Self {
        match almide_base::env::var("ALMIDE_TEST_WASM_RUNNER").as_deref() {
            Some("wasmtime") => WasmTestRunner::WasmtimeCli,
            _ => WasmTestRunner::Embedded,
        }
    }
}

/// A finished run's verdict from what it printed and whether it exited 0 —
/// shared by both runners so the classification is one rule.
fn wasm_run_outcome(test_file: &str, declared_tests: usize, bytes_len: usize, success: bool, stdout: &str, stderr: &str) -> WasmTestOutcome {
    if success {
        let ran = stdout.matches("ok\n").count();
        return WasmTestOutcome::Pass {
            file: test_file.to_string(),
            count: ran,
            // The runner is synthesized over the SELECTED tests, so
            // what `--run` excluded is only knowable from the source.
            filtered_out: declared_tests.saturating_sub(ran),
            bytes: bytes_len,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        };
    }
    let mut last_test = String::new();
    for line in stdout.lines() {
        if line.starts_with("test: ") { last_test = line.to_string(); }
    }
    let mut detail = String::new();
    if !last_test.is_empty() { detail.push_str(&format!("  trapped at: {}\n", last_test)); }
    // From the failure block when there is one: the first
    // stderr lines may be the program's own (#2538), which
    // `printed` carries under their own label.
    for line in super::test_output::wasm_failure_lines(stderr) {
        detail.push_str(&format!("  {}\n", line));
    }
    let printed = super::test_output::TestOutput::wasm(stdout, stderr);
    WasmTestOutcome::Fail { file: test_file.to_string(), detail, raw: format!("{stdout}{stderr}"), printed }
}

/// The one-line note for every cause that kept the wasm leg from STARTING on
/// some files (`SkipKind::Environment`), once per run and cause (#3046): the
/// files still run on the native fallback, and the summary's count must not
/// be the only trace of why.
pub(super) fn note_wasm_leg_unavailable(outcomes: &[WasmTestOutcome]) {
    let mut causes: Vec<(&str, usize)> = Vec::new();
    for o in outcomes {
        if let WasmTestOutcome::Skip { reason, kind: SkipKind::Environment, .. } = o {
            match causes.iter_mut().find(|(r, _)| *r == reason.as_str()) {
                Some((_, n)) => *n += 1,
                None => causes.push((reason.as_str(), 1)),
            }
        }
    }
    for (reason, n) in causes {
        err(&format!("note: the wasm leg could not start for {n} file(s) ({reason}); they run on the native fallback"));
    }
}

/// `compile_and_run_wasm_test`'s type-check phase. Unlike `almide
/// build`/`run --target wasm` (which print full diagnostics on a type
/// error), test mode returns the errors compactly: the default harness
/// routes the file to the native fallback, which re-runs and reports them
/// authoritatively (printing here would duplicate every diagnostic); the
/// standalone `--target wasm` harness prints this summary itself (#957).
fn typecheck_wasm_test_program(test_file: &str, source_text: &str, program: &mut almide_lang::ast::Program, resolved: &resolve::ResolvedModules) -> Result<check::Checker, String> {
    let canon = canonicalize::canonicalize_program(
        program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(test_file, source_text);
    checker.diagnostics = canon.diagnostics;
    // #785: module top-let types must be fully inferred before the entry
    // program reads them (drivers infer the entry FIRST; without this the
    // readers see the registration seed — Unknown for non-literal inits).
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(program);
    if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
        return Err(render_error_summary(diagnostics.iter().filter(|d| d.level == diagnostic::Level::Error)));
    }
    Ok(checker)
}

/// Compact per-file error summary for the standalone wasm harness's FAIL
/// output: the first three error diagnostics, one line each.
fn render_error_summary<'a>(errors: impl Iterator<Item = &'a diagnostic::Diagnostic>) -> String {
    let mut detail = String::new();
    for d in errors.take(3) {
        let loc = match (d.line, d.col) {
            (Some(l), Some(c)) => format!(":{}:{}", l, c),
            _ => String::new(),
        };
        detail.push_str(&format!("  type error{}: {}\n", loc, d.message));
    }
    detail
}

/// `compile_and_run_wasm_test`'s pre-register + lower phase: pre-register
/// versioned module names, lower the entry program, then lower each
/// resolved user module via the shared `build::lower_one_wasm_module` (the
/// same per-module lowering `compile_to_wasm_bytes` uses — this loop body
/// used to be a byte-for-byte duplicate of it). link/optimize/monomorphize
/// stay in the caller so the ALMIDE_PROFILE "lower_modules" mark lands at
/// the same point as before. Extracted verbatim.
fn lower_wasm_test_modules(program: &almide_lang::ast::Program, checker: &mut check::Checker, resolved: &mut resolve::ResolvedModules) -> Result<almide::ir::IrProgram, String> {
    almide::wasm_leg::register_versioned_module_names(checker, &resolved.modules);
    let mut ir_program = almide::lower::lower_program(program, &checker.env, &checker.type_map);
    let mut module_diags = Vec::new();
    let sources = std::mem::take(&mut resolved.sources);
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        super::build::lower_one_wasm_module(
            checker, name, mod_prog, pkg_id, &mut ir_program, &sources, &mut module_diags,
        );
    }
    resolved.sources = sources;
    // An imported module's own type errors are NOT printed here: like the
    // entry program's, they route the file to the authoritative native
    // fallback, which prints them (#862) — printing twice would duplicate
    // every diagnostic in `almide test`'s output. They ARE returned
    // compactly, so the standalone `--target wasm` harness can report its
    // own FAIL verdict instead of absorbing them as a skip (#957).
    if module_diags.iter().any(|(_, _, ds)| ds.iter().any(|d| d.level == diagnostic::Level::Error)) {
        return Err(render_error_summary(
            module_diags.iter().flat_map(|(_, _, ds)| ds.iter()).filter(|d| d.level == diagnostic::Level::Error),
        ));
    }
    Ok(ir_program)
}

pub(super) fn compile_and_run_wasm_test(test_file: &str, wasm_path: std::path::PathBuf, run_filter: Option<&str>) -> WasmTestOutcome {
    let skip = |reason: String| WasmTestOutcome::Skip {
        file: test_file.to_string(),
        reason,
        kind: SkipKind::Wall,
    };
    let skip_env = |reason: String| WasmTestOutcome::Skip {
        file: test_file.to_string(),
        reason,
        kind: SkipKind::Environment,
    };
    let compile_error = |detail: String| WasmTestOutcome::CompileError { file: test_file.to_string(), detail };
    let prof = almide_base::env::flag("ALMIDE_PROFILE");
    let mut marks: Vec<(&'static str, std::time::Instant)> = vec![("start", std::time::Instant::now())];

    let (mut program, source_text, parse_errors) = parse_file(test_file);
    // Counted before any filtering so the summary can say what `--run` excluded.
    let declared_tests = program
        .decls
        .iter()
        .filter(|d| matches!(d, almide_lang::ast::Decl::Test { .. }))
        .count();
    mark(prof, &mut marks, "parse");
    // `// wasm:skip` marker / parse errors (a real Fail, not a benign skip —
    // see `wasm_test_preflight_outcome`'s doc comment) / the main+test
    // co-presence gap in the v1 test-mode runner.
    if let Some(outcome) = wasm_test_preflight_outcome(test_file, &program, &source_text, &parse_errors) {
        return outcome;
    }

    let mut resolved = match resolve_wasm_test_deps(test_file, &program) {
        Ok(r) => r,
        // An unresolvable import is broken on every target, not a wall.
        Err(e) => return compile_error(format!("  resolve error: {}\n", e)),
    };
    mark(prof, &mut marks, "resolve");

    let mut checker = match typecheck_wasm_test_program(test_file, &source_text, &mut program, &resolved) {
        Ok(c) => c,
        Err(detail) => return compile_error(detail),
    };
    mark(prof, &mut marks, "check_user");

    let mut ir_program = match lower_wasm_test_modules(&program, &mut checker, &mut resolved) {
        Ok(ir) => ir,
        Err(detail) => return compile_error(detail),
    };
    mark(prof, &mut marks, "lower_modules");
    // Nothing to run on any leg — the v1 renderer would refuse this program
    // ("no `main` and no test blocks") and the refusal read as a WALL, i.e. a
    // decline, which the zero-test verdict rightly exempts; so the same file
    // exited 5 on native and 0 here (#2204). Decided after the checks above so
    // a file that does not compile still fails as one on both targets.
    if declared_tests == 0 && !ir_program.functions.iter().any(|f| f.name.as_str() == "main") {
        return WasmTestOutcome::Empty { file: test_file.to_string() };
    }
    // The ONE driver — see the note in src/cli/build.rs. This is the site whose order the
    // migration FLIPPED (ir_link first → last), so its acceptance check is byte-identity of
    // spec/wasm_cross against the pre-migration capture, not merely a green suite.
    almide_driver::link_ir(&mut ir_program);
    mark(prof, &mut marks, "opt_mono");
    // Native-only matrix ops (e.g. qwen3_block_q1_0_kv) have no WASM lowering;
    // skip with a clear reason instead of reaching the emitter (whose panic would
    // surface as a generic "WASM codegen panic" skip).
    if let Some(op) = almide::codegen::program_uses_native_only_matrix_on_wasm(&ir_program) {
        return skip(format!("matrix.{op} is native-only — no WASM lowering"));
    }
    // `ALMIDE_WALL_REASON=1` prints WHICH stage declined (lower, emit,
    // validate, host audit, to_wasi).
    let explain = almide_base::env::flag("ALMIDE_WALL_REASON");

    // The SAME leg `almide build --target wasm` uses (#2179): the test lane
    // and the product lane are one compiler.
    let module_bytes = structural_test_render(test_file, &source_text, &ir_program, declared_tests, run_filter, explain);
    let runner = WasmTestRunner::from_env();
    let run_module = |module: &TestModule| -> WasmTestOutcome {
        if runner == WasmTestRunner::Embedded {
            // No time limit and no stdin, as the CLI route had: a test runs to
            // completion exactly as its native twin does. The host reads the
            // real environment and cwd, which `-S inherit-env=y` and the
            // ALMIDE_CWD pin gave the CLI route.
            return match almide_wasm_run::run_wasm_unbounded(&module.almide) {
                Ok(r) => wasm_run_outcome(test_file, declared_tests, module.wasi.len(), r.exit == 0, &r.stdout, &r.stderr),
                // A module the host cannot instantiate FAILS, as the CLI's
                // non-zero exit did — it is not a benign skip.
                Err(e) => wasm_run_outcome(test_file, declared_tests, module.wasi.len(), false, "", &format!("Error: embedded wasm host: {e:#}\n")),
            };
        }
        // `ALMIDE_TEST_WASM_RUNNER=wasmtime`: write the stock artifact and run
        // it under the wasmtime CLI. `-S inherit-env=y` mirrors the embedded
        // host: `env.get` in a test observes the same host variables native
        // does (the env cross-target contract).
        if let Err(e) = std::fs::write(&wasm_path, &module.wasi) {
            return skip_env(format!("write: {}", e));
        }
        let mut cmd = std::process::Command::new("wasmtime");
        super::run::wasmtime_fs_args(&mut cmd);
        cmd.arg("-S").arg("inherit-env=y");
        // Same ALMIDE_CWD pin as `cmd_run_wasm` (#874): relative fs paths in a
        // test resolve against the real launcher cwd, not a stale PWD. On
        // Windows the guest spelling (`.`) comes from `wasmtime_fs_args`.
        if !cfg!(windows) {
            if let Some(cwd) = super::run::almide_cwd() {
                cmd.arg(format!("--env=ALMIDE_CWD={}", cwd));
            }
        }
        match cmd.arg(&wasm_path).output() {
            Ok(result) => wasm_run_outcome(
                test_file,
                declared_tests,
                module.wasi.len(),
                result.status.success(),
                &String::from_utf8_lossy(&result.stdout),
                &String::from_utf8_lossy(&result.stderr),
            ),
            Err(e) => skip_env(format!("ALMIDE_TEST_WASM_RUNNER=wasmtime, and the `wasmtime` CLI did not start: {e}")),
        }
    };
    if prof {
        mark(prof, &mut marks, "codegen");
        print_wasm_test_profile(test_file, &marks);
    }
    // v1 is the ONLY wasm path (#782), and where it renders its verdict is FINAL:
    // a v1 run failure routes to the authoritative NATIVE fallback, never to a
    // retry on unverified codegen. The old v0 retry existed for the #790 vein
    // (v1 runtime defects trapping where v0 ran) — that vein is closed, and the
    // retry's real effect had inverted: v0 DCEs whole test bodies (#792 vacuous
    // ok), so a GENUINELY failing test (v1 correctly aborting on `none!`) was
    // overwritten by a hollow v0 "pass". A v1 WALL is an honest skip that routes
    // the file to native — the shrinking #813 remainder.
    match module_bytes {
        Some(m) => run_module(&m),
        // The leg declined — the same verdict `almide build --target wasm`
        // gives this file (E082).
        None => skip(format!(
            "wasm leg walled: the structural leg declined{} — ALMIDE_WALL_REASON=1 names the stage",
            if declared_tests > 0 { format!(" ({declared_tests} test block(s) route to native)") } else { String::new() }
        )),
    }
}

pub fn cmd_test_wasm(file: &str, run_filter: Option<&str>, allow_no_tests: bool, show_output: bool) {
    let test_files: Vec<String> = discover_test_files(file, &[]);
    let scratch = std::sync::Arc::new(TestScratch::new());

    // Parallel: each file's compile+run is independent and rustc/cargo-free,
    // so there's no global build lock to serialize on (unlike the native path).
    // The phase carries `run_filter` into every worker (#2085 — this leg used to drop it).
    let mut outcomes = run_wasm_test_phase(&test_files, &scratch, cpu_slots(), run_filter);
    let file_of = |o: &WasmTestOutcome| match o {
        WasmTestOutcome::Pass { file, .. }
        | WasmTestOutcome::Fail { file, .. }
        | WasmTestOutcome::CompileError { file, .. }
        | WasmTestOutcome::Skip { file, .. }
        | WasmTestOutcome::Empty { file } => file.clone(),
    };
    outcomes.sort_by(|a, b| file_of(a).cmp(&file_of(b)));
    note_wasm_leg_unavailable(&outcomes);

    let mut failed = 0;
    let mut passed = 0;
    let mut skipped = 0;
    let mut walled: Vec<String> = Vec::new();
    let mut counts = TestCounts::default();
    for o in &outcomes {
        match o {
            WasmTestOutcome::Pass { file, count, filtered_out, bytes, stdout, stderr } => {
                err(&format!("{}: {} tests passed ({} bytes)", file, count, bytes));
                if show_output {
                    err_no_nl(&super::test_output::TestOutput::wasm(stdout, stderr).render_passing(file));
                }
                counts.add(TestCounts { ran: *count, filtered_out: *filtered_out });
                passed += 1;
            }
            WasmTestOutcome::Fail { file, detail, printed, .. } => {
                err(&format!("FAIL {}", file));
                err_no_nl(&format!("{}", detail));
                // The wasm runner stops at the first failure, so the test that
                // never printed its `ok` is the failing one.
                err_no_nl(&printed.failure_stdout(None));
                err_no_nl(&printed.render_rest(&[None], show_output));
                failed += 1;
            }
            // Broken on every target — a FAIL verdict here, matching the
            // default harness; SKIP is reserved for honest walls (#957).
            WasmTestOutcome::CompileError { file, detail } => {
                err(&format!("FAIL {} (does not compile)", file));
                err_no_nl(&format!("{}", detail));
                failed += 1;
            }
            // A DECLARED or ENVIRONMENT skip is a skip. A WALL is not: the
            // caller asked for wasm, this file's tests did not run there, and
            // nothing in the file says that was expected (#2121).
            WasmTestOutcome::Skip { file, reason, kind: SkipKind::Wall } => {
                // A stable, greppable prefix: the wall register's gate reads
                // these lines, and a wall that looked like every other skip is
                // how five of them went unnoticed (#2121).
                err(&format!("WALL {} (tests did not run on wasm: {})", file, reason));
                walled.push(file.clone());
                skipped += 1;
            }
            WasmTestOutcome::Skip { file, reason, .. } => {
                err(&format!("SKIP {} ({})", file, reason));
                skipped += 1;
            }
            // Counted with the passes, as native counts a file whose binary
            // ran zero tests; the zero-test verdict below is what fails it.
            WasmTestOutcome::Empty { file } => {
                err(&format!("{}: no test blocks (nothing to run)", file));
                passed += 1;
            }
        }
    }

    err("");
    if skipped > 0 {
        err(&format!("{} passed, {} failed, {} skipped (of {} files)",
            passed, failed, skipped, test_files.len()));
    } else {
        err(&format!("{} passed, {} failed (of {} files)",
            passed, failed, test_files.len()));
    }
    if !walled.is_empty() {
        err("");
        err(&format!(
            "{} file(s) did not run on wasm: a renderer declined them. This is not the same \
             verdict as a declared `// wasm:skip`, which says wasm CANNOT run the file —",
            walled.len()
        ));
        for f in &walled {
            err(&format!("  {}", f));
        }
        err("it says a leg has not lowered the shape yet. Fix the wall rather than marking");
        err("the file: a `// wasm:skip` for subset debt is refused by the skip ledger (#812).");
    }
    scratch.finish();
    if failed > 0 {
        std::process::exit(1);
    }
    // `skipped` files are declines, not absences — see finish_test_run.
    finish_test_run(counts, test_files.len(), skipped, allow_no_tests);
}

/// `cmd_test_fast`'s Phase 1: run every file on the fast rustc-free WASM
/// path, in parallel (bounded by `cpus`). Extracted verbatim.
pub(super) fn run_wasm_test_phase(test_files: &[String], scratch: &std::sync::Arc<TestScratch>, cpus: usize, run_filter: Option<&str>) -> Vec<WasmTestOutcome> {
    let run_filter: Option<String> = run_filter.map(str::to_string);
    let scratch = scratch.clone();
    bounded_parallel(test_files.to_vec(), cpus, move |tf: String| {
        // A panic routes the file to the native fallback like any other
        // compile error; that leg reports it authoritatively.
        guard_worker_panic(
            || compile_and_run_wasm_test(&tf, scratch.wasm_module_path(&tf), run_filter.as_deref()),
            |detail| WasmTestOutcome::CompileError { file: tf.clone(), detail: format!("{detail}\n") },
        )
    })
}

/// `cmd_test_fast`'s Phase 2: native rustc fallback (authoritative) for
/// everything the WASM path didn't pass, parallel with per-file scratch
/// dirs. Output is captured — see [`run_test_binaries_parallel`].
/// The second element names the files whose native build FAILED — no test
/// ran, so their result is a build error, not a verdict (#3424).
pub(super) fn run_native_fallback_phase(fallback: &[String], program_args: &std::sync::Arc<Vec<String>>, no_check: bool, cpus: usize, scratch: &std::sync::Arc<TestScratch>) -> (Vec<TestRun>, std::collections::HashSet<String>) {
    let args = program_args.clone();
    let scratch = scratch.clone();
    let mut v: Vec<(TestRun, bool)> = bounded_parallel(fallback.to_vec(), cpus, move |tf: String| {
        let worker_dir = scratch.native_worker_dir(&tf);
        let (built, (code, stdout, stderr)) = guard_worker_panic(
            || match super::run::compile_to_binary(&tf, no_check, true, false, Some(&worker_dir)) {
                Ok(bin) => (true, super::run::run_binary_captured_io(&bin, &args)),
                Err(e) => (false, (1, format!("Compile error for {}:\n{}", tf, e), String::new())),
            },
            |msg| (false, (1, format!("Compile error for {}:\n{}", tf, msg), String::new())),
        );
        ((tf, code, stdout, stderr), built)
    });
    v.sort_by(|a, b| a.0.0.cmp(&b.0.0));
    let unbuilt = v.iter().filter(|(_, built)| !built).map(|(r, _)| r.0.clone()).collect();
    (v.into_iter().map(|(r, _)| r).collect(), unbuilt)
}

/// A file that FAILED on the wasm leg whose native re-run did not build:
/// the wasm failure is the verdict (`FAILED`, the failing test, what it
/// printed), and the native build error follows as a note (#3424).
pub(super) fn report_wasm_verdict_without_native(file: &str, detail: &str, printed: Option<&super::test_output::TestOutput>, native_error: &str, show_output: bool) {
    err(&format!("FAILED: {}", file));
    err_no_nl(detail);
    if let Some(printed) = printed {
        // The wasm runner stops at the first failure, so the test that never
        // printed its `ok` is the failing one.
        err_no_nl(&printed.failure_stdout(None));
        err_no_nl(&printed.render_rest(&[None], show_output));
    }
    err("note: this failure is the wasm leg's verdict; the native re-run that checks it could not build:");
    for line in native_error.lines() {
        err(&format!("  {}", line));
    }
}
