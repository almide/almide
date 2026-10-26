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
// `eprintln` + `process.exit(1)`. Then the exit-time teardown main's return
// runs too (#4214): every `process.start` child neither waited on nor stopped
// is stopped before the exit status is reported.
pub fn almide_rt_process_exit(code: i64) -> ! {
    almide_fan_trap_wait();
    almide_at_exit_run();
    almide_stdout_flush();
    std::process::exit(code as i32);
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


fn almide_process_status((code, stdout, stderr): (i64, String, String)) -> AlmideProcessStatus {
    AlmideProcessStatus { code, stdout: AlmideString::from(stdout), stderr: AlmideString::from(stderr) }
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



pub fn almide_rt_process_pid() -> i64 {
    std::process::id() as i64
}



pub fn almide_rt_process_kill(pid: i64, signal: i64) -> Result<(), AlmideString> { let __almide_r: Result<(), String> = (|| { almide_proc_kill(pid, signal, almide_process_signal) })(); __almide_r.map_err(Into::into) }

/// `process.output` (R-process-02): the core's run with native's stop probe,
/// SIGPIPE ignored while the stdin text is written (ALS-R5).
pub fn almide_rt_process___os_output(cmd: &str, args: &[AlmideString], dir: &str, stdin: &str, timeout_ms: i64) -> Result<AlmideProcessStatus, AlmideString> {
    let __almide_r: Result<AlmideProcessStatus, String> = (|| {
        almide_stdout_flush();
        let _quiet = AlmideSigpipeIgnored::new();
        almide_proc_output(cmd, args, dir, stdin, timeout_ms, almide_process_poll_child).map(almide_process_status)
    })();
    __almide_r.map_err(Into::into)
}

/// The core's signal syscall (#4125): kill(2) itself (sys.rs), never a `kill`
/// looked up on PATH; Windows has none and uses the core's System32 tools.
#[cfg(unix)]
fn almide_process_signal(pid: i32, sig: i32) -> Result<(), i32> {
    almide_rt_sys_ffi::send_signal(pid, sig)
}
#[cfg(windows)]
fn almide_process_signal(pid: i32, sig: i32) -> Result<(), i32> {
    almide_proc_signal_windows(pid, sig)
}

/// `process.run` (R-process-07): attached to this terminal.
pub fn almide_rt_process___os_run(cmd: &str, args: &[AlmideString], dir: &str, stdin: &str, timeout_ms: i64) -> Result<i64, AlmideString> {
    let __almide_r: Result<i64, String> = (|| {
        almide_stdout_flush();
        let _quiet = AlmideSigpipeIgnored::new();
        almide_proc_run_cmd(cmd, args, dir, stdin, timeout_ms)
    })();
    __almide_r.map_err(Into::into)
}

/// `process.start` (R-process-03): the handle of a child that runs on.
pub fn almide_rt_process___os_start(cmd: &str, args: &[AlmideString], dir: &str, stdin: &str, timeout_ms: i64) -> Result<i64, AlmideString> {
    let __almide_r: Result<i64, String> = (|| {
        almide_stdout_flush();
        // #4214: from the first start on, every exit route stops the children
        // still running (the prelude's one exit-time slot, set once).
        let _ = ALMIDE_AT_EXIT.set(almide_proc_stop_all_unwaited);
        let _quiet = AlmideSigpipeIgnored::new();
        almide_proc_start(cmd, args, dir, stdin, timeout_ms)
    })();
    __almide_r.map_err(Into::into)
}

/// Whether the child has ended (its outcome is then `__os_wait`'s at once).
pub fn almide_rt_process___os_poll(h: i64) -> Result<bool, AlmideString> {
    let __almide_r: Result<bool, String> = almide_proc_with(h, |c| c.step(almide_process_poll_child).is_some());
    __almide_r.map_err(Into::into)
}

pub fn almide_rt_process___os_wait(h: i64) -> Result<AlmideProcessStatus, AlmideString> {
    let __almide_r: Result<AlmideProcessStatus, String> =
        almide_proc_with(h, |c| c.wait(almide_process_poll_child)).and_then(|r| r).map(almide_process_status);
    __almide_r.map_err(Into::into)
}

pub fn almide_rt_process___os_stop(h: i64, grace_ms: i64) -> Result<AlmideProcessStatus, AlmideString> {
    let __almide_r: Result<AlmideProcessStatus, String> =
        almide_proc_with(h, |c| c.stop(grace_ms)).and_then(|r| r).map(almide_process_status);
    __almide_r.map_err(Into::into)
}

/// The text the child wrote to stdout (`which` 1) or stderr (2) since the last read.
pub fn almide_rt_process___os_read_new(h: i64, which: i64) -> AlmideString {
    AlmideString::from(almide_proc_with(h, |c| c.read_new(which == 2)).unwrap_or_default())
}

pub fn almide_rt_process___os_child_pid(h: i64) -> i64 {
    almide_proc_with(h, |c| c.pid()).unwrap_or(-1)
}




