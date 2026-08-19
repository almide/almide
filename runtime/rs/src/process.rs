// process extern — Rust native implementations

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessStatus {
    pub code: i64,
    pub stdout: String,
    pub stderr: String,
}

pub fn almide_rt_process_exec(cmd: &str, args: &[String]) -> Result<String, String> {
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
        Err(e) => Err(e.to_string()),
    }
}

pub fn almide_rt_process_exit(code: i64) -> ! {
    std::process::exit(code as i32);
}

pub fn almide_rt_process_args() -> Vec<String> {
    std::env::args().collect()
}

pub fn almide_rt_process_stdin_lines() -> Result<Vec<String>, String> {
    use std::io::BufRead;
    std::io::stdin()
        .lock()
        .lines()
        .collect::<Result<Vec<String>, _>>()
        .map_err(|e| e.to_string())
}

pub fn almide_rt_process_exec_in(dir: &str, cmd: &str, args: &[String]) -> Result<String, String> {
    match std::process::Command::new(cmd).args(args).current_dir(dir).output() {
        Ok(out) => {
            if out.status.success() {
                Ok(String::from_utf8_lossy(&out.stdout).to_string())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).to_string())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn almide_rt_process_exec_with_stdin(cmd: &str, args: &[String], input: &str) -> Result<String, String> {
    use std::io::Write;
    let mut child = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(input.as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_exec() {
        let result = almide_rt_process_exec("echo", &vec!["hello".into()]);
        assert!(result.is_ok());
        assert!(result.unwrap().trim() == "hello");
    }
}

pub fn almide_rt_process_exec_status(cmd: &str, args: &[String]) -> Result<ProcessStatus, String> {
    match std::process::Command::new(cmd).args(args).output() {
        Ok(out) => {
            let code = out.status.code().unwrap_or(-1) as i64;
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            Ok(ProcessStatus { code, stdout, stderr })
        }
        Err(e) => Err(format!("exec failed: {}", e)),
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
pub fn almide_rt_process_exec_status_timeout(
    cmd: &str,
    args: &[String],
    timeout_ms: i64,
) -> Result<ProcessStatus, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("exec failed: {}", e))?;
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
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    // Join the drains so the pipes close cleanly before we return.
                    let _ = out_h.join();
                    let _ = err_h.join();
                    return Err(format!("exec timed out after {}ms", timeout_ms));
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => return Err(format!("exec failed: {}", e)),
        }
    };
    let stdout = String::from_utf8_lossy(&out_h.join().unwrap_or_default()).to_string();
    let stderr = String::from_utf8_lossy(&err_h.join().unwrap_or_default()).to_string();
    Ok(ProcessStatus { code: status.code().unwrap_or(-1) as i64, stdout, stderr })
}

pub fn almide_rt_process_pid() -> i64 {
    std::process::id() as i64
}

pub fn almide_rt_process_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

pub fn almide_rt_process_spawn(cmd: &str, args: &[String]) -> Result<i64, String> {
    std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .spawn()
        .map(|child| child.id() as i64)
        .map_err(|e| format!("spawn '{}' failed: {}", cmd, e))
}

pub fn almide_rt_process_kill(pid: i64, signal: i64) -> Result<(), String> {
    #[cfg(unix)]
    {
        let cmd = std::process::Command::new("kill")
            .args([&format!("-{}", signal), &pid.to_string()])
            .output()
            .map_err(|e| format!("kill failed: {}", e))?;
        if cmd.status.success() { Ok(()) }
        else { Err(String::from_utf8_lossy(&cmd.stderr).trim().to_string()) }
    }
    #[cfg(windows)]
    {
        let cmd = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output()
            .map_err(|e| format!("kill failed: {}", e))?;
        if cmd.status.success() { Ok(()) }
        else { Err(String::from_utf8_lossy(&cmd.stderr).trim().to_string()) }
    }
}

pub fn almide_rt_process_sleep(ms: i64) {
    std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
}

pub fn almide_rt_process_is_alive(pid: i64) -> bool {
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
