// process extern — Rust native implementations

// #2090: a host call that fails at the syscall names ITSELF and the operand
// that identifies it, then the platform's own text verbatim as a SUFFIX:
//
//     process.exec("ghh-not-a-binary"): No such file or directory (os error 2)
//
// The suffix is load-bearing. A reader who recognises `(os error 2)` keeps
// recognising it, and anything classifying on the errno tail keeps working.
// `fs.rs` carries the twin of this pair (its `io_err`); the subprocess
// family's own copy is `almide_proc_call_err` in the shared core below, which
// the embedded wasm host runs too (#2589).
fn call_err(call: &str, args: &str, e: impl std::fmt::Display) -> String {
    format!("{call}({args}): {e}")
}
// The quoting twin (`almide_proc_q`: quotes plus C-418's control/bidi
// escapes, the rule `fs` reproduces in its hand-written WAT) lives in the
// shared core.

// The runtime-side twin of stdlib/process.almd's `type ProcessStatus = { code,
// stdout, stderr }` — the AlmideFileStat treatment (fs.rs, #1821): the emitter
// spells the type under this reserved name and skips the bundled decl, so a
// user's own `type ProcessStatus` keeps the bare spelling. The repr prints the
// Almide-level literal form.
#[derive(Clone, Debug, PartialEq)]
pub struct AlmideProcessStatus {
    pub code: i64,
    pub stdout: AlmideString,
    pub stderr: AlmideString,
}
impl AlmideRepr for AlmideProcessStatus {
    fn almide_repr(&self) -> String {
        format!(
            "ProcessStatus {{ code: {}, stdout: {}, stderr: {} }}",
            self.code.almide_repr(), self.stdout.almide_repr(), self.stderr.almide_repr()
        )
    }
}

// #2589 (ADR-0025): the subprocess family's bodies live in the shared core,
// which the embedded wasm host links too — so a process call answers the same
// text, code and err on native and on `almide run --target wasm`. These
// wrappers add what is native's alone: the stdout flush before a child runs,
// the `AlmideProcessStatus` record, and the `waitpid` stop probe (`unsafe`,
// which the core may not hold).
include!("../../../crates/almide-rt-core/src/process_core.rs");

pub fn almide_rt_process_exec(cmd: &str, args: &[AlmideString]) -> Result<AlmideString, AlmideString> {
    let __almide_v: Result<String, AlmideString> = {
        let __almide_r: Result<String, String> = (|| {
            almide_stdout_flush();
            almide_proc_exec(cmd, args)
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}

// Inside a `fan` element the exit waits for the elements below it, exactly as a
// trap does (ADR-0024 D6): a failed assert outside a test lowers to
// `eprintln` + `process.exit(1)`.
pub fn almide_rt_process_exit(code: i64) -> ! {
    almide_fan_trap_wait();
    almide_stdout_flush();
    std::process::exit(code as i32);
}

pub fn almide_rt_process_args() -> Vec<AlmideString> { let __almide_v: Vec<String> = { std::env::args().collect() }; __almide_v.into_almide() }

pub fn almide_rt_process_stdin_lines() -> Result<Vec<AlmideString>, AlmideString> {
    let __almide_v: Result<Vec<String>, AlmideString> = {
        let __almide_r: Result<Vec<String>, String> = (|| {
            almide_stdout_flush();
            use std::io::BufRead;
            std::io::stdin()
                .lock()
                .lines()
                .collect::<Result<Vec<String>, _>>()
                // No identifying operand — stdin is the one the writer did not name —
                // so the call alone is what this can add, and it is still the
                // difference between "which of my host calls failed" and errno alone.
                .map_err(|e| call_err("process.stdin_lines", "", e))
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}

pub fn almide_rt_process_exec_in(dir: &str, cmd: &str, args: &[AlmideString]) -> Result<AlmideString, AlmideString> {
    let __almide_v: Result<String, AlmideString> = {
        let __almide_r: Result<String, String> = (|| {
            almide_stdout_flush();
            almide_proc_exec_in(dir, cmd, args)
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}

/// SIGPIPE ignored while it lives, its previous disposition restored after.
/// `main` sets SIGPIPE back to its default (#1950), so writing the input
/// into the pipe of a child that already exited would end the whole program;
/// ignored, the write fails with EPIPE, as on the embedded host (ALS-R5).
struct AlmideSigpipeIgnored {
    #[cfg(unix)]
    previous: almide_rt_sys_ffi::Disposition,
}

impl AlmideSigpipeIgnored {
    fn new() -> Self {
        #[cfg(unix)]
        return AlmideSigpipeIgnored {
            previous: almide_rt_sys_ffi::set_signal(almide_rt_sys_ffi::SIGPIPE, almide_rt_sys_ffi::Disposition::IGNORE),
        };
        #[cfg(not(unix))]
        AlmideSigpipeIgnored {}
    }
}

impl Drop for AlmideSigpipeIgnored {
    fn drop(&mut self) {
        #[cfg(unix)]
        almide_rt_sys_ffi::set_signal(almide_rt_sys_ffi::SIGPIPE, self.previous);
    }
}

pub fn almide_rt_process_exec_with_stdin(cmd: &str, args: &[AlmideString], input: &str) -> Result<AlmideString, AlmideString> {
    let __almide_v: Result<String, AlmideString> = {
        let __almide_r: Result<String, String> = (|| {
            almide_stdout_flush();
            let _quiet = AlmideSigpipeIgnored::new();
            almide_proc_exec_with_stdin(cmd, args, input)
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}

fn almide_process_status((code, stdout, stderr): (i64, String, String)) -> AlmideProcessStatus {
    AlmideProcessStatus { code, stdout: AlmideString::from(stdout), stderr: AlmideString::from(stderr) }
}

pub fn almide_rt_process_exec_status(cmd: &str, args: &[AlmideString]) -> Result<AlmideProcessStatus, AlmideString> {
    let __almide_r: Result<AlmideProcessStatus, String> = (|| {
        almide_stdout_flush();
        almide_proc_exec_status(cmd, args).map(almide_process_status)
    })();
    __almide_r.map_err(Into::into)
}

/// #1040: the timeout twin of `exec_status` (C-214) — the core's loop with
/// native's stop probe, so a child that touches the terminal from its
/// background group is named at once (#2540) instead of waiting out the
/// deadline.
pub fn almide_rt_process_exec_status_timeout(
    cmd: &str,
    args: &[AlmideString],
    timeout_ms: i64,
) -> Result<AlmideProcessStatus, AlmideString> {
    let __almide_r: Result<AlmideProcessStatus, String> = (|| {
        almide_stdout_flush();
        almide_proc_exec_status_timeout(cmd, args, timeout_ms, almide_process_poll_child).map(almide_process_status)
    })();
    __almide_r.map_err(Into::into)
}

/// `try_wait` that ALSO reports a child stopped for touching the terminal
/// (#2540). First the ordinary non-blocking reap; only if the child is still
/// running, a `waitpid(pid, WNOHANG | WUNTRACED)` — which reports a stop
/// without reaping. If the child exits between the two calls that waitpid
/// reaps it, so its raw status is returned as the exit status (the Child
/// handle is then never waited again: the caller breaks out on it).
fn almide_process_poll_child(child: &mut std::process::Child) -> std::io::Result<AlmideChildPoll> {
    if let Some(status) = child.try_wait()? {
        return Ok(AlmideChildPoll::Exited(status));
    }
    #[cfg(unix)]
    {
        // POSIX values, identical on Linux and macOS / BSD.
        const SIGTTIN: i32 = 21;
        const SIGTTOU: i32 = 22;
        if let Some(raw) = almide_rt_sys_ffi::wait_exited_or_stopped(child.id() as i32) {
            // WIFSTOPPED / WSTOPSIG (the same encoding on Linux and macOS).
            if raw & 0xff == 0x7f {
                return Ok(match (raw >> 8) & 0xff {
                    SIGTTOU => AlmideChildPoll::TerminalStop("SIGTTOU"),
                    SIGTTIN => AlmideChildPoll::TerminalStop("SIGTTIN"),
                    _ => AlmideChildPoll::Running,
                });
            }
            use std::os::unix::process::ExitStatusExt;
            return Ok(AlmideChildPoll::Exited(std::process::ExitStatus::from_raw(raw)));
        }
    }
    Ok(AlmideChildPoll::Running)
}

pub fn almide_rt_process_run(cmd: &str, args: &[AlmideString]) -> Result<i64, AlmideString> {
    let __almide_r: Result<i64, String> = (|| {
        almide_stdout_flush();
        almide_proc_run(cmd, args)
    })();
    __almide_r.map_err(Into::into)
}

pub fn almide_rt_process_run_in(dir: &str, cmd: &str, args: &[AlmideString]) -> Result<i64, AlmideString> {
    let __almide_r: Result<i64, String> = (|| {
        almide_stdout_flush();
        almide_proc_run_in(dir, cmd, args)
    })();
    __almide_r.map_err(Into::into)
}

pub fn almide_rt_process_pid() -> i64 {
    std::process::id() as i64
}

pub fn almide_rt_process_env(key: &str) -> Option<AlmideString> { let __almide_v: Option<String> = { std::env::var(key).ok() }; __almide_v.into_almide() }

pub fn almide_rt_process_spawn(cmd: &str, args: &[AlmideString]) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { almide_proc_spawn(cmd, args) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_process_kill(pid: i64, signal: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_proc_kill(pid, signal) })(); __almide_r.map_err(Into::into) }

pub fn almide_rt_process_sleep(ms: i64) {
    std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
}

pub fn almide_rt_process_is_alive(pid: i64) -> bool {
    almide_proc_is_alive(pid)
}
