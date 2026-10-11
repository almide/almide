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

use crate::generator::{inspect_gen::Target, Engine, Family};

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
}

#[derive(Clone, Copy)]
enum Leg {
    Native,
    Wasm,
}

pub fn cmd_inspect(args: &[String]) {
    let Some(name) = args.iter().find(|a| !a.starts_with("--") && a.contains('.')).cloned() else {
        eprintln!("usage: xtarget-fuzz inspect <module.fn> [--order 1|log|n|nlogn|n2] [--order-rebind ..] [--n N] [--count C] [--seed S]");
        std::process::exit(2);
    };
    let repo = super::resolve_repo(args);
    let ctx = Ctx {
        almide: super::resolve_almide(&repo, args),
        wasmtime: super::resolve_wasmtime(),
        dir: std::env::temp_dir().join(format!("xtarget-inspect-{}", std::process::id())),
        icount: icount_available(),
    };
    let _ = std::fs::create_dir_all(&ctx.dir);
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
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    while child.try_wait().map_err(|e| e.to_string())?.is_none() {
        if start.elapsed() > RUN_CAP {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("timed out ({}s cap)", RUN_CAP.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("exit {:?}: {}", out.status.code(), first_error(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
        let err = String::from_utf8_lossy(&out.stderr);
        let live = err.lines().find_map(|l| l.split_whitespace().find_map(|w| w.strip_prefix("live=")).map(String::from));
        match live.as_deref() {
            Some("0") => parts.push(format!("{label} live=0")),
            Some(v) => {
                ok = false;
                parts.push(format!("{label} live={v}"))
            }
            None => {
                ok = false;
                parts.push(format!("{label} no count ({})", first_error(&out.stderr)))
            }
        }
    }
    format!("{}: {}", if ok { "PASS" } else { "FAIL" }, parts.join(", "))
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
    if !nat.status.success() || !was.status.success() {
        return format!("FAIL: native exit {:?} ({}), wasm exit {:?} ({})", nat.status.code(), first_error(&nat.stderr), was.status.code(), first_error(&was.stderr));
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
/// then count (program - baseline) at n, 2n, 4n, 8n and 16n on both legs.
fn judge_counted(
    ctx: &Ctx,
    label: &str,
    order: Option<&str>,
    n: u64,
    programs: &dyn Fn(u64, u64) -> (String, String),
) -> Result<String, String> {
    const CALIBRATE_INSTRUCTIONS: f64 = 2.0e7;
    let mut k = 1u64;
    loop {
        let (p, b) = programs(n, k);
        let dn = counted_delta(ctx, &p, &b, Leg::Native).map_err(|e| format!("{label} {e}"))?;
        let dw = counted_delta(ctx, &p, &b, Leg::Wasm).map_err(|e| format!("{label} {e}"))?;
        if dn.0.max(dw.0) >= CALIBRATE_INSTRUCTIONS || k >= MAX_K {
            break;
        }
        k = (k * 4).min(MAX_K);
    }
    let sizes: Vec<u64> = (0..5).map(|i| n << i).collect();
    let mut line = format!("{label} k={k} meter=instructions:u n={n}..{}", n << 4);
    let mut worst = 0usize;
    for leg in [Leg::Native, Leg::Wasm] {
        let mut ys = Vec::new();
        let mut ts = Vec::new();
        for &m in &sizes {
            let (p, b) = programs(m, k);
            let (y, t) = counted_delta(ctx, &p, &b, leg).map_err(|e| format!("{label} {e}"))?;
            ys.push(y);
            ts.push(t);
        }
        let (class, rms) = fit_class(&sizes, &ys);
        worst = worst.max(class);
        let tag = if matches!(leg, Leg::Native) { "native" } else { "wasm" };
        let counts: Vec<String> = ys.iter().map(|y| format!("{:.3e}", y)).collect();
        let times: Vec<String> = ts.iter().map(|t| format!("{:.3}", t.as_secs_f64())).collect();
        line.push_str(&format!(
            " {tag} {} fit {} (rms {:.1}%) wall {}s",
            counts.join("/"),
            CLASSES[class].0,
            rms * 100.0,
            times.join("/")
        ));
    }
    let measured = CLASSES[worst].0;
    match order.and_then(|o| CLASSES.iter().position(|(c, _)| *c == o)) {
        None => Err(format!("{line} (order undeclared: measured {measured})")),
        Some(declared) if worst <= declared => Ok(format!("{line} (order {} ok)", CLASSES[declared].0)),
        Some(declared) => Err(format!("{line} (order {} exceeded: measured {measured})", CLASSES[declared].0)),
    }
}

/// The cheapest class whose `a + c·f(n)` least-squares fit leaves every
/// point within [`FIT_TOLERANCE`] (relative RMS), else the best-fitting one.
/// Counts that never leave a few thousand instructions are O(1) outright.
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

/// (program - baseline) user-space instructions on one leg, and the same
/// difference in wall time for the record.
fn counted_delta(ctx: &Ctx, prog: &str, base: &str, leg: Leg) -> Result<(f64, Duration), String> {
    let (cp, tp) = counted(ctx, prog, leg)?;
    let (cb, tb) = counted(ctx, base, leg)?;
    Ok(((cp as f64 - cb as f64).max(0.0), tp.saturating_sub(tb)))
}

/// The fewer instructions of two runs (they repeat to ~0.05%; the minimum
/// drops a rare interrupt-path outlier), with the same output checks the
/// timed judge makes: deterministic, and equal across legs.
fn counted(ctx: &Ctx, src: &str, leg: Leg) -> Result<(u64, Duration), String> {
    let art = build(ctx, src, leg)?;
    let mut best = (u64::MAX, Duration::MAX);
    let mut first: Option<String> = None;
    for _ in 0..2 {
        let (out, count, wall) = run_counted(ctx, &art, leg)?;
        best = (best.0.min(count), best.1.min(wall));
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

/// glibc copies and fills large blocks with `rep movsb` / `rep stosb`, which
/// retire as ONE instruction whatever the length: on c4-standard-4, copying
/// 20 x 16 MB counted the same 56.1M instructions as 20 x 1 MB. A quadratic
/// built from copies would count as linear. Raising the thresholds makes
/// glibc copy with vector loops, whose count grows with the bytes moved
/// (measured: +0.57M instructions per extra 300 MB, which a fit separates).
/// Both legs reach glibc's memmove for bulk moves (wasmtime's
/// `memory.copy` included), so both are counted the same way.
const GLIBC_COUNTABLE_COPIES: &str =
    "glibc.cpu.x86_rep_movsb_threshold=0xffffffffffffffff:glibc.cpu.x86_rep_stosb_threshold=0xffffffffffffffff";

/// One run under `perf stat -x, -e instructions:u`: stdout, the count, and
/// wall time. The count goes to a file so the program's stderr stays its own.
fn run_counted(ctx: &Ctx, art: &Path, leg: Leg) -> Result<(String, u64, Duration), String> {
    let stat = ctx.dir.join(format!("stat-{}.csv", std::process::id()));
    let mut cmd = Command::new("perf");
    cmd.env("GLIBC_TUNABLES", GLIBC_COUNTABLE_COPIES);
    cmd.args(["stat", "-x", ",", "-e", "instructions:u", "-o"]).arg(&stat).arg("--");
    match leg {
        Leg::Native => cmd.arg(art),
        Leg::Wasm => cmd.arg(&ctx.wasmtime).arg("run").arg(art),
    };
    let start = Instant::now();
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    while child.try_wait().map_err(|e| e.to_string())?.is_none() {
        if start.elapsed() > RUN_CAP {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("timed out ({}s cap)", RUN_CAP.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let wall = start.elapsed();
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("exit {:?}: {}", out.status.code(), first_error(&out.stderr)));
    }
    let count = parse_instructions(&std::fs::read_to_string(&stat).unwrap_or_default())
        .ok_or_else(|| "perf reported no instructions:u count".to_string())?;
    Ok((String::from_utf8_lossy(&out.stdout).into_owned(), count, wall))
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
    fn perf_csv_is_parsed() {
        assert_eq!(parse_instructions("135023,,instructions:u,362206,100.00,,\n"), Some(135023));
        assert_eq!(parse_instructions("<not counted>,,instructions:u,0,0.00,,\n"), None);
    }
}
