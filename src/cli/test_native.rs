//! Building a run's native test binaries (#4441): ONE program per run.
//!
//! Each test file is first CHECKED on its own, exactly as its compile always
//! judged it — its own diagnostics, its own `[permissions]` verdict, its own
//! compile error — up to code generation. The files that pass are then
//! compiled as one program ([`crate::compile_driver::compile_test_run`]):
//! every test file a module of an empty entry, every module they reach
//! checked, lowered and emitted once, the runtime the whole program needs,
//! all in one crate that ONE rustc builds — the `go test` model. The crate's
//! dir is stable across runs, so an edit recompiles incrementally.
//!
//! The binary is then run once per FILE, selecting that file's tests
//! ([`super::test_report::run_native_test_binary_in`]), so a file still runs
//! in a process of its own, with the output, counts and verdict it had.
//!
//! A run whose one program does not compile or build is not a verdict: its
//! files fall back to one crate each, the per-file route, with the
//! diagnostics they already printed silenced the second time.

use super::run::NativeSource;
use super::test_scratch::TestScratch;
use super::worker_pool::{bounded_parallel, cpu_slots, guard_worker_panic};
use std::path::PathBuf;
use std::sync::Arc;

/// Where a file's tests live in the run's binary.
#[derive(Clone, Debug)]
pub(crate) struct MergedModule {
    /// The prefix the file's test fns carry, `almide_rt_<module>_`.
    pub prefix: String,
    /// How many tests the OTHER files hold: libtest counts them as filtered
    /// out of this file's run, and the file's report must not.
    pub others: usize,
}

/// A built native test binary for one file.
#[derive(Clone, Debug)]
pub(crate) struct NativeTestBin {
    pub path: PathBuf,
    /// `Some` when the binary is the run's one program.
    pub module: Option<MergedModule>,
}

/// What, besides its Rust, shapes a file's crate: the native deps and the
/// package root (and with it the `native/` tree).
type Context = (Vec<crate::project::NativeDep>, Option<PathBuf>);

/// Check every file in parallel, then build: one program per package
/// context, a crate of its own for a file alone in its context and for each
/// file of a program that did not build. Results in `files` order.
pub(super) fn compile_native_tests(files: &[String], no_check: bool, scratch: &Arc<TestScratch>) -> Vec<(String, Result<NativeTestBin, String>)> {
    if no_check {
        // `--no-check` skips the very judgement the one program relies on.
        return build_per_file(files.to_vec(), true, false, scratch);
    }
    let t = super::run::PhaseTimer::start();
    let mut checked: Vec<(String, Result<Context, String>)> = bounded_parallel(files.to_vec(), cpu_slots(), move |file: String| {
        let verdict = guard_worker_panic(
            // The per-file route's own error text: its diagnostics are printed.
            || crate::compile_driver::check_through_link(&file)
                .map(|()| super::load_native_build_config(&file))
                .map_err(|_| "compile failed".to_string()),
            Err,
        );
        (file, verdict)
    });
    checked.sort_by(|a, b| a.0.cmp(&b.0));
    t.lap("per-file check");

    let mut groups: Vec<(String, Context, Vec<String>)> = Vec::new();
    let mut results: Vec<(String, Result<NativeTestBin, String>)> = Vec::new();
    for (file, verdict) in checked {
        match verdict {
            Ok(context) => {
                let key = context_key(&context);
                match groups.iter_mut().find(|(k, _, _)| *k == key) {
                    Some((_, _, members)) => members.push(file),
                    None => groups.push((key, context, vec![file])),
                }
            }
            // The compile error, as the per-file compile reported it.
            Err(e) => results.push((file, Err(e))),
        }
    }

    let mut per_file: Vec<String> = Vec::new();
    for (key, context, members) in groups {
        // `almide survive`'s overlay changes what one file reads; the one
        // program is compiled by a child process that does not carry it.
        if members.len() < 2 || almide::source_overlay::is_armed() {
            per_file.extend(members);
            continue;
        }
        let ctx = match crate::compile_driver::test_run_context() {
            Ok(ctx) => ctx,
            Err(e) => {
                note_fallback(members.len(), &e);
                per_file.extend(members);
                continue;
            }
        };
        let plan = crate::compile_driver::plan_test_run(&ctx, &members);
        t.lap("one-program plan");
        per_file.extend(plan.alone);
        let scratch_ = scratch.clone();
        // The batches compile and build in parallel, each its own crate
        // (usually there is one).
        let jobs: Vec<_> = plan.batches.into_iter().map(|b| (key.clone(), context.clone(), b)).collect();
        for (built, alone) in bounded_parallel(jobs, cpu_slots(), move |(key, context, batch)| run_batch(&key, &context, batch, &scratch_)) {
            results.extend(built);
            per_file.extend(alone);
        }
        t.lap("one-program compile+build");
    }
    results.extend(build_per_file(per_file, false, true, scratch));
    let order: std::collections::HashMap<&String, usize> = files.iter().enumerate().map(|(i, f)| (f, i)).collect();
    results.sort_by_key(|(f, _)| order.get(f).copied().unwrap_or(usize::MAX));
    results
}

/// The per-file route: each file compiled and built as its own crate, in
/// parallel. `quiet` when the file's diagnostics were already printed.
fn build_per_file(files: Vec<String>, no_check: bool, quiet: bool, scratch: &Arc<TestScratch>) -> Vec<(String, Result<NativeTestBin, String>)> {
    if files.is_empty() {
        return Vec::new();
    }
    let was = quiet.then(|| crate::SUPPRESS_WARNINGS.swap(true, std::sync::atomic::Ordering::Relaxed));
    let scratch = scratch.clone();
    let built = bounded_parallel(files, cpu_slots(), move |file: String| {
        let worker_dir = scratch.native_worker_dir(&file);
        let built = guard_worker_panic(
            || super::run::native_test_source(&file, no_check).and_then(|src| src.build(true, false, Some(&worker_dir))),
            Err,
        )
        .map(|path| NativeTestBin { path, module: None });
        (file, built)
    });
    if let Some(was) = was {
        crate::SUPPRESS_WARNINGS.store(was, std::sync::atomic::Ordering::Relaxed);
    }
    built
}

fn context_key((deps, root): &Context) -> String {
    let deps = deps
        .iter()
        .map(|d| format!("{}={}@{}", d.name, d.spec, d.target.as_deref().unwrap_or("")))
        .collect::<Vec<_>>()
        .join(",");
    format!("deps={deps};root={root:?}")
}

fn note_fallback(files: usize, why: &str) {
    if almide_base::env::flag("ALMIDE_TEST_VERBOSE") {
        crate::err(&format!("note: a native test program of {files} file(s) did not build; building them apart:\n{why}"));
    }
}

/// Compile and build one batch of the plan as one program. A batch that
/// fails sheds the files the failure names and tries again, or — when it
/// names none — splits in two; a file left alone takes the per-file route,
/// which reports its own failure as it always did. Returns the built files
/// and the ones left to the per-file route.
fn run_batch(
    key: &str,
    context: &Context,
    batch: Vec<crate::compile_driver::PlannedTestFile>,
    scratch: &TestScratch,
) -> (Vec<(String, Result<NativeTestBin, String>)>, Vec<String>) {
    let mut built = Vec::new();
    let mut alone = Vec::new();
    let mut work = vec![batch];
    while let Some(batch) = work.pop() {
        if batch.len() < 2 {
            alone.extend(batch.into_iter().map(|p| p.file));
            continue;
        }
        let files: Vec<String> = batch.iter().map(|p| p.file.clone()).collect();
        let dir_key = format!("{key};files={}", files.join(","));
        let attempt = compile_in_child(&batch, scratch)
            .and_then(|program| {
                guard_worker_panic(|| build_one_program(&dir_key, context.clone(), &files, program, scratch), Err)
                    .map_err(|e| {
                        let culprits = crate::compile_driver::culprits_in(&e, &batch);
                        (e, culprits)
                    })
            });
        match attempt {
            Ok(b) => built.extend(b),
            Err((why, culprits)) => {
                note_fallback(batch.len(), &why);
                if !culprits.is_empty() && culprits.len() < batch.len() {
                    let (out, rest): (Vec<_>, Vec<_>) = batch.into_iter().partition(|p| culprits.contains(&p.file));
                    alone.extend(out.into_iter().map(|p| p.file));
                    work.push(rest);
                } else {
                    let mut first = batch;
                    let second = first.split_off(first.len() / 2);
                    work.push(first);
                    work.push(second);
                }
            }
        }
    }
    (built, alone)
}

/// Compile a batch as one program in a child `almide test-run-compile`:
/// its own process, so nothing it prints reaches the run's output (the files'
/// own checks printed every diagnostic already) and a gate that ends a
/// process ends only it. Its stderr is shown under ALMIDE_TEST_VERBOSE.
fn compile_in_child(batch: &[crate::compile_driver::PlannedTestFile], scratch: &TestScratch) -> Result<crate::compile_driver::TestRunProgram, (String, Vec<String>)> {
    let unattributed = |why: String| (why, Vec::new());
    let out = scratch.run_file(&format!("one-program-{}.rs", batch[0].index));
    let exe = std::env::current_exe().map_err(|e| unattributed(e.to_string()))?;
    let output = std::process::Command::new(exe)
        .arg("test-run-compile")
        .arg("--out")
        .arg(&out)
        .args(batch.iter().map(|p| format!("{}={}", p.index, p.file)))
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| unattributed(format!("the test-run compile did not start: {e}")))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if almide_base::env::flag("ALMIDE_TEST_VERBOSE") {
        crate::err_no_nl(&stderr);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let verdict: serde_json::Value = stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str(l).ok())
        .ok_or_else(|| unattributed(format!("the test-run compile ended with {} and no verdict", output.status)))?;
    let strings = |key: &str| -> Vec<String> {
        verdict[key].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default()
    };
    if verdict["ok"].as_bool() != Some(true) {
        return Err((verdict["message"].as_str().unwrap_or("").to_string(), strings("culprits")));
    }
    let rs_code = std::fs::read_to_string(&out).map_err(|e| unattributed(format!("{}: {e}", out.display())))?;
    let _ = std::fs::remove_file(&out);
    Ok(crate::compile_driver::TestRunProgram { rs_code, test_prefixes: strings("prefixes") })
}

/// `almide test-run-compile --out FILE <index>=<path>…` (#4441): the child
/// side of [`compile_in_child`]. Prints one JSON line: `{"ok":true,
/// "prefixes":[…]}` with the Rust in FILE, or `{"ok":false,"message":…,
/// "culprits":[…]}`.
pub fn cmd_test_run_compile(out: &str, files: &[String]) {
    let batch: Vec<crate::compile_driver::PlannedTestFile> = files
        .iter()
        .filter_map(|f| f.split_once('='))
        .filter_map(|(i, file)| Some(crate::compile_driver::PlannedTestFile { index: i.parse().ok()?, file: file.to_string() }))
        .collect();
    let verdict = crate::compile_driver::test_run_context()
        .map_err(|message| crate::compile_driver::BatchFailure { message, culprits: vec![] })
        .and_then(|ctx| crate::compile_driver::compile_test_batch(&ctx, &batch))
        .and_then(|program| {
            std::fs::write(out, program.rs_code)
                .map(|()| program.test_prefixes)
                .map_err(|e| crate::compile_driver::BatchFailure { message: format!("{out}: {e}"), culprits: vec![] })
        });
    let line = match verdict {
        Ok(prefixes) => serde_json::json!({ "ok": true, "prefixes": prefixes }),
        Err(f) => serde_json::json!({ "ok": false, "message": f.message, "culprits": f.culprits }),
    };
    crate::out(&line.to_string());
}

/// Build one program of the run, and tell each of its files where its tests
/// are.
fn build_one_program(
    key: &str,
    (native_deps, source_root): Context,
    files: &[String],
    program: crate::compile_driver::TestRunProgram,
    scratch: &TestScratch,
) -> Result<Vec<(String, Result<NativeTestBin, String>)>, String> {
    let source = NativeSource {
        rs_code: super::build::inject_fs_scope(super::build::arm_alloc_count(program.rs_code)),
        native_deps,
        source_root,
    };
    let path = source.build(true, false, Some(&scratch.native_merged_dir(key)))?;
    let listed = list_tests(&path)?;
    let total = listed.len();
    Ok(files
        .iter()
        .zip(program.test_prefixes)
        .map(|(file, prefix)| {
            let own_path = format!("tests::{prefix}__");
            let own = listed.iter().filter(|t| t.starts_with(&own_path)).count();
            (file.clone(), Ok(NativeTestBin { path: path.clone(), module: Some(MergedModule { prefix, others: total - own }) }))
        })
        .collect())
}

/// Every test a libtest binary holds, ignored ones included (`--list`).
fn list_tests(bin: &std::path::Path) -> Result<Vec<String>, String> {
    let (code, stdout, stderr) = super::run::run_binary_captured_io(bin, &["--list".to_string(), "--format".to_string(), "terse".to_string()]);
    if code != 0 {
        return Err(format!("the test run's binary could not list its tests:\n{stdout}{stderr}"));
    }
    Ok(stdout.lines().filter_map(|l| l.strip_suffix(": test")).map(str::to_string).collect())
}
