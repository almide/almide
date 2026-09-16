//! `almide test --target wasm` renders a file the incumbent walls on the structural leg (#2121, #2179).
//!
//! `almide build --target wasm` always did: when the incumbent walls, the
//! build's reverse handover lets the structural leg render a `main`-carrying
//! program, and the artifact runs. The test runner took only the incumbent's
//! verdict, so the same entry reported SKIP — and a structural-leg defect in
//! it could hide behind that SKIP. #2121 gave a test-free main file the
//! build's route; a file with test blocks still reported a WALL, since the
//! structural leg had no test mode. #2179 gave the lane the product's routing
//! outright — structural first, with the shared `__test_runner` synthesis —
//! so both shapes below run their tests on the structural leg.

use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

// A sibling whose export-mode shape the incumbent walls (#2160): a pub fn
// matching a locally produced variant and returning a String.
const JUDGE: &str = "type T = | A(String) | B\n\
                     local fn make(s: String) -> T = if s == \"\" then B else A(s)\n\
                     pub fn verdict(s: String) -> String = match make(s) { A(x) => \"some ${x}\", B => \"none\" }\n";
const MAIN: &str = "import self.judge\n\
                    fn main() -> Unit = {\n  println(judge.verdict(\"x\"))\n  println(judge.verdict(\"\"))\n}\n";
const MAIN_WITH_TEST: &str = "import self.judge\n\
                              fn main() -> Unit = println(judge.verdict(\"x\"))\n\
                              test \"verdict\" { assert_eq(judge.verdict(\"\"), \"none\") }\n";

fn package(dir: &Path, main: &str) {
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"handover\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/judge.almd"), JUDGE).unwrap();
    std::fs::write(dir.join("src/main.almd"), main).unwrap();
}

fn test_wasm(dir: &Path, extra: &[&str]) -> (bool, String) {
    let out = Command::new(almide())
        .current_dir(dir)
        .args(["test", "src/main.almd", "--target", "wasm", "--verbose"])
        .args(extra)
        .output()
        .expect("run almide test");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

#[test]
fn a_test_free_main_the_incumbent_walls_runs_on_the_structural_leg() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path(), MAIN);
    // Precondition: the build already ships this program on the structural leg.
    let built = Command::new(almide()).current_dir(dir.path())
        .args(["build", "src/main.almd", "--target", "wasm", "-o", "out.wasm"]).output().unwrap();
    assert!(built.status.success(), "build must ship it:\n{}", String::from_utf8_lossy(&built.stderr));
    let (ok, text) = test_wasm(dir.path(), &["--allow-no-tests"]);
    assert!(ok, "the test runner must take the same route as the build:\n{text}");
    assert!(!text.contains("SKIP"), "no skip once the structural leg can run it:\n{text}");
    assert!(text.contains("1 passed, 0 failed"), "{text}");
}

/// The file with a test block the incumbent walls: its test RUNS on the lane
/// (#2179) — the structural leg renders the synthesized runner, the user
/// `main` is dropped exactly as native's test mode drops it, and nothing is
/// reported as a wall or a skip.
#[test]
fn a_file_with_test_blocks_runs_its_tests_on_the_structural_leg() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path(), MAIN_WITH_TEST);
    let (ok, text) = test_wasm(dir.path(), &[]);
    assert!(ok, "the test lane must take the build's route:\n{text}");
    assert!(!text.contains("WALL") && !text.contains("SKIP"), "neither a wall nor a skip:\n{text}");
    assert!(text.contains("1 tests passed"), "and the test itself ran:\n{text}");
}
