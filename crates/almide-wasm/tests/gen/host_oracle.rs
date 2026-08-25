//! The third oracle: run a generated program on the pinned almide
//! binary's WASM leg — the SAME reference definition the run manifest
//! uses. Never the native leg: stage-68 found the a877 native
//! double-evaluates side-effectful HOF callbacks (its own wasm leg
//! disagrees with it, m1 f4 m1 m1 m2 f7 m2 vs m1 m2 f4 f7), so on
//! generated programs the native leg is not a referee. Deterministic
//! per (seed, src); the temp file is pid-keyed and removed.

/// (stdout, stderr, exit) of `oracle run <src>`, or a harness error.
pub fn native_run(oracle: &str, seed: u64, src: &str) -> Result<(String, String, i32), String> {
    let dir = std::env::temp_dir().join(format!("gf-fuzz-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("tmp dir: {e}"))?;
    let f = dir.join(format!("s{seed}.almd"));
    std::fs::write(&f, src).map_err(|e| format!("tmp write: {e}"))?;
    let mut cmd = std::process::Command::new(oracle);
    cmd.arg("run").arg(&f).arg("--target").arg("wasm");
    // wasmtime lives off the sandbox PATH locally (/opt/homebrew/bin).
    if let Ok(path) = std::env::var("PATH")
        && !path.contains("/opt/homebrew/bin")
    {
        cmd.env("PATH", format!("/opt/homebrew/bin:{path}"));
    }
    let out = cmd
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("host oracle spawn: {e}"))?;
    let _ = std::fs::remove_file(&f);
    Ok((
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    ))
}
