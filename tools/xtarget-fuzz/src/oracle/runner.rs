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

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
}

impl ProcResult {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out && !self.spawn_failed
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
    /// and enforce the timeout by polling, killing on overrun.
    fn spawn_timed(&self, mut cmd: Command) -> ProcResult {
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        // Make the child its own process-GROUP leader, so a timeout kill can
        // signal the whole tree. `almide run` executes the built program as a
        // GRANDCHILD: killing only the parent orphaned a hung program, which
        // kept the stdout pipe open — the reader threads below never saw EOF,
        // wedging this worker AND the campaign's final join — and leaked the
        // process forever.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return ProcResult {
                    stdout: Vec::new(),
                    stderr: format!("spawn failed: {e}").into_bytes(),
                    exit_code: None,
                    timed_out: false,
                    spawn_failed: true,
                };
            }
        };

        // Drain both pipes concurrently on threads. A blocked writer in
        // the child (full 64 KiB pipe buffer) would otherwise deadlock
        // the timeout poll — these readers keep the pipes flowing.
        let out_pipe = child.stdout.take();
        let err_pipe = child.stderr.take();
        let out_handle = out_pipe.map(|mut o| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = o.read_to_end(&mut buf);
                buf
            })
        });
        let err_handle = err_pipe.map(|mut e| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = e.read_to_end(&mut buf);
                buf
            })
        });

        let deadline = Instant::now() + self.timeout;
        let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

        let timed_out = loop {
            match child.try_wait() {
                Ok(Some(_status)) => break false,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        // Kill the whole process GROUP (pgid == child pid via
                        // process_group(0) above): the grandchild program dies
                        // with the parent, closing every pipe writer.
                        #[cfg(unix)]
                        unsafe {
                            libc::kill(-(child.id() as i32), libc::SIGKILL);
                        }
                        let _ = child.kill();
                        let _ = child.wait();
                        break true;
                    }
                    std::thread::sleep(poll_interval);
                }
                Err(_) => break false,
            }
        };

        // Join the readers (they finish once the child's pipe ends close,
        // which the kill above guarantees).
        let stdout = out_handle.and_then(|h| h.join().ok()).unwrap_or_default();
        let stderr = err_handle.and_then(|h| h.join().ok()).unwrap_or_default();
        let exit_code = child.wait().ok().and_then(|s| s.code());

        ProcResult {
            stdout,
            stderr,
            exit_code,
            timed_out,
            spawn_failed: false,
        }
    }
}

/// Polling granularity while waiting on a child. Small enough that a
/// hung program is killed promptly, large enough not to busy-spin.
const POLL_INTERVAL_MS: u64 = 10;
