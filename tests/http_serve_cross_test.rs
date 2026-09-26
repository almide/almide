//! The native ⇄ EMBEDDED-lane server net (#2650, C-367). The server fixture
//! `spec/serve_cross/http_serve_replay.almd` never exits, so no generic
//! runner executes it: this driver starts it with `almide run` natively and
//! with `almide run --target wasm` (the embedded almide.* host), waits for
//! its "ready" line on stderr, replays ONE request script against each and
//! requires the raw response bytes and the stderr transcript to be
//! byte-identical. Two more properties ride along:
//!
//! - one instance per run: `/boot` answers a random draw main made once and
//!   the handler captured — two requests of one run must see the same value
//!   (a per-request instance would draw again);
//! - the bind failure: on an occupied port both legs abort with the same
//!   stderr line and exit code.

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

fn start(wasm: bool, port: u16) -> Child {
    let mut c = Command::new(almide_bin());
    c.arg("run").arg(fixture());
    if wasm {
        c.args(["--target", "wasm"]);
    }
    c.arg("--").arg(port.to_string());
    // Own process group: `almide run` spawns the native binary as a child,
    // and the kill below must take the whole tree.
    c.process_group(0).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    c.spawn().expect("almide runs")
}

fn kill_group(child: &mut Child) {
    let _ = Command::new("kill").args(["-9", &format!("-{}", child.id())]).status();
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

fn replay(wasm: bool) -> Run {
    let port = free_port();
    let mut child = start(wasm, port);
    let mut err = BufReader::new(child.stderr.take().expect("stderr piped"));
    let mut first = String::new();
    err.read_line(&mut first).expect("read the ready line");
    if first != "ready\n" {
        let mut rest = String::new();
        let _ = err.read_to_string(&mut rest);
        kill_group(&mut child);
        panic!("wasm={wasm}: expected the ready line, got {first:?}{rest}");
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
fn http_serve_answers_byte_identically_on_native_and_the_embedded_lane() {
    let native = replay(false);
    let wasm = replay(true);
    // The script's own shape, so an empty answer on both legs cannot pass.
    assert!(native.responses[0].ends_with(b"\r\n\r\nhello"), "{:?}", String::from_utf8_lossy(&native.responses[0]));
    assert!(
        native.responses[7].starts_with(b"HTTP/1.1 500 Internal Server Error\r\n")
            && native.responses[7].ends_with(b"Internal error: denied /fail"),
        "{:?}",
        String::from_utf8_lossy(&native.responses[7])
    );
    for (i, (n, w)) in native.responses.iter().zip(&wasm.responses).enumerate() {
        assert_eq!(
            String::from_utf8_lossy(n),
            String::from_utf8_lossy(w),
            "request {i} ({:?}) answered differently",
            String::from_utf8_lossy(&SCRIPT[i][..SCRIPT[i].len().min(40)])
        );
        assert_eq!(n, w, "request {i}: bytes differ");
    }
    assert_eq!(native.stderr, wasm.stderr, "the stderr transcripts differ");
    for run in [&native, &wasm] {
        assert_eq!(run.boots[0], run.boots[1], "one run answered two different draws: not one instance");
        assert!(!run.boots[0].is_empty());
    }
}

#[cfg_attr(debug_assertions, ignore = "serve-cross net is release-only (CI: release-shape job)")]
#[test]
fn a_bind_failure_aborts_identically_on_native_and_the_embedded_lane() {
    let held = TcpListener::bind("0.0.0.0:0").expect("hold a port");
    let port = held.local_addr().expect("addr").port();
    let run = |wasm: bool| {
        let mut c = Command::new(almide_bin());
        c.arg("run").arg(fixture());
        if wasm {
            c.args(["--target", "wasm"]);
        }
        c.arg("--").arg(port.to_string());
        c.stdin(Stdio::null()).output().expect("almide runs")
    };
    let native = run(false);
    let wasm = run(true);
    let native_err = String::from_utf8_lossy(&native.stderr);
    assert!(native_err.starts_with("ready\nError: bind failed: "), "{native_err:?}");
    assert_eq!(native.status.code(), Some(1));
    assert_eq!(native.status.code(), wasm.status.code());
    assert_eq!(native_err, String::from_utf8_lossy(&wasm.stderr));
    assert_eq!(native.stdout, wasm.stdout);

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
    let run = |wasm: bool| {
        let mut c = Command::new(almide_bin());
        c.arg("run").arg(&prog);
        if wasm {
            c.args(["--target", "wasm"]);
        }
        c.stdin(Stdio::null()).output().expect("almide runs")
    };
    for out in [run(false), run(true)] {
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
