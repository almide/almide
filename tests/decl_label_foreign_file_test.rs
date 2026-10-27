//! A "defined here" secondary label is drawn only over the file that holds
//! the declaration (#4271).
//!
//! A fn's declaration span is a position in its DECLARING file. E005 on a
//! stdlib call (`string.replace`) attached that span as a secondary label of
//! the caller's diagnostic, and the renderer drew it over the caller's file:
//! an unrelated user line came out as "fn string.replace() defined here".
//! The same held for a fn of another user module. A fn declared in the file
//! being checked keeps its label.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4271-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    root
}

/// `almide check <file>` in `dir`: (succeeded, stdout + stderr).
fn check(dir: &Path, file: &str) -> (bool, String) {
    let out = Command::new(almide())
        .args(["check", file])
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .output()
        .expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// Enough filler that the stdlib declaration's line number lands on a line
/// of this file — the shape of the original report.
fn filler() -> String {
    (0..400).map(|i| format!("// filler line {i}\n")).collect()
}

#[test]
fn a_stdlib_fn_is_not_labelled_over_the_callers_file() {
    let dir = scratch("stdlib");
    let src = format!(
        "fn f(o: Option[String]) -> String = string.replace(\"abc\", o, \"b\")\n{}fn main() -> Unit = println(f(some(\"a\")))\n",
        filler()
    );
    write(&dir.join("lbl.almd"), &src);
    let (ok, text) = check(&dir, "lbl.almd");
    assert!(!ok, "check must fail:\n{text}");
    assert!(text.contains("E005"), "expected E005:\n{text}");
    assert!(!text.contains("defined here"), "a stdlib declaration was labelled over the user's file:\n{text}");
    assert!(!text.contains("filler line"), "an unrelated user line was drawn:\n{text}");
}

#[test]
fn another_modules_fn_is_not_labelled_over_the_callers_file() {
    let dir = scratch("module");
    write(&dir.join("almide.toml"), "[package]\nname = \"p4271\"\nversion = \"0.1.0\"\n");
    write(
        &dir.join("src").join("util.almd"),
        &format!("{}fn shout(s: String) -> String = s + \"!\"\n", filler()),
    );
    write(
        &dir.join("src").join("main.almd"),
        &format!("import self.util\n\n{}fn main() -> Unit = println(util.shout(1))\n", filler()),
    );
    let (ok, text) = check(&dir, "src/main.almd");
    assert!(!ok, "check must fail:\n{text}");
    assert!(text.contains("E005"), "expected E005:\n{text}");
    assert!(!text.contains("defined here"), "a module's declaration was labelled over main.almd:\n{text}");
}

#[test]
fn a_fn_of_the_same_file_keeps_its_label() {
    let dir = scratch("own");
    let src = "fn g(s: String) -> String = s\n\nfn main() -> Unit = println(g(1))\n";
    write(&dir.join("own.almd"), src);
    let (ok, text) = check(&dir, "own.almd");
    assert!(!ok, "check must fail:\n{text}");
    assert!(text.contains("E005"), "expected E005:\n{text}");
    let label = text.lines().position(|l| l.contains("fn g() defined here"))
        .unwrap_or_else(|| panic!("the same-file label is missing:\n{text}"));
    assert!(
        text.lines().take(label).any(|l| l.contains("fn g(s: String)")),
        "the label is not drawn under g's declaration:\n{text}"
    );
}
