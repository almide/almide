// env extern — Rust native implementations

// argv[1..], verbatim. A `--` among the program's arguments is the PROGRAM's:
// `almide run app.almd -- a` has already consumed its own separator (clap) by
// the time the binary is exec'd, so this used to strip a second one — a built
// binary run as `./p a -- b` answered ["b"] where both wasm legs answered
// ["a", "--", "b"] (#2485). The wasm legs build the same list from WASI
// args_get with argv[0] skipped (stdlib/env_args.almd).
pub fn almide_rt_env_args() -> Vec<String> {
    std::env::args().skip(1).collect()
}

pub fn almide_rt_env_get(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

pub fn almide_rt_env_set(name: &str, value: &str) {
    std::env::set_var(name, value);
}

pub fn almide_rt_env_cwd() -> Result<String, String> {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

pub fn almide_rt_env_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn almide_rt_env_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// A negative duration sleeps 0 ms (#2486): `ms as u64` made -1 into u64::MAX
// milliseconds, so the native leg never returned where every wasm host clamps
// the count at 0 (crates/almide-wasm/src/host_env.rs, the embedded host's op 36,
// the stock-p1 shim). `process.sleep` has always clamped the same way. A
// deadline computed as `deadline - now` goes negative once it has passed, and
// "already late" means "do not wait", not "wait forever" or abort.
pub fn almide_rt_env_sleep_ms(ms: i64) {
    std::thread::sleep(std::time::Duration::from_millis(ms.max(0) as u64));
}

pub fn almide_rt_env_temp_dir() -> String {
    std::env::temp_dir().to_string_lossy().replace('\\', "/")
}

pub fn almide_rt_env_os() -> String {
    if cfg!(target_os = "windows") { "windows".to_string() }
    else if cfg!(target_os = "macos") { "macos".to_string() }
    else if cfg!(target_os = "linux") { "linux".to_string() }
    else { "unknown".to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_cwd() {
        assert!(almide_rt_env_cwd().is_ok());
    }

    #[test]
    fn test_env_timestamp() {
        assert!(almide_rt_env_unix_timestamp() > 0);
    }

    #[test]
    fn test_env_os() {
        let os = almide_rt_env_os();
        assert!(["macos", "linux", "windows", "unknown"].contains(&os));
    }
}
