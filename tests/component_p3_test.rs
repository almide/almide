//! #1628 stage 2, increment 1: `ALMIDE_COMPONENT_P3=1` + `--component`
//! emits a WASI 0.3 component — stdio over component-model streams on the
//! async canonical ABI (sync stream/future builtins inside an async-lifted
//! `run`). The observable contract is the same as every other leg: stdout,
//! stderr and the exit code are byte-identical to the plain wasm artifact
//! (and thereby to native, which the run-manifest already pins for the
//! fixture programs).
//!
//! The runtime needs wasmtime 46+ with `component-model-async` +
//! `component-model-more-async-builtins` (the 🚝 sync builtins) and
//! `-S p3=y`; CI installs 47.x. An environment whose wasmtime lacks the
//! features skips the execution half (the emission half always runs).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn build_p3(src: &Path, out: &Path) -> String {
    let o = Command::new(almide_bin())
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "--component",
            "-o",
            out.to_str().unwrap(),
        ])
        .env("ALMIDE_COMPONENT_P3", "1")
        .output()
        .expect("spawn almide");
    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "p3 build failed:\n{stderr}");
    stderr
}

fn build_core(src: &Path, out: &Path) {
    let o = Command::new(almide_bin())
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("spawn almide");
    assert!(o.status.success(), "core build failed:\n{}", String::from_utf8_lossy(&o.stderr));
}

/// Run a module/component under wasmtime with the p3 feature set.
/// `None` = this wasmtime cannot host the p3 feature set (skip);
/// `Some((stdout, stderr, code))` otherwise.
fn run_p3(module: &Path, stdin: &str) -> Option<(String, String, i32)> {
    let mut child = Command::new("wasmtime")
        .args([
            "run",
            "-W",
            "component-model-async=y,component-model-more-async-builtins=y",
            "-S",
            "p3=y",
            module.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wasmtime");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait wasmtime");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // A wasmtime without the flags/feature refuses at the CLI or the
    // validator — that is an environment gap, not a product failure.
    if !out.status.success()
        && (stderr.contains("unexpected argument")
            || stderr.contains("unknown")
            || stderr.contains("requires the component model"))
    {
        eprintln!("skipping p3 execution: this wasmtime lacks the p3 async feature set");
        return None;
    }
    Some((
        String::from_utf8_lossy(&out.stdout).to_string(),
        stderr,
        out.status.code().unwrap_or(-1),
    ))
}

fn run_core(module: &Path, stdin: &str) -> (String, String, i32) {
    let mut child = Command::new("wasmtime")
        .args(["run", module.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wasmtime");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait wasmtime");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join("almide-component-p3");
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

// The stdin family (cursor op-35 + read-to-end op-31), interpolation, and
// both output streams — the five-op host surface minus clock/entropy,
// which are covered by the abort fixture below and the full-surface
// smoke in the PR record.
const ECHO: &str = r#"import io

effect fn main() -> Unit = {
  let b = io.read_byte()
  let l = io.read_line()!
  let rest = io.read_all()
  eprintln("err-side")
  println("b=${b} l=${l} rest=${rest}")
}
"#;

const ECHO_STDIN: &str = "Xline-one\nrest-of-it";

// Clock + a `!` unwrap abort: the trap→exit rewrite must land exit code 1
// through `wasi:cli/exit@0.3.0` exactly as p1/p2 spell it.
const ABORT: &str = r#"import datetime

effect fn main() -> Unit = {
  let t = datetime.now()
  if t > 0 then println("clock ok") else println("clock BAD")
  let xs: List[Int] = []
  println(int.to_string(list.get(xs, 5)!))
}
"#;

#[test]
fn p3_component_emits_and_matches_core() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir();
    let src = d.join("echo.almd");
    std::fs::write(&src, ECHO).expect("write");
    let core = d.join("echo_core.wasm");
    let p3 = d.join("echo_p3.wasm");
    build_core(&src, &core);
    let line = build_p3(&src, &p3);
    assert!(
        line.contains("WASI 0.3 component (direct, async ABI)"),
        "the p3 build must name its leg:\n{line}"
    );
    // Component layer marker (bytes 6..8: layer 1 = component).
    let bytes = std::fs::read(&p3).expect("read");
    assert_eq!(&bytes[6..8], &[1, 0], "not a component-layer artifact");

    if !wasmtime_available() {
        return;
    }
    let (core_out, core_err, core_code) = run_core(&core, ECHO_STDIN);
    assert_eq!(core_out, "b=88 l=line-one rest=rest-of-it\n");
    assert_eq!(core_code, 0);
    let Some((p3_out, p3_err, p3_code)) = run_p3(&p3, ECHO_STDIN) else {
        return;
    };
    assert_eq!(p3_out, core_out, "p3 stdout diverged from the core module");
    assert_eq!(p3_err, core_err, "p3 stderr diverged from the core module");
    assert_eq!(p3_code, core_code, "p3 exit code diverged");
}


// fan.* on the p3 component (#1628 stage 2, the deterministic half): the
// C-004 contract's combinators — list-order map, first-success any,
// settle — must answer byte-identically through the async canonical ABI.
// Deterministic fan needs NO overlap machinery (arms evaluate in list
// order by the language's own semantics); the async-I/O overlap half
// arrives with an async import surface (wasi:http p3) and rides the SAME
// plumbing this pins.
const FAN: &str = r#"effect fn dbl(x: Int) -> Result[Int, String] = ok(x * 2)

effect fn main() -> Unit = {
  let doubled = fan.map([1, 2, 3, 4], (x) => dbl(x))!
  println(doubled |> list.map((x) => int.to_string(x)) |> list.join(","))
  let first = fan.any([9, 10], (x) => dbl(x)) ?? -1
  println(int.to_string(first))
}
"#;

#[test]
fn p3_component_runs_fan_deterministically() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir();
    let src = d.join("fan.almd");
    std::fs::write(&src, FAN).expect("write");
    let core = d.join("fan_core.wasm");
    let p3 = d.join("fan_p3.wasm");
    build_core(&src, &core);
    build_p3(&src, &p3);
    if !wasmtime_available() {
        return;
    }
    let (core_out, _, core_code) = run_core(&core, "");
    assert_eq!(core_out, "2,4,6,8\n18\n");
    assert_eq!(core_code, 0);
    let Some((p3_out, _, p3_code)) = run_p3(&p3, "") else {
        return;
    };
    assert_eq!(p3_out, core_out, "fan output diverged on the p3 leg");
    assert_eq!(p3_code, core_code);
}

// The filesystem READ surface (#1628 increment 2a): exists/is_dir/is_file
// via stat-at, read_text via open-at + sync stream reads, the if-exists
// none leg, and the not-found error leg — all against the first preopen,
// byte-identical to the incumbent adapter leg. The structural leg still
// routes fs programs to the incumbent by default (the write surface is
// not ported), so the build uses the frontier probe switch
// ALMIDE_WASM_STRUCTURAL=1 — the documented lever the eventual route
// flip is verified with.
const FS_READ: &str = r#"import fs

effect fn main() -> Unit = {
  println(if fs.exists("data.txt") then "exists" else "missing")
  println(if fs.is_file("data.txt") then "file" else "not-file")
  println(if fs.is_dir("sub") then "dir" else "not-dir")
  let t = fs.read_text("data.txt")!
  println("len=${string.len(t)}")
  println(string.trim_end(t))
  let opt = fs.read_text_if_exists("nope.txt")!
  println(opt ?? "(none)")
}
"#;

const FS_ERR: &str = r#"import fs

effect fn main() -> Unit = {
  let t = fs.read_text("missing.txt")!
  println(t)
}
"#;

fn build_p3_structural(src: &Path, out: &Path) -> String {
    let o = Command::new(almide_bin())
        .args([
            "build",
            src.to_str().unwrap(),
            "--target",
            "wasm",
            "--component",
            "-o",
            out.to_str().unwrap(),
        ])
        .env("ALMIDE_COMPONENT_P3", "1")
        .env("ALMIDE_WASM_STRUCTURAL", "1")
        .env("ALMIDE_DBG_FAN", "1")
        .output()
        .expect("spawn almide");
    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "p3 structural build failed:\n{stderr}");
    stderr
}

/// Run under wasmtime with the p3 feature set AND `--dir .` from `cwd`.
fn run_p3_dir(module: &Path, cwd: &Path) -> Option<(String, String, i32)> {
    let out = Command::new("wasmtime")
        .args([
            "run",
            "-W",
            "component-model-async=y,component-model-more-async-builtins=y",
            "-S",
            "p3=y",
            "--dir",
            ".",
            module.to_str().unwrap(),
        ])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .expect("spawn wasmtime");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success()
        && (stderr.contains("unexpected argument")
            || stderr.contains("unknown")
            || stderr.contains("requires the component model"))
    {
        eprintln!("skipping p3 execution: this wasmtime lacks the p3 async feature set");
        return None;
    }
    Some((
        String::from_utf8_lossy(&out.stdout).to_string(),
        stderr,
        out.status.code().unwrap_or(-1),
    ))
}

#[test]
fn p3_component_reads_the_filesystem() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir().join("fsread");
    std::fs::create_dir_all(d.join("sub")).expect("mkdir");
    std::fs::write(d.join("data.txt"), "hello from a file\nsecond line\n").expect("write");
    let src = d.join("rd.almd");
    std::fs::write(&src, FS_READ).expect("write");
    let p3 = d.join("rd_p3.wasm");
    let line = build_p3_structural(&src, &p3);
    assert!(
        line.contains("structural leg, WASI 0.3 component (direct, async ABI)"),
        "the forced-structural p3 build must stay on the direct leg:\n{line}"
    );
    if !wasmtime_available() {
        return;
    }
    let Some((out, err, code)) = run_p3_dir(&p3, &d) else {
        return;
    };
    assert_eq!(
        out,
        "exists\nfile\ndir\nlen=30\nhello from a file\nsecond line\n(none)\n",
        "fs read surface diverged (stderr: {err})"
    );
    assert_eq!(code, 0);

    // The not-found error leg: the WIT-derived no-entry mapping must
    // answer the native io::Error Display string, exit 1.
    let esrc = d.join("err.almd");
    std::fs::write(&esrc, FS_ERR).expect("write");
    let ep3 = d.join("err_p3.wasm");
    build_p3_structural(&esrc, &ep3);
    let Some((eout, eerr, ecode)) = run_p3_dir(&ep3, &d) else {
        return;
    };
    assert_eq!(eout, "");
    assert!(
        eerr.contains("No such file or directory (os error 2)"),
        "the not-found leg must carry the native error string:\n{eerr}"
    );
    assert_eq!(ecode, 1);
}

// The fan prefetch (#1628 increment 2b): fan.map whose arm body is one
// fs.read_text lowers to start-all (op 40, [async-lower]open-at joined
// to the one waitable set) then await-in-arm-order (op 41, the drain
// loop). Output must be byte-identical to the sequential semantics on
// BOTH the happy path and the first-err path (where remaining awaits
// drain and discard).
const FAN_PREFETCH: &str = r#"import fs

effect fn main() -> Unit = {
  let texts = fan.map(["f1.txt", "f2.txt", "f3.txt"], (p) => fs.read_text(p)!)!
  println(texts |> list.map((t) => string.trim_end(t)) |> list.join("|"))
}
"#;

const FAN_PREFETCH_ERR: &str = r#"import fs

effect fn main() -> Unit = {
  let texts = fan.map(["f1.txt", "gone.txt", "f3.txt"], (p) => fs.read_text(p)!)!
  println(texts |> list.join("|"))
}
"#;

#[test]
fn p3_component_fan_prefetch_reads_concurrently() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir().join("fanpf");
    std::fs::create_dir_all(&d).expect("mkdir");
    for k in 1..=3 {
        std::fs::write(d.join(format!("f{k}.txt")), format!("file-{k} body\n")).expect("write");
    }
    let src = d.join("pf.almd");
    std::fs::write(&src, FAN_PREFETCH).expect("write");
    let p3 = d.join("pf_p3.wasm");
    let build_log = build_p3_structural(&src, &p3);
    // The build must actually take the PREFETCH route — asserted on the
    // emitter's own route line (ALMIDE_DBG_FAN), because the import
    // assertion below is vacuous on its own: to_p3 declares the async
    // imports unconditionally, so a pattern regression that fell back to
    // the sequential accumulator would still carry them.
    assert!(
        build_log.contains("prefetch lowering engaged"),
        "fan.map over fs.read_text must lower through the prefetch route:
{build_log}"
    );
    let bytes = std::fs::read(&p3).expect("read");
    let hay = String::from_utf8_lossy(&bytes).into_owned();
    assert!(
        hay.contains("[async-lower][method]descriptor.open-at")
            && hay.contains("[waitable-set-wait]"),
        "the prefetch build must import the async open + waitable-set builtins"
    );
    if !wasmtime_available() {
        return;
    }
    let Some((out, err, code)) = run_p3_dir(&p3, &d) else {
        return;
    };
    assert_eq!(out, "file-1 body|file-2 body|file-3 body\n", "stderr: {err}");
    assert_eq!(code, 0);

    // First-err path: arm 2 fails; arms 1/3 drain and discard.
    let esrc = d.join("pferr.almd");
    std::fs::write(&esrc, FAN_PREFETCH_ERR).expect("write");
    let ep3 = d.join("pferr_p3.wasm");
    build_p3_structural(&esrc, &ep3);
    let Some((eout, eerr, ecode)) = run_p3_dir(&ep3, &d) else {
        return;
    };
    assert_eq!(eout, "");
    assert!(
        eerr.contains("No such file or directory (os error 2)"),
        "the err leg must carry the native error string:\n{eerr}"
    );
    assert_eq!(ecode, 1);
}

// fan.any over the same shape (#1628 increment 2c): first ok in ARM
// order wins, the started loser arms are ABANDONED via subtask.cancel —
// the p3 run must exit clean (an outstanding subtask at task exit would
// trap), and all-fail answers the C-004 ledger Err.
const FAN_ANY: &str = r#"import fs

effect fn main() -> Unit = {
  let first = fan.any(["gone-a.txt", "f2.txt", "f3.txt"], (p) => fs.read_text(p)!) ?? "(none)"
  println(string.trim_end(first))
  let nores = fan.any(["gone-a.txt", "gone-b.txt"], (p) => fs.read_text(p)!)
  match nores {
    ok(_)  => println("BAD"),
    err(m) => println("allfail=${m}"),
  }
}
"#;

#[test]
fn p3_component_fan_any_cancels_the_losers() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir().join("fanany");
    std::fs::create_dir_all(&d).expect("mkdir");
    for k in 2..=3 {
        std::fs::write(d.join(format!("f{k}.txt")), format!("file-{k} body\n")).expect("write");
    }
    let src = d.join("anyp.almd");
    std::fs::write(&src, FAN_ANY).expect("write");
    let p3 = d.join("anyp_p3.wasm");
    let build_log = build_p3_structural(&src, &p3);
    assert!(
        build_log.contains("prefetch-any lowering engaged"),
        "fan.any over fs.read_text must lower through the prefetch-any route:\n{build_log}"
    );
    if !wasmtime_available() {
        return;
    }
    let Some((out, err, code)) = run_p3_dir(&p3, &d) else {
        return;
    };
    assert_eq!(
        out, "file-2 body\nallfail=fan.any: all candidates failed\n",
        "fan.any prefetch diverged (stderr: {err})"
    );
    assert_eq!(code, 0, "a leaked loser subtask would trap at task exit");
}

// The filesystem WRITE surface (#1628 increment 2d): recursive mkdir_p
// (create-directory-at per prefix, exist idempotent), write (truncate) /
// append / write_bytes (slot packing) through write-via-stream with the
// completion-future durability handshake, remove / remove_all via
// stat-then-unlink-or-rmdir, and the missing-parent error leg — all
// byte-identical to the incumbent adapter leg.
const FS_WRITE: &str = r#"import fs

effect fn main() -> Unit = {
  let _ = fs.mkdir_p("wtmp/deep/nest")!
  let _ = fs.write("wtmp/deep/nest/x.txt", "written-body")!
  let _ = fs.append("wtmp/deep/nest/x.txt", "+tail")!
  println(fs.read_text("wtmp/deep/nest/x.txt")!)
  let _ = fs.write_bytes("wtmp/deep/nest/b.bin", [72, 73, 10])!
  println(fs.read_text("wtmp/deep/nest/b.bin")!)
  let _ = fs.remove("wtmp/deep/nest/x.txt")!
  let _ = fs.remove("wtmp/deep/nest/b.bin")!
  let _ = fs.remove("wtmp/deep/nest")!
  let _ = fs.remove_all("wtmp/deep")!
  println(if fs.exists("wtmp/deep") then "BAD-still-there" else "cleaned")
  match fs.write("wtmp/deep/nope.txt", "x") {
    ok(_)  => println("BAD-ghost-dir"),
    err(m) => println("werr=${m}"),
  }
  let _ = fs.remove("wtmp")!
}
"#;

#[test]
fn p3_component_writes_the_filesystem() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir().join("fswrite");
    std::fs::create_dir_all(&d).expect("mkdir");
    let src = d.join("wr.almd");
    std::fs::write(&src, FS_WRITE).expect("write");
    let p3 = d.join("wr_p3.wasm");
    build_p3_structural(&src, &p3);
    if !wasmtime_available() {
        return;
    }
    let Some((out, err, code)) = run_p3_dir(&p3, &d) else {
        return;
    };
    assert_eq!(
        out,
        "written-body+tail
HI

cleaned
werr=No such file or directory (os error 2)
",
        "fs write surface diverged (stderr: {err})"
    );
    assert_eq!(code, 0);
    assert!(!d.join("wtmp").exists(), "the fixture must clean up after itself");
}

#[test]
fn p3_requested_fs_builds_route_structurally_by_default() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir().join("fsflip");
    std::fs::create_dir_all(&d).expect("mkdir");
    let src = d.join("flip.almd");
    std::fs::write(
        &src,
        "import fs

effect fn main() -> Unit = {
  println(if fs.exists(\"x\") then \"y\" else \"n\")
}
",
    )
    .expect("write");
    // With the p3 component requested, an fs program's build takes the
    // STRUCTURAL leg by default (#1584's first default-route slice) —
    // no ALMIDE_WASM_STRUCTURAL override.
    let o = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "--component", "-o",
               d.join("flip_p3.wasm").to_str().unwrap()])
        .env("ALMIDE_COMPONENT_P3", "1")
        .output()
        .expect("spawn almide");
    let log = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "build failed:\n{log}");
    assert!(
        log.contains("structural leg, WASI 0.3 component (direct, async ABI)"),
        "the p3-requested fs build must route structurally by default:\n{log}"
    );
    // Without the p3 request the incumbent keeps the route (the p1/p2
    // transforms carry no fs ops).
    let o = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "--component", "-o",
               d.join("flip_ad.wasm").to_str().unwrap()])
        .output()
        .expect("spawn almide");
    let log = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "build failed:\n{log}");
    assert!(
        log.contains("incumbent v1 leg"),
        "the non-p3 fs build must keep the incumbent route:\n{log}"
    );
}

#[test]
fn p3_component_abort_answers_exit_one() {
    if Command::new(almide_bin()).arg("--version").output().is_err() {
        return;
    }
    let d = dir();
    let src = d.join("abort.almd");
    std::fs::write(&src, ABORT).expect("write");
    let p3 = d.join("abort_p3.wasm");
    build_p3(&src, &p3);
    if !wasmtime_available() {
        return;
    }
    let Some((out, err, code)) = run_p3(&p3, "") else {
        return;
    };
    assert_eq!(out, "clock ok\n");
    assert!(err.contains("Error: none"), "abort message missing:\n{err}");
    assert_eq!(code, 1, "the abort must answer exit 1 on the p3 leg");
}

// ── #1710 PR B: the wasi:http@0.3 client leg ──────────────────────────
//
// The five-fn http string family (ops 43..=47) rides the p3 component
// through an async-lowered exchange: the trailers future-write, the body
// stream-writes and `send` itself are all `[async-lower]` builtins joined
// on one waitable set, drained by a guest scheduler loop — the sync
// lowers deadlock on the host's rendezvous (the write's reader only
// appears inside `send`), which the bring-up bisect proved empirically.
// The big-body PUT below crosses the host's 1MiB
// `http-outgoing-body-buffer-chunks` rendezvous buffer on purpose: it is
// the regression fixture for that deadlock class.

/// One-connection-per-request HTTP/1.1 echo server: GET answers a fixed
/// body, DELETE a marker, POST/PUT/PATCH echo the decoded body length
/// (chunked and content-length both). Serves until the process exits.
fn spawn_http_echo() -> std::net::SocketAddr {
    use std::io::{BufRead, BufReader, Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind echo server");
    let addr = l.local_addr().expect("local addr");
    std::thread::spawn(move || {
        for conn in l.incoming() {
            let Ok(c) = conn else { break };
            let mut r = BufReader::new(c);
            let mut line = String::new();
            if r.read_line(&mut line).is_err() || line.is_empty() {
                continue;
            }
            let method = line.split_whitespace().next().unwrap_or("").to_string();
            let mut clen = 0usize;
            let mut chunked = false;
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
                let hl = h.to_ascii_lowercase();
                if let Some(v) = hl.strip_prefix("content-length:") {
                    clen = v.trim().parse().unwrap_or(0);
                }
                if hl.starts_with("transfer-encoding:") && hl.contains("chunked") {
                    chunked = true;
                }
            }
            let mut body = Vec::new();
            if chunked {
                loop {
                    let mut sz = String::new();
                    if r.read_line(&mut sz).is_err() {
                        break;
                    }
                    let n = usize::from_str_radix(
                        sz.trim().split(';').next().unwrap_or("0"),
                        16,
                    )
                    .unwrap_or(0);
                    if n == 0 {
                        let mut crlf = String::new();
                        let _ = r.read_line(&mut crlf);
                        break;
                    }
                    let mut chunk = vec![0u8; n + 2];
                    if r.read_exact(&mut chunk).is_err() {
                        break;
                    }
                    chunk.truncate(n);
                    body.extend_from_slice(&chunk);
                }
            } else if clen > 0 {
                body = vec![0u8; clen];
                let _ = r.read_exact(&mut body);
            }
            let resp = match method.as_str() {
                "GET" => "hello from p3".to_string(),
                "DELETE" => "gone".to_string(),
                _ => format!("len:{}", body.len()),
            };
            let mut c = r.into_inner();
            let _ = write!(
                c,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp.len(),
                resp
            );
        }
    });
    addr
}

/// Run a p3 component with the http feature mounted, under a watchdog:
/// a scheduler regression is a deadlock, and it must fail the test in
/// two minutes, not hang the suite.
fn run_p3_http(module: &Path) -> Option<(String, String, i32)> {
    let mut child = Command::new("wasmtime")
        .args([
            "run",
            "-W",
            "component-model-async=y,component-model-more-async-builtins=y",
            "-S",
            "p3=y",
            "-S",
            "http=y",
            module.to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wasmtime");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        match child.try_wait().expect("try_wait") {
            Some(_) => break,
            None if std::time::Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("p3 http component deadlocked (120s watchdog) — the async-lowered exchange regressed into a sync rendezvous");
            }
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    let out = child.wait_with_output().expect("wait wasmtime");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if !out.status.success()
        && (stderr.contains("unexpected argument")
            || stderr.contains("unknown")
            || stderr.contains("requires the component model"))
    {
        eprintln!("skipping p3 http execution: this wasmtime lacks the p3 http feature set");
        return None;
    }
    Some((
        String::from_utf8_lossy(&out.stdout).to_string(),
        stderr,
        out.status.code().unwrap_or(-1),
    ))
}

#[test]
fn p3_component_speaks_http() {
    let addr = spawn_http_echo();
    // All five family members in program order; the PUT body is 2MiB —
    // past the host's rendezvous buffer, the deadlock-class fixture.
    let probe = format!(
        r#"import http

effect fn main() -> Unit = {{
  match http.get("http://{addr}/hello") {{
    ok(b) => println("get:${{b}}"),
    err(e) => println("get-err:${{e}}"),
  }}
  match http.post("http://{addr}/echo", "tiny body") {{
    ok(b) => println("post:${{b}}"),
    err(e) => println("post-err:${{e}}"),
  }}
  let chunk = string.repeat("abcdefgh", 32768)
  let body = chunk + chunk + chunk + chunk + chunk + chunk + chunk + chunk
  match http.put("http://{addr}/echo", body) {{
    ok(b) => println("put:${{b}}"),
    err(e) => println("put-err:${{e}}"),
  }}
  match http.patch("http://{addr}/echo", "patch") {{
    ok(b) => println("patch:${{b}}"),
    err(e) => println("patch-err:${{e}}"),
  }}
  match http.delete("http://{addr}/gone") {{
    ok(b) => println("del:${{b}}"),
    err(e) => println("del-err:${{e}}"),
  }}
}}
"#
    );
    let d = dir();
    let src = d.join("http_probe.almd");
    std::fs::write(&src, probe).expect("write probe");
    let out = d.join("http_probe.p3.wasm");
    build_p3(&src, &out);
    if !wasmtime_available() {
        eprintln!("skipping p3 http execution: wasmtime not installed");
        return;
    }
    let Some((stdout, stderr, code)) = run_p3_http(&out) else {
        return;
    };
    assert_eq!(code, 0, "p3 http probe exit code; stderr:\n{stderr}");
    assert_eq!(
        stdout,
        "get:hello from p3\npost:len:9\nput:len:2097152\npatch:len:5\ndel:gone\n",
        "p3 http probe stdout; stderr:\n{stderr}"
    );
}
