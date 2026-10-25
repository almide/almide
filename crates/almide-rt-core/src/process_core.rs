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
pub fn almide_proc_exec(cmd: &str, args: &[impl AsRef<str>]) -> Result<String, String> {
    match std::process::Command::new(cmd).args(args.iter().map(|a| a.as_ref())).output() {
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
pub fn almide_proc_exec_in(dir: &str, cmd: &str, args: &[impl AsRef<str>]) -> Result<String, String> {
    match std::process::Command::new(cmd).args(args.iter().map(|a| a.as_ref())).current_dir(dir).output() {
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
pub fn almide_proc_exec_with_stdin(cmd: &str, args: &[impl AsRef<str>], input: &str) -> Result<String, String> {
    let fail = |e: std::io::Error| almide_proc_call_err("process.exec_with_stdin", &almide_proc_q(cmd), e);
    let mut child = std::process::Command::new(cmd)
        .args(args.iter().map(|a| a.as_ref()))
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
pub fn almide_proc_exec_status(cmd: &str, args: &[impl AsRef<str>]) -> Result<(i64, String, String), String> {
    match std::process::Command::new(cmd).args(args.iter().map(|a| a.as_ref())).output() {
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
    args: &[impl AsRef<str>],
    timeout_ms: i64,
    poll: fn(&mut std::process::Child) -> std::io::Result<AlmideChildPoll>,
) -> Result<(i64, String, String), String> {
    let mut command = std::process::Command::new(cmd);
    command.args(args.iter().map(|a| a.as_ref())).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
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
                         run terminal programs with process.run"
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

/// The terminal-attached run (#2540, `process.run` since #3379): the child
/// inherits stdin, stdout and stderr; nothing is captured; the answer is the
/// exit code (-1 if signalled).
pub fn almide_proc_run(cmd: &str, args: &[impl AsRef<str>]) -> Result<i64, String> {
    match std::process::Command::new(cmd).args(args.iter().map(|a| a.as_ref())).status() {
        Ok(status) => Ok(status.code().unwrap_or(-1) as i64),
        Err(e) => Err(almide_proc_call_err("process.run", &format!("{cmd:?}"), e)),
    }
}

/// `run` in `dir`; a bad `dir` fails at spawn too, so both operands are named
/// (exec_in's form).
pub fn almide_proc_run_in(dir: &str, cmd: &str, args: &[impl AsRef<str>]) -> Result<i64, String> {
    match std::process::Command::new(cmd).args(args.iter().map(|a| a.as_ref())).current_dir(dir).status() {
        Ok(status) => Ok(status.code().unwrap_or(-1) as i64),
        Err(e) => Err(almide_proc_call_err(
            "process.run_in",
            &format!("{}, {}", almide_proc_q(dir), almide_proc_q(cmd)),
            e,
        )),
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
pub fn almide_proc_spawn(cmd: &str, args: &[impl AsRef<str>]) -> Result<i64, String> {
    let child = std::process::Command::new(cmd)
        .args(args.iter().map(|a| a.as_ref()))
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

// ── The ProcessCommand family (R-process-02, -03, -05, -07) ──
//
// One record of settings — the program, its arguments, the working directory
// ("" keeps this one), the text for its stdin ("" = none) and a deadline in
// ms (0 = none) — and three ways to run it: `output` (wait for it, capture
// everything), the `start` handle (poll / wait / stop / read_new_* /
// child_pid) and `run` (on this terminal). A captured child runs in its own
// process group, so a deadline or `stop` takes its whole tree; a deadline's
// err is C-214's `exec timed out after <ms>ms`.

/// A captured child and what it has written so far.
pub struct AlmideProcChild {
    child: std::process::Child,
    out: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    err: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    pumps: Vec<std::thread::JoinHandle<()>>,
    deadline: Option<std::time::Instant>,
    timeout_ms: i64,
    exited: Option<std::process::ExitStatus>,
    done: Option<Result<(i64, String, String), String>>,
    out_seen: usize,
    err_seen: usize,
    call: &'static str,
    operand: String,
}

/// Copy `r` into `buf` until it ends, on a thread of its own, so a chatty
/// child never blocks on a full pipe while the parent waits.
fn almide_proc_pump<R: std::io::Read + Send + 'static>(
    r: Option<R>,
    buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let Some(mut r) = r else { return };
        let mut chunk = [0u8; 8192];
        loop {
            match r.read(&mut chunk) {
                Ok(0) => return,
                Ok(n) => buf.lock().unwrap_or_else(|p| p.into_inner()).extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return,
            }
        }
    })
}

impl AlmideProcChild {
    /// Start `cmd` captured. `stdin`'s text is written in full before this
    /// returns (the caller ignores SIGPIPE around it: a child that exits
    /// without reading makes the write fail, not the program end).
    pub fn start(
        call: &'static str,
        cmd: &str,
        args: &[impl AsRef<str>],
        dir: &str,
        stdin: &str,
        timeout_ms: i64,
    ) -> Result<AlmideProcChild, String> {
        let mut command = std::process::Command::new(cmd);
        command
            .args(args.iter().map(|a| a.as_ref()))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(if stdin.is_empty() { std::process::Stdio::null() } else { std::process::Stdio::piped() });
        if !dir.is_empty() {
            command.current_dir(dir);
        }
        #[cfg(unix)]
        {
            std::os::unix::process::CommandExt::process_group(&mut command, 0);
        }
        let operand = if dir.is_empty() { format!("{cmd:?}") } else { format!("{}, {}", almide_proc_q(dir), almide_proc_q(cmd)) };
        let mut child = command.spawn().map_err(|e| almide_proc_call_err(call, &operand, e))?;
        let out = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let err = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let pumps = vec![almide_proc_pump(child.stdout.take(), out.clone()), almide_proc_pump(child.stderr.take(), err.clone())];
        if let Some(mut w) = child.stdin.take() {
            let _ = std::io::Write::write_all(&mut w, stdin.as_bytes());
        }
        let deadline = (timeout_ms > 0).then(|| std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64));
        Ok(AlmideProcChild {
            child, out, err, pumps, deadline, timeout_ms,
            exited: None, done: None, out_seen: 0, err_seen: 0, call, operand,
        })
    }

    pub fn pid(&self) -> i64 {
        self.child.id() as i64
    }

    fn text(buf: &std::sync::Mutex<Vec<u8>>) -> String {
        String::from_utf8_lossy(&buf.lock().unwrap_or_else(|p| p.into_inner())).to_string()
    }

    fn finish_ok(&mut self, status: std::process::ExitStatus) {
        for p in self.pumps.drain(..) {
            let _ = p.join();
        }
        self.done = Some(Ok((status.code().unwrap_or(-1) as i64, Self::text(&self.out), Self::text(&self.err))));
    }

    /// One look, never blocking: the outcome once the child has exited and
    /// both pipes have ended (#2065: a descendant may hold them past the
    /// exit), or once the deadline passed.
    pub fn step(&mut self, poll: fn(&mut std::process::Child) -> std::io::Result<AlmideChildPoll>) -> Option<Result<(i64, String, String), String>> {
        if self.done.is_none() {
            if self.exited.is_none() {
                match poll(&mut self.child) {
                    Ok(AlmideChildPoll::Running) => {}
                    Ok(AlmideChildPoll::Exited(s)) => self.exited = Some(s),
                    Ok(AlmideChildPoll::TerminalStop(sig)) => {
                        almide_proc_stop_tree(&mut self.child);
                        self.done = Some(Err(format!(
                            "{}({}): the child was stopped by {sig}: it tried to use the terminal from a \
                             background process group; run terminal programs with process.run",
                            self.call, self.operand
                        )));
                    }
                    Err(e) => {
                        almide_proc_stop_tree(&mut self.child);
                        self.done = Some(Err(almide_proc_call_err(self.call, &self.operand, e)));
                    }
                }
            }
            if self.done.is_none() {
                if let (Some(s), true) = (self.exited, self.pumps.iter().all(|p| p.is_finished())) {
                    self.finish_ok(s);
                } else if self.deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                    almide_proc_stop_tree(&mut self.child);
                    self.done = Some(Err(format!("exec timed out after {}ms", self.timeout_ms)));
                }
            }
        }
        self.done.clone()
    }

    /// Block until `step` answers.
    pub fn wait(&mut self, poll: fn(&mut std::process::Child) -> std::io::Result<AlmideChildPoll>) -> Result<(i64, String, String), String> {
        loop {
            if let Some(r) = self.step(poll) {
                return r;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// SIGTERM to the child's group, `grace_ms` to exit, then SIGKILL; the
    /// outcome, code -1 when a signal ended it. Calling it again (or after
    /// the child ended) answers the same outcome.
    pub fn stop(&mut self, grace_ms: i64) -> Result<(i64, String, String), String> {
        if let Some(d) = &self.done {
            return d.clone();
        }
        if self.exited.is_none() {
            #[cfg(unix)]
            {
                let _ = std::process::Command::new("/bin/kill")
                    .args(["-TERM", "--", &format!("-{}", self.child.id())])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            let until = std::time::Instant::now() + std::time::Duration::from_millis(grace_ms.max(0) as u64);
            while self.exited.is_none() && std::time::Instant::now() < until {
                if let Ok(Some(s)) = self.child.try_wait() {
                    self.exited = Some(s);
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
            if self.exited.is_none() {
                almide_proc_stop_tree(&mut self.child);
                self.exited = self.child.try_wait().ok().flatten();
            }
        }
        // Pipes a stray descendant still holds are not waited for past 1 s.
        let until = std::time::Instant::now() + std::time::Duration::from_millis(1000);
        while !self.pumps.iter().all(|p| p.is_finished()) && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let code = self.exited.and_then(|s| s.code()).unwrap_or(-1) as i64;
        if self.pumps.iter().all(|p| p.is_finished()) {
            for p in self.pumps.drain(..) {
                let _ = p.join();
            }
        }
        let r = Ok((code, Self::text(&self.out), Self::text(&self.err)));
        self.done = Some(r.clone());
        r
    }

    /// The text written to stdout (`err` false) or stderr since the last
    /// read; never blocks. A UTF-8 sequence cut by the read waits for its
    /// rest; an invalid one is U+FFFD.
    pub fn read_new(&mut self, err: bool) -> String {
        let (buf, seen) = if err { (&self.err, &mut self.err_seen) } else { (&self.out, &mut self.out_seen) };
        let b = buf.lock().unwrap_or_else(|p| p.into_inner());
        let new = &b[*seen..];
        let take = match std::str::from_utf8(new) {
            Ok(_) => new.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => new.len(),
        };
        *seen += take;
        String::from_utf8_lossy(&new[..take]).to_string()
    }
}

/// `process.output`: run to the end, every exit code ok.
pub fn almide_proc_output(
    cmd: &str,
    args: &[impl AsRef<str>],
    dir: &str,
    stdin: &str,
    timeout_ms: i64,
    poll: fn(&mut std::process::Child) -> std::io::Result<AlmideChildPoll>,
) -> Result<(i64, String, String), String> {
    AlmideProcChild::start("process.output", cmd, args, dir, stdin, timeout_ms)?.wait(poll)
}

/// `process.run`: the child shares this program's terminal (#2540) — its
/// stdout and stderr, and its stdin unless `stdin` names text to pipe in —
/// and stays in this process group, so a pager or an editor works. A
/// deadline ends the child itself and answers C-214's err.
pub fn almide_proc_run_cmd(cmd: &str, args: &[impl AsRef<str>], dir: &str, stdin: &str, timeout_ms: i64) -> Result<i64, String> {
    let mut command = std::process::Command::new(cmd);
    command.args(args.iter().map(|a| a.as_ref()));
    if !dir.is_empty() {
        command.current_dir(dir);
    }
    if !stdin.is_empty() {
        command.stdin(std::process::Stdio::piped());
    }
    let operand = if dir.is_empty() { format!("{cmd:?}") } else { format!("{}, {}", almide_proc_q(dir), almide_proc_q(cmd)) };
    let mut child = command.spawn().map_err(|e| almide_proc_call_err("process.run", &operand, e))?;
    if let Some(mut w) = child.stdin.take() {
        let _ = std::io::Write::write_all(&mut w, stdin.as_bytes());
    }
    let deadline = (timeout_ms > 0).then(|| std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code().unwrap_or(-1) as i64),
            Ok(None) => {}
            Err(e) => return Err(almide_proc_call_err("process.run", &operand, e)),
        }
        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("exec timed out after {}ms", timeout_ms));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// The children `process.start` made, by handle (their slot). Process-wide.
pub fn almide_proc_handles() -> &'static std::sync::Mutex<Vec<Option<AlmideProcChild>>> {
    static HANDLES: std::sync::OnceLock<std::sync::Mutex<Vec<Option<AlmideProcChild>>>> = std::sync::OnceLock::new();
    HANDLES.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// Run `f` on the child behind handle `h`.
pub fn almide_proc_with<R>(h: i64, f: impl FnOnce(&mut AlmideProcChild) -> R) -> Result<R, String> {
    let mut t = almide_proc_handles().lock().unwrap_or_else(|p| p.into_inner());
    let c = usize::try_from(h).ok().and_then(|i| t.get_mut(i)).and_then(|c| c.as_mut())
        .ok_or_else(|| format!("invalid ProcessChild handle: {h}"))?;
    Ok(f(c))
}

/// The grace `almide_proc_stop_all_unwaited` gives each child between SIGTERM
/// and SIGKILL — the 500 ms the stdlib docs show `process.stop` with.
pub const ALMIDE_PROC_EXIT_GRACE_MS: i64 = 500;

/// At program exit (#4214): stop every `process.start` child whose outcome is
/// not recorded yet — neither waited on nor stopped — the way `process.stop`
/// does (`AlmideProcChild::stop`, ALMIDE_PROC_EXIT_GRACE_MS), so no child the
/// program started outlives it. A child already waited on or stopped is left
/// alone. A poisoned table is still walked; a table another thread holds (a
/// `wait` in a `fan` element) is waited for at most 1 s, then left — an exit
/// never deadlocks on it.
pub fn almide_proc_stop_all_unwaited() {
    let m = almide_proc_handles();
    let until = std::time::Instant::now() + std::time::Duration::from_millis(1000);
    let mut t = loop {
        match m.try_lock() {
            Ok(g) => break g,
            Err(std::sync::TryLockError::Poisoned(p)) => break p.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) if std::time::Instant::now() < until => {
                std::thread::sleep(std::time::Duration::from_millis(5))
            }
            Err(std::sync::TryLockError::WouldBlock) => return,
        }
    };
    for c in t.iter_mut().flatten() {
        if c.done.is_none() {
            let _ = c.stop(ALMIDE_PROC_EXIT_GRACE_MS);
        }
    }
}

/// `process.start`: the new child's handle.
pub fn almide_proc_start(cmd: &str, args: &[impl AsRef<str>], dir: &str, stdin: &str, timeout_ms: i64) -> Result<i64, String> {
    let c = AlmideProcChild::start("process.start", cmd, args, dir, stdin, timeout_ms)?;
    let mut t = almide_proc_handles().lock().unwrap_or_else(|p| p.into_inner());
    t.push(Some(c));
    Ok((t.len() - 1) as i64)
}
