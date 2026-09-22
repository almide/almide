//! The native build scratch dirs' housekeeping: a rustc ICE on a stale
//! incremental session recovers in one retry, `almide clean` empties the
//! dirs, and week-old artifacts are evicted while a cache hit keeps its
//! binary alive — for `almide run`'s shared dir (#2500) and for `almide
//! test`'s per-test-file worker dirs (#2504).
//!
//! Every test points `ALMIDE_RUN_PROJECT_DIR` at its own tempdir. The real
//! shared `<temp>/almide-run` is in use by other processes on a developer
//! machine and is never touched from here; the tests that reach the temp-dir
//! caches (`clean`, and every #2504 test) additionally redirect `HOME` and
//! the temp-dir variables, so the machine's own `<temp>/almide-test/native`
//! is out of reach.
//!
//! `ALMIDE_NO_RTLIB=1` forces the cargo-based build (the bare-rustc fast path
//! has no incremental session to corrupt); the generated crate has no
//! dependencies, so cargo needs no network.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// An `almide` command building in `run_dir` through the cargo path.
fn almide_in(run_dir: &Path) -> Command {
    let mut cmd = Command::new(almide());
    cmd.env("ALMIDE_RUN_PROJECT_DIR", run_dir).env("ALMIDE_NO_RTLIB", "1");
    cmd
}

fn write_program(dir: &Path, name: &str, printed: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("effect fn main() -> Unit = println(\"{printed}\")\n")).unwrap();
    path
}

fn run(run_dir: &Path, program: &Path) -> (bool, String, String) {
    let out = almide_in(run_dir).arg("run").arg(program).output().expect("spawn almide run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn find_files(dir: &Path, pred: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            find_files(&p, pred, out);
        } else if pred(&p) {
            out.push(p);
        }
    }
}

/// The cached `almide-<hash>` binaries in `target/debug`.
fn cached_binaries(run_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(run_dir.join("target/debug"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("almide-") && n.len() == "almide-".len() + 16)
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn set_mtime(path: &Path, when: SystemTime) {
    std::fs::File::options().write(true).open(path).unwrap().set_modified(when).unwrap();
}

fn age_of(path: &Path) -> Duration {
    SystemTime::now().duration_since(std::fs::metadata(path).unwrap().modified().unwrap()).unwrap()
}

/// The issue's 完了条件: corrupt an incremental session the way an interrupted
/// build does (one `*.pre-lto.bc` missing), run `almide run`, and it succeeds
/// and says it recovered. Before the fix this failed on every later build of
/// the dir, blaming rustc.
#[test]
fn a_stale_incremental_session_recovers_in_one_retry_and_says_so() {
    let td = tempfile::tempdir().unwrap();
    let run_dir = td.path().join("run");
    let program = write_program(td.path(), "hello.almd", "first");

    let (ok, stdout, stderr) = run(&run_dir, &program);
    assert!(ok, "the first build must succeed:\n{stderr}");
    assert_eq!(stdout, "first\n");

    let mut bitcode = Vec::new();
    find_files(
        &run_dir.join("target/debug/incremental"),
        &|p| p.extension().is_some_and(|e| e == "bc"),
        &mut bitcode,
    );
    assert!(
        !bitcode.is_empty(),
        "the cargo path left no `*.pre-lto.bc` work product in target/debug/incremental — \
         this toolchain does not produce the session shape #2500 corrupts; the recovery \
         needs a different corruption to be exercised"
    );
    // Every one, not just one: the rebuild reuses the unchanged codegen
    // units' bitcode, and which unit the edit below invalidates is rustc's
    // partitioning decision, not ours.
    for bc in &bitcode {
        std::fs::remove_file(bc).unwrap();
    }

    // A changed program: a cache miss that reuses the (now broken) session.
    let program = write_program(td.path(), "hello.almd", "second");
    let (ok, stdout, stderr) = run(&run_dir, &program);
    assert!(ok, "the build after the corruption must recover, stderr:\n{stderr}");
    assert_eq!(stdout, "second\n");
    assert!(
        stderr.contains("note: rustc crashed on a stale incremental session; cleared ")
            && stderr.contains("and rebuilt successfully"),
        "the recovery must be announced in one line, stderr:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("stale incremental session").count(),
        1,
        "exactly one recovery line, stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("unexpectedly panicked"),
        "the ICE the retry recovered from must not leak into the user's stderr:\n{stderr}"
    );

    // The very next run is a plain cache hit: silent.
    let (ok, stdout, stderr) = run(&run_dir, &program);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "second\n");
    assert!(!stderr.contains("stale incremental session"), "a hit must not announce anything:\n{stderr}");
}

/// The other 完了条件: `almide clean` empties the run dir. Only the build
/// lockfile may remain (it is kept on purpose so an in-flight builder's
/// flock stays on the same inode).
#[test]
fn clean_empties_the_run_dir_and_leaves_only_the_lockfile() {
    let td = tempfile::tempdir().unwrap();
    let run_dir = td.path().join("run");
    let home = td.path().join("home");
    let tmp = td.path().join("tmp");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&tmp).unwrap();
    let program = write_program(td.path(), "hello.almd", "hi");
    let (ok, _, stderr) = run(&run_dir, &program);
    assert!(ok, "{stderr}");
    assert!(run_dir.join("target").is_dir() && run_dir.join("src/main.rs").is_file(), "the build filled the dir");

    let clean = || {
        Command::new(almide())
            .arg("clean")
            .current_dir(td.path())
            .env("ALMIDE_RUN_PROJECT_DIR", &run_dir)
            .env("HOME", &home)
            .env("TMPDIR", &tmp)
            .env("TMP", &tmp)
            .env("TEMP", &tmp)
            .output()
            .expect("spawn almide clean")
    };
    let out = clean();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains(&format!("Cleaned {}", run_dir.display())),
        "clean must name the run dir it emptied:\n{stderr}"
    );
    let left: Vec<String> = std::fs::read_dir(&run_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        left.iter().all(|n| n == ".almide-build.lock"),
        "the run dir must be empty but for the lockfile, found {left:?}"
    );

    // Nothing left: clean says so and still exits 0.
    let out = clean();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(stderr.contains("No cache to clean"), "{stderr}");

    // And the dir builds again afterwards.
    let (ok, stdout, stderr) = run(&run_dir, &program);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "hi\n");
}

/// Growth is bounded by age: a binary unused for a week goes at the next
/// build's sweep, a binary a cache hit touched stays, and the kept
/// `almide_out-*` objects in `deps/` go with the binaries.
#[test]
fn week_old_binaries_are_evicted_and_a_cache_hit_keeps_its_binary() {
    let td = tempfile::tempdir().unwrap();
    let run_dir = td.path().join("run");
    let a = write_program(td.path(), "a.almd", "A");
    let b = write_program(td.path(), "b.almd", "B");
    let c = write_program(td.path(), "c.almd", "C");

    assert!(run(&run_dir, &a).0);
    let a_bin = cached_binaries(&run_dir);
    assert_eq!(a_bin.len(), 1, "one cached binary after one program: {a_bin:?}");
    assert!(run(&run_dir, &b).0);
    let ab = cached_binaries(&run_dir);
    assert_eq!(ab.len(), 2, "{ab:?}");
    let b_bin = ab.iter().find(|n| *n != &a_bin[0]).unwrap().clone();
    assert!(run_dir.join(".almide-evict-stamp").is_file(), "the first build stamps its sweep");

    // A month ago: both binaries, the cargo output and the kept objects.
    let month_ago = SystemTime::now() - Duration::from_secs(30 * 24 * 60 * 60);
    let mut aged = Vec::new();
    find_files(
        &run_dir.join("target/debug"),
        &|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with("almide-") || n.starts_with("almide_out-")
        },
        &mut aged,
    );
    let kept_objects: Vec<&PathBuf> = aged
        .iter()
        .filter(|p| p.parent().is_some_and(|d| d.file_name().is_some_and(|n| n == "deps")))
        .collect();
    for p in &aged {
        set_mtime(p, month_ago);
    }
    // The sweep is rate-limited by the stamp; drop it so the next build sweeps.
    std::fs::remove_file(run_dir.join(".almide-evict-stamp")).unwrap();

    // A cache hit on `a` refreshes its mtime — lock-free, without rebuilding.
    let (ok, stdout, stderr) = run(&run_dir, &a);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "A\n");
    assert!(
        age_of(&run_dir.join("target/debug").join(&a_bin[0])) < Duration::from_secs(60 * 60),
        "a cache hit must touch the binary it is about to exec"
    );
    assert!(
        age_of(&run_dir.join("target/debug").join(&b_bin)) > Duration::from_secs(20 * 24 * 60 * 60),
        "the hit on a must not touch b"
    );

    // A miss (c) sweeps: b goes, a stays, c arrives.
    let (ok, stdout, stderr) = run(&run_dir, &c);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "C\n");
    let after = cached_binaries(&run_dir);
    assert!(after.contains(&a_bin[0]), "the touched binary must survive the sweep: {after:?}");
    assert!(!after.contains(&b_bin), "the week-old binary must be evicted: {after:?}");
    assert_eq!(after.len(), 2, "a and c: {after:?}");
    assert!(run_dir.join(".almide-evict-stamp").is_file(), "the sweep re-stamps");
    // c's build recreates some of these names (the crate's own `deps/` binary
    // and `.d`, fresh); what must not remain is a month-old one.
    let stale_objects_left: Vec<&&PathBuf> = kept_objects
        .iter()
        .filter(|p| p.exists() && age_of(p) > Duration::from_secs(20 * 24 * 60 * 60))
        .collect();
    assert!(
        !kept_objects.is_empty() && stale_objects_left.is_empty(),
        "month-old `deps/almide_out-*` objects must be swept with the binaries (had {}, left {:?})",
        kept_objects.len(),
        stale_objects_left
    );
    assert!(
        run_dir.join("target/debug/incremental").is_dir(),
        "the sweep never touches the incremental session store"
    );

    // A second miss within the day does not sweep again (the stamp is fresh):
    // backdate a again and check it is still there after building b anew.
    set_mtime(&run_dir.join("target/debug").join(&a_bin[0]), month_ago);
    assert!(run(&run_dir, &b).0);
    assert!(
        cached_binaries(&run_dir).contains(&a_bin[0]),
        "a sweep within the stamp interval must not run"
    );
}

// ── #2504: `almide test`'s per-test-file native worker dirs ────────────────
//
// One dir per test-file ABSOLUTE path under `<temp>/almide-test/native/`,
// each with its own `target/`; nothing ever removed one (4,510 dirs / 39 GB
// measured 2026-09-22). These tests fabricate that tree inside their own
// tempdir and redirect every temp-dir variable, so the machine's real cache
// is never read or written.

/// A fabricated worker dir: the lockfile, the harness scratch, and one
/// content-keyed binary — aged as a whole, with the binary's own age
/// separately settable (that is what a lock-free cache HIT refreshes).
fn fake_worker(native: &Path, name: &str, age: Duration, binary_age: Duration) {
    let dir = native.join(name);
    std::fs::create_dir_all(dir.join("target/debug")).unwrap();
    std::fs::write(dir.join(".almide-build.lock"), b"").unwrap();
    std::fs::write(dir.join("almide_test_bin"), vec![0u8; 1024]).unwrap();
    std::fs::write(dir.join("almide_test_main.rs"), "fn main() {}\n").unwrap();
    let binary = dir.join("target/debug/almide-0123456789abcdef");
    std::fs::write(&binary, vec![0u8; 2048]).unwrap();
    // Files only, which is what the used-signal reads (a directory's mtime is
    // filesystem-dependent and deliberately not part of the rule).
    let then = SystemTime::now() - age;
    for p in [
        dir.join(".almide-build.lock"),
        dir.join("almide_test_bin"),
        dir.join("almide_test_main.rs"),
    ] {
        set_mtime(&p, then);
    }
    set_mtime(&binary, SystemTime::now() - binary_age);
}

/// What is left in a worker dir, sorted.
fn worker_entries(native: &Path, name: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(native.join(name))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A tempdir with `<tmp>/almide-test/native` fabricated, plus a trivial
/// passing test file; returns (tempdir, native cache path, test file path).
fn worker_tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let td = tempfile::tempdir().unwrap();
    let native = td.path().join("tmp/almide-test/native");
    std::fs::create_dir_all(&native).unwrap();
    std::fs::create_dir_all(td.path().join("home")).unwrap();
    let test_file = td.path().join("a_test.almd");
    std::fs::write(&test_file, "test \"adds\" {\n  assert_eq(1 + 1, 2)\n}\n").unwrap();
    (td, native, test_file)
}

/// `almide test <file>` with every temp-dir variable pointed at `<td>/tmp`.
fn almide_test(td: &Path, test_file: &Path) -> std::process::Output {
    let tmp = td.join("tmp");
    Command::new(almide())
        .arg("test")
        .arg(test_file)
        .current_dir(td)
        .env("HOME", td.join("home"))
        .env("TMPDIR", &tmp)
        .env("TMP", &tmp)
        .env("TEMP", &tmp)
        .env("ALMIDE_RUN_PROJECT_DIR", td.join("run"))
        .output()
        .expect("spawn almide test")
}

/// The 完了条件: a worker dir untouched for 7 days is emptied by the next
/// `almide test`, a dir used today survives — including one whose only
/// recent event is a cache hit touching its binary.
#[test]
fn almide_test_evicts_week_old_worker_dirs_and_keeps_used_ones() {
    let (td, native, test_file) = worker_tree();
    let month = Duration::from_secs(30 * 24 * 60 * 60);
    fake_worker(&native, "stale", month, month);
    fake_worker(&native, "hit-today", month, Duration::from_secs(60));
    fake_worker(&native, "fresh", Duration::from_secs(60 * 60), Duration::from_secs(60 * 60));

    let out = almide_test(td.path(), &test_file);
    assert!(
        out.status.success(),
        "the run itself must still pass:\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert_eq!(
        worker_entries(&native, "stale"),
        vec![".almide-build.lock".to_string()],
        "a worker dir nothing used for a month must be emptied, lockfile kept"
    );
    assert_eq!(
        worker_entries(&native, "hit-today").len(),
        4,
        "a dir whose binary a cache hit touched today is in use and must survive"
    );
    assert_eq!(worker_entries(&native, "fresh").len(), 4, "a dir used an hour ago must survive");
    assert!(native.join(".almide-evict-stamp").is_file(), "the sweep stamps the cache root");

    // Rate limit: a second run the same day does not sweep again.
    let stale_again = native.join("fresh");
    for p in [
        stale_again.join("almide_test_bin"),
        stale_again.join("almide_test_main.rs"),
        stale_again.join(".almide-build.lock"),
        stale_again.join("target/debug/almide-0123456789abcdef"),
    ] {
        set_mtime(&p, SystemTime::now() - month);
    }
    let out = almide_test(td.path(), &test_file);
    assert!(out.status.success());
    assert_eq!(
        worker_entries(&native, "fresh").len(),
        4,
        "a sweep within the stamp interval must not run"
    );
}

/// A worker dir another process is building in holds its lock; the sweep
/// takes each dir's lock WITHOUT waiting and leaves that one alone.
#[test]
fn a_locked_worker_dir_is_skipped_by_the_sweep() {
    let (td, native, test_file) = worker_tree();
    let month = Duration::from_secs(30 * 24 * 60 * 60);
    fake_worker(&native, "busy", month, month);
    fake_worker(&native, "idle", month, month);

    // Hold "busy"'s lock for the duration of the run, as a builder would.
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(native.join("busy/.almide-build.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();

    let out = almide_test(td.path(), &test_file);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    assert_eq!(
        worker_entries(&native, "busy").len(),
        4,
        "the sweep must not empty a dir another process holds the lock on"
    );
    assert_eq!(
        worker_entries(&native, "idle"),
        vec![".almide-build.lock".to_string()],
        "an idle stale dir is still emptied in the same sweep"
    );
    fs2::FileExt::unlock(&lock).unwrap();
}

/// `almide clean` empties every worker dir, whatever its age, and reports
/// the tree — the manual lever for the 39 GB.
#[test]
fn clean_empties_every_worker_dir_and_reports_the_tree() {
    let (td, native, _) = worker_tree();
    fake_worker(&native, "old", Duration::from_secs(30 * 24 * 60 * 60), Duration::from_secs(30 * 24 * 60 * 60));
    fake_worker(&native, "today", Duration::from_secs(60), Duration::from_secs(60));

    let tmp = td.path().join("tmp");
    let clean = || {
        Command::new(almide())
            .arg("clean")
            .current_dir(td.path())
            .env("HOME", td.path().join("home"))
            .env("TMPDIR", &tmp)
            .env("TMP", &tmp)
            .env("TEMP", &tmp)
            .env("ALMIDE_RUN_PROJECT_DIR", td.path().join("run"))
            .output()
            .expect("spawn almide clean")
    };
    let out = clean();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains(&format!("Cleaned {} (2 test worker dir(s))", native.display())),
        "clean must name the worker tree and how many dirs it emptied:\n{stderr}"
    );
    for name in ["old", "today"] {
        assert_eq!(
            worker_entries(&native, name),
            vec![".almide-build.lock".to_string()],
            "{name} must be emptied to its lockfile"
        );
    }

    // Idempotent: nothing left to empty.
    let out = clean();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(!stderr.contains("test worker dir(s)"), "nothing to report the second time:\n{stderr}");
    assert!(stderr.contains("No cache to clean"), "{stderr}");
}
