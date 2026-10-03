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
// The quoting twin (`almide_proc_q`, plain quotes, no escape table — the
// rule `fs` can reproduce in its hand-written WAT) lives in the shared core.

// The runtime-side twin of stdlib/process.almd's `type ProcessStatus = { code,
// stdout, stderr }` — the AlmideFileStat treatment (fs.rs, #1821): the emitter
// spells the type under this reserved name and skips the bundled decl, so a
// user's own `type ProcessStatus` keeps the bare spelling. The repr prints the
// Almide-level literal form.
#[derive(Clone, Debug, PartialEq)]
pub struct AlmideProcessStatus {
    pub code: i64,
    pub stdout: String,
    pub stderr: String,
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

pub fn almide_rt_process_exec(cmd: &str, args: &[String]) -> Result<String, String> {
    almide_stdout_flush();
    almide_proc_exec(cmd, args)
}

// Inside a `fan` element the exit waits for the elements below it, exactly as a
// trap does (ADR-0024 D6): a failed assert outside a test lowers to
// `eprintln` + `process.exit(1)`.
pub fn almide_rt_process_exit(code: i64) -> ! {
    almide_fan_trap_wait();
    almide_stdout_flush();
    std::process::exit(code as i32);
}

pub fn almide_rt_process_args() -> Vec<String> {
    std::env::args().collect()
}

pub fn almide_rt_process_stdin_lines() -> Result<Vec<String>, String> {
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
}

pub fn almide_rt_process_exec_in(dir: &str, cmd: &str, args: &[String]) -> Result<String, String> {
    almide_stdout_flush();
    almide_proc_exec_in(dir, cmd, args)
}

pub fn almide_rt_process_exec_with_stdin(cmd: &str, args: &[String], input: &str) -> Result<String, String> {
    almide_stdout_flush();
    almide_proc_exec_with_stdin(cmd, args, input)
}

fn almide_process_status((code, stdout, stderr): (i64, String, String)) -> AlmideProcessStatus {
    AlmideProcessStatus { code, stdout, stderr }
}

pub fn almide_rt_process_exec_status(cmd: &str, args: &[String]) -> Result<AlmideProcessStatus, String> {
    almide_stdout_flush();
    almide_proc_exec_status(cmd, args).map(almide_process_status)
}

/// #1040: the timeout twin of `exec_status` (C-214) — the core's loop with
/// native's stop probe, so a child that touches the terminal from its
/// background group is named at once (#2540) instead of waiting out the
/// deadline.
pub fn almide_rt_process_exec_status_timeout(
    cmd: &str,
    args: &[String],
    timeout_ms: i64,
) -> Result<AlmideProcessStatus, String> {
    almide_stdout_flush();
    almide_proc_exec_status_timeout(cmd, args, timeout_ms, almide_process_poll_child).map(almide_process_status)
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
        extern "C" {
            fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        }
        // POSIX values, identical on Linux and macOS / BSD.
        const WNOHANG: i32 = 1;
        const WUNTRACED: i32 = 2;
        const SIGTTIN: i32 = 21;
        const SIGTTOU: i32 = 22;
        let mut raw: i32 = 0;
        let r = unsafe { waitpid(child.id() as i32, &mut raw, WNOHANG | WUNTRACED) };
        if r == child.id() as i32 {
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

pub fn almide_rt_process_exec_attached(cmd: &str, args: &[String]) -> Result<i64, String> {
    almide_stdout_flush();
    almide_proc_exec_attached(cmd, args)
}

pub fn almide_rt_process_pid() -> i64 {
    std::process::id() as i64
}

pub fn almide_rt_process_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

pub fn almide_rt_process_spawn(cmd: &str, args: &[String]) -> Result<i64, String> {
    almide_proc_spawn(cmd, args)
}

pub fn almide_rt_process_kill(pid: i64, signal: i64) -> Result<(), String> {
    almide_proc_kill(pid, signal)
}

pub fn almide_rt_process_sleep(ms: i64) {
    std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
}

pub fn almide_rt_process_is_alive(pid: i64) -> bool {
    almide_proc_is_alive(pid)
}
