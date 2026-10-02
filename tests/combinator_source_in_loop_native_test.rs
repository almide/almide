//! #3208: a list read by a list combinator inside a loop or closure body must
//! BUILD on the native target, not only run on wasm.
//!
//! The extra use count a repeated body gives every outer var
//! (`almide_ir::use_count`) skipped iterator chains, so the fused
//! `list.find(ids, f)` in a `for` body saw `ids` as single-use, lowered it to
//! `(ids).into_iter().find(..)` and rustc refused the program (E0382, use of
//! moved value) while wasm printed the right answer. The two spec files hold
//! the shapes and the family matrix (one op per combinator family x three
//! element types x a for body and a closure body; the full 408-cell sweep is
//! cited in the PR). `almide test` runs them on the wasm leg, which never
//! failed, so this gate rebuilds each one as a native program: every
//! `test` block becomes a plain fn and `main` calls them in order. One rustc
//! build per file.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// The spec file as a native program: `test "..." {` → `fn spec_case_N() -> Unit = {`,
/// and a `main` that calls every case, then prints `ok`.
fn as_program(spec: &str) -> String {
    let mut n = 0;
    let mut out = String::new();
    for line in spec.lines() {
        if line.starts_with("test \"") && line.ends_with('{') {
            n += 1;
            out.push_str(&format!("fn spec_case_{n}() -> Unit = {{\n"));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str("\neffect fn main() -> Unit = {\n");
    for i in 1..=n {
        out.push_str(&format!("  spec_case_{i}()\n"));
    }
    out.push_str("  println(\"ok\")\n}\n");
    assert!(n > 0, "the spec file lost its test blocks");
    out
}

fn assert_builds_and_passes_natively(name: &str) {
    let path = format!("{}/spec/lang/{name}", env!("CARGO_MANIFEST_DIR"));
    let spec = std::fs::read_to_string(&path).expect("read spec file");
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join(name.replace("_test.almd", "_native.almd"));
    std::fs::write(&src, as_program(&spec)).expect("write program");
    let out = Command::new(almide()).args(["run", src.to_str().unwrap()]).output().expect("run almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.trim_end().ends_with("ok"),
        "{name} failed on the native target (a chain source moved in a repeated body again?):\n{}\n{stdout}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The ownership certifier re-derives every native verdict from the final
    // IR; a param a repeated body only reads must not be rendered owned (C4,
    // #3214), and nothing else in these files may be flagged either.
    let out = Command::new(almide())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .env("ALMIDE_CERTIFY_OWNERSHIP", "report")
        .output()
        .expect("emit rust");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let violations: Vec<&str> = stderr.lines().filter(|l| l.contains("[CERTIFY OWNERSHIP]")).collect();
    assert!(violations.is_empty(), "{name}: the ownership certifier flagged the native verdicts:\n{}", violations.join("\n"));
}

#[test]
fn combinator_sources_in_repeated_bodies_build_natively() {
    assert_builds_and_passes_natively("combinator_source_in_loop_test.almd");
}

#[test]
fn combinator_family_matrix_builds_natively() {
    assert_builds_and_passes_natively("combinator_source_in_loop_matrix_test.almd");
}

/// #3214: the matrix with `js` as a PARAM. `list.find_index` / `list.find_map`
/// were unfused, so the element their callback captured made the param owned.
#[test]
fn combinator_param_matrix_builds_natively_and_borrows() {
    assert_builds_and_passes_natively("combinator_source_param_matrix_test.almd");
}
