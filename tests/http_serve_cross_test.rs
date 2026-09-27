//! The native ⇄ EMBEDDED ⇄ STOCK server net (#2650 / #2659, C-367). The
//! server fixture `spec/serve_cross/http_serve_replay.almd` never exits, so
//! no generic runner executes it: this driver starts it three ways — `almide
//! run` natively, `almide run --target wasm` (the embedded almide.* host), and
//! the `almide build --target wasm` artifact (a WASI 0.3 component) under a
//! stock `wasmtime run -S p3 -S inherit-network` — waits for its "ready" line
//! on stderr, replays ONE request script against each and requires the raw
//! response bytes and the stderr transcript to be byte-identical. On the wasm
//! legs the request parse and the response bytes are a transcription of the
//! native server core (stdlib/http_serve.almd), so this replay is what holds
//! them to native. Two more properties ride along:
//!
//! - one instance per run: `/boot` answers a random draw main made once and
//!   the handler captured — two requests of one run must see the same value
//!   (a per-request instance would draw again);
//! - the bind failure: on an occupied port every leg aborts with
//!   `Error: bind failed: …` and exit 1 (the embedded leg with native's exact
//!   line; the stock artifact names the wasi:sockets error-code instead of
//!   the OS text, which a component cannot see).
//!
//! The stock leg needs a p3-capable wasmtime (46+): `ALMIDE_P3_WASMTIME`
//! names one, else `wasmtime` on PATH. Without one the stock leg is skipped
//! with a note locally and FAILS under CI (`CI` set), so the leg cannot go
//! quiet in the one place it is run.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().expect("utf8 path").to_string();
    }
    "almide".to_string()
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/serve_cross/http_serve_replay.almd")
}

/// The three ways the fixture is served.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Leg {
    Native,
    Embedded,
    Stock,
}

/// The stock runtime's flags: the p3 world, the sync stream builtins the
/// component's guest-pull loop uses, and the network the listener needs —
/// the command docs/wasm/README.md documents.
const STOCK_FLAGS: &[&str] = &["run", "-W", "component-model-more-async-builtins", "-S", "p3", "-S", "inherit-network"];

/// A p3-capable wasmtime, or None (skip locally, fail under CI).
fn p3_wasmtime() -> Option<String> {
    let bin = std::env::var("ALMIDE_P3_WASMTIME").unwrap_or_else(|_| "wasmtime".to_string());
    let major = Command::new(&bin)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|v| v.split_whitespace().nth(1).and_then(|n| n.split('.').next()).and_then(|m| m.parse::<u32>().ok()));
    match major {
        Some(m) if m >= 46 => Some(bin),
        _ => {
            assert!(
                std::env::var_os("CI").is_none(),
                "the stock leg needs a p3-capable wasmtime (46+): set ALMIDE_P3_WASMTIME ({bin} answered {major:?})"
            );
            eprintln!("skipping the stock leg: no p3-capable wasmtime (set ALMIDE_P3_WASMTIME)");
            None
        }
    }
}

/// `almide build --target wasm` of `src` into a scratch file: the stock
/// artifact, a WASI 0.3 component.
fn build_stock(src: &Path, tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-serve-stock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let out = dir.join(format!("{tag}.wasm"));
    let o = Command::new(almide_bin())
        .args(["build", src.to_str().expect("utf8 path"), "--target", "wasm", "-o", out.to_str().expect("utf8 path")])
        .output()
        .expect("almide builds");
    assert!(o.status.success(), "stock build of {tag} failed:\n{}", String::from_utf8_lossy(&o.stderr));
    out
}

/// The command that serves `src` on `leg`; `stock` = (wasmtime, artifact).
fn serve_cmd(leg: Leg, src: &Path, stock: Option<&(String, PathBuf)>) -> Command {
    match leg {
        Leg::Stock => {
            let (bin, wasm) = stock.expect("the stock leg has its artifact");
            let mut c = Command::new(bin);
            c.args(STOCK_FLAGS).arg(wasm);
            c
        }
        _ => {
            let mut c = Command::new(almide_bin());
            c.arg("run").arg(src);
            if leg == Leg::Embedded {
                c.args(["--target", "wasm"]);
            }
            c.arg("--");
            c
        }
    }
}

/// The request script: GET, POST with a UTF-8 body and a header, a header
/// read that misses, a percent-encoded query (a pair without `=`, a value
/// holding `=`, `+`), a status outside the reason table plus a lowercase
/// header name, a redirect, a 404, a handler err (500), another method.
const SCRIPT: &[&[u8]] = &[
    b"GET /hello HTTP/1.1\r\nHost: t\r\n\r\n",
    "POST /echo HTTP/1.1\r\nHost: t\r\nX-Token: abc\r\nContent-Length: 12\r\n\r\n日本語 ok".as_bytes(),
    b"GET /echo HTTP/1.1\r\n\r\n",
    b"GET /query?b=2&a=%E7%8C%AB&c=x+y&nokey&d=1=2 HTTP/1.1\r\n\r\n",
    b"GET /teapot HTTP/1.1\r\n\r\n",
    b"GET /redirect HTTP/1.1\r\n\r\n",
    b"GET /missing HTTP/1.1\r\n\r\n",
    b"GET /fail HTTP/1.1\r\n\r\n",
    b"DELETE /hello HTTP/1.1\r\n\r\n",
];

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port").local_addr().expect("addr").port()
}

fn start(leg: Leg, port: u16, stock: Option<&(String, PathBuf)>) -> Child {
    let mut c = serve_cmd(leg, &fixture(), stock);
    c.arg(port.to_string());
    // Own process group: `almide run` spawns the native binary as a child,
    // and the kill below must take the whole tree.
    c.process_group(0).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c.spawn().expect("almide runs")
}

fn kill_group(child: &mut Child) {
    // `-9 -- -<pgid>`, the one spelling both kills agree on (measured in
    // ubuntu:24.04): procps `kill -9 -<pgid>` exits 0 and kills NOTHING, and
    // `-s KILL -- -<pgid>` is a usage error there — either left the server
    // running and the wait below hung the CI job until its timeout.
    let group = Command::new("kill").args(["-9", "--", &format!("-{}", child.id())]).status();
    assert!(group.as_ref().is_ok_and(|s| s.success()), "could not kill the server's process group: {group:?}");
    let _ = child.kill();
    let _ = child.wait();
}

/// One request on a fresh connection; the answer is everything up to the
/// server's close. Connection attempts retry while the server binds (its
/// "ready" line is printed just before `http.serve`).
fn ask(port: u16, raw: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut conn = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(c) => break c,
            Err(e) => {
                assert!(Instant::now() < deadline, "server on {port} never accepted: {e}");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };
    conn.set_read_timeout(Some(Duration::from_secs(60))).expect("timeout");
    conn.write_all(raw).expect("send request");
    let mut out = Vec::new();
    conn.read_to_end(&mut out).expect("read response to close");
    out
}

struct Run {
    responses: Vec<Vec<u8>>,
    boots: Vec<Vec<u8>>,
    stderr: String,
}

fn replay(leg: Leg, stock: Option<&(String, PathBuf)>) -> Run {
    let port = free_port();
    let mut child = start(leg, port, stock);
    let mut err = BufReader::new(child.stderr.take().expect("stderr piped"));
    let mut first = String::new();
    err.read_line(&mut first).expect("read the ready line");
    if first != "ready\n" {
        let mut rest = String::new();
        let _ = err.read_to_string(&mut rest);
        kill_group(&mut child);
        panic!("{leg:?}: expected the ready line, got {first:?}{rest}");
    }
    let responses: Vec<Vec<u8>> = SCRIPT.iter().map(|r| ask(port, r)).collect();
    let boots: Vec<Vec<u8>> = (0..2)
        .map(|_| {
            let r = ask(port, b"GET /boot HTTP/1.1\r\n\r\n");
            let cut = r.windows(4).position(|w| w == b"\r\n\r\n").expect("a response head") + 4;
            r[cut..].to_vec()
        })
        .collect();
    kill_group(&mut child);
    let mut stderr = first;
    let _ = err.read_to_string(&mut stderr);
    Run { responses, boots, stderr }
}

#[cfg_attr(debug_assertions, ignore = "serve-cross net is release-only (CI: release-shape job)")]
#[test]
fn http_serve_answers_byte_identically_on_native_the_embedded_lane_and_the_stock_artifact() {
    let stock = p3_wasmtime().map(|bin| (bin, build_stock(&fixture(), "replay")));
    let native = replay(Leg::Native, None);
    let mut legs = vec![(Leg::Embedded, replay(Leg::Embedded, None))];
    if let Some(st) = stock.as_ref() {
        legs.push((Leg::Stock, replay(Leg::Stock, Some(st))));
    }
    // The script's own shape, so an empty answer on both legs cannot pass.
    assert!(native.responses[0].ends_with(b"\r\n\r\nhello"), "{:?}", String::from_utf8_lossy(&native.responses[0]));
    assert!(
        native.responses[7].starts_with(b"HTTP/1.1 500 Internal Server Error\r\n")
            && native.responses[7].ends_with(b"Internal error: denied /fail"),
        "{:?}",
        String::from_utf8_lossy(&native.responses[7])
    );
    for (leg, wasm) in &legs {
        for (i, (n, w)) in native.responses.iter().zip(&wasm.responses).enumerate() {
            assert_eq!(
                String::from_utf8_lossy(n),
                String::from_utf8_lossy(w),
                "{leg:?}: request {i} ({:?}) answered differently",
                String::from_utf8_lossy(&SCRIPT[i][..SCRIPT[i].len().min(40)])
            );
            assert_eq!(n, w, "{leg:?}: request {i}: bytes differ");
        }
        assert_eq!(native.stderr, wasm.stderr, "{leg:?}: the stderr transcripts differ");
    }
    for run in std::iter::once(&native).chain(legs.iter().map(|(_, r)| r)) {
        assert_eq!(run.boots[0], run.boots[1], "one run answered two different draws: not one instance");
        assert!(!run.boots[0].is_empty());
    }
}

#[cfg_attr(debug_assertions, ignore = "serve-cross net is release-only (CI: release-shape job)")]
#[test]
fn a_bind_failure_aborts_on_every_leg() {
    let held = TcpListener::bind("0.0.0.0:0").expect("hold a port");
    let port = held.local_addr().expect("addr").port();
    let p3 = p3_wasmtime();
    let stock = p3.as_ref().map(|bin| (bin.clone(), build_stock(&fixture(), "bind")));
    let run = |leg: Leg| {
        let mut c = serve_cmd(leg, &fixture(), stock.as_ref());
        c.arg(port.to_string());
        c.stdin(Stdio::null()).output().expect("the server runs")
    };
    let native = run(Leg::Native);
    let wasm = run(Leg::Embedded);
    let native_err = String::from_utf8_lossy(&native.stderr);
    assert!(native_err.starts_with("ready\nError: bind failed: "), "{native_err:?}");
    assert_eq!(native.status.code(), Some(1));
    assert_eq!(native.status.code(), wasm.status.code());
    assert_eq!(native_err, String::from_utf8_lossy(&wasm.stderr));
    assert_eq!(native.stdout, wasm.stdout);
    // The stock artifact: the same abort; its message names the
    // wasi:sockets error-code (a component never sees the OS text).
    if stock.is_some() {
        let st = run(Leg::Stock);
        assert_eq!(st.status.code(), Some(1), "{:?}", String::from_utf8_lossy(&st.stderr));
        assert_eq!(String::from_utf8_lossy(&st.stderr), "ready\nError: bind failed: address in use\n");
        assert_eq!(native.stdout, st.stdout);
    }

    // NOT in tail position: `http.serve` is typed never-err, so the `!` is a
    // no-op and the native runtime's returned Err used to vanish here — the
    // program printed "after" and exited 0 with no server. Both legs abort.
    let dir = std::env::temp_dir().join(format!("almide-serve-bind-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let prog = dir.join("nontail.almd");
    std::fs::write(
        &prog,
        format!(
            "import http\n\neffect fn main() -> Unit = {{\n  println(\"before\")\n  http.serve({port}, (req) => http.response(200, \"x\"))!\n  eprintln(\"after\")\n}}\n"
        ),
    )
    .expect("write the program");
    let nontail_stock = p3.as_ref().map(|bin| (bin.clone(), build_stock(&prog, "nontail")));
    let run = |leg: Leg| {
        let mut c = serve_cmd(leg, &prog, nontail_stock.as_ref());
        c.stdin(Stdio::null()).output().expect("the server runs")
    };
    let mut outs = vec![run(Leg::Native), run(Leg::Embedded)];
    if nontail_stock.is_some() {
        outs.push(run(Leg::Stock));
    }
    for out in outs {
        assert_eq!(out.status.code(), Some(1), "{:?}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "before\n");
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("Error: bind failed: "),
            "{:?}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    drop(held);
}
