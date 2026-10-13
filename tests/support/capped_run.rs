//! Run a child under a wall-clock cap. A cross-target sweep that waits on a
//! program forever hides a hang as a cancelled CI job (#3679: a tcp fixture
//! held `Test Rust` for 45 minutes and the release-only job for 30). Here a
//! run that outlives its cap kills its whole process group (`almide run`, the
//! program it built, wasmtime) and comes back as an err naming the cap and
//! the output it had produced — a failed test, never a silent stall.

use std::os::unix::process::CommandExt;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub fn output_capped(mut cmd: Command, cap: Duration) -> Result<Output, String> {
    let dir = tempfile::tempdir().map_err(|e| format!("scratch dir: {e}"))?;
    let (out_path, err_path) = (dir.path().join("stdout"), dir.path().join("stderr"));
    let out = std::fs::File::create(&out_path).map_err(|e| format!("stdout file: {e}"))?;
    let err = std::fs::File::create(&err_path).map_err(|e| format!("stderr file: {e}"))?;
    cmd.stdin(Stdio::null()).stdout(out).stderr(err).process_group(0);
    let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    let start = Instant::now();
    let read = |p: &std::path::Path| std::fs::read(p).unwrap_or_default();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| format!("wait: {e}"))? {
            return Ok(Output { status, stdout: read(&out_path), stderr: read(&err_path) });
        }
        if start.elapsed() > cap {
            let _ = Command::new("kill").arg("-KILL").arg(format!("-{}", child.id())).status();
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "still running after {}s — killed\n--- stdout so far\n{}\n--- stderr so far\n{}",
                cap.as_secs(),
                String::from_utf8_lossy(&read(&out_path)),
                String::from_utf8_lossy(&read(&err_path)),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
