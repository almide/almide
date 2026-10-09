//! A user module named `__entry` is a module, not the entry program (#3485).
//!
//! The checker's top-let pre-pass told the entry program from a module by the
//! module NAME `__entry`. A user file `__entry.almd` is a module of that name,
//! so its refreshed top-let types were adopted, unqualified, onto the entry's
//! same-named lets: the entry's `let total = int.parse("5") ?? 0` was typed
//! `Float` (rustc E0308 natively, an E082 wall on wasm), and an entry
//! `let total = "ab" + "cd"` was typed `Int` (a false E001 at `println`).
//! The entry is now known by a scope tag, and its pre-pass runs under a
//! pseudo-module no identifier spells. Each program runs on both legs and
//! against a `zentry` twin, so a collision shows as a divergence.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// `(module source, entry source, expected stdout)`, written for a module
/// named `__entry`.
const FLOAT_OVER_INT: (&str, &str, &str) = (
    "let total = float.parse(\"1.5\") ?? 0.0\n\nfn show() -> String = \"${total}\"\n",
    "import __entry\n\nlet total = int.parse(\"5\") ?? 0\n\neffect fn main() -> Unit = {\n  println(__entry.show())\n  println(\"${total}\")\n}\n",
    "1.5\n5\n",
);

const INT_OVER_STRING: (&str, &str, &str) = (
    "let base = [1, 2]\nlet total = list.len(base) + 10\n\nfn show() -> String = \"${total}\"\n",
    "import __entry\n\nlet total = \"ab\" + \"cd\"\n\neffect fn main() -> Unit = {\n  println(__entry.show())\n  println(total)\n}\n",
    "12\nabcd\n",
);

/// Write the two-file project with the module named `module`; the entry path.
fn project(dir: &Path, case: (&str, &str, &str), module: &str) -> PathBuf {
    let (module_src, entry_src, _) = case;
    std::fs::write(dir.join(format!("{module}.almd")), module_src).expect("module");
    let entry = dir.join("main.almd");
    std::fs::write(&entry, entry_src.replace("__entry", module)).expect("entry");
    entry
}

fn log(out: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

fn run(case: (&str, &str, &str), module: &str, target: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let entry = project(dir.path(), case, module);
    let out = Command::new(almide_bin())
        .args(["run", entry.to_str().expect("path"), "--target", target])
        .output()
        .expect("spawn almide run");
    assert!(out.status.success(), "{target} run with module `{module}`:\n{}", log(&out));
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn check(case: (&str, &str, &str), module: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let entry = project(dir.path(), case, module);
    let out = Command::new(almide_bin())
        .args(["check", entry.to_str().expect("path")])
        .output()
        .expect("spawn almide check");
    assert!(out.status.success(), "check with module `{module}`:\n{}", log(&out));
}

fn answers_like_its_twin(target: &str) {
    for case in [FLOAT_OVER_INT, INT_OVER_STRING] {
        assert_eq!(run(case, "zentry", target), case.2, "{target}: the neutral twin");
        assert_eq!(run(case, "__entry", target), case.2, "{target}: a module named `__entry`");
    }
}

#[test]
fn a_module_named_entry_keeps_its_top_let_types_on_native() {
    answers_like_its_twin("rust");
}

#[test]
fn a_module_named_entry_keeps_its_top_let_types_on_wasm() {
    answers_like_its_twin("wasm");
}

#[test]
fn a_module_named_entry_does_not_retype_the_entry_at_check() {
    for case in [FLOAT_OVER_INT, INT_OVER_STRING] {
        check(case, "zentry");
        check(case, "__entry");
    }
}
