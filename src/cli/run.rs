use std::process::Command;
use crate::try_compile;
use crate::err;
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
    let t = PhaseTimer::start();
    let rs_code = try_compile(file, no_check).map_err(|_| "compile failed".to_string())?;
    t.lap("frontend+emit");
    let rs_code = if native_verified && !test_mode {
        super::render_v1_native_or_fallback(file, rs_code)
    } else {
        // The NATIVE TEST harness rides v0, which has no deterministic meter:
        // a budget/timeout prim reaching it would die later as an opaque
        // rustc E0425 in generated code. Refuse with the real reason instead
        // (the wasm test leg is the metered one; it runs first by default).
        if test_mode
            && (rs_code.contains("almide_rt_prim_budget_")
                || rs_code.contains("almide_rt_prim_timeout_"))
        {
            return Err(
                "fan.bounded / fan.race / fan.timeout tests run on the WASM test leg \
                 (the native test harness has no deterministic meter). This file fell \
                 back to the native harness, so its wasm render declined — fix that \
                 wall (run with ALMIDE_WALL_REASON=1 to see it)"
                    .to_string(),
            );
        }
        rs_code
    };
    t.lap("v1-native-render");

    // Load native deps from almide.toml (search in input file's directory, then CWD).
    // source_root is the directory containing almide.toml (where native/ lives).
    let (native_deps, source_root) = super::load_native_build_config(file);

    let use_test_harness = test_mode || (!rs_code.contains("\nfn almide_main(") && !rs_code.contains("\nfn main(") && !rs_code.contains("\npub fn main("));
    let out = build_native_cached(&rs_code, use_test_harness, release, project_dir_override, &native_deps, source_root.as_deref());
    t.lap("cargo");
    out
}

/// Phase timing for the edit loop, behind `ALMIDE_TIME_PHASES=1`.
///
/// Unit 0.49 spent two retractions attributing the ~4s edit-loop cost by re-running the
/// phases as separate commands and summing them: the parts came to 0.86s against a 3.98s
/// whole, because a separately-invoked phase can hit a cache the real pipeline misses. This
/// measures the pipeline itself, which is the only thing that can be wrong about it.
pub(crate) struct PhaseTimer {
    on: bool,
    start: std::time::Instant,
    last: std::cell::Cell<std::time::Instant>,
}

impl PhaseTimer {
    pub(crate) fn start() -> Self {
        let now = std::time::Instant::now();
        Self { on: std::env::var_os("ALMIDE_TIME_PHASES").is_some(), start: now, last: std::cell::Cell::new(now) }
    }
    pub(crate) fn lap(&self, label: &str) {
        if !self.on { return; }
        let now = std::time::Instant::now();
        eprintln!(
            "[phase] {:<18} {:>7.0}ms   (cumulative {:>7.0}ms)",
            label,
            now.duration_since(self.last.get()).as_secs_f64() * 1000.0,
            now.duration_since(self.start).as_secs_f64() * 1000.0,
        );
        self.last.set(now);
    }
}

/// Build a native binary from GENERATED Rust source through a content-addressed
/// cache: the key is the generated code itself (+ harness/profile/deps), never
/// the caller's source path. Identical generated code from ANY entry point —
/// `almide run`, `almide build`, a test harness compiling from a fresh tempdir —
/// reuses one cached binary and skips cargo entirely. (The hit test was
/// previously gated on a per-source-path side file, so path-unstable callers
/// like the 268-fixture cross-target gate paid a full rustc per fixture per
/// run even when the generated code was byte-identical.)
/// A content digest of every file under `<root>/native/` (recursively — asset
/// subdirectories travel with the modules and are `include_str!`d by them, so
/// they shape the binary too). Sorted by path so the digest is deterministic.
/// Empty when there is no `native/` directory.
fn native_sources_key(root: &std::path::Path) -> String {
    fn walk(dir: &std::path::Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if let Ok(bytes) = std::fs::read(&p) {
                out.push((p, bytes));
            }
        }
    }
    let native = root.join("native");
    if !native.is_dir() {
        return String::new();
    }
    let mut files = Vec::new();
    walk(&native, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut acc = String::new();
    for (path, bytes) in files {
        acc.push_str(&format!("{}:{:016x};", path.display(), hash64(&bytes)));
    }
    acc
}

pub(crate) fn build_native_cached(
    rs_code: &str,
    use_test_harness: bool,
    release: bool,
    project_dir_override: Option<&std::path::Path>,
    native_deps: &[crate::project::NativeDep],
    source_root: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    // Scratch dir. A per-call `project_dir_override` (one dir per test file)
    // gives each parallel worker its own `src/main.rs`, so cold rustc builds
    // run truly in parallel instead of serializing on the shared dir's
    // `BUILD_LOCK`. Otherwise: `ALMIDE_RUN_PROJECT_DIR`, else a shared default.
    let project_dir = project_dir_override.map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("ALMIDE_RUN_PROJECT_DIR").map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::env::temp_dir().join("almide-run"));
    std::fs::create_dir_all(&project_dir)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;

    // Deps and source_root shape the generated Cargo.toml (and thus the built
    // binary), so they are part of the key: the same rs_code built against
    // different [native-deps] must not collide on one cache entry.
    let dep_key = native_deps.iter()
        .map(|d| format!("{}={}", d.name, d.spec))
        .collect::<Vec<_>>()
        .join(",");
    // The `native/*.rs` modules are compiled INTO the binary, so their
    // CONTENTS are part of its identity (#887). Keyed only by the source_root
    // PATH, editing a native module was a cache hit: nothing recompiled and
    // `almide build` reported success while shipping the previous binary —
    // exit 0 even with syntactically invalid Rust in the module.
    let native_key = source_root.map(native_sources_key).unwrap_or_default();
    let hash_input = format!(
        "{}:test={}:release={}:deps={}:root={:?}:native={}",
        &rs_code, use_test_harness, release, dep_key, source_root, native_key
    );
    let code_hash = format!("{:016x}", hash64(hash_input.as_bytes()));
    let profile_dir = if release { "release" } else { "debug" };
    let bin_path = project_dir.join("target").join(profile_dir).join(format!("almide-{}", code_hash));

    // The binary's NAME is its full content key and it lands via atomic rename
    // (below), so bare existence is a complete, lock-free cache hit.
    if bin_path.exists() {
        return Ok(bin_path);
    }

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
    if bin_path.exists() {
        return Ok(bin_path);
    }

    let result = if use_test_harness {
        cargo_build_test_with_native(&rs_code, &project_dir, native_deps, source_root)
    } else {
        cargo_build_generated_with_native(&rs_code, &project_dir, release, native_deps, source_root)
    };

    match result {
        Ok(built_path) => {
            // Copy the built binary to its content-keyed cached path. The
            // bare-rustc fast path doesn't create a cargo `target/<profile>/`
            // dir, so ensure bin_path's parent exists, and surface a copy
            // failure instead of silently leaving bin_path missing (→ "Failed
            // to execute" at run).
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
            Ok(bin_path)
        }
        Err(e) => Err(e),
    }
}

/// The launcher's own working directory, exported to the child as
/// `ALMIDE_CWD` on BOTH targets. The wasm guest resolves relative fs paths
/// against it (the `PWD` env var can be STALE when a parent process sets the
/// child cwd without updating it — Node `execFileSync(..., {cwd})`, IDE run
/// configs — #874); the native child gets the same variable so `env.get`
/// observes an identical environment across targets.
pub fn almide_cwd() -> Option<String> {
    std::env::current_dir()
        .ok()
        .and_then(|d| d.to_str().map(|s| s.to_string()))
}

/// The launch command for a compiled binary — shared by the inheriting and the
/// capturing runners so they cannot drift in environment.
fn binary_command(bin: &std::path::Path, program_args: &[String]) -> Command {
    let mut cmd = Command::new(bin);
    cmd.env("RUST_MIN_STACK", "8388608");
    if let Some(cwd) = almide_cwd() {
        cmd.env("ALMIDE_CWD", cwd);
    }
    cmd.args(program_args);
    cmd
}

/// Run `attempt` through the ETXTBSY back-off. Belt for the parallel-test race
/// (the staging rename in `build_native_cached` is the root fix): if another
/// thread's stale write handle still overlaps the exec, back off briefly and
/// retry instead of failing the whole suite.
fn with_exec_retry<T>(mut attempt: impl FnMut() -> std::io::Result<T>) -> Option<T> {
    let mut delay = std::time::Duration::from_millis(20);
    for _ in 0..6 {
        match attempt() {
            Ok(v) => return Some(v),
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(delay);
                delay *= 2;
            }
            Err(e) => {
                err(&format!("Failed to execute: {}", e));
                std::process::exit(1);
            }
        }
    }
    err(&format!("Failed to execute: Text file busy (persisted after retries)"));
    None
}

/// Run a compiled binary with the given args, returning exit code.
pub fn run_binary(bin: &std::path::Path, program_args: &[String]) -> i32 {
    with_exec_retry(|| binary_command(bin, program_args).status())
        .map_or(1, |s| s.code().unwrap_or(1))
}

/// [`run_binary`], capturing stdout+stderr instead of inheriting them.
///
/// `almide test` needs the bytes to build its structured failure report, and
/// capturing is what makes a parallel suite's output DETERMINISTIC: each file's
/// output is printed whole, in sorted file order, instead of interleaving live
/// with every other worker (agents diff one run against the next).
pub fn run_binary_captured(bin: &std::path::Path, program_args: &[String]) -> (i32, String) {
    let Some(out) = with_exec_retry(|| binary_command(bin, program_args).output()) else {
        return (1, String::new());
    };
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(1), text)
}

/// Compile + run one file, with the D5 dual-time report leg (`--time-report`).
fn cmd_run_inner_report(file: &str, program_args: &[String], no_check: bool, test_mode: bool, release: bool, native_verified: bool, time_report: bool) -> i32 {
    match compile_to_binary_with(file, no_check, test_mode, release, None, native_verified) {
        Ok(bin) => {
            if time_report {
                let mut cmd = Command::new(&bin);
                cmd.env("RUST_MIN_STACK", "8388608");
                if let Some(cwd) = almide_cwd() {
                    cmd.env("ALMIDE_CWD", cwd);
                }
                cmd.args(program_args);
                run_with_time_report(cmd)
            } else {
                run_binary(&bin, program_args)
            }
        }
        Err(e) => {
            err(&format!("Compile error:\n{}", e));
            1
        }
    }
}

/// Run `cmd` with stderr captured (stdout stays inherited), swallow the raw
/// `__ALMD_PROBE` line, and print the ADR-0001 D5 dual-time line: the
/// deterministic time (consumed charge units × CM-1) next to the measured
/// wall clock. The two never claim to be the same quantity — the declared
/// band between them is D5's ratio-only contract.
fn run_with_time_report(mut cmd: Command) -> i32 {
    let t0 = std::time::Instant::now();
    let child = match cmd.stderr(std::process::Stdio::piped()).spawn() {
        Ok(c) => c,
        Err(e) => {
            err(&format!("Failed to execute: {}", e));
            return 1;
        }
    };
    let out = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            err(&format!("Failed to execute: {}", e));
            return 1;
        }
    };
    let wall_ns = t0.elapsed().as_nanos() as i64;
    let mut consumed: Option<i64> = None;
    for line in String::from_utf8_lossy(&out.stderr).lines() {
        if let Some(rest) = line.strip_prefix("__ALMD_PROBE ") {
            consumed = rest.split_whitespace().next().and_then(|s| s.parse().ok());
        } else {
            eprintln!("{line}");
        }
    }
    match consumed {
        Some(units) => {
            let det_ms =
                units as f64 * almide_mir::charge_probe::CM1_NS_PER_CHARGE as f64 / 1e6;
            let wall_ms = wall_ns as f64 / 1e6;
            eprintln!("time: {det_ms:.3}ms deterministic (≈{wall_ms:.3}ms wall here)");
        }
        None => {
            eprintln!("time: no deterministic meter in this run (probe line missing)");
        }
    }
    out.status.code().unwrap_or(1)
}

/// Flags for [`cmd_run`] — bundled into one struct (was 7 positional
/// params, a max-params violation) so the function signature stays under
/// the params threshold. Field names match `dispatch_run`'s locals 1:1.
pub struct RunArgs<'a> {
    pub file: &'a str,
    pub program_args: &'a [String],
    pub no_check: bool,
    pub release: bool,
    pub target: Option<&'a str>,
    pub verified: bool,
    pub native_verified: bool,
    /// ADR-0001 D5 dual-time report: compile with the deterministic meter and
    /// print `time: <det>ms deterministic (≈<wall>ms wall here)` after the run.
    pub time_report: bool,
}

pub fn cmd_run(args: RunArgs) {
    let RunArgs { file, program_args, no_check, release, target, verified, native_verified, time_report } = args;
    let code = match target {
        // Default and explicit native target: the cargo/rustc path.
        None | Some("rust") | Some("native") => cmd_run_inner_report(file, program_args, no_check, false, release, native_verified, time_report),
        // WASM target: build the same module `almide build --target wasm`
        // emits, then execute it on the `wasmtime` CLI. Both targets must
        // produce byte-identical stdout/stderr/exit — the cross-target gate.
        Some("wasm") | Some("wasm32") | Some("wasi") => cmd_run_wasm(file, program_args, verified, time_report),
        Some(other) => {
            err(&format!(
                "error: unknown run target '{}'\n  \
                 in `almide run --target {}`\n  \
                 supported targets: rust (default, native binary), wasm (wasmtime)\n  \
                 hint: drop --target to run natively, or use `--target wasm`",
                other, other
            ));
            1
        }
    };
    std::process::exit(code);
}

/// The preopen strategy per host OS (#1066). Unix mirrors native absolute
/// paths by preopening the host root (`--dir=/`). Windows has no "/" to
/// preopen: the guest gets the CWD (relative fs paths keep working) plus the
/// host's real temp dir mapped at the WASI `/tmp` convention, with `TMPDIR`
/// steering `fs.temp_dir`/`env.temp_dir` there (the guest-side rule is
/// `$TMPDIR ?? "/tmp"`, C-189 — the explicit `--env` wins over inherit-env,
/// which on Windows would carry no TMPDIR at all). Shared by `cmd_run_wasm`
/// and the wasm test harness so both legs see one filesystem contract.
pub(crate) fn wasmtime_fs_args(cmd: &mut Command) {
    if cfg!(windows) {
        cmd.arg("--dir=.");
        // GetTempPath answers with a trailing separator; wasmtime's
        // `HOST::GUEST` mapping wants the bare directory.
        let tmp = std::env::temp_dir();
        let tmp = tmp.to_string_lossy();
        cmd.arg(format!("--dir={}::/tmp", tmp.trim_end_matches(['\\', '/'])));
        cmd.arg("--env=TMPDIR=/tmp");
        // The #874 cwd pin, Windows spelling: a host-absolute ALMIDE_CWD
        // (`D:\a\…`) can never match a guest preopen, and the inherited PWD
        // is a git-bash unix-style host path — equally unmatchable. "." IS
        // the launcher cwd here (the `--dir=.` preopen), so the guest's
        // relative-path prefix becomes `./…` and resolves inside it.
        cmd.arg("--env=ALMIDE_CWD=.");
    } else {
        cmd.arg("--dir=/");
    }
}

/// Build `file` to a wasm32-wasi module and execute it on the `wasmtime` CLI.
///
/// Mirrors the test runner's wasm invocation (`wasmtime --dir=/ <module>`) so
/// the observable behavior matches `almide test --target wasm` and the
/// `spec/wasm_cross` gate. Program args after `--` are forwarded to the guest.
/// `wasmtime`'s own exit code is propagated unchanged, so a guest
/// `proc_exit(n)` surfaces as `n` exactly as a native binary's exit would.
fn cmd_run_wasm(file: &str, program_args: &[String], verified: bool, time_report: bool) -> i32 {
    // `run` does not expose the `--emit-unverified` waiver: running a module that
    // failed the Perceus RC gate would silently execute leaky/double-freeing code,
    // so a verification failure is always a hard error here. The waiver is
    // build-only (you opt into shipping a known-bad artifact, not into running it).
    let (bytes, structural) = match super::build::compile_to_wasm_bytes(file, false, verified, false) {
        Ok(b) => b,
        Err(()) => return 1,
    };

    // Structural-leg modules import `almide.*` and execute on the EMBEDDED
    // host — the exact host the 610/610 corpus acceptance measured (fs, env
    // and stdin included), so `run --target wasm` reproduces the measured
    // bytes without an external runtime. Program args stay unsupported on
    // this leg the honest way: a program that READS them walls at emit.
    if structural {
        let started = std::time::Instant::now();
        return match almide_wasm_run::run_wasm_real_stdin_args(&bytes, program_args) {
            Ok(r) => {
                print!("{}", r.stdout);
                eprint!("{}", r.stderr);
                use std::io::Write as _;
                let _ = std::io::stdout().flush();
                if time_report {
                    eprintln!("[almide] wall {} ms (embedded wasmtime)", started.elapsed().as_millis());
                }
                r.exit.clamp(0, 255)
            }
            Err(e) => {
                err(&format!("error: embedded wasm host: {e}"));
                1
            }
        };
    }

    // Stage the module under a per-content temp name so concurrent `almide run`
    // invocations never race on one path (the build scratch dir is shared).
    let wasm_name = format!("almide-run-{:016x}.wasm", hash64(&bytes));
    let wasm_path = std::env::temp_dir().join(wasm_name);
    if let Err(e) = std::fs::write(&wasm_path, &bytes) {
        err(&format!("error: failed to stage wasm module {}: {}", wasm_path.display(), e));
        return 1;
    }

    // Preopens per host (#1066) + `-S inherit-env=y`, which passes the host
    // environment through WASI so `env.get` observes the SAME variables native
    // `std::env::var` does (without it every guest lookup is none — a silent
    // cross-target divergence). Program args go after the module path;
    // wasmtime forwards them to the guest as argv.
    let mut cmd = Command::new("wasmtime");
    wasmtime_fs_args(&mut cmd);
    cmd.arg("-S").arg("inherit-env=y");
    // The guest resolves relative fs paths against ALMIDE_CWD (in preference
    // to a possibly-stale inherited PWD — #874); `--env` overrides win over
    // `inherit-env`, so this pins the real launcher cwd either way. On
    // Windows `wasmtime_fs_args` already pinned the guest spelling (`.`);
    // a host-absolute path here would shadow it with an unmatchable one.
    if !cfg!(windows) {
        if let Some(cwd) = almide_cwd() {
            cmd.arg(format!("--env=ALMIDE_CWD={}", cwd));
        }
    }
    cmd.arg(&wasm_path).args(program_args);
    if time_report {
        // Wall time here includes wasmtime's own module compile (~ms scale) —
        // honest for a "wall here" report, and the deterministic side is
        // unaffected (it comes from the guest's own meter).
        let code = run_with_time_report(cmd);
        let _ = std::fs::remove_file(&wasm_path);
        return code;
    }
    let status = cmd.status();
    let _ = std::fs::remove_file(&wasm_path);
    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            err(&format!(
                "error: failed to run wasm module on wasmtime: {}\n  \
                 in `almide run --target wasm {}`\n  \
                 hint: the `wasmtime` CLI must be on PATH to execute wasm \
                 (install: https://wasmtime.dev) — or run natively without --target",
                e, file
            ));
            1
        }
    }
}
