//! Recovering from a stale rustc incremental session (#2500): recognise
//! rustc's ICE banner, clear the session stores, retry the build once.
//! Part of the native build recipe — `build_environment_key` hashes this
//! file's source into every cache key.

/// Does a failed build's output carry rustc's internal-compiler-error banner?
///
/// rustc prints `error: the compiler unexpectedly panicked. This is a bug.`
/// (older / query-path ICEs say `internal compiler error`) through its
/// diagnostic emitter, so the phrase survives cargo's `--message-format=json`
/// re-rendering and the `wrap_codegen_leak` banner alike. This is the ONLY
/// signal the stale-incremental-session recovery keys on (#2500): a build
/// that merely fails to compile never matches, so a genuine error is never
/// retried.
pub(super) fn is_rustc_ice(stderr: &str) -> bool {
    stderr.contains("the compiler unexpectedly panicked") || stderr.contains("internal compiler error")
}

/// Remove every NON-EMPTY `<project_dir>/target/<profile>/incremental`
/// session store. Returns the directories that held a session and were
/// removed. The CALLER holds the dir's `BuildDirLock`: a session store is
/// rewritten by any build in the dir, so it is only ever touched under the
/// same lock that serializes those builds.
///
/// Empty is not "cleared": cargo creates `target/<profile>/incremental/`
/// even when incremental compilation is OFF (`CARGO_INCREMENTAL=0`, which
/// this repo's own CI sets for every job). Counting that empty directory as
/// something recovered would make the caller retry a genuine ICE once for
/// nothing, in exactly the environment where no session can have gone stale.
pub(super) fn clear_incremental_sessions(project_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(profiles) = std::fs::read_dir(project_dir.join("target")) else { return Vec::new() };
    let mut cleared = Vec::new();
    let mut clear = |inc: std::path::PathBuf| {
        let holds_a_session = std::fs::read_dir(&inc).map(|mut rd| rd.next().is_some()).unwrap_or(false);
        if holds_a_session && std::fs::remove_dir_all(&inc).is_ok() {
            cleared.push(inc);
        }
    };
    for entry in profiles.flatten() {
        clear(entry.path().join("incremental"));
        // A cross build (#2772) keeps its profiles one level down:
        // `target/<triple>/<profile>/incremental`.
        if let Ok(nested) = std::fs::read_dir(entry.path()) {
            for sub in nested.flatten() {
                clear(sub.path().join("incremental"));
            }
        }
    }
    cleared.sort();
    cleared
}

/// Run `build` once and, if it failed with rustc's ICE banner, clear the
/// dir's incremental session stores and run it once more (#2500).
///
/// An interrupted build (ENOSPC, a killed process) can leave a rustc
/// incremental session with its `work-products.bin` naming a `*.pre-lto.bc`
/// that was never written. rustc then panics on every later build that
/// reuses the session — the same program shape fails forever, the message
/// blames rustc and names a temp path, and nothing tells the user to delete
/// it. The session store is a pure cache, so the recovery is to drop it and
/// rebuild. Exactly one retry: if it also fails, the ORIGINAL error is
/// reported (the retry's, if different, is not what the user's build said).
/// A recovered build says so on stderr in one line.
///
/// The caller holds whatever lock serializes builds in `project_dir` (the
/// `BuildDirLock` of `build_native_cached` / the cdylib build; the REPL's
/// dir is private to its one interactive process).
pub(super) fn build_recovering_from_ice(
    project_dir: &std::path::Path,
    mut build: impl FnMut() -> Result<std::path::PathBuf, String>,
) -> Result<std::path::PathBuf, String> {
    let first = build();
    let Err(first_err) = &first else { return first };
    if !is_rustc_ice(first_err) {
        return first;
    }
    let cleared = clear_incremental_sessions(project_dir);
    if cleared.is_empty() {
        // Nothing stale to recover from: a genuine rustc ICE on this code.
        return first;
    }
    match build() {
        Ok(bin) => {
            let names = cleared.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
            crate::err(&format!(
                "note: rustc crashed on a stale incremental session; cleared {} and rebuilt successfully",
                names
            ));
            Ok(bin)
        }
        Err(_) => first,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_recovering_from_ice, clear_incremental_sessions, is_rustc_ice};

    // #2500: the stale-incremental-session recovery. The end-to-end shape (a
    // real rustc ICE from a deleted `*.pre-lto.bc`) is `tests/run_cache_recovery_test.rs`;
    // these pin the retry POLICY with a scripted build.

    const ICE: &str = "thread 'rustc' panicked at compiler/rustc_codegen_ssa/src/back/write.rs:2290:29:\n\
        failed to open bitcode file `.../incremental/almide_out-1/s-2-working/3.pre-lto.bc`: No such file or directory\n\
        error: the compiler unexpectedly panicked. This is a bug\n";

    #[test]
    fn the_ice_banner_is_the_only_trigger() {
        assert!(is_rustc_ice(ICE));
        assert!(is_rustc_ice("error: internal compiler error: unexpected panic"));
        // The codegen-bug wrapper keeps the banner inside its own text.
        assert!(is_rustc_ice(&super::super::cargo_build::wrap_codegen_leak(format!("{ICE}\nerror: could not compile `almide-out`"))));
        assert!(!is_rustc_ice("error[E0599]: no method named `foo`\nerror: could not compile `almide-out`"));
        assert!(!is_rustc_ice("thread 'main' panicked at src/main.rs:3:5"));
    }

    fn scripted(outcomes: Vec<Result<&'static str, &'static str>>) -> (impl FnMut() -> Result<std::path::PathBuf, String>, std::rc::Rc<std::cell::Cell<usize>>) {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let c = calls.clone();
        let mut it = outcomes.into_iter();
        (
            move || {
                c.set(c.get() + 1);
                it.next().expect("more build calls than scripted")
                    .map(std::path::PathBuf::from)
                    .map_err(String::from)
            },
            calls,
        )
    }

    #[test]
    fn an_ice_clears_the_sessions_and_retries_once() {
        let dir = tempfile::tempdir().unwrap();
        let inc = dir.path().join("target/debug/incremental/almide_out-1/s-2-working");
        std::fs::create_dir_all(&inc).unwrap();
        std::fs::write(inc.join("work-products.bin"), b"x").unwrap();
        let (build, calls) = scripted(vec![Err(ICE), Ok("bin")]);
        let out = build_recovering_from_ice(dir.path(), build);
        assert_eq!(out.as_deref().ok(), Some(std::path::Path::new("bin")));
        assert_eq!(calls.get(), 2);
        assert!(!dir.path().join("target/debug/incremental").exists(), "the session store must be gone");
        assert!(dir.path().join("target/debug").is_dir(), "only the incremental store is cleared, not the profile dir");
    }

    #[test]
    fn a_retry_that_also_fails_reports_the_original_error_and_never_loops() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("target/debug/incremental/almide_out-1")).unwrap();
        let (build, calls) = scripted(vec![Err(ICE), Err("error: the compiler unexpectedly panicked. This is a bug\n(second)")]);
        let out = build_recovering_from_ice(dir.path(), build);
        assert_eq!(out, Err(ICE.to_string()), "the user's build said the first error; that is what is reported");
        assert_eq!(calls.get(), 2, "exactly one retry, even though the retry was itself an ICE");
    }

    #[test]
    fn a_plain_compile_error_is_not_retried_and_keeps_its_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let inc = dir.path().join("target/debug/incremental/almide_out-1");
        std::fs::create_dir_all(&inc).unwrap();
        let (build, calls) = scripted(vec![Err("error[E0308]: mismatched types")]);
        let out = build_recovering_from_ice(dir.path(), build);
        assert!(out.is_err());
        assert_eq!(calls.get(), 1);
        assert!(inc.is_dir(), "a genuine compile error must not throw the session store away");
    }

    #[test]
    fn an_ice_with_no_session_store_is_a_genuine_ice_and_is_not_retried() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("target/debug")).unwrap();
        let (build, calls) = scripted(vec![Err(ICE)]);
        let out = build_recovering_from_ice(dir.path(), build);
        assert_eq!(out, Err(ICE.to_string()));
        assert_eq!(calls.get(), 1);
    }

    /// `CARGO_INCREMENTAL=0` (what this repo's CI sets for every job) still
    /// leaves an EMPTY `target/<profile>/incremental/` behind. Nothing there
    /// can have gone stale, so an ICE under it is genuine and must not cost
    /// a retry.
    #[test]
    fn an_empty_session_dir_is_not_something_to_recover_from() {
        let dir = tempfile::tempdir().unwrap();
        let inc = dir.path().join("target/debug/incremental");
        std::fs::create_dir_all(&inc).unwrap();
        assert!(clear_incremental_sessions(dir.path()).is_empty(), "an empty session dir is not a session");
        assert!(inc.is_dir(), "and it is not removed either");
        let (build, calls) = scripted(vec![Err(ICE)]);
        assert_eq!(build_recovering_from_ice(dir.path(), build), Err(ICE.to_string()));
        assert_eq!(calls.get(), 1, "no retry when there was no session to clear");
    }
}
