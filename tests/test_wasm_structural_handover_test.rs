//! `almide test --target wasm` hands a walled main file to the structural leg (#2121).
//!
//! `almide build --target wasm` already did: when the incumbent walls, the
//! build's reverse handover lets the structural leg render a `main`-carrying
//! program, and the artifact runs. The test runner took only the incumbent's
//! verdict, so the same entry reported SKIP — and a structural-leg defect in
//! it could hide behind that SKIP. Now a test-free main file takes the same
//! route; a file with test blocks reports a WALL (not a declared SKIP —
//! #2180), since the structural leg has no test mode and running its main
//! alone would report nothing.

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

#[test]
fn a_file_with_test_blocks_reports_the_wall_and_says_why() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path(), MAIN_WITH_TEST);
    let (_, text) = test_wasm(dir.path(), &[]);
    // A renderer wall is its own verdict (WALL), never a declared `// wasm:skip`.
    assert!(text.contains("WALL") && text.contains("tests did not run on wasm"), "tests cannot run on the structural leg:\n{text}");
    assert!(!text.contains("no verified wasm rendering"), "the wall names the leg, not the product:\n{text}");
    assert!(text.contains("route to native"), "the wall names the reason:\n{text}");
    assert!(text.contains("no structural-leg test route"), "{text}");
}
