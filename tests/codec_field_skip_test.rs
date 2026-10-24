//! #4108 — the Codec field-skip family and its matrix fixture.
//!
//! `@skip` (neither decoded nor encoded) and `@skip_decode` (never decoded,
//! still encoded) are two cells of one declared family, {decode, encode} ×
//! {kept, skipped}: `CODEC_FIELD_SKIPS` in crates/almide-frontend/src/
//! attr_vocab.rs. That table is the ONE declaration — both derive halves read
//! it through `codec_field_use`, and this gate drives a program per row from
//! the same table, on the native AND the wasm leg, so a row the derive does
//! not honour in either direction on either leg fails here.
//!
//! The matrix fixture `spec/wasm_cross/codec_field_skip_matrix.almd` is
//! pinned on its mass-assignment cell, and the negative control
//! (`ALMIDE_CONTROL_IGNORE_SKIP_DECODE=1`, the derive reading a
//! `@skip_decode` field again) must flip that cell to `admin=true` — proving
//! the pin reads the derive's output, not a constant.
//!
//! Skips cleanly when the `almide` binary is unavailable; the wasm leg also
//! needs wasmtime (PATH=/opt/homebrew/bin locally).

use almide_frontend::attr_vocab::{CodecFieldUse, CODEC_FIELD_SKIPS, KNOWN_FIELD_ATTRS};
use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// `almide run` on `src` (a path), optional `--target wasm`, optional env.
fn run(src: &Path, wasm: bool, env: Option<(&str, &str)>) -> (bool, String, String) {
    let mut cmd = Command::new(almide_bin());
    cmd.arg("run").arg(src);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    if let Some((k, v)) = env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("failed to spawn almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// The intentional omissions of the family — shrink-only. The output-only
/// cell (`@skip_encode`: decoded, never encoded) is hiding a value from
/// output, which is the `Secret[A]` type's job or a separate output type's.
const INTENTIONAL_OMISSIONS: &[CodecFieldUse] = &[CodecFieldUse { decoded: true, encoded: false }];

#[test]
fn the_codec_skip_family_is_complete() {
    // Every cell of {decode, encode} × {kept, skipped} is either a row (the
    // plain field is the kept/kept row) or a named omission — never neither,
    // never both.
    let plain = CodecFieldUse { decoded: true, encoded: true };
    for decoded in [true, false] {
        for encoded in [true, false] {
            let cell = CodecFieldUse { decoded, encoded };
            let rows = CODEC_FIELD_SKIPS.iter().filter(|(_, u)| *u == cell).count() + usize::from(cell == plain);
            let omitted = INTENTIONAL_OMISSIONS.contains(&cell);
            assert!(rows + usize::from(omitted) == 1, "cell {cell:?}: {rows} row(s), omitted={omitted} — exactly one must hold");
        }
    }
    // Every family attribute is in the field vocabulary, or E053 refuses it.
    for (name, _) in CODEC_FIELD_SKIPS {
        assert!(KNOWN_FIELD_ATTRS.contains(name), "@{name} is a family row but not in KNOWN_FIELD_ATTRS");
    }
}

#[test]
fn every_family_row_is_honoured_by_the_derive_on_both_legs() {
    if !tool_available() {
        eprintln!("almide unavailable — skipping");
        return;
    }
    let dir = std::env::temp_dir().join(format!("almide-codec-skip-family-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rows: Vec<(Option<&str>, CodecFieldUse)> = std::iter::once((None, CodecFieldUse { decoded: true, encoded: true }))
        .chain(CODEC_FIELD_SKIPS.iter().map(|(n, u)| (Some(*n), *u)))
        .collect();
    let legs: &[bool] = if wasmtime_available() { &[false, true] } else { &[false] };
    for (attr, want) in rows {
        let at = attr.map(|a| format!("@{a} ")).unwrap_or_default();
        let src = format!(
            "import json\n\
             type T: Codec = {{ name: String, {at}x: Int = 0 }}\n\
             fn main() -> Unit = {{\n\
               let d = T.decode(json.parse(\"{{\\\"name\\\": \\\"m\\\", \\\"x\\\": 7}}\") ?? value.null())\n\
               println(match d {{ ok(t) => \"decoded x=${{t.x}}\", err(e) => \"err ${{e}}\" }})\n\
               println(\"encoded ${{json.stringify(T.encode(T {{ name: \\\"m\\\", x: 7 }}))}}\")\n\
             }}\n"
        );
        let path = dir.join(format!("row_{}.almd", attr.unwrap_or("plain")));
        std::fs::write(&path, &src).unwrap();
        for &wasm in legs {
            let leg = if wasm { "wasm" } else { "native" };
            let (ok, out, err) = run(&path, wasm, None);
            assert!(ok, "{leg} {attr:?}: run failed\n{err}");
            let decoded = out.contains("decoded x=7");
            let encoded = out.contains("\"x\":7");
            assert!(out.contains("decoded x="), "{leg} {attr:?}: decode did not succeed:\n{out}");
            assert_eq!(decoded, want.decoded, "{leg} {attr:?}: decode direction wrong:\n{out}");
            assert_eq!(encoded, want.encoded, "{leg} {attr:?}: encode direction wrong:\n{out}");
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

const MATRIX: &str = "spec/wasm_cross/codec_field_skip_matrix.almd";
/// The cell #4108 exists for: a request body carrying `"is_admin": true`
/// must not make a `@skip_decode` field true.
const MASS_ASSIGNMENT_CELL: &str = "skip_decode present decode        ok name=m admin=false";

#[test]
fn the_matrix_pins_the_mass_assignment_cell() {
    if !tool_available() {
        eprintln!("almide unavailable — skipping");
        return;
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(MATRIX);
    let (ok, out, err) = run(&path, false, None);
    assert!(ok, "matrix run failed:\n{err}");
    assert!(out.lines().any(|l| l == MASS_ASSIGNMENT_CELL), "missing `{MASS_ASSIGNMENT_CELL}`:\n{out}");
}

/// Negative control: with the derive told to ignore `@skip_decode`, the same
/// pin must FAIL — the cell reads `admin=true`. If it still passes, the pin is
/// not reading the derive and the gate above proves nothing.
#[test]
fn ignoring_skip_decode_breaks_the_pin() {
    if !tool_available() {
        eprintln!("almide unavailable — skipping");
        return;
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(MATRIX);
    let (ok, out, err) = run(&path, false, Some(("ALMIDE_CONTROL_IGNORE_SKIP_DECODE", "1")));
    assert!(ok, "control run failed:\n{err}");
    assert!(!out.lines().any(|l| l == MASS_ASSIGNMENT_CELL), "control did not flip the cell:\n{out}");
    assert!(
        out.lines().any(|l| l == "skip_decode present decode        ok name=m admin=true"),
        "control should read admin=true:\n{out}"
    );
}
