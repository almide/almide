//! The native build scratch dirs: the cross-process lock every build holds,
//! the shared dir `almide run` / `almide build` fill, emptying a dir under its
//! lock (`almide clean`), and the age-based eviction that bounds the caches.

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
pub(super) fn evict_stale_artifacts(project_dir: &std::path::Path) {
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
