//! #1456: Almide accepts every UAX #31 identifier, and both native renders
//! carry the user's names into the generated Rust verbatim. rustc's three
//! identifier lints (`uncommon_codepoints`, `confusable_idents`,
//! `mixed_script_confusables`) warn by default on exactly such names, so the
//! generated crate must allow them: the name was legal Almide, and a warning
//! about it in generated code is noise the author cannot act on.
//!
//! The programs: a lone Cyrillic `а` (the whole Cyrillic usage is confusable,
//! and it collides with the runtime's Latin `a`) and a name from an excluded
//! (archaic) script. A Latin / Cyrillic look-alike pair in one file is E101
//! since #4266 (UTS #39), so it never reaches a render; the lone `а` still
//! meets the runtime's Latin `a` in the generated crate.

use std::process::Command;

const LONE_CYRILLIC: &str = "fn \u{430}(n: Int) -> Int = n + 1\n\neffect fn main() -> Unit = {\n  println(int.to_string(\u{430}(1)))\n}\n";
const ARCHAIC_SCRIPT: &str = "fn \u{16a0}(n: Int) -> Int = n + 3\n\neffect fn main() -> Unit = {\n  println(int.to_string(\u{16a0}(1)))\n}\n";

const PROGRAMS: [(&str, &str); 2] = [("lone Cyrillic", LONE_CYRILLIC), ("archaic script", ARCHAIC_SCRIPT)];

/// The identifier-lint warnings rustc reports for `rust` (type-checked only).
fn identifier_lint_warnings(rust: &str) -> Vec<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("main.rs");
    std::fs::write(&file, rust).expect("write rust");
    let out = Command::new("rustc")
        .args(["--edition", "2021", "--emit=metadata", "--crate-type", "bin", "-o"])
        .arg(dir.path().join("main.rmeta"))
        .arg(&file)
        .output()
        .expect("spawn rustc");
    let stderr = String::from_utf8_lossy(&out.stderr);
    stderr
        .lines()
        .filter(|l| {
            l.starts_with("warning")
                && (l.contains("identifier contains")
                    || l.contains("as identifiers, which look alike")
                    || l.contains("mixed script confusables"))
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn the_classic_rust_emit_allows_the_identifier_lints() {
    for (what, src) in PROGRAMS {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("names.almd");
        std::fs::write(&file, src).expect("write source");
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg(&file)
            .args(["--target", "rust"])
            .output()
            .expect("emit rust");
        assert!(out.status.success(), "{what}: {}", String::from_utf8_lossy(&out.stderr));
        let warnings = identifier_lint_warnings(&String::from_utf8_lossy(&out.stdout));
        assert!(warnings.is_empty(), "{what}: the emitted Rust warns on a legal Almide name: {warnings:?}");
    }
}

#[test]
fn the_v1_native_render_allows_the_identifier_lints() {
    for (what, src) in PROGRAMS {
        let rust = almide_mir::pipeline::try_render_rust_source(src)
            .unwrap_or_else(|e| panic!("{what}: the v1 native render walled: {e:?}"));
        assert!(rust.contains(if what == "archaic script" { '\u{16a0}' } else { '\u{430}' }),
            "{what}: the v1 render no longer spells the user's name, so this test no longer covers it");
        let warnings = identifier_lint_warnings(&rust);
        assert!(warnings.is_empty(), "{what}: the v1 native Rust warns on a legal Almide name: {warnings:?}");
    }
}

#[test]
fn the_external_runtime_slim_main_allows_the_identifier_lints() {
    let header = almide_codegen::slim_main_with_external_runtime(&format!("{}\nfn x() {{}}\n", almide_codegen::RT_BOUNDARY_MARKER))
        .expect("the marker splits");
    let runtime = almide_codegen::emit_runtime_crate();
    for (what, text) in [("slim main", header.as_str()), ("runtime crate", runtime.as_str())] {
        let first = text.lines().find(|l| l.starts_with("#![allow(")).unwrap_or_default();
        for lint in ["uncommon_codepoints", "confusable_idents", "mixed_script_confusables"] {
            assert!(first.contains(lint), "{what}: the crate attribute does not allow `{lint}`: {first}");
        }
    }
}
