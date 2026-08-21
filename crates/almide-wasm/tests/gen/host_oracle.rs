//! The third-oracle native leg: run a generated program on the RELEASED
//! almide binary (split from fuzz_differential.rs for the file-size
//! discipline). Deterministic per (seed, src); the temp file is
//! pid-keyed and removed after the run.

/// (stdout, exit) of `oracle run <src>`, or a harness-level error.
pub fn native_run(oracle: &str, seed: u64, src: &str) -> Result<(String, i32), String> {
    let dir = std::env::temp_dir().join(format!("gf-fuzz-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("tmp dir: {e}"))?;
    let f = dir.join(format!("s{seed}.almd"));
    std::fs::write(&f, src).map_err(|e| format!("tmp write: {e}"))?;
    let out = std::process::Command::new(oracle)
        .arg("run")
        .arg(&f)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("host oracle spawn: {e}"))?;
    let _ = std::fs::remove_file(&f);
    Ok((String::from_utf8_lossy(&out.stdout).to_string(), out.status.code().unwrap_or(-1)))
}
