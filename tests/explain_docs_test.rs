//! `almide explain <code>` must work from the BINARY ALONE (#923).
//!
//! The explanation texts are `docs/diagnostics/<CODE>.md`, embedded at build
//! time by the root build.rs. Before that, the command probed the disk (a
//! 6-level parent walk from the executable) with a hardcoded E001–E010
//! fallback: from a checkout every documented code explained, from an installed
//! binary (`make install` ships no docs/) 21 of them answered "Unknown error
//! code" — while docs/diagnostics/README.md told users to run exactly that
//! command.
//!
//! Every test here runs the binary from a temp directory with no checkout
//! above it, so a reintroduced disk dependency cannot pass by accident.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// Every code with a `docs/diagnostics/<CODE>.md` file. Scanned here
/// INDEPENDENTLY of build.rs's scan, so the two derivations check each other:
/// a doc the build script's filter missed fails this test rather than silently
/// shipping unexplainable.
fn documented_codes() -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/diagnostics");
    let mut codes: Vec<String> = std::fs::read_dir(dir)
        .expect("docs/diagnostics exists")
        .filter_map(|e| {
            let path = e.ok()?.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            (path.extension()?.to_str()? == "md"
                && stem.starts_with('E')
                && stem[1..].chars().all(|c| c.is_ascii_digit()))
            .then_some(stem)
        })
        .collect();
    codes.sort();
    assert!(
        codes.len() >= 31,
        "expected the documented-code corpus (31 files as of #923), found {}",
        codes.len()
    );
    codes
}

#[test]
fn every_documented_code_explains_from_the_binary_alone() {
    let tmp = tempfile::tempdir().expect("tempdir");
    for code in documented_codes() {
        let doc = std::fs::read_to_string(format!(
            "{}/docs/diagnostics/{}.md",
            env!("CARGO_MANIFEST_DIR"),
            code
        ))
        .expect("read doc");
        let out = Command::new(almide())
            .args(["explain", &code])
            .current_dir(tmp.path())
            .env_remove("ALMIDE_DIAGNOSTICS_DIR")
            .output()
            .expect("run almide explain");
        assert!(
            out.status.success(),
            "`almide explain {code}` must succeed away from the checkout, got status {:?}\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            stdout.trim_end(),
            doc.trim_end(),
            "`almide explain {code}` must print the embedded doc verbatim"
        );
    }
}

/// Lowercase is the same code: `explain e005` used to work on macOS (its
/// filesystem is case-insensitive) and fail on Linux — a host-dependent split
/// the embedded table ends.
#[test]
fn lowercase_codes_normalize() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = Command::new(almide())
        .args(["explain", "e005"])
        .current_dir(tmp.path())
        .output()
        .expect("run almide explain");
    assert!(out.status.success(), "e005 must explain as E005");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("E005"),
        "the E005 doc is the answer"
    );
}

/// A code with no doc stays a LOUD miss — exit 1 and the code named.
#[test]
fn an_unknown_code_exits_nonzero() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = Command::new(almide())
        .args(["explain", "E999"])
        .current_dir(tmp.path())
        .output()
        .expect("run almide explain");
    assert!(!out.status.success(), "E999 has no doc and must exit non-zero");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("E999"),
        "the unknown code is named"
    );
}
