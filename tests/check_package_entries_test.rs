//! `almide check` with no file inside a package (#2165).
//!
//! The bare form used to check `src/mod.almd` alone and print `No errors
//! found` while `src/main.almd` in the same package did not type-check. Four
//! consecutive red CI runs came from exactly that: the local command that
//! reads as "check this project" was checking one file it never named.
//!
//! What is asserted: every `.almd` under `src/` is judged, each names itself,
//! a broken sibling turns the whole check red, and the single-file form's
//! output is byte-for-byte what it was.

use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const TOML: &str = "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\n";
const MOD: &str = "fn greet(who: String) -> String = \"hi ${who}\"\n";
const MAIN_OK: &str = "import self as pkg\neffect fn main() -> Unit = println(pkg.greet(\"a\"))\n";
// One argument short: mod.almd's signature moved and this entry did not.
const MAIN_BROKEN: &str = "import self as pkg\neffect fn main() -> Unit = println(pkg.greet())\n";

fn package(dir: &Path, main: &str) {
    std::fs::write(dir.join("almide.toml"), TOML).expect("write almide.toml");
    std::fs::create_dir_all(dir.join("src/inner")).expect("mkdir src");
    std::fs::write(dir.join("src/mod.almd"), MOD).expect("write mod");
    std::fs::write(dir.join("src/main.almd"), main).expect("write main");
    std::fs::write(dir.join("src/inner/util.almd"), "fn one() -> Int = 1\n").expect("write util");
}

fn check_in(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(almide())
        .current_dir(dir)
        .arg("check")
        .args(args)
        .output()
        .expect("run almide check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn bare_check_judges_every_entry_and_names_each() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path(), MAIN_OK);
    let (ok, text) = check_in(dir.path(), &[]);
    assert!(ok, "expected a clean check, got:\n{text}");
    // mod first, main second, the rest in path order — each named.
    let mod_at = text.find("src/mod.almd: ok").expect("mod.almd named");
    let main_at = text.find("src/main.almd: ok").expect("main.almd named");
    let util_at = text.find("src/inner/util.almd: ok").expect("nested file named");
    assert!(mod_at < main_at && main_at < util_at, "entry order drifted:\n{text}");
    assert!(text.contains("Checked 3 files — No errors found"), "summary missing:\n{text}");
}

#[test]
fn a_broken_sibling_entry_turns_the_bare_check_red() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path(), MAIN_BROKEN);
    let (ok, text) = check_in(dir.path(), &[]);
    assert!(!ok, "a package with a broken entry must not check clean:\n{text}");
    assert!(text.contains("src/mod.almd: ok"), "the clean entry before it still reports:\n{text}");
    assert!(text.contains("src/main.almd"), "the diagnostic names the broken entry:\n{text}");
    assert!(!text.contains("No errors found"), "no green verdict on a red check:\n{text}");
}

#[test]
fn single_file_form_is_unchanged() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path(), MAIN_BROKEN);
    // Naming mod.almd explicitly is still a one-file check: the broken
    // sibling is not consulted, and the verdict is the plain line.
    let (ok, text) = check_in(dir.path(), &["src/mod.almd"]);
    assert!(ok, "explicit single file must check on its own:\n{text}");
    assert_eq!(text.trim(), "No errors found", "single-file verdict must not change:\n{text}");
}

#[test]
fn json_on_a_clean_package_is_silent_and_green() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path(), MAIN_OK);
    let (ok, text) = check_in(dir.path(), &["--json"]);
    assert!(ok, "--json on a clean package:\n{text}");
    assert!(!text.contains(": ok"), "--json is rows only, no per-entry verdict lines:\n{text}");
    assert_eq!(text.trim(), "", "a clean package has no rows:\n{text}");
}

/// #2253: the bare `--json` form walks the package like the bare form does —
/// one row per diagnostic across every entry, each naming its `file`, so a
/// harness no longer checks the entries singly (re-inferring the siblings
/// each time).
#[test]
fn json_judges_every_entry_and_each_row_names_its_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path(), MAIN_BROKEN);
    // A second broken entry, later in the walk order, so the report is shown
    // to continue past the first one.
    std::fs::write(dir.path().join("src/inner/util.almd"), "fn one() -> Int = \"1\"\n").expect("write util");
    let (ok, text) = check_in(dir.path(), &["--json"]);
    // Both entries parse, but a type error is still a rejection, so the exit
    // code is the single-file form's `1` — the same answer the plain form
    // gives for this package. This assertion used to read `ok` with the note
    // "type errors keep the json exit code at 0": the test pinned #2350 as if
    // it were the rule. What the case is actually about is one row per
    // diagnostic across every entry, in walk order, each naming its file, and
    // all of that is asserted below unchanged.
    assert!(!ok, "a type error must turn the json check red too:\n{text}");
    let rows: Vec<serde_json::Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("row is not JSON ({e}): {l}")))
        .collect();
    let files: Vec<&str> = rows.iter().map(|r| r["file"].as_str().expect("row names its file")).collect();
    assert!(files.contains(&"src/main.almd"), "the broken main entry is reported:\n{text}");
    assert!(files.contains(&"src/inner/util.almd"), "the broken nested entry is reported too:\n{text}");
    let main_at = files.iter().position(|f| *f == "src/main.almd").unwrap();
    let util_at = files.iter().position(|f| *f == "src/inner/util.almd").unwrap();
    assert!(main_at < util_at, "rows follow the bare form's entry order:\n{text}");
    assert!(rows.iter().all(|r| r["level"] == "error"), "{text}");
}

#[test]
fn json_exit_code_is_one_when_an_entry_does_not_parse() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path(), MAIN_OK);
    std::fs::write(dir.path().join("src/inner/util.almd"), "fn one( -> Int = 1\n").expect("write util");
    let (ok, text) = check_in(dir.path(), &["--json"]);
    assert!(!ok, "an entry that did not parse keeps the single-file exit code 1:\n{text}");
    assert!(text.contains("\"file\":\"src/inner/util.almd\"") || text.contains("src/inner/util.almd"), "{text}");
}
