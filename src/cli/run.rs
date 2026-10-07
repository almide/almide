use std::process::Command;
use crate::try_compile;
use crate::err;
use super::{hash64, cargo_build_generated_with_native, cargo_build_test_with_native};
use super::build_dir::{evict_stale_artifacts, shared_run_project_dir, touch_used, BuildDirLock};

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
    // ALMIDE_ALLOC_COUNT (#2228): the counting allocator, when armed.
    let rs_code = super::build::arm_alloc_count(rs_code);

    // Load native deps from almide.toml (search in input file's directory, then CWD).
    // source_root is the directory containing almide.toml (where native/ lives).
    let (native_deps, source_root) = super::load_native_build_config(file);

    // #2370: one predicate, shared with the auto-`main` guards. This site was
    // already anchored; the guards were not, and they drifted apart.
    let use_test_harness = test_mode || !super::cargo_build::defines_entry_point(&rs_code);
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
        Self { on: almide_base::env::flag("ALMIDE_TIME_PHASES"), start: now, last: std::cell::Cell::new(now) }
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
/// cache: the key is the generated code itself (+ harness/profile/deps/target and every
/// file copied into the crate, see `CrateInputs`), never
/// the caller's source path. Identical generated code from ANY entry point —
/// `almide run`, `almide build`, a test harness compiling from a fresh tempdir —
/// reuses one cached binary and skips cargo entirely. (The hit test was
/// previously gated on a per-source-path side file, so path-unstable callers
/// like the 268-fixture cross-target gate paid a full rustc per fixture per
/// run even when the generated code was byte-identical.)
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
        .unwrap_or_else(shared_run_project_dir);
    std::fs::create_dir_all(&project_dir)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;

    // Deps and source_root shape the generated Cargo.toml (and thus the built
    // binary), so they are part of the key: the same rs_code built against
    // different [native-deps] must not collide on one cache entry.
    let dep_key = native_deps.iter()
        .map(|d| format!("{}={}@{}", d.name, d.spec, d.target.as_deref().unwrap_or("")))
        .collect::<Vec<_>>()
        .join(",");
    // Everything copied INTO the crate besides `rs_code` — the package's and
    // every dependency package's `native/` tree, and the dependencies'
    // `[native-deps]` — is collected once here, hashed into the key, and the
    // SAME value is what the build writes (#887, #3091). Keyed by anything
    // narrower, editing an input the key did not see was a cache hit that
    // shipped the previous binary with exit 0.
    let inputs = super::cargo_build::CrateInputs::collect(source_root)?;
    inputs.warn_legacy_callbacks(rs_code);
    let native_key = inputs.cache_key();
    // The target triple (#2772) is part of the identity too: a musl build of
    // the same code is a different binary, and keyed without it `almide build
    // --target x86_64-unknown-linux-musl` would be a hit on the host binary.
    let triple = super::native_target::cross_target().unwrap_or_default();
    // And what shapes the binary from OUTSIDE the crate (#3091): the build
    // recipe, the toolchain, the flags and cargo config it reads.
    let env_key = super::cargo_build::build_environment_key(&project_dir);
    let hash_input = format!(
        "{}:test={}:release={}:deps={}:root={:?}:native={}:target={}:env={}",
        &rs_code, use_test_harness, release, dep_key, source_root, native_key, triple, env_key
    );
    let code_hash = format!("{:016x}", hash64(hash_input.as_bytes()));
    let profile_dir = if release { "release" } else { "debug" };
    let bin_path = project_dir.join("target").join(profile_dir).join(format!("almide-{}", code_hash));

    // The binary's NAME is its full content key and it lands via atomic rename
    // (below), so bare existence is a complete, lock-free cache hit. The
    // touch is what keeps a hit out of the age-based eviction (#2500).
    if bin_path.exists() {
        touch_used(&bin_path);
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
    // The mutex guards no data — only the window — so a lock poisoned by a
    // build that panicked inside it is still the right lock to take.
    let _guard = project_dir_override
        .is_none()
        .then(|| BUILD_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    let _flock = BuildDirLock::acquire(&project_dir)?;

    // Re-check the cache under the lock: another process/thread may have built
    // this exact binary while we waited, making a rebuild redundant.
    if bin_path.exists() {
        touch_used(&bin_path);
        return Ok(bin_path);
    }

    // Housekeeping before the build, so a dir full of week-old binaries is
    // relieved before rustc needs the space (#2500). Under the lock: no
    // build is writing here, and the removal never touches a directory.
    evict_stale_artifacts(&project_dir);
    // And by total size (#2608): the age rule never fires inside a burst of
    // distinct programs. Same lock, oldest artifacts first.
    if let Some(cap) = super::cache_bound::cache_max_bytes() {
        super::cache_bound::bound_build_artifacts(&project_dir, cap);
    }

    // One rustc ICE on a stale incremental session clears the session store
    // (under this same lock) and rebuilds once; see `build_recovering_from_ice`.
    let result = super::cargo_build::build_recovering_from_ice(&project_dir, || {
        if use_test_harness {
            cargo_build_test_with_native(rs_code, &project_dir, native_deps, source_root, &inputs)
        } else {
            cargo_build_generated_with_native(rs_code, &project_dir, release, native_deps, source_root, &inputs)
        }
    });

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

/// Run a compiled binary with the given args, returning exit code. Unix
/// execs instead ([`exec_binary`]).
#[cfg(not(unix))]
pub fn run_binary(bin: &std::path::Path, program_args: &[String]) -> i32 {
    with_exec_retry(|| binary_command(bin, program_args).status())
        .map_or(1, |s| s.code().unwrap_or(1))
}

/// `almide run`'s native launch (#2809): the program REPLACES the launcher.
///
/// A launcher that spawned the program and waited kept a pid of its own, and a
/// signal sent to that pid alone — a supervisor's SIGTERM, a `kill <pid>` —
/// ended the launcher and left the program running, so the shutdown drain
/// `http.serve` does on SIGTERM/SIGINT (#2692) never started. Forwarding the
/// signal from a waiting launcher was the other design, and it is wrong the
/// moment the signal also reaches the program directly: a terminal Ctrl-C and
/// a `kill -- -<pgid>` or cgroup-wide stop signal the whole group, so the
/// program would see each signal TWICE — and a second signal is the forced
/// stop (exit 1, no drain). `exec` has no such double: the launcher's pid is
/// the program's, every signal (SIGTERM, SIGINT, SIGHUP, …) arrives once with
/// the program's own disposition, and the status the caller reaps is the
/// program's own — an exit code as C-350 passes it, or a death by signal
/// reported as a death by signal, never folded into a 128+n that C-350 lets
/// `process.exit` produce on purpose.
///
/// Only a failed exec returns: then the ETXTBSY back-off retries it, and any
/// other failure is reported and exits 1 as before.
#[cfg(unix)]
fn exec_binary(bin: &std::path::Path, program_args: &[String]) -> i32 {
    use std::io::Write as _;
    use std::os::unix::process::CommandExt as _;
    // exec discards the launcher's unflushed buffers.
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    let _: Option<()> = with_exec_retry(|| Err(binary_command(bin, program_args).exec()));
    1
}

/// Windows has no exec: the launcher waits for the program. A console Ctrl-C
/// or Ctrl-Break reaches every process attached to the console, the program
/// included, so nothing is forwarded; the launcher only ignores it, so that it
/// does not exit before the program has drained and exited (#2809).
#[cfg(not(unix))]
fn exec_binary(bin: &std::path::Path, program_args: &[String]) -> i32 {
    #[cfg(windows)]
    let _ = ctrlc::set_handler(|| {});
    run_binary(bin, program_args)
}

/// [`run_binary`], capturing stdout+stderr instead of inheriting them.
///
/// `almide test` needs the bytes to build its structured failure report, and
/// capturing is what makes a parallel suite's output DETERMINISTIC: each file's
/// output is printed whole, in sorted file order, instead of interleaving live
/// with every other worker (agents diff one run against the next).
pub fn run_binary_captured(bin: &std::path::Path, program_args: &[String]) -> (i32, String) {
    let (code, stdout, stderr) = run_binary_captured_io(bin, program_args);
    (code, stdout + &stderr)
}

/// [`run_binary_captured`] with the two streams kept apart: `almide test`
/// attributes a test's stdout to the test from libtest's markers, which only
/// the stdout stream carries (#2538).
pub fn run_binary_captured_io(bin: &std::path::Path, program_args: &[String]) -> (i32, String, String) {
    let Some(out) = with_exec_retry(|| binary_command(bin, program_args).output()) else {
        return (1, String::new(), String::new());
    };
    (
        out.status.code().unwrap_or(1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `almide run`'s native leg: compile + run one file, with the D5 dual-time
/// report leg (`--time-report`).
fn cmd_run_native(args: &RunArgs) -> i32 {
    let RunArgs { file, program_args, no_check, release, native_verified, time_report, .. } = *args;
    match compile_to_binary_with(file, no_check, false, release, None, native_verified) {
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
                exec_binary(&bin, program_args)
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
#[derive(Clone, Copy)]
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
    let RunArgs { file, program_args, target, verified, time_report, .. } = args;
    let code = match target {
        // Default and explicit native target: the cargo/rustc path.
        None | Some("rust") | Some("native") => cmd_run_native(&args),
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

/// The first function import of `bytes` outside the `almide.*` host contract
/// (#2275): a program's own `@extern(wasm, ..)` declaration.
fn foreign_import(bytes: &[u8]) -> Option<(String, String)> {
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let Ok(wasmparser::Payload::ImportSection(r)) = payload {
            for i in r.into_imports().flatten() {
                if matches!(i.ty, wasmparser::TypeRef::Func(_)) && i.module != "almide" {
                    return Some((i.module.to_string(), i.name.to_string()));
                }
            }
        }
    }
    None
}

/// Build `file` to a wasm module and execute it on the embedded host.
///
/// The same host `almide test`'s wasm leg runs on (#3046), so the observable
/// behavior matches `almide test --target wasm` and the `spec/wasm_cross`
/// gate. Program args after `--` are forwarded to the guest.
/// `wasmtime`'s own exit code is propagated unchanged, so a guest
/// `proc_exit(n)` surfaces as `n` exactly as a native binary's exit would.
fn cmd_run_wasm(file: &str, program_args: &[String], verified: bool, time_report: bool) -> i32 {
    // #2752: the deterministic meter (ADR-0001 D5, the `time:` line) is
    // native-only now — its wasm twin was the incumbent renderer's Σ-probe
    // instrumentation, and the structural leg carries no charge trace.
    // Refuse by name rather than print a report with half its line missing.
    if time_report || almide_base::env::flag("ALMIDE_FUEL_PROBE") {
        err("error: `--time-report` (the ALMIDE_FUEL_PROBE charge meter) is native-only: its deterministic meter has no wasm twin since the incumbent wasm renderer was retired (#2752)");
        err(&format!("  hint: `almide run {file} --time-report` reports both times natively; `ALMIDE_WASM_ALLOC_COUNT=1 almide run {file} --target wasm` measures the wasm run's allocations"));
        return 2;
    }
    // `run` does not expose the `--emit-unverified` waiver: running a module that
    // failed the Perceus RC gate would silently execute leaky/double-freeing code,
    // so a verification failure is always a hard error here. The waiver is
    // build-only (you opt into shipping a known-bad artifact, not into running it).
    let (bytes, _host_ops) = match super::wasm_compile::compile_to_wasm_bytes(file, false, verified, false, true) {
        Ok(b) => b,
        Err(()) => return 1,
    };

    // The module imports `almide.*` and executes on the EMBEDDED host — the
    // exact host the corpus acceptance measured (fs, env and stdin
    // included), so `run --target wasm` reproduces the measured bytes
    // without an external runtime.
    // #2275: a declared `@extern(wasm, ..)` import has no host here — say
    // so, instead of wasmtime's "unknown import" at instantiation.
    if let Some((module, name)) = foreign_import(&bytes) {
        err(&format!(
            "error: this program imports `{module}.{name}` (an `@extern(wasm, \"{module}\", \"{name}\")` declaration), and `almide run --target wasm` has no host for it\n  \
             hint: `almide build {file} --target wasm --host js` writes the module with a JS host next to it — run it under node or in a page, where `init({{ js: {{ {name} }} }})` serves the import"
        ));
        return 1;
    }
    match almide_wasm_run::run_wasm_real_stdin_args(&bytes, program_args) {
        Ok(r) => {
            print!("{}", r.stdout);
            eprint!("{}", r.stderr);
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
            // ALMIDE_WASM_ALLOC_COUNT (#2407): the counters the armed
            // module carried, in native's `__ALMD_ALLOC` line form.
            if almide_base::env::flag("ALMIDE_WASM_ALLOC_COUNT") {
                match r.alloc_count {
                    Some(c) => eprintln!(
                        "__ALMD_WASM_ALLOC {c} heap_end={}",
                        r.heap_end.map_or_else(|| "?".to_string(), |h| h.to_string())
                    ),
                    None => eprintln!("__ALMD_WASM_ALLOC absent (the module carries no counters)"),
                }
            }
            r.exit.clamp(0, 255)
        }
        Err(e) => {
            err(&format!("error: embedded wasm host: {e}"));
            1
        }
    }
}
