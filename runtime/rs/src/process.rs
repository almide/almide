// process extern — Rust native implementations

pub fn almide_rt_process_exec(cmd: String, args: Vec<String>) -> Result<String, String> {
    match std::process::Command::new(&cmd).args(&args).output() {
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

pub fn almide_rt_process_exit(code: i64) {
    std::process::exit(code as i32);
}

pub fn almide_rt_process_stdin_lines() -> Result<Vec<String>, String> {
    use std::io::BufRead;
    std::io::stdin()
        .lock()
        .lines()
        .collect::<Result<Vec<String>, _>>()
        .map_err(|e| e.to_string())
}

pub fn almide_rt_process_exec_in(dir: String, cmd: String, args: Vec<String>) -> Result<String, String> {
    match std::process::Command::new(&cmd).args(&args).current_dir(&dir).output() {
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

pub fn almide_rt_process_exec_with_stdin(cmd: String, args: Vec<String>, input: String) -> Result<String, String> {
    use std::io::Write;
    let mut child = std::process::Command::new(&cmd)
        .args(&args)
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
        let result = almide_rt_process_exec("echo".into(), vec!["hello".into()]);
        assert!(result.is_ok());
        assert!(result.unwrap().trim() == "hello");
    }
}

pub fn almide_rt_process_exec_status(cmd: String, args: Vec<String>) -> Result<(i64, String, String), String> {
    match std::process::Command::new(&cmd).args(&args).output() {
        Ok(out) => {
            let code = out.status.code().unwrap_or(-1) as i64;
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            Ok((code, stdout, stderr))
        }
        Err(e) => Err(format!("exec failed: {}", e)),
    }
}
