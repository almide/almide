// process_core (#2589, ADR-0025): the subprocess family, ONE source for two
// consumers —
//
// - runtime/rs/src/process.rs `include!`s this file verbatim, so the native
//   `almide_rt_process_*` intrinsics are thin wrappers (stdout flush + the
//   record type) over these functions;
// - almide-wasm-run links this crate and calls the same functions when the
//   embedded host serves `almide:process/spawn` (host_process.rs).
//
// So every observable a process call produces — the captured text, the exit
// code, the err strings (#2090's `call(operand): host error` form, C-214's
// timeout text) — is the same code on native and on the embedded host.
//
// Splice discipline (lib.rs): no `crate::` paths, no `mod`, no `use` lines,
// no `unsafe` (the workspace forbids it; the native-only `waitpid` stop probe
// stays in process.rs and is passed in as `poll`).

/// `call(operand): host error` — #2090's form for a host call that fails at
/// the syscall (the platform's text kept verbatim as the suffix).
pub fn almide_proc_call_err(call: &str, args: &str, e: impl std::fmt::Display) -> String {
    format!("{call}({args}): {e}")
}

/// Source-shaped quoting (plain quotes, no escape table) — the family's rule.
pub fn almide_proc_q(s: &str) -> String {
    format!("\"{s}\"")
}

/// Stdout on exit 0; else the stderr text, or a status line when it is empty.
pub fn almide_proc_exec(cmd: &str, args: &[String]) -> Result<String, String> {
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
        // The SPAWN failure (#2090): name the command, keep the errno text.
        Err(e) => Err(almide_proc_call_err("process.exec", &almide_proc_q(cmd), e)),
    }
}

/// `exec` in `dir`; a bad `dir` fails at spawn too, so both operands are named.
pub fn almide_proc_exec_in(dir: &str, cmd: &str, args: &[String]) -> Result<String, String> {
    match std::process::Command::new(cmd).args(args).current_dir(dir).output() {
        Ok(out) => {
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).to_string())
            }
        }
        Err(e) => Err(almide_proc_call_err(
            "process.exec_in",
            &format!("{}, {}", almide_proc_q(dir), almide_proc_q(cmd)),
            e,
        )),
    }
}

/// `exec` with `input` piped into the child's stdin. The pipe write and the
/// wait are inner steps of this one call, so they report the call.
pub fn almide_proc_exec_with_stdin(cmd: &str, args: &[String], input: &str) -> Result<String, String> {
    let fail = |e: std::io::Error| almide_proc_call_err("process.exec_with_stdin", &almide_proc_q(cmd), e);
    let mut child = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(fail)?;
    if let Some(stdin) = child.stdin.as_mut() {
        std::io::Write::write_all(stdin, input.as_bytes()).map_err(fail)?;
    }
    let out = child.wait_with_output().map_err(fail)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// `(code, stdout, stderr)`; code -1 when the child was killed by a signal.
pub fn almide_proc_exec_status(cmd: &str, args: &[String]) -> Result<(i64, String, String), String> {
    match std::process::Command::new(cmd).args(args).output() {
        Ok(out) => Ok((
            out.status.code().unwrap_or(-1) as i64,
            String::from_utf8_lossy(&out.stdout).to_string(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )),
        // C-214: quote the command, omit arguments, retain the host error.
        Err(e) => Err(almide_proc_call_err("process.exec_status", &format!("{cmd:?}"), e)),
    }
}

/// One non-blocking look at a running child.
pub enum AlmideChildPoll {
    Running,
    Exited(std::process::ExitStatus),
    /// Stopped by SIGTTOU / SIGTTIN (the signal's name): a background-group
    /// child touched the controlling terminal and will never resume on its own.
    TerminalStop(&'static str),
}

/// The plain poll: `try_wait` only. A consumer that cannot ask the kernel
/// for stops (no `waitpid` without `unsafe`) passes this; a child stopped on
/// the terminal then runs into the deadline instead of being named.
pub fn almide_proc_try_wait_poll(child: &mut std::process::Child) -> std::io::Result<AlmideChildPoll> {
    Ok(match child.try_wait()? {
        Some(status) => AlmideChildPoll::Exited(status),
        None => AlmideChildPoll::Running,
    })
}

/// #1040 / C-214: the timeout twin of `exec_status` — IF the deadline fires,
/// the err is exactly `exec timed out after <ms>ms` and the child's whole
/// process group is killed; WHETHER it fires is a function of the host.
/// stdout/stderr drain on reader threads, so a chatty child cannot deadlock
/// against a full pipe while the parent polls. The child runs in its own
/// process group (#2065); `poll` reports a terminal stop (#2540) where the
/// consumer can see one.
pub fn almide_proc_exec_status_timeout(
    cmd: &str,
    args: &[String],
    timeout_ms: i64,
    poll: fn(&mut std::process::Child) -> std::io::Result<AlmideChildPoll>,
) -> Result<(i64, String, String), String> {
    let mut command = std::process::Command::new(cmd);
    command.args(args).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(unix)]
    {
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
    }
    let operand = format!("{cmd:?}, {timeout_ms}");
    let mut child = command
        .spawn()
        .map_err(|e| almide_proc_call_err("process.exec_status_timeout", &operand, e))?;
    fn drain<R: std::io::Read + Send + 'static>(r: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms.max(0) as u64);
    let mut exit_status = None;
    let status = loop {
        if exit_status.is_none() {
            match poll(&mut child) {
                Ok(AlmideChildPoll::Running) => {}
                Ok(AlmideChildPoll::Exited(status)) => exit_status = Some(status),
                Ok(AlmideChildPoll::TerminalStop(sig)) => {
                    almide_proc_stop_tree(&mut child);
                    return Err(format!(
                        "process.exec_status_timeout({operand}): the child was stopped by {sig}: \
                         it tried to use the terminal from a background process group; \
                         run terminal programs with process.exec_attached"
                    ));
                }
                Err(e) => {
                    almide_proc_stop_tree(&mut child);
                    return Err(almide_proc_call_err("process.exec_status_timeout", &operand, e));
                }
            }
        }
        // #2065: child exit does not imply pipe EOF: descendants may still
        // hold either pipe. The same deadline covers BOTH process and drains.
        // (No let-chain: the native splice compiles under the generated
        // crate's edition, not this one's.)
        if let (Some(status), true) = (exit_status, out_h.is_finished() && err_h.is_finished()) {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            almide_proc_stop_tree(&mut child);
            // Never join an unfinished reader on the error path.
            return Err(format!("exec timed out after {}ms", timeout_ms));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let stdout = String::from_utf8_lossy(&out_h.join().unwrap_or_default()).to_string();
    let stderr = String::from_utf8_lossy(&err_h.join().unwrap_or_default()).to_string();
    Ok((status.code().unwrap_or(-1) as i64, stdout, stderr))
}

/// Kill the child's whole process group (its own since spawn), then reap it.
pub fn almide_proc_stop_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // An absolute tool path avoids depending on the executed command's PATH.
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

/// The terminal-attached run (#2540): the child inherits stdin, stdout and
/// stderr; nothing is captured; the answer is the exit code (-1 if signalled).
pub fn almide_proc_exec_attached(cmd: &str, args: &[String]) -> Result<i64, String> {
    match std::process::Command::new(cmd).args(args).status() {
        Ok(status) => Ok(status.code().unwrap_or(-1) as i64),
        Err(e) => Err(almide_proc_call_err("process.exec_attached", &format!("{cmd:?}"), e)),
    }
}

/// The children this process spawned and has not yet reaped, by pid (#2494):
/// the handle is kept so `is_alive` can reap an exited child with `try_wait`
/// instead of reporting a zombie alive forever. Process-wide.
pub fn almide_proc_spawned_children() -> &'static std::sync::Mutex<std::collections::HashMap<u32, std::process::Child>> {
    static CHILDREN: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u32, std::process::Child>>> =
        std::sync::OnceLock::new();
    CHILDREN.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Start `cmd` in the background (stdin null); its pid.
pub fn almide_proc_spawn(cmd: &str, args: &[String]) -> Result<i64, String> {
    let child = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| almide_proc_call_err("process.spawn", &almide_proc_q(cmd), e))?;
    let pid = child.id();
    let mut children = almide_proc_spawned_children().lock().unwrap_or_else(|e| e.into_inner());
    // Reap the children that have exited since the last look, so a program
    // that spawns without ever asking is_alive does not accumulate zombies.
    children.retain(|_, c| matches!(c.try_wait(), Ok(None)));
    children.insert(pid, child);
    Ok(pid as i64)
}

/// Send `signal` to `pid`; the err is the platform tool's stderr.
pub fn almide_proc_kill(pid: i64, signal: i64) -> Result<(), String> {
    #[cfg(unix)]
    let cmd = std::process::Command::new("kill").args([&format!("-{}", signal), &pid.to_string()]).output();
    #[cfg(windows)]
    let cmd = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).output();
    let cmd = cmd.map_err(|e| almide_proc_call_err("process.kill", &format!("{pid}, {signal}"), e))?;
    if cmd.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&cmd.stderr).trim().to_string())
    }
}

/// True while `pid` exists. A child of ours answers through its handle (an
/// exited child is reaped and answers false); any other pid asks the platform.
pub fn almide_proc_is_alive(pid: i64) -> bool {
    if let Ok(pid32) = u32::try_from(pid) {
        let mut children = almide_proc_spawned_children().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(child) = children.get_mut(&pid32) {
            let running = matches!(child.try_wait(), Ok(None));
            if !running {
                children.remove(&pid32);
            }
            return running;
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
            .map(|out| String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}
