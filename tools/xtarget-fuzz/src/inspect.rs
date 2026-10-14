//! `xtarget-fuzz inspect <module.fn>` — the harness behind the stdlib
//! inspection ledger (proofs/stdlib-inspection.toml). One run checks one
//! function against the criteria the ledger states and prints the evidence
//! lines for its row:
//!
//!   parity      a focused differential campaign (criteria 1 and 4)
//!   complexity  the plain call and each rebind use-shape, from built
//!               artifacts on both legs (criterion 2): user-space
//!               instructions (`perf stat -e instructions:u`) at n .. 16n,
//!               fitted to the lowest order class that explains them; wall
//!               time where no PMU counts (reported, never stampable)
//!   alloc       0 live wasm blocks at exit for every timing program
//!   diamond     the call through a module diamond equals the direct call,
//!               on both legs (criterion 5)
//!
//! Criterion 3 (codopsy A) is a property of the change, not of a run: it is
//! measured when a fix lands and recorded in the row's `stamp`.
//!
//! A verdict is PASS, FAIL or a named reason it could not be judged; only an
//! all-PASS run may be stamped.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::generator::{
    inspect_gen::{Target, ERR_PATH, OFF_DOMAIN},
    Engine, Family,
};

const NOISE_FLOOR: Duration = Duration::from_millis(60);
const CALIBRATE_TARGET: Duration = Duration::from_millis(250);
const MAX_K: u64 = 1 << 22;

struct Ctx {
    almide: PathBuf,
    wasmtime: PathBuf,
    dir: PathBuf,
    /// `perf stat -e instructions:u` works here (a PMU is exposed): the
    /// complexity criterion counts instructions instead of timing.
    icount: bool,
    /// The LD_PRELOAD byte counter ([`MEMSHIM_C`], compiled at start), or
    /// None when no C compiler is here: then a counted verdict is reported
    /// but not stampable, since bulk copies are invisible to instructions.
    memshim: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum Leg {
    Native,
    Wasm,
}

pub fn cmd_inspect(args: &[String]) {
    let Some(name) = args.iter().find(|a| !a.starts_with("--") && a.contains('.')).cloned() else {
        eprintln!("usage: xtarget-fuzz inspect <module.fn> [--order 1|log|n|nlogn|n2] [--order-rebind ..] [--n N] [--count C] [--seed S] [--emit DIR]");
        std::process::exit(2);
    };
    let repo = super::resolve_repo(args);
    let dir = std::env::temp_dir().join(format!("xtarget-inspect-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let icount = icount_available();
    let ctx = Ctx {
        almide: super::resolve_almide(&repo, args),
        wasmtime: super::resolve_wasmtime(),
        memshim: if icount { build_memshim(&dir) } else { None },
        dir,
        icount,
    };
    let target = Engine::with_family(&repo, Family::Synthesis).inspect_target(&name).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let order = super::flag_value(args, "--order");
    // Every size must sit in one memory tier: from 2000 to 8000 Int
    // elements a list crosses L1 (16 to 64 KiB), and the 4n step read x3.6
    // for a linear map on native — a cache cliff, not an order. Starting at
    // 200,000 elements (1.6 MB) keeps n, 2n and 4n all past L2; only a declared
    // quadratic function stays small enough to finish. Not a power of two:
    // at exactly 2^18 Int elements (2 MiB) wasm list.map ran 4x slower than
    // at 262,000 or 2^19 with identical allocation counts — a placement
    // effect of two blocks exactly 2 MiB apart, not an order.
    // Counted instructions see no cache tier at all, so the counted judge
    // starts small (n .. 16n); the timed fallback keeps the past-L2 sizes.
    let default_n = match (order == Some("n2"), ctx.icount) {
        (true, true) => 500,
        (true, false) => 2000,
        (false, true) => 20_000,
        (false, false) => 200_000,
    };
    let n: u64 = super::flag_value(args, "--n").and_then(|s| s.parse().ok()).unwrap_or(default_n);
    let order_rebind = super::flag_value(args, "--order-rebind");
    // `--emit DIR`: write the probe programs at size n and stop, so a row's
    // evidence can be re-run by hand on both legs (#3666's audit).
    if let Some(out) = super::flag_value(args, "--emit") {
        let root = Path::new(out);
        for (path, src) in probe_files(&target, n) {
            let file = root.join(path);
            if let Err(e) = file.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&file, src)) {
                eprintln!("{}: {e}", file.display());
                std::process::exit(2);
            }
        }
        let _ = std::fs::remove_dir_all(&ctx.dir);
        return;
    }

    let mut verdicts = Vec::new();
    verdicts.push(("parity", parity(&ctx, &name, args)));
    verdicts.push(("complexity", complexity(&ctx, &target, n, order, order_rebind)));
    verdicts.push(("alloc", alloc(&ctx, &target, n)));
    verdicts.push(("diamond", diamond(&ctx, &target, n)));

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

/// The programs the criteria run at size `n`, as files: the plain call
/// (3 calls), each rebind shape (3 steps) and the diamond project (at
/// `n.min(64)`, as [`diamond`] runs it).
fn probe_files(t: &Target, n: u64) -> Vec<(String, String)> {
    let mut files = vec![("call.almd".to_string(), t.direct_program(n, 3, true))];
    files.extend(t.rebind_programs(n, 3).into_iter().map(|(shape, src)| (format!("{shape}.almd"), src)));
    files.extend(t.diamond_project(n.min(64)).files.into_iter().map(|(p, src)| (format!("diamond/{p}"), src)));
    files
}

fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Criteria 1 and 4: a fresh `--focus` campaign through the full ladder.
fn parity(ctx: &Ctx, name: &str, args: &[String]) -> String {
    let count = super::flag_value(args, "--count").unwrap_or("200");
    let seed = super::flag_value(args, "--seed").map(String::from).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs().to_string()).unwrap_or_default()
    });
    let out = Command::new(std::env::current_exe().expect("self"))
        .args(["run", "--family", "synthesis", "--focus", name, "--count", count, "--seed", &seed, "--jobs", "4"])
        .arg("--almide")
        .arg(&ctx.almide)
        .arg("--out")
        .arg(ctx.dir.join("findings"))
        .output();
    let Ok(out) = out else { return "SKIP: could not start the campaign".into() };
    let err = String::from_utf8_lossy(&out.stderr);
    let findings = err
        .lines()
        .find_map(|l| l.trim().strip_prefix("unique findings").map(|r| r.trim_start_matches([' ', '=']).trim().to_string()))
        .unwrap_or_else(|| "?".into());
    let focus = err
        .lines()
        .find(|l| l.contains("program(s) call it"))
        .and_then(|l| l.split_once('=').map(|(_, r)| r.trim().to_string()))
        .unwrap_or_else(|| "no program called it".into());
    // "<hits> program(s) call it, <clean> of them clean on every leg"
    let clean = focus.split(", ").nth(1).and_then(|c| c.split_whitespace().next()?.parse::<u64>().ok()).unwrap_or(0);
    let verdict = if findings.starts_with('0') && clean > 0 { "PASS" } else { "FAIL" };
    format!("{verdict}: seed {seed}, {count} programs; {focus}; unique findings {findings}")
}

/// A rebind step that multiplies the value (`flat_map`, `intersperse`)
/// cannot run k steps; it is reported, not timed. Judged on the loop shape
/// at size 16: the digest after 4 steps against the start.
fn grows_per_step(ctx: &Ctx, t: &Target) -> Option<String> {
    let at = |k: u64| -> Option<i64> {
        let src = t.rebind_programs(16, k).into_iter().find(|(s, _)| *s == "loop")?.1;
        let art = build(ctx, &src, Leg::Native).ok()?;
        run_artifact(ctx, &art, Leg::Native).ok()?.trim().parse().ok()
    };
    let (Some(a), Some(b)) = (at(0), at(4)) else { return None };
    (b > 2 * a + 8).then(|| format!("the value grows geometrically per step ({a} -> {b} after 4 steps)"))
}

fn expected_ratio(order: &str) -> Option<f64> {
    Some(match order {
        "1" => 1.0,
        "log" => 1.15,
        "n" => 2.0,
        "nlogn" => 2.3,
        "n2" => 4.0,
        _ => return None,
    })
}

/// Criterion 2. The plain call is judged against `--order`; every rebind
/// shape against `--order-rebind`. An undeclared order is reported, not passed.
fn complexity(ctx: &Ctx, t: &Target, n: u64, order: Option<&str>, order_rebind: Option<&str>) -> String {
    let mut parts = Vec::new();
    let mut ok = true;
    if t.size_param.is_none() {
        return "PASS: no size parameter — constant-size arguments only (the plain call is the parity campaign's)".into();
    }
    let direct = |n: u64, k: u64, on: bool| t.direct_program(n, k, on);
    match judge(ctx, "call", order, n, &|n, k| (direct(n, k, true), direct(n, k, false))) {
        Ok(s) => parts.push(s),
        Err(s) => {
            ok = false;
            parts.push(s)
        }
    }
    if let Some(why) = t.rebind_param.and_then(|_| grows_per_step(ctx, t)) {
        parts.push(format!("rebind shapes not timed: {why}"));
        return format!("{}: {}", if ok { "PASS" } else { "FAIL" }, parts.join("; "));
    }
    for shape in ["loop", "tail", "fold"] {
        if t.rebind_param.is_none() {
            break;
        }
        let pick = |n: u64, k: u64| {
            let p = t.rebind_programs(n, k).into_iter().find(|(s, _)| *s == shape).map(|(_, p)| p).unwrap_or_default();
            let base = t.rebind_programs(n, 0).into_iter().find(|(s, _)| *s == shape).map(|(_, p)| p).unwrap_or_default();
            (p, base)
        };
        match judge(ctx, shape, order_rebind, n, &pick) {
            Ok(s) => parts.push(s),
            Err(s) => {
                ok = false;
                parts.push(s)
            }
        }
    }
    format!("{}: {}", if ok { "PASS" } else { "FAIL" }, parts.join("; "))
}

/// Criterion 2's judge, dispatched on the meter this host offers.
fn judge(
    ctx: &Ctx,
    label: &str,
    order: Option<&str>,
    n: u64,
    programs: &dyn Fn(u64, u64) -> (String, String),
) -> Result<String, String> {
    if ctx.icount {
        return judge_counted(ctx, label, order, n, programs);
    }
    judge_timed(ctx, label, order, n, programs).map_err(|e| format!("{e} [meter=wall-time, no PMU: not stampable]"))
        .and_then(|ok| Err(format!("{ok} [meter=wall-time, no PMU: not stampable]")))
}

/// The wall-time judge: calibrate `k` at size `n`, then time (program -
/// baseline) at n, 2n and 4n on both legs; the growth per doubling must fit
/// the declared order. Kept for hosts without a PMU, where it reports but
/// never passes: a cache tier crossed between two sizes reads as an order.
fn judge_timed(
    ctx: &Ctx,
    label: &str,
    order: Option<&str>,
    n: u64,
    programs: &dyn Fn(u64, u64) -> (String, String),
) -> Result<String, String> {
    let mut k = 1u64;
    loop {
        let (p, b) = programs(n, k);
        // Both legs: a call one leg optimizes away must not drive k so high
        // that the other leg runs for hours (eight list.* runs hit the
        // 30-minute cap that way).
        let dn = delta(ctx, &p, &b, Leg::Native).map_err(|e| format!("{label} {e}"))?;
        let dw = delta(ctx, &p, &b, Leg::Wasm).map_err(|e| format!("{label} {e}"))?;
        if dn.max(dw) >= CALIBRATE_TARGET || k >= MAX_K {
            break;
        }
        k = (k * 4).min(MAX_K);
    }
    let mut line = format!("{label} k={k}");
    let mut worst: f64 = 0.0;
    for leg in [Leg::Native, Leg::Wasm] {
        let mut ds = Vec::new();
        for m in [n, 2 * n, 4 * n] {
            let (p, b) = programs(m, k);
            ds.push(delta(ctx, &p, &b, leg).map_err(|e| format!("{label} {e}"))?);
        }
        let r1 = ratio(ds[0], ds[1]);
        let r2 = ratio(ds[1], ds[2]);
        let tag = if matches!(leg, Leg::Native) { "native" } else { "wasm" };
        line.push_str(&format!(
            " {tag} {:.3}/{:.3}/{:.3}s x{r1:.2} x{r2:.2}",
            ds[0].as_secs_f64(),
            ds[1].as_secs_f64(),
            ds[2].as_secs_f64()
        ));
        if ds[1] >= NOISE_FLOOR {
            worst = worst.max(r2);
        }
        if ds[0] >= NOISE_FLOOR {
            worst = worst.max(r1);
        }
    }
    if worst == 0.0 {
        // Every timing sat under the noise floor at the largest k: the call
        // costs at most floor/k at every size, which bounds it, not grows it.
        let bound = NOISE_FLOOR.as_secs_f64() / k as f64 * 1e9;
        line.push_str(&format!(" (under the {}ms floor at every size: <= {bound:.1} ns/call)", NOISE_FLOOR.as_millis()));
        return match order {
            Some("1" | "log") => Ok(format!("{line} (order {} ok)", order.unwrap_or("?"))),
            Some(o) => Err(format!("{line} (declared order {o} is not visible at this n — raise --n)")),
            None => Err(format!("{line} (order undeclared)")),
        };
    }
    match order.and_then(expected_ratio) {
        None => Err(format!("{line} (order undeclared: measured worst x{worst:.2})")),
        Some(e) if worst <= e * 1.35 + 0.15 => Ok(format!("{line} (order {} ok)", order.unwrap_or("?"))),
        Some(_) => Err(format!("{line} (order {} exceeded: worst x{worst:.2})", order.unwrap_or("?"))),
    }
}

fn ratio(a: Duration, b: Duration) -> f64 {
    b.as_secs_f64() / a.as_secs_f64().max(1e-4)
}

/// Built-artifact time of `prog` minus `base`, best of three each, after
/// checking both print the same thing on this leg's twin run.
fn delta(ctx: &Ctx, prog: &str, base: &str, leg: Leg) -> Result<Duration, String> {
    let tp = best_time(ctx, prog, leg)?;
    let tb = best_time(ctx, base, leg)?;
    Ok(tp.saturating_sub(tb))
}

fn best_time(ctx: &Ctx, src: &str, leg: Leg) -> Result<Duration, String> {
    let art = build(ctx, src, leg)?;
    let mut best = Duration::MAX;
    let mut first: Option<String> = None;
    for _ in 0..3 {
        let start = Instant::now();
        let out = run_artifact(ctx, &art, leg)?;
        best = best.min(start.elapsed());
        if first.get_or_insert_with(|| out.clone()) != &out {
            return Err("nondeterministic output".into());
        }
    }
    // Same output on the other leg (criterion 1, on the timing programs too).
    if matches!(leg, Leg::Wasm) {
        let native = run_artifact(ctx, &build(ctx, src, Leg::Native)?, Leg::Native)?;
        if first.as_deref() != Some(native.as_str()) {
            return Err(format!("native and wasm differ: {native:?} vs {first:?}"));
        }
    }
    Ok(best)
}

fn build(ctx: &Ctx, src: &str, leg: Leg) -> Result<PathBuf, String> {
    let file = ctx.dir.join(format!("p{:016x}.almd", hash(src)));
    std::fs::write(&file, src).map_err(|e| e.to_string())?;
    let art = file.with_extension(if matches!(leg, Leg::Wasm) { "wasm" } else { "bin" });
    if art.is_file() {
        return Ok(art);
    }
    let mut c = Command::new(&ctx.almide);
    c.arg("build").arg(&file).arg("-o").arg(&art);
    if matches!(leg, Leg::Wasm) {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("build failed: {}", first_error(&out.stderr)));
    }
    Ok(art)
}

/// One run of a timing program may take this long; past it the run is
/// killed and reported as a timeout, never as a time.
const RUN_CAP: Duration = Duration::from_secs(60);

fn run_artifact(ctx: &Ctx, art: &Path, leg: Leg) -> Result<String, String> {
    let mut cmd = match leg {
        Leg::Native => Command::new(art),
        Leg::Wasm => {
            let mut c = Command::new(&ctx.wasmtime);
            c.arg("run").arg(art);
            c
        }
    };
    let child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let (out, _) = wait_capped(child, RUN_CAP)?;
    if !out.status.success() {
        return Err(run_failure(&out));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Wait for `child` under `cap`, draining its stdout and stderr WHILE it
/// runs. Reading them only after exit deadlocked any probe that wrote more
/// than a pipe buffer holds (64 KiB on Linux): the child blocked on the full
/// pipe, and the cap reported the block as a timeout (`io.write` /
/// `io.write_bytes`, every call shape). Returns the output and the wall time.
fn wait_capped(mut child: std::process::Child, cap: Duration) -> Result<(std::process::Output, Duration), String> {
    use std::io::Read;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out_t = drain(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err_t = drain(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() > cap {
            let _ = child.kill();
            let _ = child.wait();
            let _ = (out_t.join(), err_t.join());
            return Err(format!("timed out ({}s cap)", cap.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let wall = start.elapsed();
    let stdout = out_t.join().map_err(|_| "stdout reader panicked".to_string())?;
    let stderr = err_t.join().map_err(|_| "stderr reader panicked".to_string())?;
    Ok((std::process::Output { status, stdout, stderr }, wall))
}

/// Why a probe run failed: its exit and its first error line — or, when the
/// program stopped because the probed call returned `err` ([`ERR_PATH`]),
/// that it took the error path (#3666), or, when a call given an in-domain
/// input returned `none` / `false` ([`OFF_DOMAIN`]), that it refused it
/// (#3671). Every verdict reads a failed run through this, so such a run is
/// never timed, counted or matched.
fn run_failure(out: &std::process::Output) -> String {
    format!("exit {:?}: {}", out.status.code(), failure_reason(&out.stderr))
}

fn failure_reason(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    // The wasm allocator's exit report (`__ALMD_WASM_ALLOC ...`) can follow
    // the panic message on the same line.
    let reason = |rest: &str| rest.split("__ALMD_").next().unwrap_or("").trim_start_matches(':').trim().to_string();
    let marked = |marker: &str| text.lines().find_map(|l| l.split_once(marker).map(|(_, rest)| reason(rest).chars().take(160).collect::<String>()));
    if let Some(why) = marked(ERR_PATH) {
        format!("took the error path (the probed call returned err: {why})")
    } else if let Some(what) = marked(OFF_DOMAIN) {
        format!("refused its in-domain input (the probed call returned {what})")
    } else {
        first_error(stderr)
    }
}

/// The first `error` line of a failed run's stderr, else its first line: a
/// warning printed ahead of the error (a deprecated call's E052) must not
/// stand in for the reason.
fn first_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines.clone().next().unwrap_or("");
    lines.find(|l| l.contains("error")).unwrap_or(first).chars().take(200).collect()
}

fn hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Live wasm blocks at exit, on the embedded host, for the plain call and
/// every rebind shape at size n.
fn alloc(ctx: &Ctx, t: &Target, n: u64) -> String {
    let mut progs = vec![("call", t.direct_program(n, 3, true))];
    // A rebind that multiplies the value cannot take 50 steps: it runs out
    // of memory, and the count read off a trapped run is not a leak.
    if t.rebind_param.is_some() && grows_per_step(ctx, t).is_none() {
        progs.extend(t.rebind_programs(n, 50));
    }
    let mut parts = Vec::new();
    let mut ok = true;
    for (label, src) in progs {
        let file = ctx.dir.join(format!("a{:016x}.almd", hash(&src)));
        if std::fs::write(&file, &src).is_err() {
            return "SKIP: scratch write failed".into();
        }
        let out = Command::new(&ctx.almide).arg("run").arg(&file).args(["--target", "wasm"]).env("ALMIDE_WASM_ALLOC_COUNT", "1").output();
        let Ok(out) = out else { return "SKIP: could not run almide".into() };
        let (pass, part) = alloc_part(label, &out);
        ok &= pass;
        parts.push(part);
    }
    format!("{}: {}", if ok { "PASS" } else { "FAIL" }, parts.join(", "))
}

/// One alloc probe's verdict from its `almide run` output: `live=0` passes.
/// The counters are printed at exit whatever the exit, so a run that failed
/// reports its exit: blocks an abort left behind are not a leak (#3636).
fn alloc_part(label: &str, out: &std::process::Output) -> (bool, String) {
    if !out.status.success() {
        return (false, format!("{label} exit {:?} ({})", out.status.code(), failure_reason(&out.stderr)));
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let live = err.lines().find_map(|l| l.split_whitespace().find_map(|w| w.strip_prefix("live=")).map(String::from));
    match live.as_deref() {
        Some("0") => (true, format!("{label} live=0")),
        Some(v) => (false, format!("{label} live={v}")),
        None => (false, format!("{label} no count ({})", first_error(&out.stderr))),
    }
}

/// Criterion 5: the three lines (direct, via left, via right) agree, and the
/// whole output agrees across legs.
fn diamond(ctx: &Ctx, t: &Target, n: u64) -> String {
    if !t.ret_is_observable() {
        return "SKIP: the return type is not printable".into();
    }
    let project = t.diamond_project(n.min(64));
    let root = ctx.dir.join("diamond");
    let Ok(entry) = project.write_to(&root) else { return "SKIP: scratch write failed".into() };
    let run = |wasm: bool| {
        let mut c = Command::new(&ctx.almide);
        c.arg("run").arg(&entry).current_dir(root.join("app"));
        if wasm {
            c.args(["--target", "wasm"]);
        }
        c.output()
    };
    let (Ok(nat), Ok(was)) = (run(false), run(true)) else { return "SKIP: could not run almide".into() };
    diamond_verdict(t.diamond_prints(), &nat, &was)
}

/// Criterion 5's verdict from the diamond project's run on each leg.
/// `prints` false: a Unit return and no `mut` parameter leave nothing to
/// compare (#3636) — reported as a SKIP once both legs exited 0, never a PASS.
fn diamond_verdict(prints: bool, nat: &std::process::Output, was: &std::process::Output) -> String {
    if !nat.status.success() || !was.status.success() {
        return format!("FAIL: native exit {:?} ({}), wasm exit {:?} ({})", nat.status.code(), failure_reason(&nat.stderr), was.status.code(), failure_reason(&was.stderr));
    }
    if !prints {
        return "SKIP: nothing to compare — a Unit return and no mut parameter (the three calls ran to exit 0 on both legs)".into();
    }
    let (a, b) = (String::from_utf8_lossy(&nat.stdout).into_owned(), String::from_utf8_lossy(&was.stdout).into_owned());
    // `render_print` writes `<name> = <value>`: compare the values.
    let lines: Vec<&str> = a.lines().map(|l| l.split_once(" = ").map(|(_, v)| v).unwrap_or(l)).collect();
    let thirds = lines.len() / 3;
    let same_three = thirds > 0 && lines.len() % 3 == 0 && lines[..thirds] == lines[thirds..2 * thirds] && lines[..thirds] == lines[2 * thirds..];
    match (same_three, a == b) {
        (true, true) => format!("PASS: direct = via left = via right on both legs ({} line(s) each)", thirds),
        (false, _) => format!("FAIL: the three calls differ on native: {a:?}"),
        (true, false) => format!("FAIL: native {a:?} vs wasm {b:?}"),
    }
}

// ── The counted judge ───────────────────────────────────────────────────
//
// Wall time on a shared VM crosses cache tiers between sizes: a linear
// `list.drop` read x3.1 from 200k to 400k Ints (L2) and x3.5 from 2M to 4M
// (L3) while every other doubling read x2.0. Retired instructions see no
// tier and repeat to the instruction (`/bin/true` 135,023-135,024 over three
// runs on c4-standard-4). This is rustc-perf's primary metric
// (`instructions:u`); the class is chosen the way Google Benchmark's `oAuto`
// chooses one, by least squares over the candidate curves, here with an
// intercept for the program-shape constant the baseline does not cancel.

/// The order classes, cheapest first, and their curves.
const CLASSES: &[(&str, fn(f64) -> f64)] = &[
    ("1", |_| 1.0),
    ("log", |n| n.ln()),
    ("n", |n| n),
    ("nlogn", |n| n * n.ln()),
    ("n2", |n| n * n),
    ("n3", |n| n * n * n),
];

/// Relative residual a class must stay within to explain the counts. Counts
/// repeat to ~0.05%, so 3% is far above measurement and far below the 20%+
/// a wrong class leaves over a 16x size range.
const FIT_TOLERANCE: f64 = 0.03;

/// Calibrate `k` until the call's own instructions at size `n` dominate,
/// then count the MARGINAL cost of k more calls — the program at 2k calls
/// minus the same program at k — at n, 2n, 4n, 8n and 16n on both legs.
///
/// Not (program at k) - (baseline at 0): that difference also holds every
/// one-off cost the call's presence changes — a final copy, a teardown walk,
/// the first call's reallocation into a fresh heap — and an O(n) one-off
/// grows with n like an O(n) call would. Measured on reuse-attr (c4-standard-4):
/// three rebind shapes read `log` from a one-off whose step doubled with n,
/// and a list builder's moved bytes left a linear fit by 9-28% at the
/// smallest size only. Between k and 2k calls every one-off cancels and the
/// series is the steady per-call cost (times k).
fn judge_counted(
    ctx: &Ctx,
    label: &str,
    order: Option<&str>,
    n: u64,
    programs: &dyn Fn(u64, u64) -> (String, String),
) -> Result<String, String> {
    let marginal = |m: u64, k: u64| (programs(m, 2 * k).0, programs(m, k).0);
    let mut k = 1u64;
    loop {
        let (p, b) = marginal(n, k);
        let dn = counted_delta(ctx, &p, &b, Leg::Native).map_err(|e| format!("{label} {e}"))?;
        let dw = counted_delta(ctx, &p, &b, Leg::Wasm).map_err(|e| format!("{label} {e}"))?;
        if counted_calibrated(&dn, &dw) || k >= MAX_K {
            break;
        }
        k = (k * 4).min(MAX_K);
    }
    let sizes: Vec<u64> = (0..5).map(|i| n << i).collect();
    let mut line = format!("{label} k={k}..{} meter=instructions:u+bytes-moved n={n}..{}", 2 * k, n << 4);
    let mut worst = 0usize;
    let mut unexplained = false;
    for leg in [Leg::Native, Leg::Wasm] {
        let mut ys = Vec::new();
        let mut bs = Vec::new();
        let mut ts = Vec::new();
        for &m in &sizes {
            let (p, b) = marginal(m, k);
            let d = counted_delta(ctx, &p, &b, leg).map_err(|e| format!("{label} {e}"))?;
            ys.push(d.instructions);
            bs.push(d.moved);
            ts.push(d.wall);
        }
        let (class, rms) = fit_series(&sizes, &ys, k);
        let (bclass, brms) = fit_series(&sizes, &bs, k);
        worst = worst.max(class).max(bclass);
        unexplained |= rms > FIT_TOLERANCE || brms > FIT_TOLERANCE;
        let tag = if matches!(leg, Leg::Native) { "native" } else { "wasm" };
        let sci = |v: &[f64]| v.iter().map(|y| format!("{:.3e}", y)).collect::<Vec<_>>().join("/");
        let times: Vec<String> = ts.iter().map(|t| format!("{:.3}", t.as_secs_f64())).collect();
        line.push_str(&format!(
            " {tag} {} fit {} (rms {:.1}%) moved {}B fit {} (rms {:.1}%) wall {}s",
            sci(&ys),
            CLASSES[class].0,
            rms * 100.0,
            sci(&bs),
            CLASSES[bclass].0,
            brms * 100.0,
            times.join("/")
        ));
    }
    // A series no class explains within tolerance has no order to compare:
    // the best-fitting class of such a series can be anything, O(1)
    // included, so it never passes.
    if unexplained {
        return Err(format!("{line} (unexplained: a series fits no class within {:.0}%)", FIT_TOLERANCE * 100.0));
    }
    if ctx.memshim.is_none() {
        return Err(format!("{line} [meter=instructions only, no bytes-moved meter: not stampable]"));
    }
    let measured = CLASSES[worst].0;
    match order.and_then(|o| CLASSES.iter().position(|(c, _)| *c == o)) {
        None => Err(format!("{line} (order undeclared: measured {measured})")),
        Some(declared) if worst <= declared => Ok(format!("{line} (order {} ok)", CLASSES[declared].0)),
        Some(declared) => Err(format!("{line} (order {} exceeded: measured {measured})", CLASSES[declared].0)),
    }
}

/// Whether the counted judge's k is large enough: either meter, on either
/// leg, past its target. Instructions alone undercount a call whose cost is
/// one large copy — `rep movsb` retires as one instruction whatever the
/// length (see `MEMSHIM_C`) — so an instructions-only target drove k up by
/// 4x until each run copied gigabytes at 16n and hit the 60 s cap
/// (set.insert / set.union's call shape on GCE, 2026-10-14). 2e8 bytes is
/// about 20 ms of copying on c4-standard-4, the same order as 2e7
/// instructions.
fn counted_calibrated(native: &Delta, wasm: &Delta) -> bool {
    const CALIBRATE_INSTRUCTIONS: f64 = 2.0e7;
    const CALIBRATE_BYTES: f64 = 2.0e8;
    native.instructions.max(wasm.instructions) >= CALIBRATE_INSTRUCTIONS || native.moved.max(wasm.moved) >= CALIBRATE_BYTES
}

/// The cheapest class whose `a + c·f(n)` least-squares fit leaves every
/// point within [`FIT_TOLERANCE`] (relative RMS), else the best-fitting one
/// with its residual above the tolerance (the caller refuses to pass it).
/// A series that stays under [`COUNT_FLOOR_DIVISOR`]'s per-call floor at
/// every size is O(1) outright: it cannot hold a linear term, and what is
/// left is the run's own jitter, which a relative fit reads as no class at
/// all. Measured:
/// - `int.to_int16_saturating` at k = 1M read 7.8e5 / 147 / 579 / 82 / 48
///   instructions, which a relative fit called 1789% off every class.
/// - wasm `string.drop`'s loop at k = 16 read 177 / 159 / 127 / 243 / 335
///   per call (rms 17.5%).
/// - native `string.drop_end`'s loop at k = 64 read 566 / 625 / 607 / 585 /
///   632 per call (rms 3.5%).
fn fit_series(sizes: &[u64], ys: &[f64], k: u64) -> (usize, f64) {
    let floor = sizes[0] as f64 / COUNT_FLOOR_DIVISOR * k as f64;
    if ys.iter().all(|&y| y < floor) {
        return (0, 0.0);
    }
    fit_class(sizes, ys)
}

/// The per-call floor is `n / 16` at the smallest size `n`, held at every
/// size. Work linear in the input touches every element; even at 64 bytes
/// per instruction (one AVX-512 load), m elements cost at least m / 64 per
/// call. Across the judge's 16x range, a linear term therefore adds at least
/// (16n - n) / 64 ≈ 0.23n per call between the first and last point. A
/// series that stays below n / 16 ≈ 0.06n at every point has no room for
/// that rise, so the floor sits about 3.75x under the smallest linear term
/// it could hide. The same floor applies to bytes moved, where a linear copy
/// moves at least m bytes per call. Owner ruling 2026-10-12: the counted
/// twin of the timed judge's [`NOISE_FLOOR`].
const COUNT_FLOOR_DIVISOR: f64 = 16.0;

fn fit_class(sizes: &[u64], ys: &[f64]) -> (usize, f64) {
    let top = ys.iter().cloned().fold(0.0f64, f64::max);
    if top < 5.0e3 {
        return (0, 0.0);
    }
    let floor = top * 0.01;
    let mut best = (0usize, f64::MAX);
    for (i, (_, f)) in CLASSES.iter().enumerate() {
        let xs: Vec<f64> = sizes.iter().map(|&m| f(m as f64)).collect();
        let rms = fit_rms(&xs, ys, floor);
        if rms <= FIT_TOLERANCE {
            return (i, rms);
        }
        if rms < best.1 {
            best = (i, rms);
        }
    }
    best
}

/// Relative RMS residual of the least-squares line `y = a + c·x`.
fn fit_rms(xs: &[f64], ys: &[f64], floor: f64) -> f64 {
    let n = xs.len() as f64;
    let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
    let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    let sxy: f64 = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    let c = if sxx > 0.0 { sxy / sxx } else { 0.0 };
    let a = my - c * mx;
    let sq: f64 = xs.iter().zip(ys).map(|(x, y)| ((y - (a + c * x)) / y.abs().max(floor)).powi(2)).sum();
    (sq / n).sqrt()
}

/// One leg's (program - baseline) difference under both meters, and wall
/// time for the record.
struct Delta {
    instructions: f64,
    moved: f64,
    wall: Duration,
}

fn counted_delta(ctx: &Ctx, prog: &str, base: &str, leg: Leg) -> Result<Delta, String> {
    let p = counted(ctx, prog, leg)?;
    let b = counted(ctx, base, leg)?;
    Ok(Delta {
        instructions: (p.instructions as f64 - b.instructions as f64).max(0.0),
        moved: (p.moved as f64 - b.moved as f64).max(0.0),
        wall: p.wall.saturating_sub(b.wall),
    })
}

/// One run's meters.
struct Counts {
    instructions: u64,
    moved: u64,
    wall: Duration,
}

/// The smaller counts of two runs (instructions repeat to ~0.05%; the
/// minimum drops a rare interrupt-path outlier), with the same output checks
/// the timed judge makes: deterministic, and equal across legs.
fn counted(ctx: &Ctx, src: &str, leg: Leg) -> Result<Counts, String> {
    let art = build(ctx, src, leg)?;
    let mut best = Counts { instructions: u64::MAX, moved: u64::MAX, wall: Duration::MAX };
    let mut first: Option<String> = None;
    for _ in 0..2 {
        let (out, c) = run_counted(ctx, &art, leg)?;
        best = Counts {
            instructions: best.instructions.min(c.instructions),
            moved: best.moved.min(c.moved),
            wall: best.wall.min(c.wall),
        };
        if first.get_or_insert_with(|| out.clone()) != &out {
            return Err("nondeterministic output".into());
        }
    }
    if matches!(leg, Leg::Wasm) {
        let native = run_artifact(ctx, &build(ctx, src, Leg::Native)?, Leg::Native)?;
        if first.as_deref() != Some(native.as_str()) {
            return Err(format!("native and wasm differ: {native:?} vs {first:?}"));
        }
    }
    Ok(best)
}

/// The second meter. glibc copies and fills large blocks with `rep movsb` /
/// `rep stosb`, which retire as ONE instruction whatever the length, and
/// wasm's `memory.copy` / `memory.fill` reach the same glibc routines through
/// wasmtime: on c4-standard-4, 20 `memory.copy`s of 1 MB and of 16 MB both
/// counted 6.27M instructions, so a quadratic built from copies would count
/// as constant. (Raising glibc's rep thresholds did not help: +0.57M per
/// 300 MB.) This shim counts the bytes each memcpy / memmove / memset is
/// asked to move — the same probe read 21.6M / 84.5M / 336M bytes for 1 / 4
/// / 16 MB — and the judge fits that series as well.
const MEMSHIM_C: &str = include_str!("memshim.c");

fn build_memshim(dir: &Path) -> Option<PathBuf> {
    let src = dir.join("memshim.c");
    let so = dir.join("memshim.so");
    std::fs::write(&src, MEMSHIM_C).ok()?;
    let ok = Command::new("cc")
        .args(["-O2", "-shared", "-fPIC", "-o"])
        .arg(&so)
        .arg(&src)
        .arg("-ldl")
        .output()
        .is_ok_and(|o| o.status.success());
    ok.then_some(so)
}

/// One run under `perf stat -x, -e instructions:u`: stdout and the counts.
/// The count goes to a file so the program's stderr stays its own. The shim
/// is preloaded through `env`, which execs the program, so perf's own copies
/// are not counted and only the program's exit writes the byte total.
fn run_counted(ctx: &Ctx, art: &Path, leg: Leg) -> Result<(String, Counts), String> {
    let stat = ctx.dir.join(format!("stat-{}.csv", std::process::id()));
    let moved_file = ctx.dir.join(format!("moved-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&moved_file);
    let mut cmd = Command::new("perf");
    cmd.args(["stat", "-x", ",", "-e", "instructions:u", "-o"]).arg(&stat).arg("--");
    if let Some(shim) = &ctx.memshim {
        cmd.arg("env")
            .arg(format!("LD_PRELOAD={}", shim.display()))
            .arg(format!("MEMSHIM_OUT={}", moved_file.display()));
    }
    match leg {
        Leg::Native => cmd.arg(art),
        // Compile on every run, single-threaded: wasmtime's compile cache
        // makes a module's first run count ~10x its later ones (c4-standard-4:
        // 41.7M against 4.46M), and parallel compilation adds worker-thread
        // spinning. A fresh compile is the same work at every n, so the fit's
        // intercept absorbs it.
        Leg::Wasm => cmd.arg(&ctx.wasmtime).args(["run", "-C", "cache=n", "-C", "parallel-compilation=n"]).arg(art),
    };
    let child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let (out, wall) = wait_capped(child, RUN_CAP)?;
    if !out.status.success() {
        return Err(run_failure(&out));
    }
    let instructions = parse_instructions(&std::fs::read_to_string(&stat).unwrap_or_default())
        .ok_or_else(|| "perf reported no instructions:u count".to_string())?;
    let moved = match &ctx.memshim {
        None => 0,
        Some(_) => std::fs::read_to_string(&moved_file)
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .ok_or_else(|| "the bytes-moved shim wrote no count (is the binary dynamically linked?)".to_string())?,
    };
    Ok((String::from_utf8_lossy(&out.stdout).into_owned(), Counts { instructions, moved, wall }))
}

/// The count of the `instructions:u` row of `perf stat -x,` output.
fn parse_instructions(csv: &str) -> Option<u64> {
    csv.lines()
        .find(|l| l.contains("instructions"))
        .and_then(|l| l.split(',').next())
        .and_then(|c| c.trim().parse().ok())
}

/// Whether this host counts user-space instructions (a PMU is exposed and
/// `perf_event_paranoid` lets an unprivileged process read it).
fn icount_available() -> bool {
    let dir = std::env::temp_dir().join(format!("xtarget-icount-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let stat = dir.join("probe.csv");
    let ok = Command::new("perf")
        .args(["stat", "-x", ",", "-e", "instructions:u", "-o"])
        .arg(&stat)
        .args(["--", "true"])
        .output()
        .is_ok_and(|o| o.status.success())
        && parse_instructions(&std::fs::read_to_string(&stat).unwrap_or_default()).is_some_and(|c| c > 0);
    let _ = std::fs::remove_dir_all(&dir);
    ok
}

#[cfg(test)]
mod run_tests {
    use super::*;

    /// A probe that writes more than a pipe buffer holds (64 KiB on Linux)
    /// must finish, not block on a full pipe until the cap reads it as a
    /// timeout: `io.write` / `io.write_bytes` were "timed out (60s cap)" on
    /// every call shape this way.
    #[test]
    fn a_run_that_writes_more_than_a_pipe_holds_finishes() {
        let child = Command::new("sh")
            .args(["-c", "head -c 300000 /dev/zero; head -c 200000 /dev/zero >&2"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("sh");
        let (out, _) = wait_capped(child, Duration::from_secs(20)).expect("finishes under the cap");
        assert!(out.status.success());
        assert_eq!(out.stdout.len(), 300_000);
        assert_eq!(out.stderr.len(), 200_000);
    }

    #[test]
    fn a_run_past_the_cap_is_a_timeout() {
        let child = Command::new("sh")
            .args(["-c", "sleep 5"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("sh");
        let e = wait_capped(child, Duration::from_millis(200)).expect_err("killed at the cap");
        assert!(e.contains("timed out"), "{e}");
    }

    /// The counted judge's k stops growing on EITHER meter: a call whose
    /// cost is one large copy retires a handful of instructions (`rep
    /// movsb`), so an instructions-only target drove k up by 4x until each
    /// run copied gigabytes and hit the 60 s cap (set.insert / set.union's
    /// call shape on GCE).
    #[test]
    fn calibration_stops_on_bytes_moved_as_well_as_instructions() {
        let d = |instructions: f64, moved: f64| Delta { instructions, moved, wall: Duration::ZERO };
        assert!(!counted_calibrated(&d(1e3, 1e3), &d(1e3, 1e3)));
        assert!(counted_calibrated(&d(3e7, 0.0), &d(1e3, 0.0)));
        assert!(counted_calibrated(&d(1e3, 3e8), &d(1e3, 0.0)));
        assert!(counted_calibrated(&d(1e3, 0.0), &d(1e3, 3e8)));
    }
}

#[cfg(test)]
mod verdict_tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn output(code: i32, stdout: &str, stderr: &str) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    /// #3636: a probe that exited non-zero is a failed run, not a leak count:
    /// the allocator's exit report is printed whatever the exit, and a
    /// `live=` read off an aborted run counts blocks the abort left behind.
    #[test]
    fn alloc_reports_the_exit_of_a_failed_run_not_its_live_count() {
        let (ok, part) = alloc_part("call", &output(1, "", "error: matrix shape mismatch\nalloc: live=3 peak=9\n"));
        assert!(!ok);
        assert!(!part.contains("live="), "{part}");
        assert!(part.contains("exit Some(1)") && part.contains("matrix shape mismatch"), "{part}");
        assert_eq!(alloc_part("call", &output(0, "1\n", "alloc: live=0 peak=9\n")), (true, "call live=0".to_string()));
        assert_eq!(alloc_part("loop", &output(0, "1\n", "alloc: live=2 peak=9\n")), (false, "loop live=2".to_string()));
    }

    /// #3666: a probe that stopped because the probed call returned `err`
    /// is reported as the error path it took — in the timed / counted runs,
    /// the alloc probe and the diamond alike — never as a count or a match.
    #[test]
    fn a_run_that_took_the_error_path_is_reported_as_such() {
        let stderr = format!("PANIC: {ERR_PATH}: invalid base64 character\nalloc: live=0 peak=9\n");
        let want = "took the error path (the probed call returned err: invalid base64 character)";
        assert_eq!(run_failure(&output(1, "", &stderr)), format!("exit Some(1): {want}"));
        let (ok, part) = alloc_part("call", &output(1, "", &stderr));
        assert!(!ok && part.contains(want) && !part.contains("live="), "{part}");
        // On wasm the allocator's exit report follows the panic message on
        // the same line (measured: `...string__ALMD_WASM_ALLOC allocs=5 ...
        // live=3`); it is not part of the reason.
        let wasm = format!("PANIC: {ERR_PATH}: invalid base64 character__ALMD_WASM_ALLOC allocs=5 reused=0 live=3\n");
        let (ok, part) = alloc_part("call", &output(1, "", &wasm));
        assert!(!ok && part.ends_with(&format!("({want})")) && !part.contains("live="), "{part}");
        let v = diamond_verdict(true, &output(1, "", &stderr), &output(1, "", &stderr));
        assert!(v.starts_with("FAIL: ") && v.contains(want), "{v}");
        // Any other failure keeps its own first error line.
        assert_eq!(run_failure(&output(2, "", "warning: x\nerror: boom\n")), "exit Some(2): error: boom");
    }

    /// #3671: a probe that stopped because a call given an in-domain input
    /// returned `none` / `false` is reported as the refusal it measured, in
    /// every verdict, never as a count or a match.
    #[test]
    fn a_run_that_refused_its_in_domain_input_is_reported_as_such() {
        let stderr = format!("PANIC: {OFF_DOMAIN}: none\nalloc: live=0 peak=9\n");
        let want = "refused its in-domain input (the probed call returned none)";
        assert_eq!(run_failure(&output(1, "", &stderr)), format!("exit Some(1): {want}"));
        let wasm = format!("PANIC: {OFF_DOMAIN}: false__ALMD_WASM_ALLOC allocs=5 reused=0 live=3\n");
        let (ok, part) = alloc_part("call", &output(1, "", &wasm));
        assert!(!ok && part.ends_with("(refused its in-domain input (the probed call returned false))") && !part.contains("live="), "{part}");
        let v = diamond_verdict(true, &output(1, "", &stderr), &output(1, "", &stderr));
        assert!(v.starts_with("FAIL: ") && v.contains(want), "{v}");
    }

    /// `--emit` writes every program a verdict runs: the plain call, each
    /// rebind shape, and the diamond project.
    #[test]
    fn emit_writes_every_probe_program() {
        let t = crate::generator::inspect_gen::tests::target("result.map");
        let files = probe_files(&t, 100);
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        for want in ["call.almd", "loop.almd", "tail.almd", "fold.almd", "diamond/app/almide.toml", "diamond/app/src/main.almd"] {
            assert!(paths.contains(&want), "{want}: {paths:?}");
        }
    }

    /// #3636: a Unit function with no `mut` parameter (bytes.heap_restore)
    /// prints nothing in the diamond, and 0 lines is not "the three calls
    /// differ". It is a SKIP — never a PASS — once both legs ran to exit 0;
    /// a failed leg is still a FAIL.
    #[test]
    fn a_diamond_with_nothing_to_print_is_a_skip_not_a_mismatch() {
        let t = crate::generator::inspect_gen::tests::target("bytes.heap_restore");
        assert!(!t.diamond_prints());
        let v = diamond_verdict(t.diamond_prints(), &output(0, "", ""), &output(0, "", ""));
        assert!(v.starts_with("SKIP: "), "{v}");
        let v = diamond_verdict(t.diamond_prints(), &output(1, "", "error: boom"), &output(0, "", ""));
        assert!(v.starts_with("FAIL: "), "{v}");
        // A function that prints and printed nothing still fails.
        let v = diamond_verdict(true, &output(0, "", ""), &output(0, "", ""));
        assert!(v.starts_with("FAIL: "), "{v}");
    }
}

#[cfg(test)]
mod fit_tests {
    use super::*;

    fn sizes() -> Vec<u64> {
        (0..5).map(|i| 20_000u64 << i).collect()
    }

    fn class_of(f: impl Fn(f64) -> f64) -> &'static str {
        let ys: Vec<f64> = sizes().iter().map(|&n| 1.0e6 + f(n as f64)).collect();
        CLASSES[fit_class(&sizes(), &ys).0].0
    }

    #[test]
    fn each_curve_is_classified_as_itself_or_cheaper_never_dearer() {
        assert_eq!(class_of(|_| 3.0e7), "1");
        assert_eq!(class_of(|n| 40.0 * n), "n");
        assert_eq!(class_of(|n| 0.05 * n * n), "n2");
        assert_eq!(class_of(|n| 1.0e-6 * n * n * n), "n3");
        // n log n over a 16x range is within tolerance of a line with an
        // intercept: it reads as n or nlogn, never as n2.
        assert!(matches!(class_of(|n| 40.0 * n * n.ln()), "n" | "nlogn"));
    }

    #[test]
    fn a_single_cache_step_does_not_read_as_a_higher_order() {
        // The wall-time failure this judge replaces: linear, with one x1.5
        // jump between two sizes. Counted instructions never show it, but
        // the fit must not over-read a modest bump either.
        let ys: Vec<f64> = sizes().iter().enumerate()
            .map(|(i, &n)| 40.0 * n as f64 * if i >= 2 { 1.05 } else { 1.0 })
            .collect();
        assert_eq!(CLASSES[fit_class(&sizes(), &ys).0].0, "n");
    }

    #[test]
    fn a_series_no_class_explains_reports_its_residual_above_tolerance() {
        // Measured on wasm list.drop: a (program - baseline) difference that
        // shrinks to zero as n grows. No class explains it, and the judge
        // must see that rather than a best-fit class.
        let ys = [2.617e5, 2.409e5, 2.015e5, 1.213e5, 0.0];
        assert!(fit_class(&sizes(), &ys).1 > FIT_TOLERANCE);
    }

    #[test]
    fn under_the_per_call_floor_is_constant() {
        let ys = [7.811e5, 1.47e2, 5.79e2, 8.2e1, 4.8e1];
        assert_eq!(fit_series(&sizes(), &ys, 1 << 20), (0, 0.0));
        // 763 per call at the first size, ~0 after: under n / 16 = 1250
        // everywhere, so constant.
        assert_eq!(fit_series(&sizes(), &ys, 1024), (0, 0.0));
        // The same shape at 64 calls is 12,205 per call at the first size:
        // above the floor, so it is fitted, and no class explains it.
        assert!(fit_series(&sizes(), &ys, 64).1 > FIT_TOLERANCE);
        // The tiny flat series the floor exists for: wasm string.drop's loop
        // (k = 16) and native string.drop_end's loop (k = 64).
        let drop = [2.826e3, 2.550e3, 2.036e3, 3.894e3, 5.355e3];
        assert_eq!(fit_series(&sizes(), &drop, 16), (0, 0.0));
        let drop_end = [3.622e4, 4.002e4, 3.883e4, 3.743e4, 4.045e4];
        assert_eq!(fit_series(&sizes(), &drop_end, 64), (0, 0.0));
    }

    #[test]
    fn the_floor_never_hides_a_linear_series() {
        // The cheapest linear series physically possible: m / 64 per call
        // (64 bytes per instruction), plus a constant. It crosses the floor
        // and is fitted as n.
        for k in [1u64, 64, 1 << 20] {
            let ys: Vec<f64> = sizes().iter().map(|&m| (m as f64 / 64.0 + 100.0) * k as f64).collect();
            assert_ne!(fit_series(&sizes(), &ys, k), (0, 0.0), "k = {k}");
            assert_eq!(CLASSES[fit_series(&sizes(), &ys, k).0].0, "n", "k = {k}");
        }
    }

    #[test]
    fn perf_csv_is_parsed() {
        assert_eq!(parse_instructions("135023,,instructions:u,362206,100.00,,\n"), Some(135023));
        assert_eq!(parse_instructions("<not counted>,,instructions:u,0,0.00,,\n"), None);
    }
}
