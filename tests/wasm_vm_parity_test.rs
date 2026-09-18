//! REQ-VM-9 (#865): the qualification-scoped wasm VM runs the SHIPPED
//! artifact exactly as the stock runtime runs it. Every `spec/wasm_cross`
//! fixture is built the way users build it (`almide build --target wasm`,
//! the `to_wasi` artifact), then:
//!
//!   - an artifact whose imports are all among the five WASI preview-1 calls
//!     the VM knows MUST load, and its run MUST equal the stock runtime's —
//!     stdout, stderr and exit code, byte for byte — or stop on the VM's
//!     named trap for a call it does not serve (the clock or entropy: the
//!     Time/Rand capabilities a Critical program is never granted), after
//!     output that is a prefix of the stock runtime's;
//!   - any other artifact (fs, env, args) MUST be refused at load.
//!
//! So the VM never gives a silent wrong answer on anything the product ships:
//! it matches, it names the call it declines, or it refuses before running.
//!
//! The scope the VM exists for is tighter still: a fixture that passes
//! `almide check --profile critical` — the profile's default, every
//! capability denied, console output allowed — MUST run equal, with no
//! decline and no refusal, and its VM run is compared with the NATIVE binary
//! too (the #865 exit criterion: byte-identical to both the stock-runtime and
//! the native execution of the same program). An `--allow IO` grant also
//! reaches `fs`, whose artifact imports calls beyond the five: that program is
//! outside the VM's scope and falls under the refusal law above. A native difference on a fixture
//! with a `// @xt-allow:` line is the tracked native/wasm divergence the
//! cross-target gate already carries; it is reported, not failed.
//!
//! Release-only (a full corpus build); CI runs it in the commissioned wasm
//! gates job, which installs the stock runtime.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use almide_wasm_vm::{run_program, Limits};

const SERVED: &[&str] = &["fd_write", "proc_exit", "random_get", "clock_time_get", "fd_read"];
const DECLINE: &str = "Error: wasm trap: host call `";

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// Whether every import is one of the five WASI calls the VM knows.
fn imports_in_scope(bytes: &[u8]) -> bool {
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let Ok(wasmparser::Payload::ImportSection(reader)) = payload {
            for import in reader.into_imports() {
                let import = import.expect("a built artifact parses");
                if import.module != "wasi_snapshot_preview1" || !SERVED.contains(&import.name) {
                    return false;
                }
            }
        }
    }
    true
}

enum Verdict {
    Equal,
    Declined,
    Refused,
    /// A Critical-clean fixture whose VM run equals the stock runtime's and
    /// differs from native under a tracked `@xt-allow`.
    Tracked(String),
    Wrong(String),
}

struct Fixture<'a> {
    name: String,
    src: &'a Path,
    critical: bool,
    allow: Option<String>,
}

/// The run's three observables.
type Observed = (i32, String, String);

/// The stock-runtime law for one artifact; on an equal run, its observables.
fn judge(name: &str, wasm: &Path) -> (Verdict, Option<Observed>) {
    let bytes = std::fs::read(wasm).expect("artifact written");
    let in_scope = imports_in_scope(&bytes);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let vm = run_program(&bytes, Limits::default(), &mut std::io::empty(), &mut out, &mut err);
    let exit = match (in_scope, vm) {
        (false, Err(_)) => return (Verdict::Refused, None),
        (false, Ok(_)) => return (Verdict::Wrong(format!("{name}: imports outside the five, but the VM ran it")), None),
        (true, Err(e)) => return (Verdict::Wrong(format!("{name}: in scope, but refused: {e}")), None),
        (true, Ok(code)) => code,
    };
    let stock = Command::new("wasmtime").arg(wasm).stdin(Stdio::null()).output().expect("wasmtime runs");
    let (vout, verr) = (String::from_utf8_lossy(&out), String::from_utf8_lossy(&err));
    let (sout, serr) = (String::from_utf8_lossy(&stock.stdout), String::from_utf8_lossy(&stock.stderr));
    if exit == 1 && verr.lines().last().is_some_and(|l| l.starts_with(DECLINE)) {
        return if sout.starts_with(&*vout) {
            (Verdict::Declined, None)
        } else {
            (Verdict::Wrong(format!("{name}: output before the declined call is not the stock runtime's")), None)
        };
    }
    let trapped = exit == 1 && verr.lines().last().is_some_and(|l| l.starts_with("Error: wasm trap: "));
    let equal = if trapped {
        // the stock CLI reports a trap as its own error block after the
        // program's stderr; the program's part and the reason must agree
        let reason = verr.lines().last().unwrap_or("").trim_start_matches("Error: ");
        let program = serr.split("Error: failed to run main module").next().unwrap_or("");
        stock.status.code() != Some(0) && serr.contains(reason) && verr.strip_suffix(&format!("Error: {reason}\n")) == Some(program)
    } else {
        stock.status.code() == Some(exit) && serr == verr
    };
    if equal && sout == vout {
        (Verdict::Equal, Some((exit, vout.into_owned(), verr.into_owned())))
    } else {
        let msg = format!(
            "{name}: diverges\n  stock: exit={:?} stdout={sout:?} stderr={serr:?}\n  vm:    exit={exit} stdout={vout:?} stderr={verr:?}",
            stock.status.code()
        );
        (Verdict::Wrong(msg), None)
    }
}

/// Build and run the native binary; its three observables.
fn native(bin: &str, src: &Path, dir: &Path, name: &str) -> Result<Observed, String> {
    let exe = dir.join(format!("{name}.native"));
    let o = Command::new(bin)
        .args(["build", src.to_str().expect("utf8"), "-o", exe.to_str().expect("utf8")])
        .output()
        .expect("almide runs");
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).into_owned());
    }
    let r = Command::new(&exe).stdin(Stdio::null()).output().expect("the native binary runs");
    Ok((
        r.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&r.stdout).into_owned(),
        String::from_utf8_lossy(&r.stderr).into_owned(),
    ))
}

fn critical_clean(bin: &str, src: &Path) -> bool {
    Command::new(bin)
        .args(["check", src.to_str().expect("utf8"), "--profile", "critical"])
        .output()
        .expect("almide runs")
        .status
        .success()
}

/// The full verdict for one fixture: the stock-runtime law for every
/// artifact, then the Critical scope's stronger law.
fn verdict(bin: &str, f: &Fixture, dir: &Path) -> Verdict {
    let wasm = dir.join(format!("{}.wasm", f.name));
    if let Err(e) = build(bin, f.src, &wasm) {
        return Verdict::Wrong(format!("{}: the wasm build failed: {e}", f.name));
    }
    let (v, observed) = judge(&f.name, &wasm);
    if !f.critical {
        return v;
    }
    let vm = match (v, observed) {
        (Verdict::Equal, Some(o)) => o,
        (Verdict::Wrong(w), _) => return Verdict::Wrong(w),
        _ => return Verdict::Wrong(format!("{}: Critical-clean, but the VM did not run it to an equal end", f.name)),
    };
    match native(bin, f.src, dir, &f.name) {
        Err(e) => Verdict::Wrong(format!("{}: the native build failed: {e}", f.name)),
        Ok(n) if n == vm => Verdict::Equal,
        Ok(n) => match &f.allow {
            Some(reason) => Verdict::Tracked(format!("{}: {reason}", f.name)),
            None => Verdict::Wrong(format!(
                "{}: Critical-clean, VM differs from native\n  native: {n:?}\n  vm:     {vm:?}",
                f.name
            )),
        },
    }
}

fn build(bin: &str, src: &Path, out: &Path) -> Result<(), String> {
    let o = Command::new(bin)
        .args(["build", src.to_str().expect("utf8"), "--target", "wasm", "-o", out.to_str().expect("utf8")])
        .output()
        .expect("almide runs");
    if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).into_owned()) }
}

#[cfg_attr(debug_assertions, ignore = "full corpus build — release-only (CI: commissioned wasm gates)")]
#[test]
fn the_vm_runs_every_shipped_artifact_as_the_stock_runtime_does() {
    assert!(
        Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success()),
        "wasmtime (the stock runtime this gate compares against) is not on PATH"
    );
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross");
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("spec/wasm_cross exists")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "almd"))
        .collect();
    fixtures.sort();
    let tmp = tempfile::tempdir().expect("tempdir");
    let bin = almide_bin();
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = fixtures.len().div_ceil(workers).max(1);
    let verdicts: Vec<(bool, Verdict)> = std::thread::scope(|s| {
        let handles: Vec<_> = fixtures
            .chunks(chunk)
            .map(|part| {
                let (bin, tmp) = (&bin, tmp.path());
                s.spawn(move || {
                    part.iter()
                        .map(|src| {
                            let source = std::fs::read_to_string(src).expect("fixture reads");
                            let f = Fixture {
                                name: src.file_stem().expect("stem").to_string_lossy().into_owned(),
                                src,
                                critical: critical_clean(bin, src),
                                allow: source
                                    .lines()
                                    .find_map(|l| l.trim().strip_prefix("// @xt-allow:").map(|r| r.trim().to_string())),
                            };
                            (f.critical, verdict(bin, &f, tmp))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("worker")).collect()
    });
    let count = |f: fn(&Verdict) -> bool| verdicts.iter().filter(|(_, v)| f(v)).count();
    let equal = count(|v| matches!(v, Verdict::Equal));
    let declined = count(|v| matches!(v, Verdict::Declined));
    let refused = count(|v| matches!(v, Verdict::Refused));
    let critical = verdicts.iter().filter(|(c, v)| *c && matches!(v, Verdict::Equal)).count();
    let tracked: Vec<&String> = verdicts.iter().filter_map(|(_, v)| if let Verdict::Tracked(t) = v { Some(t) } else { None }).collect();
    let wrong: Vec<&String> = verdicts.iter().filter_map(|(_, v)| if let Verdict::Wrong(w) = v { Some(w) } else { None }).collect();
    eprintln!(
        "wasm VM parity: {equal} equal ({critical} Critical-clean, also equal to native), {} tracked native divergence(s), {declined} declined by a named trap, {refused} refused at load, {} wrong",
        tracked.len(),
        wrong.len()
    );
    for t in &tracked {
        eprintln!("  ~ tracked: {t}");
    }
    assert!(equal > 600, "only {equal} artifacts ran equal — the gate went blind");
    assert!(critical > 250, "only {critical} Critical-clean fixtures were judged against native — the gate went blind");
    assert!(wrong.is_empty(), "{} artifact(s):\n{}", wrong.len(), wrong.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"));
}
