//! `almide check` and `almide build` apply the same rules to a function in an
//! imported module as to one in the entry file (#2373).
//!
//! The two paths each carried their own list of post-solve validations, and
//! the module path's list was missing six of the entry path's twelve. The
//! visible consequence was ADR-0008: `almide check` rejected an implicitly
//! propagated `Result`, `almide build` compiled it and shipped an artifact
//! that ran with the rule removed — but only when the offending function was
//! one `import` away. Which file the author put the function in decided
//! whether the rule applied, and nothing in the source says which that is.
//!
//! These tests pin the AGREEMENT, not one rule: the same source is put to
//! both commands and their verdicts are compared. A rule that reaches one
//! list and not the other fails here whichever rule it is.

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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn scratch(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue2373-{}", name));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::write(
        root.join("almide.toml"),
        "[package]\nname = \"agree\"\nversion = \"0.1.0\"\n",
    )
    .expect("write toml");
    for (file, body) in files {
        std::fs::write(root.join("src").join(file), body).expect("write module");
    }
    root
}

fn verdict(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(almide_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to spawn almide");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), combined)
}

/// The offending fn is in an imported module — the shape from the report.
const RAW: &str = concat!(
    "effect fn read_raw(path: String) -> Result[String, String] =\n",
    "  if string.len(path) > 0 then ok(\"read \" + path) else err(\"empty path\")\n",
);

#[test]
fn build_rejects_implicit_propagation_in_an_imported_module() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "e041-module",
        &[
            (
                "reader.almd",
                &format!("{RAW}effect fn read_it(path: String) -> String = read_raw(path)\n"),
            ),
            (
                "main.almd",
                "import self.reader\neffect fn main() -> Unit = println(reader.read_it(\"a.txt\")!)\n",
            ),
        ],
    );
    let (check_ok, check_out) = verdict(&dir, &["check"]);
    let (build_ok, build_out) = verdict(&dir, &["build", "src/main.almd", "-o", "agree_out"]);
    assert!(!check_ok, "check unexpectedly accepted the E041 shape:\n{check_out}");
    assert!(
        !build_ok,
        "build accepted what check rejected — the module path is missing the rule:\n{build_out}"
    );
    assert!(build_out.contains("E041"), "build rejected it for some OTHER reason:\n{build_out}");
}

/// The must-use half of the same family, same placement.
#[test]
fn build_rejects_a_discarded_result_in_an_imported_module() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "e042-module",
        &[
            (
                "reader.almd",
                &format!(
                    "{RAW}effect fn warm(path: String) -> Unit = {{\n  read_raw(path)\n  ()\n}}\n"
                ),
            ),
            (
                "main.almd",
                "import self.reader\neffect fn main() -> Unit = reader.warm(\"a.txt\")!\n",
            ),
        ],
    );
    let (check_ok, check_out) = verdict(&dir, &["check"]);
    let (build_ok, build_out) = verdict(&dir, &["build", "src/main.almd", "-o", "agree_out"]);
    assert_eq!(
        check_ok, build_ok,
        "check and build disagree on a discarded Result in a module\ncheck:\n{check_out}\nbuild:\n{build_out}"
    );
    assert!(!build_ok, "a discarded Result in a module was accepted:\n{build_out}");
}

/// The documented migration keeps working on both paths, so the rule can be
/// satisfied rather than only reported.
#[test]
fn the_bang_fix_it_satisfies_both_paths() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "e041-fixed",
        &[
            (
                "reader.almd",
                &format!("{RAW}effect fn read_it(path: String) -> String = read_raw(path)!\n"),
            ),
            (
                "main.almd",
                "import self.reader\neffect fn main() -> Unit = println(reader.read_it(\"a.txt\")!)\n",
            ),
        ],
    );
    let (check_ok, check_out) = verdict(&dir, &["check"]);
    let (build_ok, build_out) = verdict(&dir, &["build", "src/main.almd", "-o", "agree_out"]);
    assert!(check_ok, "the `!` form no longer checks clean:\n{check_out}");
    assert!(build_ok, "the `!` form no longer builds:\n{build_out}");
}
