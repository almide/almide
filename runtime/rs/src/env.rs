// env extern — Rust native implementations

pub fn almide_rt_env_args() -> Vec<String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        // Skip binary name and everything before "--"
        if let Some(pos) = args.iter().position(|a| a == "--") {
            args[pos + 1..].to_vec()
        } else {
            args[1..].to_vec()
        }
    } else {
        vec![]
    }
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

pub fn almide_rt_env_sleep_ms(ms: i64) {
    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
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
