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
            let lock_path = project_dir.join(BUILD_LOCK_FILE);
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

    /// [`BuildDirLock::acquire`] that gives up instead of waiting: `None`
    /// means another process is building in this dir right now (#2504's
    /// sweep skips it) or the lockfile could not be opened at all.
    pub(crate) fn try_acquire(project_dir: &std::path::Path) -> Option<Self> {
        #[cfg(any(unix, windows))]
        {
            use fs2::FileExt;
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(project_dir.join(BUILD_LOCK_FILE))
                .ok()?;
            file.try_lock_exclusive().ok()?;
            Some(BuildDirLock { _file: file })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = project_dir;
            Some(BuildDirLock {})
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

/// The shared native build scratch dir of `almide run` / `almide build`:
/// `ALMIDE_RUN_PROJECT_DIR` when set, else `<temp>/almide-run`. One
/// resolution, shared by the builder and by `almide clean` (#2500), so the
/// dir `clean` empties is the dir the builds fill.
pub(crate) fn shared_run_project_dir() -> std::path::PathBuf {
    almide_base::env::var("ALMIDE_RUN_PROJECT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("almide-run"))
}

/// The lockfile name every build scratch dir is serialized on.
pub(crate) const BUILD_LOCK_FILE: &str = ".almide-build.lock";

/// Empty a build scratch dir under its own lock (#2500): waits for a build
/// in flight there to finish, then removes every entry EXCEPT the lockfile.
/// Returns whether anything was removed.
///
/// The lockfile stays on purpose. Removing it would unlink the inode this
/// process (and any builder already blocked on it) holds the flock on, while
/// the next builder creates a fresh lockfile — two builders, two inodes, no
/// mutual exclusion, and the shared `src/main.rs` race the lock exists to
/// prevent is back for exactly one build. An empty lockfile costs nothing.
///
/// What this cannot cover: a lock-free cache HIT (`build_native_cached`
/// returns the `almide-<hash>` path without locking) that execs after the
/// removal. `clean` is the user's own request to drop the cache, so that
/// one run reports "Failed to execute" and the next rebuilds.
pub(crate) fn clear_build_dir(dir: &std::path::Path) -> Result<bool, String> {
    let _lock = BuildDirLock::acquire(dir)?;
    remove_build_dir_contents(dir)
}

/// [`clear_build_dir`] for a caller that must not WAIT: takes the dir's lock
/// without blocking and does nothing if a build holds it (#2504's sweep runs
/// over thousands of worker dirs at the start of `almide test`, and a dir
/// another process is building in is by definition in use). `still_stale` is
/// re-asked UNDER the lock, so a dir that was picked from a stale-looking
/// scan but has since been rebuilt in is left alone.
pub(crate) fn clear_build_dir_if_idle(
    dir: &std::path::Path,
    still_stale: impl FnOnce() -> bool,
) -> bool {
    let Some(_lock) = BuildDirLock::try_acquire(dir) else { return false };
    if !still_stale() {
        return false;
    }
    remove_build_dir_contents(dir).unwrap_or(false)
}

/// Every entry of a build scratch dir except its lockfile. The caller holds
/// the lock.
fn remove_build_dir_contents(dir: &std::path::Path) -> Result<bool, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Failed to read {}: {}", dir.display(), e))?;
    let mut removed = false;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read {}: {}", dir.display(), e))?;
        if entry.file_name() == BUILD_LOCK_FILE {
            continue;
        }
        let path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let result = if is_dir { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        result.map_err(|e| format!("Failed to remove {}: {}", path.display(), e))?;
        removed = true;
    }
    Ok(removed)
}

/// How long a cached artifact may go unused before a sweep evicts it — the
/// `almide-<hash>` binaries of a build dir (#2500) and the whole per-test-file
/// worker dirs of `almide test` (#2504) read the same number. Every cache hit
/// refreshes the binary's mtime (`touch_used`), so "unused" is measured from
/// the last RUN, not the build; the shared dir was 22 GB and the worker tree
/// 39 GB when nothing evicted at all. Seven days keeps a week's edit loop
/// warm and bounds both to a week of distinct programs.
pub(crate) const CACHE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// How often an eviction sweep runs over one cache root. A `readdir` + `stat`
/// of the shared dir's 12,000 binaries and 47,000 kept object files measured
/// 0.5 s warm and 3.5 s cold on the machine that filed #2500, and the worker
/// tree is 4,500 dirs — too much to pay on every build for a threshold
/// measured in days. A stamp file in the root records the last sweep.
const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// The stamp file the sweep's rate limit reads.
const EVICT_STAMP_FILE: &str = ".almide-evict-stamp";

/// Is `root`'s sweep due, i.e. has it not been swept within [`SWEEP_INTERVAL`]?
pub(crate) fn sweep_due(root: &std::path::Path) -> bool {
    let stamp = root.join(EVICT_STAMP_FILE);
    match std::fs::metadata(&stamp).and_then(|m| m.modified()) {
        Ok(t) => std::time::SystemTime::now().duration_since(t).map(|since| since >= SWEEP_INTERVAL).unwrap_or(true),
        Err(_) => true,
    }
}

/// Record that `root` was swept now, so the next sweep waits a day.
pub(crate) fn stamp_sweep(root: &std::path::Path) {
    let _ = std::fs::File::create(root.join(EVICT_STAMP_FILE));
}

/// The prefix of the prebuilt-runtime rlib dirs, directly in the temp dir.
pub(crate) const RTLIB_DIR_PREFIX: &str = "almide-rtlib-";

/// Empty the prebuilt-runtime rlib dirs nothing has linked for
/// [`CACHE_MAX_AGE`], skipping `in_use` (#2504).
///
/// `<temp>/almide-rtlib-<key>` is keyed on the runtime SOURCE + the rustc
/// version + the opt level, so a new one appears on every compiler build
/// that touches the runtime and on every toolchain upgrade, and the old
/// ones are never linked again: 31 dirs / 102 MB on the machine that filed
/// the tree issues, growing ~3 MB per compiler build. They already carry
/// the same `.almide-build.lock` every build scratch dir has (the rlib
/// build takes it), so this is the same protocol as the other two caches:
/// non-blocking lock, staleness re-asked under it, lockfile kept.
///
/// The rate-limit stamp for this sweep lives in the temp dir itself
/// (`<temp>/.almide-evict-stamp`), because these dirs are siblings there
/// rather than children of one cache root.
///
/// Emptying a dir a CONCURRENT process resolved earlier (it caches the path
/// in-process) makes that process's `--extern almide_rt=<path>` fail, and
/// the fast path falls through to the self-contained cargo build — slower,
/// never wrong. A dir being built in right now holds its lock and is
/// skipped.
pub(crate) fn sweep_rtlib_cache(in_use: &std::path::Path) {
    let temp = std::env::temp_dir();
    if !sweep_due(&temp) {
        return;
    }
    let Some(cutoff) = std::time::SystemTime::now().checked_sub(CACHE_MAX_AGE) else { return };
    let Ok(entries) = std::fs::read_dir(&temp) else { return };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with(RTLIB_DIR_PREFIX)
            || !entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
        {
            continue;
        }
        let dir = entry.path();
        if dir == in_use || used_since(&dir, cutoff) {
            continue;
        }
        clear_build_dir_if_idle(&dir, || !used_since(&dir, cutoff));
    }
    stamp_sweep(&temp);
}

/// Was any FILE in `dir` — its own, or a cached binary under
/// `target/<profile>/` — modified since `cutoff`? A cache HIT touches the
/// binary it execs, so this answers "did anyone use this dir", not "did
/// anyone build in it". Stops at the first file newer than `cutoff`.
///
/// Files only: every build writes files (the harness `main.rs`, the rustc
/// output, the content-keyed binary), so a directory's own mtime adds no
/// signal — and whether it moves at all is filesystem-dependent, which is
/// not something a cache-eviction rule should rest on.
pub(crate) fn used_since(dir: &std::path::Path, cutoff: std::time::SystemTime) -> bool {
    let any_newer = |d: std::path::PathBuf| {
        std::fs::read_dir(d)
            .map(|rd| {
                rd.flatten().any(|e| {
                    e.file_type().map(|t| t.is_file()).unwrap_or(false)
                        && e.metadata().and_then(|m| m.modified()).map(|t| t > cutoff).unwrap_or(false)
                })
            })
            .unwrap_or(false)
    };
    any_newer(dir.to_path_buf())
        || any_newer(dir.join("target").join("debug"))
        || any_newer(dir.join("target").join("release"))
}

/// Refresh a cached binary's mtime to "now" so the age-based eviction sees it
/// as used. Best effort, lock-free, and through a READ handle: a write handle
/// on an executable is `ETXTBSY` against the exec racing it (Linux), and the
/// sharing violation on Windows. `futimens` on a read-only fd needs only
/// ownership, which the process that cached the file has; on Windows the
/// handle asks for `FILE_WRITE_ATTRIBUTES` alongside `GENERIC_READ`, which
/// `SetFileTime` needs and which does not conflict with an exec.
pub(crate) fn touch_used(path: &std::path::Path) {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const GENERIC_READ: u32 = 0x8000_0000;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        opts.access_mode(GENERIC_READ | FILE_WRITE_ATTRIBUTES);
    }
    if let Ok(file) = opts.open(path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
}

/// Evict the build dir's stale cached artifacts (#2500). Runs under the
/// caller's `BuildDirLock`, at most once per `EVICT_SWEEP_INTERVAL`.
///
/// What goes: in `target/<profile>/`, every `almide-*` FILE (the content-
/// keyed binaries, a crashed process's `almide-<hash>.stage-<pid>` leftover,
/// the `almide-out` cargo output) whose mtime is older than
/// `RUN_CACHE_MAX_AGE`; in `target/<profile>/deps/`, every `almide_out-*`
/// file that old — on macOS cargo's default `split-debuginfo=unpacked`
/// makes rustc KEEP every `*.rcgu.o` it linked, one set per distinct
/// program, and they were 11 GB of the 22 GB. All of it is regenerated by
/// the next build that needs it; a kept object only feeds the debug map of
/// a binary already evicted or about to be. Directories are never touched:
/// the incremental session store is the ICE recovery's alone, and it is
/// bounded by rustc's own garbage collection.
///
/// Why age and not a size cap: the removal is lock-free-hit safe only
/// because a hit touches the binary first — a binary nobody has run for a
/// week is the one thing no process is about to exec. The residual window
/// (a hit's `exists()` check racing this sweep's `remove_file` on a binary
/// unused for exactly a week) is one failed run that rebuilds on retry.
fn evict_stale_artifacts(project_dir: &std::path::Path) {
    if !sweep_due(project_dir) {
        return;
    }
    let now = std::time::SystemTime::now();
    let is_stale = |meta: &std::fs::Metadata| {
        meta.modified().ok().and_then(|t| now.duration_since(t).ok()).is_some_and(|age| age > CACHE_MAX_AGE)
    };
    let sweep = |dir: &std::path::Path, prefix: &str| {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with(prefix) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_file() && is_stale(&meta) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    };
    for profile in ["debug", "release"] {
        let dir = project_dir.join("target").join(profile);
        sweep(&dir, "almide-");
        sweep(&dir.join("deps"), "almide_out-");
    }
    stamp_sweep(project_dir);
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
        .unwrap_or_else(shared_run_project_dir);
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
    let _guard = project_dir_override.is_none().then(|| BUILD_LOCK.lock().unwrap());
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

    // One rustc ICE on a stale incremental session clears the session store
    // (under this same lock) and rebuilds once; see `build_recovering_from_ice`.
    let result = super::cargo_build::build_recovering_from_ice(&project_dir, || {
        if use_test_harness {
            cargo_build_test_with_native(rs_code, &project_dir, native_deps, source_root)
        } else {
            cargo_build_generated_with_native(rs_code, &project_dir, release, native_deps, source_root)
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
    let (bytes, structural, _host_ops) = match super::build::compile_to_wasm_bytes(file, false, verified, false, true) {
        Ok(b) => b,
        Err(()) => return 1,
    };

    // Structural-leg modules import `almide.*` and execute on the EMBEDDED
    // host — the exact host the 610/610 corpus acceptance measured (fs, env
    // and stdin included), so `run --target wasm` reproduces the measured
    // bytes without an external runtime. Program args stay unsupported on
    // this leg the honest way: a program that READS them walls at emit.
    if structural {
        // #2275: a declared `@extern(wasm, ..)` import has no host here —
        // say so, instead of wasmtime's "unknown import" at instantiation.
        if let Some((module, name)) = foreign_import(&bytes) {
            err(&format!(
                "error: this program imports `{module}.{name}` (an `@extern(wasm, \"{module}\", \"{name}\")` declaration), and `almide run --target wasm` has no host for it\n  \
                 hint: `almide build {file} --target wasm --host js` writes the module with a JS host next to it — run it under node or in a page, where `init({{ js: {{ {name} }} }})` serves the import"
            ));
            return 1;
        }
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
        };
    }

    // ALMIDE_WASM_ALLOC_COUNT (#2407) is a structural-leg instrument: the
    // incumbent module below carries no counters, and the wasmtime CLI
    // reads no globals — say so rather than print nothing.
    if almide_base::env::flag("ALMIDE_WASM_ALLOC_COUNT") {
        eprintln!("__ALMD_WASM_ALLOC absent (this program took the incumbent wasm leg, which carries no counters)");
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
