//! Building a run's native test binaries (#4441): ONE crate per run.
//!
//! Each test file is still lowered on its own — its own frontend, its own
//! `[permissions]`, its own generated Rust, exactly as before — but the
//! sources of every file that shares a package context are folded into one
//! crate ([`super::test_merge`]) and built by ONE rustc. The shared modules,
//! the runtime and the stdlib then compile once per run instead of once per
//! file, and the crate's dir is stable across runs, so an edit recompiles
//! incrementally.
//!
//! The binary is then run once per FILE, selecting that file's module
//! ([`super::test_report::run_native_test_binary`]), so a file still runs in
//! a process of its own, with the output, counts and verdict it had.
//!
//! A merged crate that does not build — a rustc error in one file, or a
//! merge the dedup got wrong — is not a verdict: the files fall back to one
//! crate each, the old route, and a real build error is reported per file
//! as before.

use super::run::NativeSource;
use super::test_scratch::TestScratch;
use super::worker_pool::{bounded_parallel, cpu_slots, guard_worker_panic};
use std::path::PathBuf;
use std::sync::Arc;

/// Where a file's tests live in a merged binary.
#[derive(Clone, Debug)]
pub(crate) struct MergedModule {
    /// The file's module, `almide_test_file_<i>`.
    pub name: String,
    /// How many tests the OTHER files' modules hold: libtest counts them as
    /// filtered out of this file's run, and the file's report must not.
    pub others: usize,
}

/// A built native test binary for one file.
#[derive(Clone, Debug)]
pub(crate) struct NativeTestBin {
    pub path: PathBuf,
    /// `Some` when the binary is the run's merged crate.
    pub module: Option<MergedModule>,
}

/// Generate every file's source in parallel, then build: one merged crate
/// per package context, per-file crates for a file alone in its context and
/// for a merged crate that did not build. Results in `files` order.
pub(super) fn compile_native_tests(files: &[String], no_check: bool, scratch: &Arc<TestScratch>) -> Vec<(String, Result<NativeTestBin, String>)> {
    let mut sources: Vec<(String, Result<NativeSource, String>)> = bounded_parallel(files.to_vec(), cpu_slots(), move |file: String| {
        let src = guard_worker_panic(|| super::run::native_test_source(&file, no_check), Err);
        (file, src)
    });
    sources.sort_by(|a, b| a.0.cmp(&b.0));

    // Files whose crates would differ only in their source: same native deps,
    // same package root (and so the same native/ tree).
    let mut groups: Vec<(String, Vec<(String, NativeSource)>)> = Vec::new();
    let mut results: Vec<(String, Result<NativeTestBin, String>)> = Vec::new();
    for (file, src) in sources {
        match src {
            Ok(src) => {
                let key = context_key(&src);
                match groups.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, members)) => members.push((file, src)),
                    None => groups.push((key, vec![(file, src)])),
                }
            }
            Err(e) => results.push((file, Err(e))),
        }
    }

    let mut per_file: Vec<(String, NativeSource)> = Vec::new();
    for (key, members) in groups {
        if members.len() < 2 {
            per_file.extend(members);
            continue;
        }
        match build_merged(&key, &members, scratch) {
            Ok(built) => results.extend(built),
            Err(e) => {
                if almide_base::env::flag("ALMIDE_TEST_VERBOSE") {
                    crate::err(&format!("note: the run's merged native test crate did not build; building one crate per file:\n{e}"));
                }
                per_file.extend(members);
            }
        }
    }

    let scratch = scratch.clone();
    results.extend(bounded_parallel(per_file, cpu_slots(), move |(file, src): (String, NativeSource)| {
        let worker_dir = scratch.native_worker_dir(&file);
        let built = guard_worker_panic(|| src.build(true, false, Some(&worker_dir)), Err)
            .map(|path| NativeTestBin { path, module: None });
        (file, built)
    }));
    let order: std::collections::HashMap<&String, usize> = files.iter().enumerate().map(|(i, f)| (f, i)).collect();
    results.sort_by_key(|(f, _)| order.get(f).copied().unwrap_or(usize::MAX));
    results
}

/// What, besides the source, shapes a file's crate.
fn context_key(src: &NativeSource) -> String {
    let deps = src
        .native_deps
        .iter()
        .map(|d| format!("{}={}@{}", d.name, d.spec, d.target.as_deref().unwrap_or("")))
        .collect::<Vec<_>>()
        .join(",");
    format!("deps={deps};root={:?}", src.source_root)
}

/// Build one merged crate for `members` and tell each file where its tests are.
fn build_merged(key: &str, members: &[(String, NativeSource)], scratch: &TestScratch) -> Result<Vec<(String, Result<NativeTestBin, String>)>, String> {
    let codes: Vec<&str> = members.iter().map(|(_, s)| s.rs_code.as_str()).collect();
    let merged = NativeSource {
        rs_code: super::test_merge::merge(&codes),
        native_deps: members[0].1.native_deps.clone(),
        source_root: members[0].1.source_root.clone(),
    };
    let path = merged.build(true, false, Some(&scratch.native_merged_dir(key)))?;
    let listed = list_tests(&path)?;
    let total = listed.len();
    Ok(members
        .iter()
        .enumerate()
        .map(|(i, (file, _))| {
            let name = super::test_merge::file_module(i);
            let prefix = format!("{name}::");
            let own = listed.iter().filter(|t| t.starts_with(&prefix)).count();
            (file.clone(), Ok(NativeTestBin { path: path.clone(), module: Some(MergedModule { name, others: total - own }) }))
        })
        .collect())
}

/// Every test a libtest binary holds, ignored ones included (`--list`).
fn list_tests(bin: &std::path::Path) -> Result<Vec<String>, String> {
    let (code, stdout, stderr) = super::run::run_binary_captured_io(bin, &["--list".to_string(), "--format".to_string(), "terse".to_string()]);
    if code != 0 {
        return Err(format!("the merged test binary could not list its tests:\n{stdout}{stderr}"));
    }
    Ok(stdout.lines().filter_map(|l| l.strip_suffix(": test")).map(str::to_string).collect())
}
