// process extern — Rust native implementations

// #2090: a host call that fails at the syscall names ITSELF and the operand
// that identifies it, then the platform's own text verbatim as a SUFFIX:
//
//     process.exec("ghh-not-a-binary"): No such file or directory (os error 2)
//
// The suffix is load-bearing. A reader who recognises `(os error 2)` keeps
// recognising it, and anything classifying on the errno tail keeps working.
// `fs.rs` carries the twin of this pair (its `io_err`), as does the embedded
// wasm host — per-file copies are the pattern here because each runtime file is
// an independently embedded chunk.
fn call_err(call: &str, args: &str, e: impl std::fmt::Display) -> String {
    format!("{call}({args}): {e}")
}
/// Source-shaped quoting, so the operand reads back as the writer spelled it.
// Name-spaced like `fs.rs`'s twin (both chunks land in ONE generated crate),
// and rendering the SAME way: plain quotes, no escape table. `fs` has to
// reproduce its quoting in hand-written WAT, so the family uses the rule the
// hardest leg can honour rather than two rules that agree only on easy input.
fn proc_q(s: &str) -> String { format!("\"{s}\"") }

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

pub fn almide_rt_process_exec(cmd: &str, args: &[String]) -> Result<String, String> {
    almide_stdout_flush();
    match std::process::Command::new(cmd).args(args).output() {
        Ok(out) => {
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                if stderr.is_empty() {
                    Err(format!("process '{}' exited with status {}", cmd, out.status))
                } else {
                    Err(stderr)
                }
            }
        }
        // The SPAWN failure (#2090). The branches above already name the
        // command when the process RAN and failed; "could not start it" said
        // errno and nothing else, so `process.exec("nope")` and a missing file
        // were byte-identical.
        Err(e) => Err(call_err("process.exec", &proc_q(cmd), e)),
    }
}

pub fn almide_rt_process_exit(code: i64) -> ! {
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
    match std::process::Command::new(cmd).args(args).current_dir(dir).output() {
        Ok(out) => {
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).to_string())
            }
        }
        // A bad `dir` fails here too, so both operands are named.
        Err(e) => Err(call_err(
            "process.exec_in",
            &format!("{}, {}", proc_q(dir), proc_q(cmd)),
            e,
        )),
    }
}

pub fn almide_rt_process_exec_with_stdin(cmd: &str, args: &[String], input: &str) -> Result<String, String> {
    almide_stdout_flush();
    use std::io::Write;
    let mut child = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        // The pipe write and the wait are INNER steps of this one call, so they
        // report the call the writer made — naming `write_all` or
        // `wait_with_output`, which they never invoked, would be worse than
        // today's bare errno.
        .map_err(|e| call_err("process.exec_with_stdin", &proc_q(cmd), e))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| call_err("process.exec_with_stdin", &proc_q(cmd), e))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| call_err("process.exec_with_stdin", &proc_q(cmd), e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

pub fn almide_rt_process_exec_status(cmd: &str, args: &[String]) -> Result<AlmideProcessStatus, String> {
    almide_stdout_flush();
    match std::process::Command::new(cmd).args(args).output() {
        Ok(out) => {
            let code = out.status.code().unwrap_or(-1) as i64;
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            Ok(AlmideProcessStatus { code, stdout, stderr })
        }
        // C-214: quote the command, omit arguments, retain the host error.
        Err(e) => Err(call_err("process.exec_status", &format!("{cmd:?}"), e)),
    }
}

/// #1040: the timeout twin of `exec_status` — the ONE place a timeout is
/// admissible: spawning a process is already outside the byte-identity
/// contract, so bounding it adds no nondeterminism the spawn did not.
/// The contract's two halves (C-214): IF the deadline fires, the error value
/// is exactly `exec timed out after <ms>ms` and the child is killed; WHETHER
/// it fires is a function of the host and is not promised.
///
/// stdout/stderr are drained on READER THREADS so a chatty child can never
/// deadlock against a full pipe while the parent polls `try_wait`.
///
/// The child runs in its OWN process group (so a timeout can stop its whole
/// tree, #2065) — which makes it a BACKGROUND job for the controlling
/// terminal. A child that changes terminal settings (`stty -echo`) or reads
/// the terminal is stopped by the kernel with SIGTTOU / SIGTTIN; that stop
/// is detected and answered at once with an error naming
/// `process.exec_attached` (#2540), instead of waiting out the deadline.
pub fn almide_rt_process_exec_status_timeout(
    cmd: &str,
    args: &[String],
    timeout_ms: i64,
) -> Result<AlmideProcessStatus, String> {
    almide_stdout_flush();
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut command = Command::new(cmd);
    command.args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()
        .map_err(|e| call_err("process.exec_status_timeout", &format!("{cmd:?}, {timeout_ms}"), e))?;
    fn drain<R: Read + Send + 'static>(r: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            buf
        })
    }
    let out_h = drain(child.stdout.take());
    let err_h = drain(child.stderr.take());
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(timeout_ms.max(0) as u64);
    let mut exit_status = None;
    let status = loop {
        if exit_status.is_none() {
            match almide_process_poll_child(&mut child) {
                Ok(AlmideChildPoll::Running) => {}
                Ok(AlmideChildPoll::Exited(status)) => exit_status = Some(status),
                Ok(AlmideChildPoll::TerminalStop(sig)) => {
                    almide_process_stop_tree(&mut child);
                    return Err(format!(
                        "process.exec_status_timeout({cmd:?}, {timeout_ms}): the child was stopped by {sig}: \
                         it tried to use the terminal from a background process group; \
                         run terminal programs with process.exec_attached"
                    ));
                }
                Err(e) => {
                    almide_process_stop_tree(&mut child);
                    return Err(call_err("process.exec_status_timeout", &format!("{cmd:?}, {timeout_ms}"), e));
                }
            }
        }
        // #2065: child exit does not imply pipe EOF: descendants may still
        // hold either pipe. The same deadline covers BOTH process and drains.
        if let Some(status) = exit_status {
            if out_h.is_finished() && err_h.is_finished() { break status; }
        }
        if std::time::Instant::now() >= deadline {
            almide_process_stop_tree(&mut child);
            // Never join an unfinished reader on the error path. Even a
            // descendant that escaped the group must not extend the deadline.
            return Err(format!("exec timed out after {}ms", timeout_ms));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let stdout = String::from_utf8_lossy(&out_h.join().unwrap_or_default()).to_string();
    let stderr = String::from_utf8_lossy(&err_h.join().unwrap_or_default()).to_string();
    Ok(AlmideProcessStatus { code: status.code().unwrap_or(-1) as i64, stdout, stderr })
}

enum AlmideChildPoll {
    Running,
    Exited(std::process::ExitStatus),
    /// Stopped by SIGTTOU / SIGTTIN (the signal's name): a background-group
    /// child touched the controlling terminal and will never resume on its own.
    TerminalStop(&'static str),
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

/// The TERMINAL-ATTACHED run (#2540): the child inherits this program's
/// stdin, stdout and stderr and stays in its process group, so it is a
/// foreground job whenever this program is — pagers, editors, `stty` and
/// anything else that reads or reconfigures the terminal work. Nothing is
/// captured; the answer is the exit code (-1 if killed by a signal).
pub fn almide_rt_process_exec_attached(cmd: &str, args: &[String]) -> Result<i64, String> {
    almide_stdout_flush();
    match std::process::Command::new(cmd).args(args).status() {
        Ok(status) => Ok(status.code().unwrap_or(-1) as i64),
        // C-214's error family: quote the command, omit arguments, keep the host error.
        Err(e) => Err(call_err("process.exec_attached", &format!("{cmd:?}"), e)),
    }
}

fn almide_process_stop_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // process_group(0) above gives this invocation its own group. An
        // absolute tool path avoids depending on the executed command's PATH.
        let _ = std::process::Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn almide_rt_process_pid() -> i64 {
    std::process::id() as i64
}

pub fn almide_rt_process_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

// The children this process spawned and has not yet reaped, by pid (#2494).
// `spawn` used to drop the `Child` handle, so an exited child stayed a zombie
// for the parent's whole life, and `is_alive` — a `kill -0`, which a zombie
// answers — reported it alive forever, before and after `process.kill`. The
// handle is kept here; `is_alive` polls it with `try_wait`, which reaps an
// exited child and answers the truth. Process-wide (not thread-local): a fan
// sibling may ask about a child the main thread spawned.
fn almide_spawned_children() -> &'static std::sync::Mutex<std::collections::HashMap<u32, std::process::Child>> {
    static CHILDREN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u32, std::process::Child>>> =
        std::sync::OnceLock::new();
    CHILDREN.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

pub fn almide_rt_process_spawn(cmd: &str, args: &[String]) -> Result<i64, String> {
    let child = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .spawn()
        // Was `spawn '{cmd}' failed: {e}` — it named the command, in a spelling
        // shared with nothing else (#2090). Nothing pinned it.
        .map_err(|e| call_err("process.spawn", &proc_q(cmd), e))?;
    let pid = child.id();
    let mut children = almide_spawned_children().lock().unwrap_or_else(|e| e.into_inner());
    // Reap the children that have exited since the last look, so a program
    // that spawns without ever asking is_alive does not accumulate zombies
    // (each `try_wait` is one non-blocking waitpid). The kernel may then
    // reuse a reaped pid; a later is_alive on that pid answers for whatever
    // the OS runs under it, exactly as it would for a pid we never spawned.
    children.retain(|_, c| matches!(c.try_wait(), Ok(None)));
    children.insert(pid, child);
    Ok(pid as i64)
}

pub fn almide_rt_process_kill(pid: i64, signal: i64) -> Result<(), String> {
    #[cfg(unix)]
    {
        let cmd = std::process::Command::new("kill")
            .args([&format!("-{}", signal), &pid.to_string()])
            .output()
            .map_err(|e| call_err("process.kill", &format!("{pid}, {signal}"), e))?;
        if cmd.status.success() { Ok(()) }
        else { Err(String::from_utf8_lossy(&cmd.stderr).trim().to_string()) }
    }
    #[cfg(windows)]
    {
        let cmd = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()
            .map_err(|e| call_err("process.kill", &format!("{pid}, {signal}"), e))?;
        if cmd.status.success() { Ok(()) }
        else { Err(String::from_utf8_lossy(&cmd.stderr).trim().to_string()) }
    }
}

pub fn almide_rt_process_sleep(ms: i64) {
    std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
}

pub fn almide_rt_process_is_alive(pid: i64) -> bool {
    // A child of ours: the handle is the oracle. `try_wait` reaps an exited
    // child (the zombie disappears with its handle) and answers false; a
    // still-running child answers true. A pid we did not spawn falls through
    // to the platform query below.
    if let Ok(pid32) = u32::try_from(pid) {
        let mut children = almide_spawned_children().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(child) = children.get_mut(&pid32) {
            return match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => {
                    children.remove(&pid32);
                    false
                }
                // waitpid itself failed (ECHILD: someone else reaped it, or
                // it was never ours after all) — nothing left to hold.
                Err(_) => {
                    children.remove(&pid32);
                    false
                }
            };
        }
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH"])
            .output()
            .map(|out| {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.contains(&pid.to_string())
            })
            .unwrap_or(false)
    }
}
