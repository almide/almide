//! `xtarget-fuzz inspect testing.assert_aborts`: the criteria for a function
//! callable only directly in a test block (E099 elsewhere). Every probe is a
//! TEST FILE (`generator/aborts_gen.rs`, `// @xf-test`) run by `almide test`
//! on native (the re-executed test binary) and on the embedded wasm lane (a
//! fresh instance per call), and compared as a TRANSCRIPT: the verdict, and
//! for a failing test the failure block normalized across the two reporters
//! ([`transcript`]).
//!
//!   parity      a campaign of generated test files whose closures are
//!               catalogue terms ending in a defined abort (panic, an index
//!               out of bounds, a division by zero, a failed assert, an effect
//!               err propagated with `!`) or, on the failing paths, in none or
//!               under an expected text the abort does not carry: 0 unique
//!               findings between native and the embedded lane (a pair that
//!               differs only where a child died at a host resource limit,
//!               C-196/C-197, is no verdict, as in the fuzzer's ladder)
//!   complexity  n = the bytes the child writes to stderr before it aborts,
//!               which the parent reads whole and scans for the last abort
//!               header; k = calls of one site in a loop (the program's size
//!               does not grow with k); a call's child spawn is a constant
//!   alloc       the embedded lane's PARENT instance: live blocks at exit after
//!               a site of every abort and k calls (each child is a store of
//!               its own and is not counted)
//!   diamond     `assert_aborts` cannot be wrapped (E099), so the diamond is
//!               the abort's: the same test reaching the aborting fn directly
//!               and through two wrapper modules, on native and the embedded
//!               lane — a passing call, then a failing one
//!
//! `assert_aborts` is generic over its closure's value (`f: () -> T`), so
//! complexity, alloc and diamond each run with every closure valued Int and
//! again valued String, joined as `[vars=Int] … || [vars=String] …` like
//! every generic's (the 10-16 ruling); parity's campaign draws T from Unit,
//! Int and String per closure.
//!
//! Every criterion has its own reading here; none is skipped.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::{judge, toml_str, Ctx, World};
use crate::generator::aborts_gen::{self, Fill};
use crate::generator::Engine;

/// Where `--emit` writes one instantiation's probes.
fn vars_dir(fill: Fill) -> &'static str {
    match fill {
        Fill::String => "vars-String",
        _ => "vars-Int",
    }
}

/// The functions inspected as test files.
pub const TESTED: &[&str] = &["testing.assert_aborts"];

/// k's ceiling: each call spawns a child process (native) or instantiates
/// the module (wasm).
const ABORTS_MAX_K: u64 = 256;

fn default_n(counted: bool) -> u64 {
    if counted { 16_384 } else { 262_144 }
}

pub(super) fn inspect(ctx: &mut Ctx, engine: &Engine, name: &str, args: &[String]) {
    ctx.max_k = ABORTS_MAX_K;
    // Every run compiles the test file first (both legs), as the embedded
    // lane does for `almide run` (see `Ctx::calibrate_scale`).
    ctx.calibrate_scale = 10.0;
    let n: u64 = super::super::flag_value(args, "--n").and_then(|s| s.parse().ok()).unwrap_or(default_n(ctx.icount));
    if let Some(out) = super::super::flag_value(args, "--emit") {
        let root = Path::new(out);
        let mut files = Vec::new();
        for fill in Fill::MEASURED {
            let dir = vars_dir(fill);
            files.push((format!("{dir}/call_test.almd"), aborts_gen::complexity_test(n, 3, fill)));
            files.push((format!("{dir}/alloc_test.almd"), aborts_gen::alloc_test(8, fill)));
            for (i, p) in aborts_gen::diamond_projects(fill).into_iter().enumerate() {
                files.extend(p.files.into_iter().map(|(f, s)| (format!("{dir}/diamond-{}/{f}", ["d", "l", "r"][i]), s)));
            }
        }
        for i in 0..3 {
            files.push((format!("parity-{i}_test.almd"), engine.abort_test(0, i)));
        }
        for (path, src) in files {
            let file = root.join(path);
            if let Err(e) = file.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&file, src)) {
                eprintln!("{}: {e}", file.display());
                std::process::exit(2);
            }
        }
        let _ = std::fs::remove_dir_all(&ctx.dir);
        return;
    }
    let order = super::super::flag_value(args, "--order");
    // `assert_aborts` is generic (`f: () -> T`): the heap criteria run once
    // per measured T and are labelled as every generic's are
    // (`super::both_instantiations`). Parity is one campaign whose closures
    // draw T from Unit, Int and String (`aborts_gen::synthesize_test`).
    let per_fill = |f: &dyn Fn(Fill) -> String| super::both_instantiations(&f(Fill::Int), &f(Fill::String));
    let verdicts = vec![
        ("parity", parity(ctx, engine, args)),
        ("complexity", per_fill(&|fill| complexity(ctx, n, order, fill))),
        ("alloc", per_fill(&|fill| alloc(ctx, fill))),
        ("diamond", per_fill(&|fill| diamond(ctx, fill))),
    ];
    let all_pass = verdicts.iter().all(|(_, v)| v.starts_with("PASS"));
    println!("# {name} — {}", if all_pass { "all criteria PASS (stampable)" } else { "NOT stampable" });
    for (k, v) in &verdicts {
        println!("{k} = {}", toml_str(v));
    }
    let _ = std::fs::remove_dir_all(&ctx.dir);
    if !all_pass {
        std::process::exit(1);
    }
}

// ── running a test file ────────────────────────────────────────────────

/// `almide test <file> --target native|wasm` (the wasm lane's default
/// runner is the embedded host), in the program's sandbox world. The world
/// is returned with the command: dropping it removes the sandbox.
pub(super) fn test_cmd(ctx: &Ctx, file: &Path, wasm: bool) -> Result<(Command, World), String> {
    let src = std::fs::read_to_string(file).unwrap_or_default();
    let world = World::for_program(&ctx.dir, &src)?;
    let mut c = Command::new(&ctx.almide);
    c.arg("test").arg(file).args(["--target", if wasm { "wasm" } else { "native" }]);
    world.enter(&mut c);
    Ok((c, world))
}

/// A test run's transcript, the same text on both reporters for the same
/// verdict: `pass`, or `fail` and the failure block's `at:` line number and
/// its `expected:` / `found:` fields with the child sections, each line
/// trimmed (native re-renders the block indented, the wasm lane prints it as
/// written). A failure with no block (a compile error, a trap) is `fail` and
/// its first error line.
pub fn transcript(code: Option<i32>, stdout: &str, stderr: &str) -> String {
    if code == Some(0) {
        return "pass\n".into();
    }
    let all = format!("{stdout}\n{stderr}");
    let lines: Vec<&str> = all.lines().collect();
    let Some(start) = lines.iter().position(|l| l.trim_start().starts_with("expected: ")) else {
        let first = lines.iter().find(|l| l.contains("error")).or_else(|| lines.iter().find(|l| !l.trim().is_empty())).unwrap_or(&"");
        return format!("fail\n{}\n", first.trim());
    };
    let at = lines[..start]
        .iter()
        .rev()
        .find(|l| l.trim_start().starts_with("at:"))
        .map(|l| l.trim().chars().rev().take_while(char::is_ascii_digit).collect::<String>().chars().rev().collect::<String>())
        .unwrap_or_default();
    let mut out = format!("fail\nat line {at}\n");
    for l in &lines[start..] {
        let t = l.trim();
        let end = t.is_empty() || t.starts_with("test: ") || t.starts_with("error: ") || t.starts_with("PANIC: testing.assert_aborts failed");
        if end {
            break;
        }
        let t = match t.split_once(':') {
            Some((key @ ("expected" | "found"), rest)) => format!("{key}: {}", rest.trim_start()),
            _ => t.to_string(),
        };
        out.push_str(&t);
        out.push('\n');
    }
    out
}

/// The judge's artifact of a test-file probe, named so `source_of` still
/// finds `p<hash>.almd`:
///
/// - wasm: the source (`p<hash>.wasmtest.almd`), run by `almide test
///   --target wasm`. That compiles in process on every run (no rustc), and the
///   counted judge reads it clean (rms 0.0–0.1% on GCE), so it is unchanged.
/// - native: the libtest binary `almide test --target native` builds
///   (`p<hash>.nativetest.bin`), built ONCE here, outside the meter, and then
///   run directly ([`test_art_cmd`]). Counting `almide test` itself counted a
///   fresh rustc compile per run (the world gives each run its own TMPDIR,
///   so the build cache never hits): a ~1.5e8-instruction intercept that
///   wobbled enough to bend the n fit (GCE batch 28). The abort-site children
///   re-execute this same binary, so they are still counted.
pub(super) fn test_art(ctx: &Ctx, file: &Path, src: &str, wasm: bool) -> Result<PathBuf, String> {
    if wasm {
        let art = file.with_extension("wasmtest.almd");
        std::fs::write(&art, src).map_err(|e| e.to_string())?;
        return Ok(art);
    }
    let art = file.with_extension("nativetest.bin");
    if art.is_file() {
        return Ok(art);
    }
    let build = file.with_extension("nativetest.d");
    let _ = std::fs::remove_dir_all(&build);
    let tmp = build.join("tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let src_file = build.join(format!("{}_test.almd", file.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default()));
    std::fs::write(&src_file, src).map_err(|e| e.to_string())?;
    let out = Command::new(&ctx.almide)
        .arg("test")
        .arg(&src_file)
        .args(["--target", "native"])
        .env("TMPDIR", &tmp)
        .current_dir(&build)
        .output()
        .map_err(|e| format!("almide test did not run: {e}"))?;
    if !out.status.success() {
        let t = output_transcript(&out);
        let _ = std::fs::remove_dir_all(&build);
        return Err(format!("the test did not pass at build: {}", t.replace('\n', " / ")));
    }
    let bin = built_test_bin(&tmp);
    let copied = bin.and_then(|b| std::fs::copy(&b, &art).map_err(|e| format!("{}: {e}", b.display())));
    let _ = std::fs::remove_dir_all(&build);
    copied.map(|_| art)
}

/// The libtest binary a native `almide test` left in its build cache:
/// `$TMPDIR/almide-test/native/<stem>-<path hash>/almide_test_bin`
/// (`src/cli/test_scratch.rs`). Exactly one, or the layout moved.
pub fn built_test_bin(tmp: &Path) -> Result<PathBuf, String> {
    let root = tmp.join("almide-test").join("native");
    let found: Vec<PathBuf> = std::fs::read_dir(&root)
        .map_err(|e| format!("no native test cache at {}: {e}", root.display()))?
        .filter_map(|d| d.ok())
        .map(|d| d.path().join(TEST_BIN))
        .filter(|p| p.is_file())
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        _ => Err(format!("expected one {TEST_BIN} under {}, found {}", root.display(), found.len())),
    }
}

const TEST_BIN: &str = "almide_test_bin";

/// Some(wasm) for an artifact [`test_art`] named, else None.
pub(super) fn test_art_leg(art: &Path) -> Option<bool> {
    let name = art.to_string_lossy();
    if name.ends_with(".wasmtest.almd") {
        Some(true)
    } else if name.ends_with(".nativetest.bin") {
        Some(false)
    } else {
        None
    }
}

/// The run of a test artifact, without the world (the caller enters it;
/// `cwd` is the world's directory). wasm: `almide test <art> --target wasm`.
/// native: the binary with the argv and env `almide test` gives it
/// (`src/cli/test_report.rs` HARNESS_FLAGS, `src/cli/run.rs`
/// `binary_command`: RUST_MIN_STACK and ALMIDE_CWD, the launcher's cwd).
pub(super) fn test_art_cmd(ctx: &Ctx, art: &Path, wasm: bool, cwd: &Path) -> Command {
    if wasm {
        let mut c = Command::new(&ctx.almide);
        c.arg("test").arg(art).args(["--target", "wasm"]);
        return c;
    }
    let mut c = Command::new(art);
    c.args(["--nocapture", "--test-threads=1"]).env("RUST_MIN_STACK", "8388608").env("ALMIDE_CWD", cwd);
    c
}

/// A finished test run's transcript; a test that did not pass is a failed
/// run for the judge (its probes are the success path).
pub(super) fn output_transcript(out: &std::process::Output) -> String {
    transcript(out.status.code(), &String::from_utf8_lossy(&out.stdout), &String::from_utf8_lossy(&out.stderr))
}

pub(super) fn run_test_art(ctx: &Ctx, art: &Path, wasm: bool, world: &World) -> Result<String, String> {
    let mut c = test_art_cmd(ctx, art, wasm, &world.dir);
    world.enter(&mut c);
    let out = c.output().map_err(|e| format!("almide test did not run: {e}"))?;
    let t = output_transcript(&out);
    if !out.status.success() {
        return Err(format!("the test did not pass: {}", t.replace('\n', " / ")));
    }
    Ok(t)
}

fn run_transcript(ctx: &Ctx, file: &Path, wasm: bool, cwd: Option<&Path>) -> Result<String, String> {
    let (mut c, _world) = test_cmd(ctx, file, wasm)?;
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let o = c.output().map_err(|e| format!("almide test did not run: {e}"))?;
    Ok(transcript(o.status.code(), &String::from_utf8_lossy(&o.stdout), &String::from_utf8_lossy(&o.stderr)))
}

// ── complexity ─────────────────────────────────────────────────────────

/// Criterion 2. `n` is the bytes the child writes to stderr before its
/// abort: the parent receives the child's stderr whole and scans it for the
/// last abort header (D4), so n is the size of the call's own reading. A
/// "call" is one `assert_aborts` call of one site in a loop, so k..2k is
/// the marginal cost of k more calls with the program unchanged. Each call
/// starts a child (a process spawn natively, an instantiation of the
/// compiled module on the embedded lane): a constant per call, in the
/// intercept of the fit. The closure is valued `fill` (T).
fn complexity(ctx: &Ctx, n: u64, order: Option<&str>, fill: Fill) -> String {
    let programs = |m: u64, k: u64| (aborts_gen::complexity_test(m, k, fill), aborts_gen::complexity_test(m, 0, fill));
    match judge(ctx, "assert_aborts(n=child stderr bytes before the abort, k=calls)", order, n, &programs) {
        Ok(s) => format!("PASS: {s}"),
        Err(s) => format!("FAIL: {s}"),
    }
}

// ── alloc ──────────────────────────────────────────────────────────────

/// Criterion "alloc": the embedded lane's armed counters for the PARENT
/// instance at exit, after one site of each abort and k=8 calls of one site,
/// every closure valued `fill` (T).
pub(super) fn alloc(ctx: &Ctx, fill: Fill) -> String {
    let src = aborts_gen::alloc_test(8, fill);
    let file = ctx.dir.join(format!("a{:016x}_test.almd", super::hash(&src)));
    if std::fs::write(&file, &src).is_err() {
        return "SKIP: scratch write failed".into();
    }
    let (mut c, _world) = match test_cmd(ctx, &file, true) {
        Ok(c) => c,
        Err(e) => return format!("SKIP: {e}"),
    };
    let Ok(out) = c.env("ALMIDE_WASM_ALLOC_COUNT", "1").output() else { return "SKIP: could not run almide".into() };
    alloc_verdict(out.status.code(), &String::from_utf8_lossy(&out.stderr))
}

pub fn alloc_verdict(code: Option<i32>, stderr: &str) -> String {
    let line = stderr.lines().find(|l| l.starts_with("__ALMD_WASM_ALLOC "));
    let live = line.and_then(|l| l.split_whitespace().find_map(|w| w.strip_prefix("live=")));
    match (code, live) {
        (Some(0), Some("0")) => "PASS: [embedded] parent live=0 after 5 sites (each abort) and 8 calls of one site".into(),
        (Some(0), Some(v)) => format!("FAIL: [embedded] parent live={v} after 5 sites and 8 calls"),
        (Some(0), None) => format!("FAIL: no counter line ({})", stderr.lines().next().unwrap_or("")),
        (c, _) => format!("FAIL: the alloc test did not pass (exit {c:?})"),
    }
}

// ── diamond ────────────────────────────────────────────────────────────

/// Criterion 5, as the abort's diamond: E099 refuses a wrapper of
/// `assert_aborts` (a call outside a test block), so there is no
/// `left.call` path to the function under inspection. What a closure may
/// name is a module's fn, so three projects run the same test, its closure
/// reaching the aborting fn as `base.boom`, `left.boom` and `right.boom`
/// (wrappers over it): a passing call, then a failing one. The three
/// transcripts must be equal on each leg, and the legs equal. `boom`, and so
/// the closure, is valued `fill` (T).
fn diamond(ctx: &Ctx, fill: Fill) -> String {
    let mut legs: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    for (p, project) in ["d", "l", "r"].into_iter().zip(aborts_gen::diamond_projects(fill)) {
        let root = ctx.dir.join(format!("diamond-aborts-{}-{p}", vars_dir(fill)));
        let Ok(entry) = project.write_to(&root) else { return "SKIP: scratch write failed".into() };
        for (i, wasm) in [false, true].into_iter().enumerate() {
            match run_transcript(ctx, &entry, wasm, Some(&root.join("app"))) {
                Ok(t) => legs[i].push(t),
                Err(e) => return format!("SKIP: {e}"),
            }
        }
    }
    diamond_verdict(&legs[0], &legs[1])
}

pub fn diamond_verdict(native: &[String], wasm: &[String]) -> String {
    let same_three = |t: &[String]| t.len() == 3 && t[0] == t[1] && t[0] == t[2];
    let shaped = native.first().is_some_and(|t| t.starts_with("fail\n") && t.contains("found: abort \"PANIC: xf-diamond t\""));
    match (same_three(native), same_three(wasm), native == wasm) {
        (true, true, true) if shaped => {
            "PASS: direct = via left = via right on both legs (the passing call passes, the failing one reports found: abort \"PANIC: xf-diamond t\")".into()
        }
        (true, true, true) => format!("FAIL: the three paths agree but not on the probe's verdict: {:?}", native[0]),
        (false, _, _) => format!("FAIL: the three paths differ on native: {native:?}"),
        (_, false, _) => format!("FAIL: the three paths differ on wasm: {wasm:?}"),
        (true, true, false) => format!("FAIL: native vs wasm: {:?} / {:?}", native[0], wasm[0]),
    }
}

// ── parity ─────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The checker refused the generated test: a generator miss.
    Rejected(String),
    /// Both legs passed.
    PassedAlike,
    /// Both legs failed with the same transcript (a failing path).
    FailedAlike,
    /// The legs differ only where a child died at a host resource limit
    /// (C-196 stack depth, C-197 address space): not an observable the ALS
    /// specifies, so no verdict — the fuzzer's resource-limit family.
    ResourceLimit,
    /// A divergence, keyed by its signature for de-duplication.
    Finding(String),
}

/// A child's abort at a host resource limit, as the fuzzer's ladder reads
/// it (`oracle::ladder::resource_limit`): the abort text a failing call
/// reports as `found: abort "…"`.
const RESOURCE_SIGNATURES: [&str; 4] = ["Error: out of memory", "call stack exhausted", "stack overflow", "bytes failed"];

/// Some transcript's failure block reports a child that died at a resource
/// limit.
pub fn resource_limited(t: &str) -> bool {
    t.lines().any(|l| l.starts_with("found: abort ") && RESOURCE_SIGNATURES.iter().any(|s| l.contains(s)))
}

/// Criteria 1 and 4: `--count` generated test files (default 200) from
/// `--seed`, each run on native and the embedded lane.
fn parity(ctx: &Ctx, engine: &Engine, args: &[String]) -> String {
    let count: u64 = super::super::flag_value(args, "--count").and_then(|s| s.parse().ok()).unwrap_or(200);
    let jobs: u64 = super::super::flag_value(args, "--jobs").and_then(|s| s.parse().ok()).unwrap_or(4).max(1);
    let seed: u64 = super::super::flag_value(args, "--seed").and_then(|s| s.parse().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
    });
    let findings_dir = std::env::temp_dir().join(format!("xtarget-aborts-findings-{seed}"));
    let _ = std::fs::remove_dir_all(&findings_dir);
    let next = AtomicU64::new(0);
    let results: Mutex<Vec<(u64, Outcome)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= count {
                    break;
                }
                let src = engine.abort_test(seed, i);
                let out = one_test(ctx, &src, i, &findings_dir);
                results.lock().unwrap().push((i, out));
            });
        }
    });
    let results = results.into_inner().unwrap();
    parity_line(seed, count, &results, &findings_dir)
}

fn parity_line(seed: u64, count: u64, results: &[(u64, Outcome)], findings_dir: &Path) -> String {
    let rejected: Vec<&String> = results.iter().filter_map(|(_, o)| if let Outcome::Rejected(e) = o { Some(e) } else { None }).collect();
    let passed = results.iter().filter(|(_, o)| *o == Outcome::PassedAlike).count();
    let failed = results.iter().filter(|(_, o)| *o == Outcome::FailedAlike).count();
    let limited = results.iter().filter(|(_, o)| *o == Outcome::ResourceLimit).count();
    let mut sigs: Vec<(String, u64)> = Vec::new();
    for (i, o) in results {
        if let Outcome::Finding(sig) = o {
            if !sigs.iter().any(|(s, _)| s == sig) {
                sigs.push((sig.clone(), *i));
            }
        }
    }
    sigs.sort();
    let reject_note = rejected.first().map(|e| format!(" (first: {e})")).unwrap_or_default();
    let mut line = format!(
        "seed {seed}, {count} generated test files; legs native, embedded; {passed} passed alike, {failed} failed alike with the same failure block; {limited} skipped at a host resource limit (C-196/C-197); {} generator rejects{reject_note}; unique findings {}",
        rejected.len(),
        sigs.len()
    );
    if !sigs.is_empty() {
        let shown: Vec<String> = sigs.iter().map(|(s, i)| format!("[{s}] e.g. #{i}")).collect();
        line.push_str(&format!(": {} — test files and transcripts in {}", shown.join("; "), findings_dir.display()));
    }
    // Both verdicts must be exercised: a campaign that only passes (or only
    // fails) compared half of what it claims.
    let pass = sigs.is_empty() && passed > 0 && failed > 0;
    format!("{}: {line}", if pass { "PASS" } else { "FAIL" })
}

fn one_test(ctx: &Ctx, src: &str, i: u64, findings: &Path) -> Outcome {
    let dir = ctx.dir.join(format!("aborts-{i}"));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("gen_test.almd");
    if std::fs::write(&file, src).is_err() {
        return Outcome::Finding("scratch write failed".into());
    }
    match Command::new(&ctx.almide).arg("check").arg(&file).output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => return Outcome::Rejected(super::first_error(&o.stderr)),
        Err(e) => return Outcome::Finding(format!("almide check did not run: {e}")),
    }
    let (n, w) = match (run_transcript(ctx, &file, false, None), run_transcript(ctx, &file, true, None)) {
        (Ok(n), Ok(w)) => (n, w),
        (Err(e), _) | (_, Err(e)) => return Outcome::Finding(e),
    };
    let out = if n != w && (resource_limited(&n) || resource_limited(&w)) {
        Outcome::ResourceLimit
    } else if n != w {
        let first = n.lines().zip(w.lines()).find(|(a, b)| a != b).map(|(a, b)| format!("{a} | {b}")).unwrap_or_else(|| "length".into());
        let key = first.split(':').next().unwrap_or("").to_string();
        let dir = findings.join(format!("{i:04}"));
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("gen_test.almd"), src);
        let _ = std::fs::write(dir.join("native.txt"), &n);
        let _ = std::fs::write(dir.join("embedded.txt"), &w);
        Outcome::Finding(format!("native vs embedded at `{key}`"))
    } else if n == "pass\n" {
        Outcome::PassedAlike
    } else {
        Outcome::FailedAlike
    };
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NATIVE_FAIL: &str = "FAILED: f_test.almd\n  at:   f_test.almd:4\n  expected: abort containing \"boom\"\n  found:    abort \"PANIC: other\n    second\"\n      child stdout:\n        out line\n  test: case\n  at:   f_test.almd:4\n  error: testing.assert_aborts failed at line 4\n\n1 test in 1 file\n";
    const WASM_FAIL: &str = "FAIL f_test.almd\n  trapped at: test: case ... \n  Error: assertion failed\n    at: line 4\n    expected: abort containing \"boom\"\n    found: abort \"PANIC: other\n  second\"\n    child stdout:\n      out line\n  PANIC: testing.assert_aborts failed at line 4\n\n0 passed, 1 failed (of 1 files)\n";

    #[test]
    fn the_two_reporters_normalize_to_one_transcript() {
        let n = transcript(Some(1), "", NATIVE_FAIL);
        let w = transcript(Some(1), WASM_FAIL, "");
        assert_eq!(n, w);
        assert_eq!(n, "fail\nat line 4\nexpected: abort containing \"boom\"\nfound: abort \"PANIC: other\nsecond\"\nchild stdout:\nout line\n");
        assert_eq!(transcript(Some(0), "anything", "at all"), "pass\n");
    }

    #[test]
    fn a_different_found_is_a_different_transcript() {
        let other = WASM_FAIL.replace("PANIC: other", "PANIC: else");
        assert_ne!(transcript(Some(1), "", NATIVE_FAIL), transcript(Some(1), &other, ""));
    }

    #[test]
    fn a_failure_without_a_block_keeps_its_first_error_line() {
        assert_eq!(transcript(Some(1), "", "warning: x\nerror[E0277]: the `?` operator\nmore\n"), "fail\nerror[E0277]: the `?` operator\n");
    }

    #[test]
    fn the_native_test_binary_is_found_where_almide_test_leaves_it() {
        let tmp = std::env::temp_dir().join(format!("xf-aborts-bin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let cache = tmp.join("almide-test").join("native");
        assert!(built_test_bin(&tmp).unwrap_err().starts_with("no native test cache"));
        std::fs::create_dir_all(cache.join("p0-1a2b")).unwrap();
        assert!(built_test_bin(&tmp).unwrap_err().contains("found 0"));
        std::fs::write(cache.join("p0-1a2b").join(TEST_BIN), "bin").unwrap();
        assert_eq!(built_test_bin(&tmp).unwrap(), cache.join("p0-1a2b").join(TEST_BIN));
        std::fs::create_dir_all(cache.join("p1-3c4d")).unwrap();
        std::fs::write(cache.join("p1-3c4d").join(TEST_BIN), "bin").unwrap();
        assert!(built_test_bin(&tmp).unwrap_err().contains("found 2"), "two builds are ambiguous");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The counted native run is the built binary with `almide test`'s argv
    /// and env — never `almide test` (whose rustc compile the meter counted).
    #[test]
    fn a_native_test_artifact_runs_the_binary_as_almide_test_does() {
        let ctx = Ctx {
            almide: "/x/almide".into(),
            wasmtime: "wasmtime".into(),
            dir: "/tmp/d".into(),
            icount: true,
            memshim: None,
            max_k: 1,
            calibrate_scale: 1.0,
        };
        let art = Path::new("/tmp/d/p00ab.nativetest.bin");
        assert_eq!(test_art_leg(art), Some(false));
        assert_eq!(test_art_leg(Path::new("/tmp/d/p00ab.wasmtest.almd")), Some(true));
        assert_eq!(test_art_leg(Path::new("/tmp/d/p00ab.nativetest.almd")), None, "the source is no longer a native artifact");
        assert_eq!(super::super::source_of(art), "", "source_of strips every kind suffix");
        let c = test_art_cmd(&ctx, art, false, Path::new("/w"));
        assert_eq!(c.get_program(), art.as_os_str());
        assert_eq!(c.get_args().collect::<Vec<_>>(), ["--nocapture", "--test-threads=1"]);
        let envs: Vec<_> = c.get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
        assert!(envs.contains(&("RUST_MIN_STACK".into(), Some("8388608".into()))) && envs.contains(&("ALMIDE_CWD".into(), Some("/w".into()))), "{envs:?}");
        let w = test_art_cmd(&ctx, Path::new("/tmp/d/p00ab.wasmtest.almd"), true, Path::new("/w"));
        assert_eq!(w.get_program(), "/x/almide");
        assert_eq!(w.get_args().collect::<Vec<_>>(), ["test", "/tmp/d/p00ab.wasmtest.almd", "--target", "wasm"]);
    }

    /// The joined criterion is what the stamp gate reads: one string, PASS
    /// only when both instantiations pass, with a `[vars=String]` half.
    #[test]
    fn a_heap_criterion_carries_both_instantiations() {
        let pass = alloc_verdict(Some(0), "__ALMD_WASM_ALLOC live=0\n");
        let joined = super::super::both_instantiations(&pass, &pass);
        assert!(joined.starts_with("PASS: [vars=Int] [embedded] parent live=0") && joined.contains(" || [vars=String] [embedded]"), "{joined}");
        let fail = alloc_verdict(Some(0), "__ALMD_WASM_ALLOC live=3\n");
        assert!(super::super::both_instantiations(&pass, &fail).starts_with("FAIL: [vars=Int]"));
        assert_eq!(vars_dir(Fill::String), "vars-String");
    }

    #[test]
    fn the_alloc_verdict_reads_the_parent_counter_line() {
        assert!(alloc_verdict(Some(0), "__ALMD_WASM_ALLOC allocs=9 reused=0 bytes=1 frees=9 reclaimed=0 live=0 heap_end=1\n").starts_with("PASS"));
        assert!(alloc_verdict(Some(0), "__ALMD_WASM_ALLOC allocs=9 live=2 heap_end=1\n").starts_with("FAIL: [embedded] parent live=2"));
        assert!(alloc_verdict(Some(0), "").starts_with("FAIL: no counter line"));
        assert!(alloc_verdict(Some(1), "__ALMD_WASM_ALLOC live=0\n").starts_with("FAIL: the alloc test did not pass"));
    }

    #[test]
    fn the_diamond_needs_three_equal_paths_on_equal_legs_with_the_probes_verdict() {
        let t = "fail\nat line 6\nexpected: xf-never-printed\nfound: abort \"PANIC: xf-diamond t\"\n".to_string();
        let three = vec![t.clone(), t.clone(), t.clone()];
        assert!(diamond_verdict(&three, &three).starts_with("PASS"));
        let pass = vec!["pass\n".to_string(); 3];
        assert!(diamond_verdict(&pass, &pass).starts_with("FAIL: the three paths agree but not"));
        let mut odd = three.clone();
        odd[2] = "pass\n".into();
        assert!(diamond_verdict(&odd, &three).starts_with("FAIL: the three paths differ on native"));
    }

    #[test]
    fn a_child_at_a_resource_limit_is_no_verdict() {
        assert!(resource_limited("fail\nat line 12\nexpected: abort containing \"x\"\nfound: abort \"Error: out of memory\n\"\n"));
        assert!(!resource_limited("fail\nat line 12\nexpected: abort containing \"x\"\nfound: abort \"PANIC: boom\"\nchild stderr:\nError: out of memory\n"));
        assert!(!resource_limited("pass\n"));
    }

    #[test]
    fn parity_passes_only_with_both_verdicts_and_no_finding() {
        let d = Path::new("/tmp/x");
        let both = vec![(0, Outcome::PassedAlike), (1, Outcome::FailedAlike), (2, Outcome::Rejected("e".into()))];
        assert!(parity_line(1, 3, &both, d).starts_with("PASS"));
        let only_pass = vec![(0, Outcome::PassedAlike)];
        assert!(parity_line(1, 1, &only_pass, d).starts_with("FAIL"));
        let finding = vec![(0, Outcome::PassedAlike), (1, Outcome::FailedAlike), (2, Outcome::Finding("x".into()))];
        assert!(parity_line(1, 3, &finding, d).contains("unique findings 1"));
    }
}
