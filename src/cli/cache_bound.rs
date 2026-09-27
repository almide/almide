//! Size bounds for the native build caches (#2608).
//!
//! #2500 and #2504 bound the caches by AGE: an artifact nothing has used for
//! `run::CACHE_MAX_AGE` (a week) is evicted, at most once a day. Nothing
//! bounded the total. A workload that tests many DISTINCT files — a task
//! bank, a corpus sweep, one suite from several worktrees — grows
//! `$TMPDIR/almide-test/native/` by one worker dir per absolute test-file
//! path (~12 MB each), and the age rule never fires inside the burst: on
//! 2026-09-24 it reached 7,144 dirs / 84.9 GB in about two hours and the
//! volume hit ENOSPC.
//!
//! The bound here is LRU by total bytes, on each cache root separately:
//!
//! - the `almide test` worker cache: whole worker dirs, least recently used
//!   first, each emptied under its own lock exactly like the age sweep
//!   (non-blocking, lockfile kept, "still the same last use" re-asked under
//!   the lock);
//! - a build scratch dir (`$TMPDIR/almide-run`, and each worker dir): the
//!   per-program artifacts the age sweep owns — the content-keyed `almide-*`
//!   binaries in `target/<profile>/` and the `almide_out-*` objects in
//!   `target/<profile>/deps/` — oldest first, under the build lock the caller
//!   already holds. Dependency rlibs and the incremental session store are
//!   not per-program and are left alone, as the age sweep leaves them.
//!
//! "Last used" is the newest file mtime, the same signal the age rule reads:
//! a cache hit `touch_used`s the binary it execs. An entry used within
//! [`EVICT_GRACE`] is never evicted, however far over the bound the cache is:
//! a lock-free cache hit checks that its binary exists and then execs it, and
//! the grace is what keeps this sweep out of that window. The bound is
//! therefore soft by at most what the machine builds in the grace window.
//!
//! The sweep is rate-limited to once per [`SIZE_SWEEP_INTERVAL`] per root (a
//! stamp file, like the age sweep's daily one): a burst of `almide test`
//! processes pays one walk a minute, not one per process.
//!
//! `ALMIDE_CACHE_MAX_BYTES` sets the bound (`4G`, `512M`, a byte count);
//! `0` / `off` turns it off. The default and how it was chosen are on
//! [`DEFAULT_CACHE_MAX_BYTES`].

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The default per-cache-root bound: 4 GiB.
///
/// Chosen as twice the largest week-long working set measured on a heavily
/// used development machine (2026-09-27: several worktrees of this repo and
/// five concurrent agents running `almide test` / `almide run`):
///
/// - `almide test` worker cache: 65 dirs, 2.0 GB (median 15 MB, p90 105 MB,
///   max 142 MB per dir) — everything the week's native-fallback files built;
/// - `almide run` scratch dir: 2.0 GB of per-program artifacts (1,539
///   binaries = 1.07 GB, 7,528 `almide_out` objects = 0.97 GB).
///
/// So a normal edit loop never evicts anything the age rule would keep, and
/// the #2608 burst (84.9 GB) stops at 4 GiB plus what one grace window builds.
pub(crate) const DEFAULT_CACHE_MAX_BYTES: u64 = 4 << 30;

/// Nothing used this recently is evicted — see the module doc.
pub(crate) const EVICT_GRACE: Duration = Duration::from_secs(120);

/// How often the size sweep may walk one cache root.
const SIZE_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// The stamp file the size sweep's rate limit reads.
const SIZE_STAMP_FILE: &str = ".almide-size-stamp";

/// The bound in force: `None` when turned off.
pub(crate) fn cache_max_bytes() -> Option<u64> {
    match almide_base::env::var("ALMIDE_CACHE_MAX_BYTES") {
        None => Some(DEFAULT_CACHE_MAX_BYTES),
        Some(v) => parse_size(&v).unwrap_or_else(|| {
            static SAID: std::sync::Once = std::sync::Once::new();
            SAID.call_once(|| crate::err(&format!(
                "ALMIDE_CACHE_MAX_BYTES={v:?} is not a size (e.g. `4G`, `512M`, `1073741824`, or `off`); using the default 4G"
            )));
            Some(DEFAULT_CACHE_MAX_BYTES)
        }),
    }
}

/// `4G` / `512m` / `100MiB` / `1073741824` → bytes (binary units);
/// `0` / `off` → `Some(None)` (no bound); anything else → `None`.
pub(crate) fn parse_size(s: &str) -> Option<Option<u64>> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("off") {
        return Some(None);
    }
    let lower = s.to_ascii_lowercase();
    let body = lower.strip_suffix("ib").or_else(|| lower.strip_suffix('b')).unwrap_or(&lower);
    let (digits, shift) = match body.chars().last()? {
        'k' => (&body[..body.len() - 1], 10),
        'm' => (&body[..body.len() - 1], 20),
        'g' => (&body[..body.len() - 1], 30),
        't' => (&body[..body.len() - 1], 40),
        _ => (body, 0),
    };
    let n: u64 = digits.trim().parse().ok()?;
    let bytes = n.checked_shl(shift).filter(|b| b >> shift == n)?;
    Some(if bytes == 0 { None } else { Some(bytes) })
}

/// Is `root`'s size sweep due (not run within [`SIZE_SWEEP_INTERVAL`])?
/// Stamps the root when it is, so concurrent processes mostly skip.
fn claim_size_sweep(root: &Path) -> bool {
    let stamp = root.join(SIZE_STAMP_FILE);
    let due = match std::fs::metadata(&stamp).and_then(|m| m.modified()) {
        Ok(t) => SystemTime::now().duration_since(t).map(|since| since >= SIZE_SWEEP_INTERVAL).unwrap_or(true),
        Err(_) => true,
    };
    if due {
        let _ = std::fs::File::create(&stamp);
    }
    due
}

/// The LRU decision, kept pure so it is testable without a filesystem: the
/// entries to evict, least recently used first, until the total is at most
/// `cap`. An entry used within `grace` of `now` is never chosen — if only
/// such entries remain, the cache stays over the bound until they age.
pub(crate) fn lru_victims<K: Clone>(
    entries: &[(K, SystemTime, u64)],
    cap: u64,
    now: SystemTime,
    grace: Duration,
) -> Vec<K> {
    let mut total: u64 = entries.iter().map(|(_, _, b)| *b).sum();
    if total <= cap {
        return Vec::new();
    }
    let mut order: Vec<&(K, SystemTime, u64)> = entries.iter().collect();
    order.sort_by_key(|(_, used, _)| *used);
    let mut victims = Vec::new();
    for (key, used, bytes) in order {
        if total <= cap {
            break;
        }
        let recent = now.duration_since(*used).map(|age| age < grace).unwrap_or(true);
        if recent {
            // Sorted oldest first: everything after this is recent too.
            break;
        }
        victims.push(key.clone());
        total = total.saturating_sub(*bytes);
    }
    victims
}

/// Total bytes of the regular files under `dir` (symlinks not followed).
pub(crate) fn dir_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_bytes(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// When a worker dir was last used: the newest file mtime in the dir itself
/// and in `target/<profile>/`, where the cached binaries a hit touches live
/// — the files `run::used_since` reads for the age rule.
pub(crate) fn last_used(dir: &Path) -> Option<SystemTime> {
    [dir.to_path_buf(), dir.join("target").join("debug"), dir.join("target").join("release")]
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|rd| rd.flatten())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter(|e| e.file_name() != super::run::BUILD_LOCK_FILE)
        .filter_map(|e| e.metadata().and_then(|m| m.modified()).ok())
        .max()
}

/// Bound the `almide test` worker cache (`root` = `<temp>/almide-test/native`)
/// to `cap` bytes by emptying whole worker dirs, least recently used first.
/// Returns how many dirs were emptied.
pub(crate) fn bound_worker_cache(root: &Path, cap: u64) -> usize {
    if !claim_size_sweep(root) {
        return 0;
    }
    bound_worker_cache_now(root, cap, SystemTime::now())
}

/// [`bound_worker_cache`] without the rate limit, at a given `now`.
pub(crate) fn bound_worker_cache_now(root: &Path, cap: u64, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else { return 0 };
    let workers: Vec<(PathBuf, SystemTime, u64)> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let dir = e.path();
            // An emptied worker (only its lockfile) has no last use and no bytes.
            let used = last_used(&dir)?;
            let bytes = dir_bytes(&dir);
            Some((dir, used, bytes))
        })
        .collect();
    let mut emptied = 0;
    for dir in lru_victims(&workers, cap, now, EVICT_GRACE) {
        let seen = workers.iter().find(|(d, ..)| *d == dir).map(|(_, used, _)| *used);
        // Under the dir's own lock: a build in flight keeps it (skipped), and
        // a dir used since the scan is no longer the LRU entry it was.
        if super::run::clear_build_dir_if_idle(&dir, || last_used(&dir) <= seen) {
            emptied += 1;
        }
    }
    emptied
}

/// Bound a build scratch dir's per-program artifacts to `cap` bytes, oldest
/// first. The caller holds the dir's `BuildDirLock`. Returns the number of
/// files removed.
pub(crate) fn bound_build_artifacts(project_dir: &Path, cap: u64) -> usize {
    if !claim_size_sweep(project_dir) {
        return 0;
    }
    bound_build_artifacts_now(project_dir, cap, SystemTime::now())
}

/// [`bound_build_artifacts`] without the rate limit, at a given `now`.
pub(crate) fn bound_build_artifacts_now(project_dir: &Path, cap: u64, now: SystemTime) -> usize {
    let mut files: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
    let mut collect = |dir: &Path, prefix: &str| {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            if !entry.file_name().to_string_lossy().starts_with(prefix) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let Ok(modified) = meta.modified() else { continue };
            files.push((entry.path(), modified, meta.len()));
        }
    };
    for profile in ["debug", "release"] {
        let dir = project_dir.join("target").join(profile);
        collect(&dir, "almide-");
        collect(&dir.join("deps"), "almide_out-");
    }
    lru_victims(&files, cap, now, EVICT_GRACE)
        .into_iter()
        .filter(|path| std::fs::remove_file(path).is_ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs_ago: u64, now: SystemTime) -> SystemTime {
        now - Duration::from_secs(secs_ago)
    }

    #[test]
    fn parse_size_reads_units_and_off() {
        assert_eq!(parse_size("4G"), Some(Some(4 << 30)));
        assert_eq!(parse_size("512m"), Some(Some(512 << 20)));
        assert_eq!(parse_size("100MiB"), Some(Some(100 << 20)));
        assert_eq!(parse_size("2kb"), Some(Some(2048)));
        assert_eq!(parse_size("1073741824"), Some(Some(1 << 30)));
        assert_eq!(parse_size(" 8 G "), Some(Some(8 << 30)));
        assert_eq!(parse_size("0"), Some(None));
        assert_eq!(parse_size("off"), Some(None));
        assert_eq!(parse_size("OFF"), Some(None));
        assert_eq!(parse_size("lots"), None);
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("-1G"), None);
        // An overflowing product is not a size, not a wrapped small one.
        assert_eq!(parse_size("99999999999T"), None);
    }

    #[test]
    fn under_the_bound_nothing_is_evicted() {
        let now = SystemTime::now();
        let e = vec![("a", t(9_000, now), 10), ("b", t(8_000, now), 10)];
        assert!(lru_victims(&e, 20, now, EVICT_GRACE).is_empty());
    }

    #[test]
    fn over_the_bound_the_least_recently_used_go_first_until_it_fits() {
        let now = SystemTime::now();
        let e = vec![
            ("new", t(600, now), 40),
            ("oldest", t(9_000, now), 30),
            ("old", t(5_000, now), 30),
            ("mid", t(1_000, now), 30),
        ];
        // 130 bytes against 70: drop `oldest` (100 left), then `old` (70).
        assert_eq!(lru_victims(&e, 70, now, EVICT_GRACE), vec!["oldest", "old"]);
    }

    #[test]
    fn nothing_used_within_the_grace_window_is_evicted() {
        let now = SystemTime::now();
        let e = vec![("stale", t(3_600, now), 10), ("hot", t(5, now), 100), ("hot2", t(60, now), 100)];
        // Far over a cap of 1, but only the stale entry is outside the grace.
        assert_eq!(lru_victims(&e, 1, now, EVICT_GRACE), vec!["stale"]);
        // A future mtime (clock skew) counts as recent, never as the oldest.
        let skew = vec![("future", now + Duration::from_secs(3_600), 50)];
        assert!(lru_victims(&skew, 1, now, EVICT_GRACE).is_empty());
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "almide-cache-bound-test-{}-{}-{:x}",
            name,
            std::process::id(),
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_aged(path: &Path, bytes: usize, secs_ago: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; bytes]).unwrap();
        let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(secs_ago)).unwrap();
    }

    /// A worker dir shaped like the real ones: lockfile, `src/main.rs`, a
    /// cached binary in `target/debug/`, bulk in `target/debug/deps/`.
    fn worker(root: &Path, name: &str, bytes: usize, secs_ago: u64) -> PathBuf {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::File::create(d.join(super::super::run::BUILD_LOCK_FILE)).unwrap();
        write_aged(&d.join("src/main.rs"), 10, secs_ago);
        write_aged(&d.join("target/debug/almide-0123456789abcdef"), 10, secs_ago);
        write_aged(&d.join("target/debug/deps/libbulk.rlib"), bytes, secs_ago);
        d
    }

    #[test]
    fn the_worker_cache_is_bounded_by_emptying_the_least_recently_used_dirs() {
        let root = scratch("workers");
        let a = worker(&root, "a_test-1", 4_000, 30_000);
        let b = worker(&root, "b_test-2", 4_000, 20_000);
        let c = worker(&root, "c_test-3", 4_000, 10_000);
        let d = worker(&root, "d_test-4", 4_000, 10); // in use right now
        // ~16 KB against 9,000 bytes: `a` then `b` go; `c` fits; `d` is hot.
        assert_eq!(bound_worker_cache_now(&root, 9_000, SystemTime::now()), 2);
        for gone in [&a, &b] {
            // Emptied, not removed: the lockfile stays (the #2500 protocol).
            let left: Vec<_> = std::fs::read_dir(gone).unwrap().flatten().map(|e| e.file_name()).collect();
            assert_eq!(left, vec![std::ffi::OsString::from(super::super::run::BUILD_LOCK_FILE)]);
        }
        assert!(c.join("target/debug/deps/libbulk.rlib").exists());
        assert!(d.join("target/debug/deps/libbulk.rlib").exists());
        // An emptied dir counts for nothing on the next sweep.
        assert_eq!(bound_worker_cache_now(&root, 9_000, SystemTime::now()), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_worker_dir_a_build_holds_is_skipped() {
        let root = scratch("held");
        let a = worker(&root, "a_test-1", 4_000, 30_000);
        let _b = worker(&root, "b_test-2", 4_000, 20_000);
        let lock = super::super::run::BuildDirLock::acquire(&a).unwrap();
        // `a` is the LRU choice but is locked; `b` alone does not bring 8 KB
        // under 1 byte, and the sweep never waits for `a`.
        assert_eq!(bound_worker_cache_now(&root, 1, SystemTime::now()), 1);
        assert!(a.join("target/debug/deps/libbulk.rlib").exists());
        drop(lock);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_artifacts_are_bounded_oldest_first_and_nothing_else_is_touched() {
        let dir = scratch("artifacts");
        let old_bin = dir.join("target/debug/almide-aaaaaaaaaaaaaaaa");
        let new_bin = dir.join("target/debug/almide-bbbbbbbbbbbbbbbb");
        let old_obj = dir.join("target/debug/deps/almide_out-1111.almide_out.0.rcgu.o");
        let dep = dir.join("target/debug/deps/libserde-2222.rlib");
        write_aged(&old_bin, 3_000, 50_000);
        write_aged(&old_obj, 3_000, 40_000);
        write_aged(&new_bin, 3_000, 1_000);
        write_aged(&dep, 50_000, 90_000);
        // Artifacts total 9,000; bound 4,000 → the two oldest artifacts go.
        // The dependency rlib is older and larger, and is not an artifact.
        assert_eq!(bound_build_artifacts_now(&dir, 4_000, SystemTime::now()), 2);
        assert!(!old_bin.exists() && !old_obj.exists());
        assert!(new_bin.exists() && dep.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_size_sweep_is_rate_limited_per_root() {
        let root = scratch("stamp");
        assert!(claim_size_sweep(&root));
        assert!(!claim_size_sweep(&root));
        let _ = std::fs::remove_dir_all(&root);
    }
}
