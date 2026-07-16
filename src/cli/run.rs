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
/// An advisory exclusive lock on a lockfile in the project dir serializes that
/// critical section across processes too — `flock(LOCK_EX)` on unix and
/// `LockFileEx` on Windows, both via `fs2::FileExt::lock_exclusive`. It is
/// crash-safe: the OS releases the lock when the holding process exits, so an
/// aborted build never deadlocks the next one. The shared `target/` dep cache
/// is preserved (builds serialize but reuse compiled deps). It covers every
/// real host (unix + Windows), so CI runs the suite in parallel on all of them
/// (no `--test-threads=1` carve-out for Windows). `wasm32` — where the compiler
/// can run as a determinism harness but never spawns build subprocesses, and
/// where `fs2` has no backing OS lock — is a no-op.
pub(crate) struct BuildDirLock {
    #[cfg(any(unix, windows))]
    _file: std::fs::File,
}

impl BuildDirLock {
    pub(crate) fn acquire(project_dir: &std::path::Path) -> Result<Self, String> {
        #[cfg(any(unix, windows))]
        {
            use fs2::FileExt;
            let lock_path = project_dir.join(".almide-build.lock");
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(|e| format!("Failed to open build lock {}: {}", lock_path.display(), e))?;
            // Blocking exclusive lock; released when `file` is dropped (close) or
            // the process exits.
            file.lock_exclusive()
                .map_err(|e| format!("Failed to acquire build lock: {}", e))?;
            Ok(BuildDirLock { _file: file })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = project_dir;
            Ok(BuildDirLock {})
        }
    }
}

/// Compile an .almd file to a native binary, returning the path to the executable.
/// Uses incremental caching: if the generated Rust code hasn't changed, skips cargo build.
pub fn compile_to_binary(file: &str, no_check: bool, test_mode: bool, release: bool, project_dir_override: Option<&std::path::Path>) -> Result<std::path::PathBuf, String> {
    compile_to_binary_with(file, no_check, test_mode, release, project_dir_override, false)
}

/// `compile_to_binary` with the NATIVE trust-spine opt-in (#764): when
/// `native_verified`, try the v1 MIR renderer first (same Perceus MIR as the
/// wasm leg; Drop erased to Rust scope-end, ownership verified pre-render) and
/// fall back to the v0 source on a WALL — a v1-rendered program is never wrong.
pub fn compile_to_binary_with(file: &str, no_check: bool, test_mode: bool, release: bool, project_dir_override: Option<&std::path::Path>, native_verified: bool) -> Result<std::path::PathBuf, String> {
    let rs_code = try_compile(file, no_check).map_err(|_| "compile failed".to_string())?;
    let rs_code = if native_verified && !test_mode {
        let source_text = std::fs::read_to_string(file).unwrap_or_default();
        match almide_mir::pipeline::try_render_rust_source(&source_text) {
            Ok(v1_code) => {
                if std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok() {
                    eprintln!("native: v1 trust-spine render");
                }
                v1_code
            }
            Err(e) => {
                if std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok() {
                    eprintln!("native: v1 walled ({e:?}) — falling back to v0 codegen");
                }
                rs_code
            }
        }
    } else {
        rs_code
    };

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
    let parsed = toml_path.exists().then(|| {
        // A broken almide.toml must not be SILENT: dropping it here also drops
        // [native-deps]/native/ injection, and the build then fails later as an
        // opaque E0433 on the native module (almide-sqlite: a hyphenated
        // package name errored in parse_toml and rusqlite never reached the
        // generated Cargo.toml).
        crate::project::parse_toml(&toml_path)
            .map_err(|e| eprintln!("warning: {} ignored: {}", toml_path.display(), e))
            .ok()
    }).flatten();
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
            // Stage via copy-to-temp + ATOMIC RENAME: a direct fs::copy onto the
            // cached path leaves a window where the file is open for writing while
            // a PARALLEL test thread execs it — ETXTBSY ("Text file busy") on
            // Linux, the CI examples-suite flake. The rename swaps a fully-written
            // inode into place atomically, so an exec sees either the complete old
            // binary or the complete new one, never a half-staged file.
            let staged = bin_path.with_extension(format!("stage-{}", std::process::id()));
            if let Err(e) = std::fs::copy(&built_path, &staged) {
                return Err(format!("failed to stage built binary {} -> {}: {}",
                    built_path.display(), staged.display(), e));
            }
            if let Err(e) = std::fs::rename(&staged, &bin_path) {
                let _ = std::fs::remove_file(&staged);
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
    // Belt for the parallel-test ETXTBSY race (the staging rename above is the
    // root fix): if another thread's stale write handle still overlaps the exec,
    // back off briefly and retry instead of failing the whole suite.
    let mut delay = std::time::Duration::from_millis(20);
    for _ in 0..6 {
        match Command::new(bin)
            .env("RUST_MIN_STACK", "8388608")
            .args(program_args)
            .status()
        {
            Ok(status) => return status.code().unwrap_or(1),
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(delay);
                delay *= 2;
            }
            Err(e) => {
                eprintln!("Failed to execute: {}", e);
                std::process::exit(1);
            }
        }
    }
    eprintln!("Failed to execute: Text file busy (persisted after retries)");
    1
}

pub fn cmd_run_inner(file: &str, program_args: &[String], no_check: bool, test_mode: bool, release: bool, native_verified: bool) -> i32 {
    match compile_to_binary_with(file, no_check, test_mode, release, None, native_verified) {
        Ok(bin) => run_binary(&bin, program_args),
        Err(e) => {
            eprintln!("Compile error:\n{}", e);
            1
        }
    }
}

pub fn cmd_run(file: &str, program_args: &[String], no_check: bool, release: bool, target: Option<&str>, verified: bool, native_verified: bool) {
    let code = match target {
        // Default and explicit native target: the cargo/rustc path.
        None | Some("rust") | Some("native") => cmd_run_inner(file, program_args, no_check, false, release, native_verified),
        // WASM target: build the same module `almide build --target wasm`
        // emits, then execute it on the `wasmtime` CLI. Both targets must
        // produce byte-identical stdout/stderr/exit — the cross-target gate.
        Some("wasm") | Some("wasm32") | Some("wasi") => cmd_run_wasm(file, program_args, verified),
        Some(other) => {
            eprintln!(
                "error: unknown run target '{}'\n  \
                 in `almide run --target {}`\n  \
                 supported targets: rust (default, native binary), wasm (wasmtime)\n  \
                 hint: drop --target to run natively, or use `--target wasm`",
                other, other
            );
            1
        }
    };
    std::process::exit(code);
}

/// Build `file` to a wasm32-wasi module and execute it on the `wasmtime` CLI.
///
/// Mirrors the test runner's wasm invocation (`wasmtime --dir=/ <module>`) so
/// the observable behavior matches `almide test --target wasm` and the
/// `spec/wasm_cross` gate. Program args after `--` are forwarded to the guest.
/// `wasmtime`'s own exit code is propagated unchanged, so a guest
/// `proc_exit(n)` surfaces as `n` exactly as a native binary's exit would.
fn cmd_run_wasm(file: &str, program_args: &[String], verified: bool) -> i32 {
    // `run` does not expose the `--emit-unverified` waiver: running a module that
    // failed the Perceus RC gate would silently execute leaky/double-freeing code,
    // so a verification failure is always a hard error here. The waiver is
    // build-only (you opt into shipping a known-bad artifact, not into running it).
    let (bytes, _produced_by_v1) = match super::build::compile_to_wasm_bytes(file, false, verified) {
        Ok(b) => b,
        Err(()) => return 1,
    };

    // Stage the module under a per-content temp name so concurrent `almide run`
    // invocations never race on one path (the build scratch dir is shared).
    let wasm_name = format!("almide-run-{:016x}.wasm", hash64(&bytes));
    let wasm_path = std::env::temp_dir().join(wasm_name);
    if let Err(e) = std::fs::write(&wasm_path, &bytes) {
        eprintln!("error: failed to stage wasm module {}: {}", wasm_path.display(), e);
        return 1;
    }

    // `--dir=/` preopens the host root so WASI fs ops resolve the same absolute
    // paths native sees — matches `compile_and_run_wasm_test`. `-S inherit-env=y`
    // passes the host environment through WASI so `env.get` observes the SAME
    // variables native `std::env::var` does (without it every guest lookup is
    // none — a silent cross-target divergence). Program args go after the module
    // path; wasmtime forwards them to the guest as argv.
    let status = Command::new("wasmtime")
        .arg("--dir=/")
        .arg("-S")
        .arg("inherit-env=y")
        .arg(&wasm_path)
        .args(program_args)
        .status();
    let _ = std::fs::remove_file(&wasm_path);
    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!(
                "error: failed to run wasm module on wasmtime: {}\n  \
                 in `almide run --target wasm {}`\n  \
                 hint: the `wasmtime` CLI must be on PATH to execute wasm \
                 (install: https://wasmtime.dev) — or run natively without --target",
                e, file
            );
            1
        }
    }
}
