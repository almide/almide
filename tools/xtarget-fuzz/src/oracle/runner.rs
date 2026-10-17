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
    /// Wall clock the child actually consumed (up to the kill on a
    /// timeout). The hang-vs-slow split (#1235) reports this, so a Slow
    /// finding carries its measured time instead of just "over budget".
    pub duration: Duration,
    /// The sandbox tree after the run ([`super::sandbox::snapshot`]), for a
    /// leg that ran in one; `None` for a build or check.
    pub tree: Option<String>,
}

impl ProcResult {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out && !self.spawn_failed
    }
}

/// The first bytes of a wasm "artifact" that is a pointer to its source,
/// for the embedded host to run (`build_wasm`).
const EMBEDDED_LEG: &str = "almide-embedded-leg:";

/// Address-space ceiling for a fuzzed program's NATIVE run: wasm32's own
/// linear-memory ceiling, so both legs stop at the same size (#2611).
pub const NATIVE_MEMORY_CAP_BYTES: u64 = 4 << 30;

/// Did this native run stop because it hit [`NATIVE_MEMORY_CAP_BYTES`]? Rust's
/// allocation-error handler prints `memory allocation of N bytes failed` and
/// aborts; that line is how the cap is recognised.
pub fn native_hit_memory_cap(run: &ProcResult) -> bool {
    let stderr = String::from_utf8_lossy(&run.stderr);
    stderr.contains("memory allocation of ") && stderr.contains(" bytes failed")
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
    /// The live-heap rung: after a clean run, re-run the program on the
    /// embedded wasm host with the allocation counters armed and report
    /// the blocks still live at exit (`FindingKind::LeakAtExit`). On by
    /// default; `--no-leak-check` turns it off.
    pub leak_check: bool,
}

impl Toolchain {
    /// This toolchain with a different per-run budget. The hang-vs-slow
    /// confirm re-run (#1235) is the only caller.
    pub fn with_timeout(&self, timeout: Duration) -> Toolchain {
        Toolchain {
            timeout,
            ..self.clone()
        }
    }

    /// Drop the per-program artifacts `almide build` leaves in this worker's
    /// build dir (#2611). `almide build` keeps every program's binary as
    /// `target/<profile>/almide-<hash>` (and, where rustc keeps them, its
    /// `deps/almide_out-*` objects) as a content-keyed cache that only a
    /// week-old sweep evicts. That cache pays off for a user's edit loop, but
    /// every fuzzed program is distinct, so here it never hits and only
    /// grows: one kept binary per program, for the whole campaign, on the
    /// runner's disk. The ladder runs the `-o` copy, never the cached one.
    ///
    /// Called between programs, from the worker thread that owns this dir:
    /// no build can be running in it. The lockfile, cargo's own metadata and
    /// every directory (the dependency build cache, the incremental store)
    /// stay, so the next build is exactly as warm as before.
    pub fn prune_build_cache(&self) -> usize {
        let mut removed = 0;
        for profile in ["debug", "release"] {
            let dir = self.scratch.join("target").join(profile);
            for (sub, prefix) in [(dir.clone(), "almide-"), (dir.join("deps"), "almide_out-")] {
                let Ok(entries) = std::fs::read_dir(&sub) else { continue };
                for e in entries.flatten() {
                    let is_file = e.file_type().map(|t| t.is_file()).unwrap_or(false);
                    if is_file && e.file_name().to_string_lossy().starts_with(prefix) && std::fs::remove_file(e.path()).is_ok() {
                        removed += 1;
                    }
                }
            }
        }
        removed
    }

    /// `almide check <file>` — type-check only.
    pub fn check(&self, file: &Path) -> ProcResult {
        self.run_almide(&["check", &file.to_string_lossy()])
    }

    /// `almide build <file> --target wasm -o <out>` — direct WASM emit.
    pub fn build_wasm(&self, file: &Path, out: &Path) -> ProcResult {
        // A program only the embedded host can serve (ADR-0027 D1): its
        // "artifact" names the checked source, and `run_wasm` runs it there.
        let src = std::fs::read_to_string(file).unwrap_or_default();
        if crate::generator::needs_embedded(&src) {
            let checked = self.run_almide(&["check", &file.to_string_lossy(), "--target", "wasm"]);
            if checked.success() {
                let _ = std::fs::write(out, format!("{EMBEDDED_LEG}{}", absolute(file).display()));
            }
            return checked;
        }
        self.run_almide(&[
            "build",
            &file.to_string_lossy(),
            "--target",
            "wasm",
            "-o",
            &out.to_string_lossy(),
        ])
    }

    /// `almide build <file> -o <out>` — native compile ONLY (cargo-backed).
    ///
    /// The native leg is built and executed as TWO timed steps, not one
    /// `almide run`. Under one budget, rustc's compile time was
    /// indistinguishable from the program's run time: on a loaded machine the
    /// compile alone blew the per-program timeout, the (cheap, rustc-free)
    /// wasm leg finished cleanly, and the ladder minted a phantom
    /// "native run hung while wasm succeeded" finding — a nightly-red class
    /// with no program divergence in it (seed 1785159097100061000 index 1206:
    /// 16s on an idle machine, nearly all of it rustc). A BUILD timeout is a
    /// toolchain resource event and skips; only the program's OWN wall-clock
    /// is hang evidence.
    pub fn build_native(&self, file: &Path, out: &Path) -> ProcResult {
        self.run_almide(&[
            "build",
            &file.to_string_lossy(),
            "-o",
            &out.to_string_lossy(),
        ])
    }

    /// Execute the native binary `build_native` produced. This is exactly the
    /// grandchild `almide run` used to exec — same stdout, same abort forms,
    /// same exit code — with the timeout now covering ONLY the program.
    pub fn run_native_bin(&self, bin: &Path) -> ProcResult {
        let bin = absolute(bin);
        self.run_sandboxed(|_| {
            let mut cmd = Command::new(&bin);
            cmd.args(super::sandbox::ARGS);
            cmd.env("NO_COLOR", "1");
            native_memory_cap(&mut cmd);
            cmd
        })
    }
}

/// The native address-space cap, applied before exec.
fn native_memory_cap(cmd: &mut Command) {
    // The address-space cap (#2611). A corpus mutation can raise a loop
    // bound to 4294967295 around an allocation (seed 578090231173 index
    // 4323: a fresh Map pushed per iteration). Native then allocates at
    // GB/s for the whole 30 s budget, well past the runner's 16 GB, and the
    // hosted runner is shut down ("The runner has received a shutdown
    // signal", exit 143) with the shard's campaign and upload lost. That
    // was the shard-loss mechanism on the aarch64 runners, measured with
    // the in-step sampler: mem_used went from ~1.3 GB to 13.5-15.6 GB with
    // mem_avail at 297-343 MB in the last sample before every kill.
    // The wasm leg already has this ceiling (wasm32's 4 GiB), so the cap
    // gives native the same one, and a program that reaches it is skipped
    // by the ladder (`native_hit_memory_cap`), never compared.
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            let lim = libc::rlimit {
                rlim_cur: NATIVE_MEMORY_CAP_BYTES as libc::rlim_t,
                rlim_max: NATIVE_MEMORY_CAP_BYTES as libc::rlim_t,
            };
            // Best effort: an OS that refuses the cap runs the program
            // uncapped, exactly as before.
            let _ = libc::setrlimit(libc::RLIMIT_AS, &lim);
            Ok(())
        });
    }
}

/// `p` resolved against the current directory: a sandboxed child runs in
/// another cwd, where a relative program or module path would not resolve.
fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf()) }
}

impl Toolchain {
    /// `ALMIDE_WASM_ALLOC_COUNT=1 almide run <file> --target wasm` — the
    /// structural leg on the embedded host with the allocation counters
    /// armed; the `__ALMD_WASM_ALLOC … live=N` line lands on stderr.
    pub fn run_wasm_counted(&self, file: &Path) -> ProcResult {
        let file = absolute(file);
        self.run_sandboxed(|_| {
            let mut cmd = Command::new(&self.almide);
            cmd.args(["run", &file.to_string_lossy(), "--target", "wasm", "--"]);
            cmd.args(super::sandbox::ARGS);
            cmd.env("ALMIDE_RUN_PROJECT_DIR", &self.scratch);
            cmd.env("NO_COLOR", "1");
            cmd.env("ALMIDE_WASM_ALLOC_COUNT", "1");
            cmd
        })
    }

    /// `wasmtime <wasm>` — execute the WASM build.
    pub fn run_wasm(&self, wasm: &Path) -> ProcResult {
        let wasm = absolute(wasm);
        if let Some(src) = std::fs::read(&wasm).ok().and_then(|b| String::from_utf8(b).ok()).and_then(|s| s.strip_prefix(EMBEDDED_LEG).map(String::from)) {
            // Absolute: the run's cwd is its sandbox.
            let almide = absolute(&self.almide);
            return self.run_sandboxed(|_| {
                let mut cmd = Command::new(&almide);
                cmd.args(["run", &src, "--target", "wasm", "--"]);
                cmd.args(super::sandbox::ARGS);
                cmd.env("ALMIDE_RUN_PROJECT_DIR", &self.scratch);
                cmd.env("NO_COLOR", "1");
                cmd
            });
        }
        self.run_sandboxed(|dir| {
            let mut cmd = Command::new(&self.wasmtime);
            cmd.arg(format!("--dir={}::.", dir.display()));
            cmd.arg(format!("--dir={}::/tmp", super::sandbox::tmp_of(dir).display()));
            for (k, v) in super::sandbox::ENV {
                cmd.arg(format!("--env={k}={v}"));
            }
            cmd.arg(&wasm);
            cmd.args(super::sandbox::ARGS);
            cmd
        })
    }

    /// Spawn `almide` with the given args under this worker's isolated
    /// scratch dir.
    fn run_almide(&self, args: &[&str]) -> ProcResult {
        let mut cmd = Command::new(&self.almide);
        cmd.args(args);
        // A project program (the shape family, #3309) is `<root>/src/x.almd`
        // under a `<root>/almide.toml`: almide resolves the manifest — and
        // its path dependencies — from the working directory, so run there.
        // A loose single file (no `src/` + manifest pair) runs as before.
        if let Some(root) = args.iter().find(|a| a.ends_with(".almd")).and_then(|f| project_root(Path::new(f))) {
            cmd.current_dir(root);
        }
        cmd.env("ALMIDE_RUN_PROJECT_DIR", &self.scratch);
        // Force deterministic, colourless diagnostics so captured stderr
        // is comparable and free of ANSI codes.
        cmd.env("NO_COLOR", "1");
        self.spawn_timed(cmd)
    }

    /// Spawn a command, capture stdout/stderr on dedicated reader
    /// threads (so a child that fills a pipe buffer never deadlocks),
    /// and enforce the timeout by polling, killing on overrun.
    fn spawn_timed(&self, cmd: Command) -> ProcResult {
        self.spawn_timed_with(cmd, None)
    }

    /// [`Self::spawn_timed`] with stdin read from `stdin` (null when `None`).
    fn spawn_timed_with(&self, mut cmd: Command, stdin: Option<std::fs::File>) -> ProcResult {
        cmd.stdin(stdin.map(Stdio::from).unwrap_or_else(Stdio::null));
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

        let started = Instant::now();
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return ProcResult {
                    stdout: Vec::new(),
                    stderr: format!("spawn failed: {e}").into_bytes(),
                    exit_code: None,
                    timed_out: false,
                    spawn_failed: true,
                    duration: Duration::ZERO,
                    tree: None,
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

        let deadline = started + self.timeout;
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

        // The child is done (or killed) HERE — measure before the reader
        // joins, which cost nothing extra but are not the program.
        let duration = started.elapsed();

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
            duration,
            tree: None,
        }
    }

    /// Run `cmd` in a fresh sandbox (docs/roadmap/active/effect-inspection-harness.md,
    /// tier E1): cwd = the seeded tree, the fixed env, stdin from the fixed
    /// file. `build` makes the command for the sandbox it is given and places
    /// the program arguments itself. The tree after the run is kept.
    fn run_sandboxed(&self, build: impl FnOnce(&Path) -> Command) -> ProcResult {
        let Ok((dir, stdin)) = super::sandbox::create(&self.scratch) else {
            return self.spawn_timed(build(&self.scratch));
        };
        let mut cmd = build(&dir);
        cmd.current_dir(&dir);
        for (k, v) in super::sandbox::ENV {
            cmd.env(k, v);
        }
        cmd.env("TMPDIR", super::sandbox::tmp_of(&dir));
        let mut res = self.spawn_timed_with(cmd, std::fs::File::open(&stdin).ok());
        res.tree = Some(super::sandbox::snapshot(&dir));
        super::sandbox::remove(&dir, &stdin);
        res
    }
}

/// `<root>` when `file` is `<root>/src/<name>.almd` and `<root>/almide.toml`
/// exists — the layout of a generated project.
fn project_root(file: &Path) -> Option<PathBuf> {
    let src = file.parent()?;
    if src.file_name()? != "src" {
        return None;
    }
    let root = src.parent()?;
    root.join("almide.toml").is_file().then(|| root.to_path_buf())
}

/// Polling granularity while waiting on a child. Small enough that a
/// hung program is killed promptly, large enough not to busy-spin.
const POLL_INTERVAL_MS: u64 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    fn tc() -> Toolchain {
        Toolchain {
            almide: PathBuf::from("almide"),
            wasmtime: PathBuf::from("wasmtime"),
            scratch: std::env::temp_dir(),
            timeout: Duration::from_secs(10),
            leak_check: false,
        }
    }

    /// #2611: the native run carries the address-space cap. `ulimit -v`
    /// reports RLIMIT_AS in KiB; Linux is where CI runs and where the cap is
    /// enforced (macOS accepts RLIMIT_AS without enforcing it).
    #[cfg(target_os = "linux")]
    #[test]
    fn a_native_run_is_capped_at_the_wasm32_ceiling() {
        let dir = std::env::temp_dir().join(format!("xtarget-cap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("show-limit.sh");
        std::fs::write(&script, "#!/bin/sh\nulimit -v\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let r = tc().run_native_bin(&script);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(String::from_utf8_lossy(&r.stdout).trim(), (NATIVE_MEMORY_CAP_BYTES / 1024).to_string());
    }

    #[test]
    fn the_cap_is_recognised_by_the_allocation_failure_line_only() {
        let run = |stderr: &str| ProcResult {
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
            exit_code: None,
            timed_out: false,
            spawn_failed: false,
            duration: Duration::ZERO,
            tree: None,
        };
        assert!(native_hit_memory_cap(&run("memory allocation of 4294967296 bytes failed\n")));
        assert!(!native_hit_memory_cap(&run("thread 'main' panicked at 'index out of bounds'\n")));
        assert!(!native_hit_memory_cap(&run("")));
    }
}
