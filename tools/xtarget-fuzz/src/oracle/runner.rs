//! Subprocess plumbing for the oracle ladder.
//!
//! Every rung shells out to the freshly built `almide` binary (and, for
//! execution, `wasmtime`) rather than linking the compiler in-process.
//! This is deliberate: a compiler ICE then crashes a *child* process we
//! can observe (non-zero exit, panic on stderr), instead of taking down
//! the fuzzer. It also exercises the exact binary a user runs.
//!
//! Per-worker isolation: each worker owns a distinct scratch directory
//! passed to `almide run`/`build` via `ALMIDE_RUN_PROJECT_DIR`, so the
//! native cargo build caches do not contend on the shared-`/tmp` flock.
//! Workers therefore scale across cores.
//!
//! ## Surviving pathological children
//!
//! Generated programs can hang or emit an unbounded volume of output (a
//! wasm string-codegen bug dumping linear memory, say). The spawn path
//! is hardened against both so a single bad program cannot wedge a worker
//! or OOM the whole campaign — the failure mode behind the nightly run's
//! exit-143 stall:
//!
//! * **Process-group teardown.** `almide run` executes the compiled
//!   native binary with `Command::status()`, which lets that binary
//!   *inherit* our captured stdout pipe. If it loops forever, killing
//!   only the direct `almide` child leaves the grandchild alive holding
//!   the pipe write end — the reader thread then blocks in `read_to_end`
//!   forever. We spawn each child as its own process-group leader and
//!   `kill(-pgid)` the entire tree on timeout, so every pipe write end
//!   closes and the readers drain to EOF.
//! * **Bounded capture.** Each stream is captured up to
//!   [`MAX_CAPTURE_BYTES`]; past the cap the reader flags an overflow and
//!   keeps draining (so the child never blocks on a full pipe) while the
//!   poll loop tears the group down. The captured head is enough to
//!   classify the divergence; the cap keeps memory and the findings
//!   artifact bounded.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Maximum bytes captured per stream. A finite generated program that
/// emits more than this is pathological; we keep the head, flag the
/// overflow, and tear the child down so it cannot OOM the campaign. The
/// cap is far above any legitimate finding's diff (the widest real output
/// — a fully expanded subnormal float — is ~330 bytes), so no genuine
/// divergence is truncated in practice.
const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024; // 4 MiB / stream

/// The captured result of running a child process to completion (or
/// timing it out).
pub struct ProcResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Process exit code; `None` if the process was killed (signal) or
    /// timed out (see `timed_out`).
    pub exit_code: Option<i32>,
    /// `true` if the process exceeded its per-program timeout and was
    /// killed — a hang is itself a finding.
    pub timed_out: bool,
    /// `true` if the binary could not be spawned at all (e.g. `wasmtime`
    /// not installed). The ladder treats this as a *skip*, not a finding.
    pub spawn_failed: bool,
    /// `true` if either stream hit [`MAX_CAPTURE_BYTES`] — the child
    /// emitted a pathological volume of output and was torn down. The
    /// captured bytes are the head of the stream.
    pub output_overflow: bool,
}

impl ProcResult {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
            && !self.timed_out
            && !self.spawn_failed
            && !self.output_overflow
    }

    fn spawn_failure(msg: String) -> Self {
        ProcResult {
            stdout: Vec::new(),
            stderr: msg.into_bytes(),
            exit_code: None,
            timed_out: false,
            spawn_failed: true,
            output_overflow: false,
        }
    }
}

/// Locations of the external tools the ladder drives, plus this worker's
/// isolated scratch directory.
#[derive(Clone)]
pub struct Toolchain {
    /// Path to the freshly built `almide` binary.
    pub almide: PathBuf,
    /// Path to `wasmtime` (for executing the WASM build).
    pub wasmtime: PathBuf,
    /// This worker's isolated build scratch dir (passed as
    /// `ALMIDE_RUN_PROJECT_DIR`), so native cargo builds do not contend.
    pub scratch: PathBuf,
    /// Per-program wall-clock timeout. A program that outruns it is a
    /// hang finding.
    pub timeout: Duration,
}

impl Toolchain {
    /// `almide check <file>` — type-check only.
    pub fn check(&self, file: &Path) -> ProcResult {
        self.run_almide(&["check", &file.to_string_lossy()])
    }

    /// `almide build <file> --target wasm -o <out>` — direct WASM emit.
    pub fn build_wasm(&self, file: &Path, out: &Path) -> ProcResult {
        self.run_almide(&[
            "build",
            &file.to_string_lossy(),
            "--target",
            "wasm",
            "-o",
            &out.to_string_lossy(),
        ])
    }

    /// `almide run <file>` — native compile + execute (cargo-backed).
    pub fn run_native(&self, file: &Path) -> ProcResult {
        self.run_almide(&["run", &file.to_string_lossy()])
    }

    /// `wasmtime <wasm>` — execute the WASM build.
    pub fn run_wasm(&self, wasm: &Path) -> ProcResult {
        let mut cmd = Command::new(&self.wasmtime);
        cmd.arg(wasm);
        self.spawn_timed(cmd)
    }

    /// Spawn `almide` with the given args under this worker's isolated
    /// scratch dir.
    fn run_almide(&self, args: &[&str]) -> ProcResult {
        let mut cmd = Command::new(&self.almide);
        cmd.args(args);
        cmd.env("ALMIDE_RUN_PROJECT_DIR", &self.scratch);
        // Force deterministic, colourless diagnostics so captured stderr
        // is comparable and free of ANSI codes.
        cmd.env("NO_COLOR", "1");
        self.spawn_timed(cmd)
    }

    /// Spawn a command, capture stdout/stderr on dedicated reader
    /// threads (so a child that fills a pipe buffer never deadlocks),
    /// and enforce the timeout by polling, tearing the whole process
    /// group down on overrun or output overflow.
    fn spawn_timed(&self, mut cmd: Command) -> ProcResult {
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        // Run the child in its own process group so a timeout can reap
        // the entire tree — including grandchildren that inherited our
        // pipes (see the module docs).
        set_own_process_group(&mut cmd);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return ProcResult::spawn_failure(format!("spawn failed: {e}")),
        };

        // Drain both pipes concurrently on threads. A blocked writer in
        // the child (full 64 KiB pipe buffer) would otherwise deadlock
        // the timeout poll — these readers keep the pipes flowing. The
        // capture is capped; an overflow is surfaced so the poll loop can
        // tear the child down promptly.
        let out_overflow = Arc::new(AtomicBool::new(false));
        let err_overflow = Arc::new(AtomicBool::new(false));
        let out_handle = child.stdout.take().map(|p| {
            let flag = Arc::clone(&out_overflow);
            std::thread::spawn(move || drain_capped(p, &flag))
        });
        let err_handle = child.stderr.take().map(|p| {
            let flag = Arc::clone(&err_overflow);
            std::thread::spawn(move || drain_capped(p, &flag))
        });

        let deadline = Instant::now() + self.timeout;
        let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

        let mut timed_out = false;
        let mut overflowed = false;
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => break,
                Ok(None) => {
                    if out_overflow.load(Ordering::Relaxed)
                        || err_overflow.load(Ordering::Relaxed)
                    {
                        overflowed = true;
                        kill_group(&mut child);
                        break;
                    }
                    if Instant::now() >= deadline {
                        timed_out = true;
                        kill_group(&mut child);
                        break;
                    }
                    std::thread::sleep(poll_interval);
                }
                Err(_) => break,
            }
        }

        // Join the readers. The group kill above closes every inherited
        // pipe write end, so `read` returns EOF and the threads finish
        // even when a grandchild was holding the pipe.
        let stdout = out_handle.and_then(|h| h.join().ok()).unwrap_or_default();
        let stderr = err_handle.and_then(|h| h.join().ok()).unwrap_or_default();
        let exit_code = child.wait().ok().and_then(|s| s.code());

        ProcResult {
            stdout,
            stderr,
            exit_code,
            timed_out,
            spawn_failed: false,
            output_overflow: overflowed,
        }
    }
}

/// Read a pipe into a buffer capped at [`MAX_CAPTURE_BYTES`]. Past the
/// cap the bytes are discarded (so the child never blocks on a full pipe)
/// and `overflow` is set, signalling the poll loop to tear the child
/// down. Returns the captured head.
fn drain_capped(mut pipe: impl Read, overflow: &AtomicBool) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break, // EOF
            Ok(n) => {
                if buf.len() < MAX_CAPTURE_BYTES {
                    let room = MAX_CAPTURE_BYTES - buf.len();
                    buf.extend_from_slice(&chunk[..n.min(room)]);
                    if buf.len() >= MAX_CAPTURE_BYTES {
                        overflow.store(true, Ordering::Relaxed);
                    }
                }
                // Past the cap: keep reading to drain the pipe (so the
                // child does not block) but discard. The poll loop kills
                // the group on seeing the overflow flag, which ends this.
            }
            Err(_) => break,
        }
    }
    buf
}

/// Mark `cmd` to spawn in a fresh process group (its PID becomes the
/// PGID), so `kill(-pgid)` later reaps the whole subtree.
#[cfg(unix)]
fn set_own_process_group(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

#[cfg(not(unix))]
fn set_own_process_group(_cmd: &mut Command) {}

/// Kill the child's entire process group (the child plus every
/// grandchild that inherited its group), then reap the direct child.
#[cfg(unix)]
fn kill_group(child: &mut Child) {
    // The child leads its own group (`process_group(0)`), so its PGID
    // equals its PID. SIGKILL to `-PID` hits the whole tree.
    let pid = child.id() as libc::pid_t;
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = child.wait();
}

#[cfg(not(unix))]
fn kill_group(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Polling granularity while waiting on a child. Small enough that a
/// hung program is killed promptly, large enough not to busy-spin.
const POLL_INTERVAL_MS: u64 = 10;

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn toolchain(timeout: Duration) -> Toolchain {
        Toolchain {
            almide: PathBuf::from("/nonexistent"),
            wasmtime: PathBuf::from("/nonexistent"),
            scratch: std::env::temp_dir(),
            timeout,
        }
    }

    /// Regression for the nightly exit-143 stall: a child that spawns a
    /// grandchild which inherits the stdout pipe and then outlives the
    /// parent must NOT wedge the reader. `sh -c 'sleep 30 & ...'` leaves
    /// the `sleep` holding the pipe after the shell waits; group teardown
    /// must kill it so `spawn_timed` returns within the timeout. Without
    /// the `kill(-pgid)`, the reader blocks in `read_to_end` for 30s and
    /// this test would hang past its own deadline.
    #[test]
    fn timeout_reaps_pipe_holding_grandchild() {
        let tc = toolchain(Duration::from_millis(300));
        let mut cmd = Command::new("sh");
        // Background a long sleeper that inherits stdout, print, then the
        // shell blocks on `wait` — so the direct child hangs AND a
        // grandchild keeps the pipe open.
        cmd.args(["-c", "sleep 30 & echo holding; wait"]);

        let start = Instant::now();
        let r = tc.spawn_timed(cmd);
        let elapsed = start.elapsed();

        assert!(r.timed_out, "expected a timeout finding");
        assert!(
            elapsed < Duration::from_secs(5),
            "spawn_timed wedged on a pipe-holding grandchild: took {elapsed:?}"
        );
        assert!(
            String::from_utf8_lossy(&r.stdout).contains("holding"),
            "should still capture the head before teardown"
        );
    }

    /// A child that floods stdout must be capped, flagged, and torn down
    /// rather than buffered without bound (the OOM half of exit-143).
    #[test]
    fn excessive_output_is_capped_and_flagged() {
        let tc = toolchain(Duration::from_secs(20));
        let mut cmd = Command::new("sh");
        // `yes` streams "y\n" forever; cap must trip well before OOM.
        cmd.args(["-c", "yes"]);

        let r = tc.spawn_timed(cmd);

        assert!(r.output_overflow, "expected the capture cap to trip");
        assert!(!r.timed_out, "overflow should fire before the 20s timeout");
        assert!(
            r.stdout.len() <= MAX_CAPTURE_BYTES + 64 * 1024,
            "captured {} bytes, expected ~{}",
            r.stdout.len(),
            MAX_CAPTURE_BYTES
        );
    }

    /// The happy path still works: a quick, well-behaved child is
    /// captured exactly with a zero exit and no false overflow/timeout.
    #[test]
    fn clean_child_is_captured_verbatim() {
        let tc = toolchain(Duration::from_secs(10));
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "printf 'hello\\n'"]);

        let r = tc.spawn_timed(cmd);

        assert!(r.success());
        assert_eq!(r.exit_code, Some(0));
        assert!(!r.timed_out);
        assert!(!r.output_overflow);
        assert_eq!(r.stdout, b"hello\n");
    }
}
