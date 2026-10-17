//! `xtarget-fuzz inspect http.serve`: the criteria for a function that
//! serves until it is stopped. `http.serve(port, f)` makes no value a
//! program could print, and it returns only after a SIGTERM (ADR-0020
//! §5.6), so every probe is a served program the harness drives as a
//! client (`oracle/serve.rs`): start it at a port the harness chose, send
//! a fixed request sequence, SIGTERM it, and compare the client-side
//! transcript (each response's status, header set and de-framed body, the
//! verdicts on the host-managed fields, and a socket host's exit).
//!
//!   parity      a campaign of generated handlers over the fixed request
//!               set, on native, the embedded lane and `wasmtime serve`
//!               (the stock export, C-375): 0 unique findings
//!   complexity  n = the request body's bytes, k = requests per session:
//!               the counted judge's marginal k..2k requests at n..16n,
//!               on native and the embedded lane, a constant handler
//!   alloc       the embedded lane's live blocks after serve returned:
//!               the script and k sized requests served, then the SIGTERM
//!   diamond     the same session through `http.serve` and through two
//!               wrapper paths, on native and the embedded lane
//!
//! Every criterion has its own reading here; none is skipped.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::{alloc_part_exiting, judge, sandbox, toml_str, Ctx, World};
use crate::generator::serve_gen::{self, CONSTANT_APP, PROBE_APP};
use crate::generator::Engine;
use crate::oracle::serve::{self as drv, Host, Plan, Session};

/// The functions inspected as served sessions. A function here never
/// returns until its process is signalled; each one needs the readings
/// below, so a new entry is a new design, not a row to add.
pub const SERVED: &[&str] = &["http.serve"];

/// k's ceiling: each request is a connection, and a session's server
/// holds every one it closed in TIME_WAIT on its own port.
const SERVE_MAX_K: u64 = 256;

/// The default body size: 16n stays under `http.serve`'s 1 MiB body limit
/// (a larger body is a 413 before the handler, ADR-0020 §5.7), with room.
fn default_n(counted: bool) -> u64 {
    if counted { 16_384 } else { 65_536 }
}

pub(super) fn inspect(ctx: &mut Ctx, engine: &Engine, name: &str, args: &[String]) {
    ctx.max_k = SERVE_MAX_K;
    // The embedded lane compiles on every run (see `Ctx::calibrate_scale`).
    ctx.calibrate_scale = 10.0;
    let n: u64 = super::super::flag_value(args, "--n").and_then(|s| s.parse().ok()).unwrap_or(default_n(ctx.icount));
    if 16 * n > 1 << 20 {
        eprintln!("--n {n}: 16n passes the 1 MiB body limit, where serve answers 413 without the handler");
        std::process::exit(2);
    }
    if let Some(out) = super::super::flag_value(args, "--emit") {
        let root = Path::new(out);
        let mut files = vec![
            ("call.almd".to_string(), complexity_program(n, 3)),
            ("alloc.almd".to_string(), alloc_program(PROBE_APP, n)),
        ];
        for (i, p) in serve_gen::diamond_projects(&Plan::Script.header(), PROBE_APP).into_iter().enumerate() {
            files.extend(p.files.into_iter().map(|(f, s)| (format!("diamond-{}/{f}", ["d", "l", "r"][i]), s)));
        }
        for i in 0..3 {
            files.push((format!("parity-{i}.almd"), parity_program(&engine.serve_handler(0, i))));
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
    let verdicts = vec![
        ("parity", parity(ctx, engine, args)),
        ("complexity", complexity(ctx, n, order)),
        ("alloc", alloc(ctx, n)),
        ("diamond", diamond(ctx)),
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

fn parity_program(handler: &str) -> String {
    serve_gen::served_program(&Plan::Script.header(), handler)
}

fn complexity_program(n: u64, k: u64) -> String {
    serve_gen::served_program(&Plan::Sized { k, n }.header(), CONSTANT_APP)
}

fn alloc_program(app: &str, n: u64) -> String {
    serve_gen::served_program(&Plan::ScriptThenSized { k: 8, n }.header(), app)
}

// ── complexity ─────────────────────────────────────────────────────────

/// Criterion 2. The size is the request body (n bytes); a "call" is one
/// request served, so k..2k is the marginal cost of k more requests. The
/// handler answers a constant, so the series is `http.serve`'s own work:
/// reading and framing the request, building the record, writing the
/// response. The counted judge (`judge_counted`) and its wall-time
/// fallback run unchanged: each run is a session (`run_artifact`,
/// `run_counted`), its output the transcript.
fn complexity(ctx: &Ctx, n: u64, order: Option<&str>) -> String {
    let programs = |m: u64, k: u64| (complexity_program(m, k), complexity_program(m, 0));
    match judge(ctx, "serve(n=request body bytes, k=requests)", order, n, &programs) {
        Ok(s) => format!("PASS: {s}"),
        Err(s) => format!("FAIL: {s}"),
    }
}

// ── alloc ──────────────────────────────────────────────────────────────

/// Criterion "alloc". `http.serve` returns after the SIGTERM, so "the call
/// leaves nothing live" has its ordinary reading: the embedded lane's
/// live-block count at exit, after serve answered the script and k sized
/// requests and returned. Two handlers: the probe app (every reader, an
/// err) and the constant one.
fn alloc(ctx: &Ctx, n: u64) -> String {
    let mut parts = Vec::new();
    let mut ok = true;
    for (label, app) in [("probe-app", PROBE_APP), ("constant-app", CONSTANT_APP)] {
        let src = alloc_program(app, n);
        let file = ctx.dir.join(format!("a{:016x}.almd", super::hash(&src)));
        if std::fs::write(&file, &src).is_err() {
            return "SKIP: scratch write failed".into();
        }
        let world = match World::for_program(&ctx.dir, &src) {
            Ok(w) => w,
            Err(e) => return format!("SKIP: {e}"),
        };
        let mut cmd = Command::new(&ctx.almide);
        cmd.arg("run").arg(&file).args(["--target", "wasm", "--"]).args(sandbox::ARGS).env("ALMIDE_WASM_ALLOC_COUNT", "1");
        world.enter(&mut cmd);
        let plan = Plan::ScriptThenSized { k: 8, n };
        match drv::drive_socket(&mut cmd, &plan, false) {
            Ok(s) => {
                let (pass, part) = alloc_part_exiting(label, &s.output, 0);
                ok &= pass;
                parts.push(format!("{part} after {} requests and the SIGTERM", plan.requests().len()));
            }
            Err(e) => {
                ok = false;
                parts.push(format!("{label} session failed: {e}"));
            }
        }
    }
    format!("{}: {}", if ok { "PASS" } else { "FAIL" }, parts.join(", "))
}

// ── diamond ────────────────────────────────────────────────────────────

/// Criterion 5. Three projects serve the same app: through `http.serve`
/// itself and through `left.call` / `right.call`, each a wrapper over
/// `base.call`. Each is driven by the same session on native (`almide
/// run`, which execs the program, #2809) and on the embedded lane; the
/// three transcripts must be equal on each leg, and the legs equal.
fn diamond(ctx: &Ctx) -> String {
    let mut legs: [Vec<String>; 2] = [Vec::new(), Vec::new()];
    for (p, project) in ["d", "l", "r"].into_iter().zip(serve_gen::diamond_projects(&Plan::Script.header(), PROBE_APP)) {
        let root = ctx.dir.join(format!("diamond-serve-{p}"));
        let Ok(entry) = project.write_to(&root) else { return "SKIP: scratch write failed".into() };
        for (i, wasm) in [false, true].into_iter().enumerate() {
            let world = match World::for_program(&ctx.dir, "") {
                Ok(w) => w,
                Err(e) => return format!("SKIP: {e}"),
            };
            let mut c = Command::new(&ctx.almide);
            c.arg("run").arg(&entry);
            if wasm {
                c.args(["--target", "wasm"]);
            }
            c.arg("--").args(sandbox::ARGS);
            world.enter(&mut c);
            c.current_dir(root.join("app"));
            match drv::drive_socket(&mut c, &Plan::Script, false) {
                Ok(s) => legs[i].push(s.transcript),
                Err(e) => return format!("FAIL: path {p} on {}: {e}", if wasm { "wasm" } else { "native" }),
            }
        }
    }
    diamond_verdict(&legs[0], &legs[1])
}

fn diamond_verdict(native: &[String], wasm: &[String]) -> String {
    let same_three = |t: &[String]| t.len() == 3 && t[0] == t[1] && t[0] == t[2];
    let lines = native.first().map_or(0, |t| t.lines().count());
    match (same_three(native), same_three(wasm), native == wasm) {
        _ if !native.iter().chain(wasm).all(|t| drv::verdicts_hold(t)) => format!("FAIL: a host-field verdict failed: {native:?} / {wasm:?}"),
        _ if !native.iter().chain(wasm).all(|t| t.ends_with("exit 0\n")) => format!("FAIL: a path did not return from serve on the SIGTERM: {native:?} / {wasm:?}"),
        (true, true, true) => format!("PASS: direct = via left = via right on both legs ({lines} transcript line(s) per path: {} responses and exit 0)", lines - 1),
        (false, _, _) => format!("FAIL: the three paths differ on native: {}", drv::first_difference(&native[0], native.get(1).unwrap_or(&String::new())).unwrap_or_else(|| "right".into())),
        (_, false, _) => format!("FAIL: the three paths differ on wasm: {}", drv::first_difference(&wasm[0], wasm.get(1).unwrap_or(&String::new())).unwrap_or_else(|| "right".into())),
        (true, true, false) => format!("FAIL: native vs wasm: {}", drv::first_difference(&native[0], &wasm[0]).unwrap_or_default()),
    }
}

// ── parity ─────────────────────────────────────────────────────────────

/// One generated program's outcome.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The checker refused the generated handler: a generator miss, not a
    /// finding (the campaign's `generator_rejects`).
    Rejected(String),
    /// Every leg answered alike and every verdict held.
    Clean,
    /// The handler trapped on a socket host, the same way on native and the
    /// embedded lane. The stock export answers its own 500 and discards the
    /// instance there (C-375), so that leg is not compared.
    TrappedAlike,
    /// A divergence, keyed by its signature for de-duplication.
    Finding(String),
}

/// Criteria 1 and 4: `--count` generated handlers (default 200) from
/// `--seed`, each served on native, the embedded lane and `wasmtime serve`
/// and driven by the fixed script. Findings are written under the temp dir
/// with the program and each leg's transcript.
fn parity(ctx: &Ctx, engine: &Engine, args: &[String]) -> String {
    let count: u64 = super::super::flag_value(args, "--count").and_then(|s| s.parse().ok()).unwrap_or(200);
    let jobs: u64 = super::super::flag_value(args, "--jobs").and_then(|s| s.parse().ok()).unwrap_or(4).max(1);
    let seed: u64 = super::super::flag_value(args, "--seed").and_then(|s| s.parse().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
    });
    let stock = stock_serves_p3(&ctx.wasmtime);
    let findings_dir = std::env::temp_dir().join(format!("xtarget-serve-findings-{seed}"));
    // A rerun of the seed must not show the last run's findings as its own.
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
                let src = parity_program(&engine.serve_handler(seed, i));
                let out = one_program(ctx, &src, i, stock.is_ok(), &findings_dir);
                results.lock().unwrap().push((i, out));
            });
        }
    });
    let results = results.into_inner().unwrap();
    let rejected: Vec<&String> = results.iter().filter_map(|(_, o)| if let Outcome::Rejected(e) = o { Some(e) } else { None }).collect();
    let clean = results.iter().filter(|(_, o)| *o == Outcome::Clean).count();
    let trapped = results.iter().filter(|(_, o)| *o == Outcome::TrappedAlike).count();
    let mut sigs: Vec<(String, u64)> = Vec::new();
    for (i, o) in &results {
        if let Outcome::Finding(sig) = o {
            if !sigs.iter().any(|(s, _)| s == sig) {
                sigs.push((sig.clone(), *i));
            }
        }
    }
    sigs.sort();
    let served = results.len() - rejected.len();
    let legs = match &stock {
        Ok(()) => "native, embedded, stock (wasmtime serve)".to_string(),
        Err(why) => format!("native, embedded; stock NOT run: {why}"),
    };
    let reject_note = rejected.first().map(|e| format!(" (first: {e})")).unwrap_or_default();
    let mut line = format!(
        "seed {seed}, {count} generated handlers over a {}-request script; legs {legs}; {served} served, {clean} clean on every leg, {trapped} trapped alike on the socket hosts (stock not compared, C-375); {} generator rejects{reject_note}; unique findings {}",
        Plan::Script.requests().len(),
        rejected.len(),
        sigs.len()
    );
    if !sigs.is_empty() {
        let shown: Vec<String> = sigs.iter().map(|(s, i)| format!("[{s}] e.g. #{i}")).collect();
        line.push_str(&format!(": {} — programs and transcripts in {}", shown.join("; "), findings_dir.display()));
    }
    let pass = sigs.is_empty() && clean > 0 && stock.is_ok();
    format!("{}: {line}", if pass { "PASS" } else { "FAIL" })
}

/// `wasmtime` runs p3 components by default from 46 on (the pin policy's
/// floor, `proofs/wasi-pin-policy.toml`); below it the stock leg cannot run.
fn stock_serves_p3(wasmtime: &Path) -> Result<(), String> {
    let v = Command::new(wasmtime).arg("--version").output().map_err(|e| format!("no wasmtime: {e}"))?;
    let text = String::from_utf8_lossy(&v.stdout).into_owned();
    let major = text.split_whitespace().nth(1).and_then(|v| v.split('.').next()?.parse::<u32>().ok()).unwrap_or(0);
    if major < 46 {
        return Err(format!("{} is below the p3 floor (46)", text.trim()));
    }
    Ok(())
}

/// Build and serve one generated program on every leg, and judge it.
fn one_program(ctx: &Ctx, src: &str, i: u64, stock: bool, findings: &Path) -> Outcome {
    let dir = ctx.dir.join(format!("serve-{i}"));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("app.almd");
    if std::fs::write(&file, src).is_err() {
        return Outcome::Finding("scratch write failed".into());
    }
    let check = Command::new(&ctx.almide).arg("check").arg(&file).output();
    match check {
        Ok(o) if o.status.success() => {}
        Ok(o) => return Outcome::Rejected(super::first_error(&o.stderr)),
        Err(e) => return Outcome::Finding(format!("almide check did not run: {e}")),
    }
    let bin = dir.join("app.bin");
    match Command::new(&ctx.almide).arg("build").arg(&file).arg("-o").arg(&bin).output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => return record(findings, i, src, &[], "native build failed".into(), &super::first_error(&o.stderr)),
        Err(e) => return Outcome::Finding(format!("almide build did not run: {e}")),
    }
    let socket = |native: bool| -> Result<Session, String> {
        let world = World::for_program(&ctx.dir, src)?;
        let mut c = if native {
            Command::new(&bin)
        } else {
            let mut c = Command::new(&ctx.almide);
            c.arg("run").arg(&file).args(["--target", "wasm", "--"]);
            c
        };
        c.args(sandbox::ARGS);
        world.enter(&mut c);
        drv::drive_socket(&mut c, &Plan::Script, false)
    };
    let native = socket(true);
    let embedded = socket(false);
    let (nv, ev) = (drv::leg_view(&native), drv::leg_view(&embedded));
    let mut views = vec![("native", nv.clone()), ("embedded", ev.clone())];
    if nv != ev {
        let what = drv::first_difference(&nv, &ev).unwrap_or_default();
        return record(findings, i, src, &views, format!("native vs embedded: {}", part_of(&what)), &what);
    }
    for (leg, v) in &views {
        if !drv::verdicts_hold(v) {
            let what = failed_verdict(v);
            return record(findings, i, src, &views, format!("{leg}: the {} verdict failed", what.split(' ').next().unwrap_or("")), &what);
        }
    }
    // Both socket hosts failed alike: a trap in the handler (the same
    // request, the same exit). A session that hung is a finding either way.
    if native.is_err() {
        if nv.contains("exited before answering") {
            return Outcome::TrappedAlike;
        }
        let what = nv.trim().to_string();
        return record(findings, i, src, &views, format!("both socket hosts: {}", what.split(" (").next().unwrap_or("")), &what);
    }
    if !nv.ends_with("exit 0\n") {
        return record(findings, i, src, &views, "serve did not return 0 after the SIGTERM on either socket host".into(), nv.lines().last().unwrap_or(""));
    }
    if !stock {
        return Outcome::Clean;
    }
    let wasm = dir.join("app.wasm");
    match Command::new(&ctx.almide).arg("build").arg(&file).args(["--target", "wasm", "-o"]).arg(&wasm).output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => return record(findings, i, src, &views, "stock build refused a serve-shaped program".into(), &super::first_error(&o.stderr)),
        Err(e) => return Outcome::Finding(format!("almide build did not run: {e}")),
    }
    let wasmtime = ctx.wasmtime.clone();
    let stock_run = drv::drive_fresh(
        &mut |port| {
            let mut c = Command::new(&wasmtime);
            c.arg("serve").arg("--addr").arg(format!("127.0.0.1:{port}")).arg(&wasm);
            c
        },
        Host::Stock,
        &Plan::Script,
        false,
    );
    let sv = drv::leg_view(&stock_run);
    views.push(("stock", sv.clone()));
    let nr = drv::responses(&nv);
    if nr != sv {
        let what = drv::first_difference(&nr, &sv).unwrap_or_default();
        let cause = native_cause(&nr, &what);
        return record(findings, i, src, &views, format!("native vs stock: {} ({cause})", part_of(&what)), &what);
    }
    if !drv::verdicts_hold(&sv) {
        let what = failed_verdict(&sv);
        return record(findings, i, src, &views, format!("stock: the {} verdict failed", what.split(' ').next().unwrap_or("")), &what);
    }
    let _ = std::fs::remove_dir_all(&dir);
    Outcome::Clean
}

/// Keep a finding's program and transcripts, and name it by its signature.
fn record(findings: &Path, i: u64, src: &str, views: &[(&str, String)], sig: String, detail: &str) -> Outcome {
    let dir = findings.join(format!("{i:04}"));
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("app.almd"), src);
    for (leg, v) in views {
        let _ = std::fs::write(dir.join(format!("{leg}.txt")), v);
    }
    let _ = std::fs::write(dir.join("signature.txt"), format!("{sig}\n{detail}\n"));
    Outcome::Finding(sig)
}

/// The part of a [`drv::first_difference`] (its last word): what a finding
/// is de-duplicated on, so one cause met at different requests counts once.
fn part_of(what: &str) -> &str {
    what.rsplit(' ').next().unwrap_or(what)
}

/// The first failed verdict of a transcript: `<name> FAILED: <why>`.
fn failed_verdict(t: &str) -> String {
    t.lines()
        .find_map(|l| l.find(" FAILED: ").map(|at| format!("{}{}", l[..at].rsplit(['(', ' ']).next().unwrap_or(""), &l[at..])))
        .or_else(|| t.lines().find(|l| l.contains("-> UNPARSEABLE")).map(|l| format!("parse {l}")))
        .unwrap_or_default()
}

/// What native answered at the first differing request, in the terms a
/// cause is told apart by: a status outside 100..=599, a response native's
/// core refused (#2822: a 500 whose body names the refused field), or
/// neither.
fn native_cause(native: &str, what: &str) -> &'static str {
    let at = what.split(' ').next().unwrap_or("");
    let Some(line) = native.lines().find(|l| l.split(' ').next() == Some(at)) else { return "native line missing" };
    let status = line.split(" -> ").nth(1).and_then(|r| r.split(' ').nth(1)).and_then(|s| s.parse::<i64>().ok());
    let headers = line.split_once(" [").and_then(|(_, h)| h.split_once("] body=")).map_or("", |(h, _)| h);
    let control = (0u8..0x20).filter(|b| *b != b'\t').chain([0x7f]).any(|b| headers.contains(&format!("\\x{b:02x}")));
    match status {
        Some(s) if !(100..=599).contains(&s) => "native wrote a status outside 100..=599",
        Some(s) if (100..200).contains(&s) => "native wrote a 1xx as the final response",
        Some(500) if line.contains("body=Internal error: response header") => "native refused a response header, #2822",
        Some(_) if control => "native wrote a header value holding a control byte",
        Some(_) => "native status in 200..=599",
        None => "native wrote no parseable status line",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(body: &str) -> String {
        format!("#0 GET / -> HTTP/1.1 200 [] body={body} (status ok, fields ok, date ok, framing ok, connection ok, reason ok)\nexit 0\n")
    }

    #[test]
    fn the_diamond_passes_only_when_three_paths_and_both_legs_agree() {
        let same = vec![t("a"), t("a"), t("a")];
        assert!(diamond_verdict(&same, &same).starts_with("PASS: direct = via left = via right on both legs (2 transcript line(s) per path: 1 responses and exit 0)"));
        assert!(diamond_verdict(&[t("a"), t("b"), t("a")], &same).starts_with("FAIL: the three paths differ on native: #0 GET body"));
        assert!(diamond_verdict(&same, &[t("b"), t("b"), t("b")]).starts_with("FAIL: native vs wasm: #0 GET body"));
        let not_returned: Vec<String> = same.iter().map(|s| s.replace("exit 0", "exit 1")).collect();
        assert!(diamond_verdict(&not_returned, &not_returned).starts_with("FAIL: a path did not return"));
    }

    #[test]
    fn every_served_function_is_declared_with_a_function_parameter_and_a_unit_return() {
        // The served readings assume a server: a port, a handler, and no
        // value of its own. A SERVED entry that stops being one is caught.
        const HTTP: &str = include_str!("../../../../stdlib/http.almd");
        for f in SERVED {
            let (module, func) = f.split_once('.').unwrap();
            assert_eq!(module, "http", "{f}: only http's declarations are read here");
            let sig = HTTP.lines().find(|l| l.starts_with(&format!("effect fn {func}("))).unwrap_or_else(|| panic!("{f}: not declared in stdlib/http.almd"));
            assert_eq!(sig, format!("effect fn {func}(port: Int, f: effect (HttpRequest) -> HttpResponse) -> Unit = _"), "{f}");
        }
    }

    /// The driver against the real server: the probe app, built natively by
    /// the workspace `almide`, answers the script, returns from `http.serve`
    /// on the SIGTERM (exit 0), and every verdict holds. A no-op without a
    /// built `almide` (as `every_in_domain_probe_takes_the_success_path`).
    #[test]
    fn the_probe_app_answers_the_script_natively_and_returns_from_serve() {
        let bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/almide");
        if !bin.exists() {
            eprintln!("skipping: no workspace almide binary");
            return;
        }
        let dir = std::env::temp_dir().join(format!("xtarget-serve-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = serve_gen::served_program(&Plan::Script.header(), PROBE_APP);
        let file = dir.join("app.almd");
        std::fs::write(&file, &src).unwrap();
        let exe = dir.join("app.bin");
        let built = Command::new(&bin).arg("build").arg(&file).arg("-o").arg(&exe).output().unwrap();
        assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
        let s = drv::drive_socket(&mut Command::new(&exe), &Plan::Script, false).expect("session");
        let _ = std::fs::remove_dir_all(&dir);
        let lines: Vec<&str> = s.transcript.lines().collect();
        assert_eq!(lines.len(), Plan::Script.requests().len() + 1, "{}", s.transcript);
        assert!(drv::verdicts_hold(&s.transcript), "{}", s.transcript);
        assert_eq!(*lines.last().unwrap(), "exit 0");
        assert!(lines[2].starts_with("#2 POST /echo -> HTTP/1.1 200 [content-type: text/plain; charset=utf-8; x-token: abc] body=POST /echo [] \\xe6\\x97\\xa5"), "{}", lines[2]);
        assert!(lines[3].starts_with("#3 PUT /a/b -> HTTP/1.1 418 [content-type: text/plain; x-kind: teapot] body=a\\xef\\xbf\\xbdb"), "{}", lines[3]);
        assert!(lines[7].starts_with("#7 GET /missing -> HTTP/1.1 500 [content-type: text/plain] body=Internal error: no route /missing"), "{}", lines[7]);
    }

    #[test]
    fn the_default_sizes_stay_under_the_body_limit() {
        for counted in [true, false] {
            let n = default_n(counted);
            let top = if counted { 16 * n } else { 4 * n };
            assert!(top < 1 << 20, "n={n}: the largest body {top} would be a 413");
        }
    }
}
