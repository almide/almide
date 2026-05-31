use std::process::Command;
use crate::try_compile;
use super::{hash64, cargo_build_generated_with_native, cargo_build_test_with_native};

/// Cross-process advisory lock on a shared build scratch dir.
///
/// `compile_to_binary` (and `cmd_build`) write a single `src/main.rs` into
/// a shared project dir, run `cargo build` there, then copy the result to
/// a per-hash binary. The in-process `BUILD_LOCK` mutex serializes threads
/// within one process, but the compiler is also invoked as separate
/// subprocesses — e.g. `almide run a.almd` & `almide run b.almd` at once,
/// or a parallel `cargo test` driving many `almide run`/`almide build`
/// children. Those races corrupt the shared `main.rs`/generated binary and
/// produce an executable built from the wrong source.
///
/// An advisory `flock` on a lockfile in the project dir serializes that
/// critical section across processes too. It is crash-safe: the kernel
/// releases the lock when the holding process exits, so an aborted build
/// never deadlocks the next one. The shared `target/` dep cache is
/// preserved (builds serialize but reuse compiled deps).
///
/// Non-unix: a no-op (those platforms keep `--test-threads=1` in CI).
pub(crate) struct BuildDirLock {
    #[cfg(unix)]
    _file: std::fs::File,
}

impl BuildDirLock {
    pub(crate) fn acquire(project_dir: &std::path::Path) -> Result<Self, String> {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let lock_path = project_dir.join(".almide-build.lock");
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(|e| format!("Failed to open build lock {}: {}", lock_path.display(), e))?;
            // SAFETY: `fd` is a valid, open file descriptor owned by `file`,
            // which outlives this call. `flock` only associates an advisory
            // lock with the open file description; it does not mutate Rust
            // state. The lock is released when `file` is dropped (close) or
            // the process exits.
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if rc != 0 {
                return Err(format!(
                    "Failed to acquire build lock: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(BuildDirLock { _file: file })
        }
        #[cfg(not(unix))]
        {
            let _ = project_dir;
            Ok(BuildDirLock {})
        }
    }
}

/// Compile an .almd file to a native binary, returning the path to the executable.
/// Uses incremental caching: if the generated Rust code hasn't changed, skips cargo build.
pub fn compile_to_binary(file: &str, no_check: bool, test_mode: bool, release: bool, project_dir_override: Option<&std::path::Path>) -> Result<std::path::PathBuf, String> {
    let rs_code = try_compile(file, no_check).map_err(|_| "compile failed".to_string())?;

    // Scratch dir. A per-call `project_dir_override` (one dir per test file)
    // gives each parallel worker its own `src/main.rs`, so cold rustc builds
    // run truly in parallel instead of serializing on the shared dir's
    // `BUILD_LOCK`. Otherwise: `ALMIDE_RUN_PROJECT_DIR`, else a shared default.
    let project_dir = project_dir_override.map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("ALMIDE_RUN_PROJECT_DIR").map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::env::temp_dir().join("almide-run"));
    std::fs::create_dir_all(&project_dir)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;

    let use_test_harness = test_mode || (!rs_code.contains("\nfn almide_main(") && !rs_code.contains("\nfn main(") && !rs_code.contains("\npub fn main("));

    let hash_input = format!("{}:test={}:release={}", &rs_code, use_test_harness, release);
    let code_hash = format!("{:016x}", hash64(hash_input.as_bytes()));
    let cache = super::incremental_cache_dir();
    let hash_file = cache.join(format!("{}.hash", file.replace('/', "_").replace('.', "_")));

    // Per-file binary: use file hash as name to avoid collisions during parallel test runs
    let bin_name = format!("almide-{}", &code_hash[..12]);
    let profile_dir = if release { "release" } else { "debug" };
    let bin_path = project_dir.join("target").join(profile_dir).join(&bin_name);

    let cache_hit = hash_file.exists()
        && bin_path.exists()
        && std::fs::read_to_string(&hash_file).ok().as_deref() == Some(&code_hash);

    if cache_hit {
        return Ok(bin_path);
    }

    // Load native deps from almide.toml (search in input file's directory, then CWD).
    // source_root is the directory containing almide.toml (where native/ lives).
    let file_dir = std::path::Path::new(file).parent()
        .map(|p| if p.as_os_str().is_empty() { std::path::PathBuf::from(".") } else { p.to_path_buf() })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let toml_path = {
        let candidate = file_dir.join("almide.toml");
        if candidate.exists() { candidate } else { std::path::PathBuf::from("almide.toml") }
    };
    let parsed = toml_path.exists()
        .then(|| crate::project::parse_toml(&toml_path).ok())
        .flatten();
    let native_deps = parsed.as_ref().map(|p| p.native_deps.as_slice()).unwrap_or(&[]);
    let toml_dir = toml_path.parent()
        .map(|p| if p.as_os_str().is_empty() { std::path::PathBuf::from(".") } else { p.to_path_buf() })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let has_deps = parsed.as_ref().map_or(false, |p| !p.dependencies.is_empty());
    let source_root = if !native_deps.is_empty() || has_deps { Some(toml_dir.as_path()) } else { None };

    // Serialize cargo builds: the shared project dir has a single src/main.rs
    // and one generated binary, overwritten per compilation. Parallel writes
    // corrupt them. `BUILD_LOCK` serializes threads in this process; the
    // `flock` extends that across separate `almide` processes. The lock spans
    // the whole write→build→copy window — without covering the copy, a
    // concurrent build could overwrite the generated binary between our build
    // and our copy-out.
    static BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // A unique per-call dir has its own src/main.rs, so the global mutex (which
    // only exists to serialize the shared default dir) isn't needed — the
    // per-dir flock still guards a separate process reusing the same dir.
    let _guard = project_dir_override.is_none().then(|| BUILD_LOCK.lock().unwrap());
    let _flock = BuildDirLock::acquire(&project_dir)?;

    // Re-check the cache under the lock: another process/thread may have built
    // this exact binary while we waited, making a rebuild redundant.
    if hash_file.exists()
        && bin_path.exists()
        && std::fs::read_to_string(&hash_file).ok().as_deref() == Some(&code_hash)
    {
        return Ok(bin_path);
    }

    let result = if use_test_harness {
        cargo_build_test_with_native(&rs_code, &project_dir, native_deps, source_root)
    } else {
        cargo_build_generated_with_native(&rs_code, &project_dir, release, native_deps, source_root)
    };

    match result {
        Ok(built_path) => {
            // Copy built binary to per-file cached path. The bare-rustc fast
            // path doesn't create a cargo `target/<profile>/` dir, so ensure
            // bin_path's parent exists, and surface a copy failure instead of
            // silently leaving bin_path missing (→ "Failed to execute" at run).
            if let Some(parent) = bin_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::copy(&built_path, &bin_path) {
                return Err(format!("failed to stage built binary {} -> {}: {}",
                    built_path.display(), bin_path.display(), e));
            }
            let _ = std::fs::create_dir_all(&cache);
            let _ = std::fs::write(&hash_file, &code_hash);
            Ok(bin_path)
        }
        Err(e) => Err(e),
    }
}

/// Run a compiled binary with the given args, returning exit code.
pub fn run_binary(bin: &std::path::Path, program_args: &[String]) -> i32 {
    let status = Command::new(bin)
        .env("RUST_MIN_STACK", "8388608")
        .args(program_args)
        .status()
        .unwrap_or_else(|e| { eprintln!("Failed to execute: {}", e); std::process::exit(1); });
    status.code().unwrap_or(1)
}

pub fn cmd_run_inner(file: &str, program_args: &[String], no_check: bool, test_mode: bool, release: bool) -> i32 {
    match compile_to_binary(file, no_check, test_mode, release, None) {
        Ok(bin) => run_binary(&bin, program_args),
        Err(e) => {
            eprintln!("Compile error:\n{}", e);
            1
        }
    }
}

pub fn cmd_run(file: &str, program_args: &[String], no_check: bool, release: bool) {
    std::process::exit(cmd_run_inner(file, program_args, no_check, false, release));
}
