//! The certified native render over its whole corpus, inside the crate that
//! holds it (#4006).
//!
//! `proofs/native-verified-floor.txt` lists the `spec/wasm_cross` programs the
//! verified native renderer renders; the root `native_verified_floor_test`
//! holds that list against the CLI. This test renders the same corpus through
//! `almide_mir::pipeline::try_render_rust_source` from the MIR crate's own test
//! binary, so each render path the floor takes, and each wall the rest of the
//! corpus stops at, runs under the crate's tests:
//!
//! - every floor program renders (`Ok`), and its Rust names the v1 trust spine;
//! - every other `spec/wasm_cross` program renders or walls, never panics;
//! - the perf shape benches and the perf suite's programs render or wall the
//!   same way, so the loop, fusion and arena paths they were written for run.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn floor() -> Vec<String> {
    std::fs::read_to_string(root().join("proofs/native-verified-floor.txt"))
        .expect("floor ledger")
        .lines()
        .filter(|l| l.starts_with("spec/"))
        .map(str::to_string)
        .collect()
}

fn almd_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "almd"))
        .collect();
    v.sort();
    v
}

/// Render `paths` on a thread with a deep stack, as the render corpus example
/// does: (path, rendered Rust or the wall's text).
fn render_all(paths: Vec<PathBuf>) -> Vec<(PathBuf, Result<String, String>)> {
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || {
            paths
                .into_iter()
                .map(|p| {
                    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
                    let r = almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}"));
                    (p, r)
                })
                .collect()
        })
        .expect("spawn")
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e))
}

#[test]
fn every_floor_program_renders_and_every_other_fixture_renders_or_walls() {
    let floor = floor();
    assert!(floor.len() > 400, "the floor ledger lost its rows: {}", floor.len());
    let rendered = render_all(almd_files(&root().join("spec/wasm_cross")));
    let mut missing = Vec::new();
    let mut on_floor = 0;
    for (p, r) in &rendered {
        let rel = p.strip_prefix(root()).map(|r| r.to_string_lossy().replace("\\", "/")).unwrap_or_default();
        let rel = rel.trim_start_matches("./").to_string();
        if floor.iter().any(|f| rel.ends_with(f.as_str())) {
            on_floor += 1;
            match r {
                Ok(rust) => assert!(rust.contains("v1 trust spine"), "{rel}: rendered without the trust spine header"),
                Err(e) => missing.push(format!("{rel}: {e}")),
            }
        }
    }
    assert!(missing.is_empty(), "floor programs that no longer render:\n{}", missing.join("\n"));
    assert_eq!(on_floor, floor.len(), "a floor row names no spec/wasm_cross file");
}

#[test]
fn the_perf_shape_benches_and_suite_programs_render_or_wall_without_panicking() {
    let mut paths = almd_files(&root().join("research/benchmark/perf/v1-shapes"));
    for dir in ["btrees", "treealloc", "binarytrees", "fib", "listbuild", "strbuild", "wordfreq", "decode", "jsonround"] {
        let d = root().join("research/benchmark/perf").join(dir);
        if d.is_dir() {
            paths.extend(almd_files(&d));
        }
    }
    assert!(paths.len() > 20, "the perf corpus went missing: {}", paths.len());
    let rendered = render_all(paths);
    // The pipeline shapes the certified render fuses, and the arena-window
    // programs, are on the certified render.
    for name in ["range_map_fold_chain", "record_map_fold_chain", "list_map_fold_chain", "record_list_map", "btrees"] {
        let hit = rendered.iter().find(|(p, _)| p.file_stem().is_some_and(|s| s == name));
        let (p, r) = hit.unwrap_or_else(|| panic!("{name}: not in the perf corpus"));
        assert!(r.is_ok(), "{}: walls on the certified render: {:?}", p.display(), r.as_ref().err());
    }
}
