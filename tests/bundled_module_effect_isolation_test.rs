//! #848: a BUNDLED stdlib module obeys the effect rule (E006) it imposes on
//! every user module.
//!
//! Bundled modules are inferred on every check, but their diagnostics were
//! dropped, so `args.flag` — a plain fn calling the effect fn `env.args` —
//! compiled for every importer while the same body in a user module was E006.
//! The check driver now reports a bundled module's E006 against
//! `<bundled stdlib>/<module>.almd` (src/wasm_leg.rs); this test imports every
//! bundled module the registry lists, one per program, and refuses any such
//! report. The cells come from `BUNDLED_MODULES`, never from a hand list.
//! (`prim` is excluded: importing it is E085 outside the stdlib by design.)

use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

#[test]
fn no_bundled_module_calls_an_effect_fn_from_a_plain_fn() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut checked = 0;
    let mut bad = Vec::new();
    for module in almide_lang::stdlib_info::BUNDLED_MODULES.iter().filter(|m| **m != "prim") {
        let path = dir.path().join(format!("use_{module}.almd"));
        std::fs::write(&path, format!("import {module}\n\neffect fn main() -> Unit = println(\"ok\")\n")).expect("write");
        let out = Command::new(almide()).arg("check").arg(&path).output().expect("run almide");
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        if text.contains("<bundled stdlib>/") {
            bad.push(format!("`import {module}`:\n{text}"));
        }
        checked += 1;
    }
    assert!(checked >= 40, "the bundled module registry looks truncated: {checked}");
    assert!(bad.is_empty(), "{} bundled module(s) break the effect rule:\n{}", bad.len(), bad.join("\n"));
}
