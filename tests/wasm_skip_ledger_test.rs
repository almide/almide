//! Every `// wasm:skip` in `spec/` must name a PLATFORM reason, not a gap.
//!
//! `almide test` runs each spec file on the wasm leg and falls back to the
//! native leg for anything wasm can't pass. A fallback is fine when wasm
//! genuinely cannot express the test — a TCP listener, a process spawn, a
//! `@extern` binding, a caught panic — and is debt when the v1 lowering subset
//! merely doesn't cover the shape yet.
//!
//! The count was 30 when #813 was filed and most were debt; it is now 9 and all
//! of them are platform-genuine. This ledger keeps it that way: a new skip must
//! declare which category it falls under, so subset debt cannot be parked here
//! as a comment and forgotten.
//!
//! Adding a genuinely native-only test: put its file in [`GENUINE_SKIPS`] with
//! the category. Adding one because the wasm leg walls: don't — fix the wall,
//! or the ledger will fail and say so.

use std::path::{Path, PathBuf};

/// Why a spec file cannot run on the wasm leg.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SkipReason {
    /// The API has no wasm implementation and is not expected to: an OS
    /// socket, a process spawn, a `@extern` binding to a native library.
    NativeOnlyApi,
    /// The file asserts a COMPILE error and has no test blocks, so there is
    /// nothing for either leg to run.
    CompileErrorFixture,
    /// wasm has no unwinding, so a panic cannot be caught and inspected.
    NoUnwinding,
}

/// Every file allowed to carry `// wasm:skip`, and why.
const GENUINE_SKIPS: &[(&str, SkipReason)] = &[
    // `@extern` binds a native symbol; there is no wasm equivalent.
    ("spec/integration/extern/extern_test.almd", SkipReason::NativeOnlyApi),
    ("spec/integration/extern_c/extern_c_test.almd", SkipReason::NativeOnlyApi),
    // OS sockets and subprocesses: outside the WASI surface the runtime targets.
    ("spec/stdlib/net_test.almd", SkipReason::NativeOnlyApi),
    ("spec/stdlib/process_ext_test.almd", SkipReason::NativeOnlyApi),
    ("spec/stdlib/process_exec_status_test.almd", SkipReason::NativeOnlyApi),
    // `http.serve` binds a TCP listener.
    ("spec/lang/effect_intrinsic_tail_test.almd", SkipReason::NativeOnlyApi),
    // zlib is `@intrinsic`-only — the bodies live in `runtime/rs/src/zlib.rs`.
    // Lifting this skip means implementing DEFLATE in Almide, not widening the
    // lowering subset.
    ("spec/stdlib/zlib_test.almd", SkipReason::NativeOnlyApi),
    // These assert a compile error and carry no test blocks.
    ("spec/integration/modules/vis_mod_error_test.almd", SkipReason::CompileErrorFixture),
    ("spec/integration/modules/vis_local_error_test.almd", SkipReason::CompileErrorFixture),
    ("spec/integration/modules/phantom_dep_error_test.almd", SkipReason::CompileErrorFixture),
    ("spec/integration/modules/deep_phantom_test.almd", SkipReason::CompileErrorFixture),
    // `assert_throws` catches a panic; a wasm trap cannot be caught.
    ("spec/stdlib/coverage_assert_throws_test.almd", SkipReason::NoUnwinding),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.almd` under `dir`, recursively.
fn almd_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            almd_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "almd") {
            out.push(p);
        }
    }
}

/// The spec files that carry a `// wasm:skip` marker, repo-relative.
fn skipped_files() -> Vec<String> {
    let root = repo_root();
    let mut files = Vec::new();
    almd_files(&root.join("spec"), &mut files);
    let mut out: Vec<String> = files
        .iter()
        .filter(|p| {
            std::fs::read_to_string(p)
                .unwrap_or_default()
                .lines()
                .any(|l| l.trim_start().starts_with("// wasm:skip"))
        })
        .map(|p| {
            p.strip_prefix(&root)
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_wasm_skip_is_a_declared_platform_limit() {
    let listed: std::collections::HashSet<&str> =
        GENUINE_SKIPS.iter().map(|(f, _)| *f).collect();
    let undeclared: Vec<String> = skipped_files()
        .into_iter()
        .filter(|f| !listed.contains(f.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "these files skip the wasm leg without a declared platform reason: {undeclared:?}\n\
         A skip is for something wasm CANNOT do (an OS API, a `@extern` binding, a caught \
         panic, a compile-error fixture). If the wasm leg merely walls the shape, that is \
         subset debt (#812) — fix the wall rather than parking it here."
    );
}

#[test]
fn the_wasm_skip_ledger_has_no_stale_rows() {
    let actual: std::collections::HashSet<String> = skipped_files().into_iter().collect();
    let stale: Vec<&str> = GENUINE_SKIPS
        .iter()
        .map(|(f, _)| *f)
        .filter(|f| !actual.contains(*f))
        .collect();
    assert!(
        stale.is_empty(),
        "the ledger lists files that no longer skip the wasm leg: {stale:?}\n\
         Remove the row — the ledger only shrinks."
    );
}
