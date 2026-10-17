//! net's Unix-domain sockets and shared-memory files on the embedded wasm lane
//! (`almide run --target wasm`) answer what native answers (#3620; the
//! interim embedded host ops 104..=116, retired when ADR-0023 step 7 runs the
//! embedded lane as the p3 component). Both legs serve the 13 functions from
//! the one core in crates/almide-rt-core/src/net_unix_core.rs, so equal
//! output is expected by construction; this test is the observation.
//!
//! The programs in tests/net_unix/ take the socket path as argv[1] (a fresh
//! path per leg: `unix_listen` needs one that does not exist yet) and print
//! descriptor PROPERTIES, never the raw numbers, which differ between the
//! native process and the embedded host's. Each must run to its closing
//! `done` line on both legs: a check of stdout and the exit code alone passes
//! two legs that fail the same way.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[path = "support/capped_run.rs"]
mod capped_run;

/// A socket path short enough for `sun_path` (104 bytes on macOS) that does
/// not exist yet.
fn socket_path(dir: &Path, name: &str, leg: &str) -> PathBuf {
    let p = dir.join(format!("{name}-{leg}.sock"));
    let _ = std::fs::remove_file(&p);
    p
}

/// Each program finishes in well under a second; 120 s per run (build
/// included) turns a hang into a named failure.
fn run(path: &Path, sock: &Path, wasm: bool) -> (String, Option<i32>, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(path);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    cmd.arg("--").arg(sock);
    let leg = if wasm { "embedded" } else { "native" };
    let out = capped_run::output_capped(cmd, Duration::from_secs(120))
        .unwrap_or_else(|e| panic!("{} ({leg}): {e}", path.display()));
    let _ = std::fs::remove_file(sock);
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn cross(name: &str) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/net_unix/{name}.almd"));
    let dir = tempfile::Builder::new().prefix("au").tempdir_in("/tmp").expect("scratch dir");
    let native = run(&src, &socket_path(dir.path(), name, "n"), false);
    assert!(native.0.ends_with("done\n"), "{name}: native did not finish:\n{}\n{}", native.0, native.2);
    let wasm = run(&src, &socket_path(dir.path(), name, "w"), true);
    assert!(wasm.0.ends_with("done\n"), "{name}: the embedded lane did not finish:\n{}\n{}", wasm.0, wasm.2);
    assert_eq!((&wasm.0, wasm.1), (&native.0, native.1), "{name}: the embedded lane diverged");
}

/// listen / connect / accept, send with descriptors / recv / take_fds,
/// poll / wait, close, and shm create / resize / write / read, each on its
/// success path, the received descriptors reaching the same open files.
#[test]
fn unix_and_shm_success_paths_agree_on_native_and_the_embedded_lane() {
    cross("net_unix_loopback");
}

/// Every one of the 13 functions on its error paths: descriptors the program
/// does not hold, argument domains, and OS refusals.
#[test]
fn unix_and_shm_error_paths_agree_on_native_and_the_embedded_lane() {
    cross("net_unix_errors");
}

/// The error program's lines are the contract's text, pinned on native: an
/// unheld descriptor is refused by the core before any system call, so a
/// program cannot reach (or close) a descriptor it was never given — the
/// standard streams included — on either leg.
#[test]
fn an_unheld_descriptor_is_refused_before_any_system_call() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/net_unix/net_unix_errors.almd");
    let dir = tempfile::Builder::new().prefix("au").tempdir_in("/tmp").expect("scratch dir");
    let (out, code, err) = run(&src, &socket_path(dir.path(), "pin", "n"), false);
    assert_eq!(code, Some(0), "{out}\n{err}");
    for want in [
        "err unix_connect(<path>): No such file or directory",
        "err unix_listen(<path>): Address already in use",
        "fd -1\n  err unix_accept: not a file descriptor: -1\n",
        "fd 0\n  err unix_accept: descriptor 0 is not open in this program\n",
        "  err unix_close: descriptor 2 is not open in this program\n",
        "fd closed\n  err unix_accept: descriptor ",
        "  err shm_write: descriptor 0 is not open in this program\n",
        "err unix_send: descriptor 0 is not open in this program\n",
        "err unix_send: not a file descriptor: -5\n",
        "err shm_create: negative size -1\n",
        "err shm_resize: negative size -1\n",
        "err shm_write: negative offset -1\n",
        "err shm_read: negative offset or length (-1, 1)\n",
        "err shm_read: negative offset or length (0, -1)\n",
        "err unix_send: 29 descriptors in one message (at most 28)\n",
        "take 0\nok false\n",
    ] {
        assert!(out.contains(want), "missing {want:?} in:\n{out}");
    }
}
