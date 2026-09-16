//! #2223: a user module whose name collides with a stdlib module's (`net`,
//! `svc`, …) is still a USER module. Its effect fns return `Result`, so a
//! method call on the bare result (`net.f().len()`) is E002 and an
//! un-annotated `let s = net.f()` is E041 — exactly what the same code gets
//! when the module is called `svc`. The checker used to treat any call whose
//! module name is bundled in the stdlib as a stdlib call, skip the `Result`
//! wrap, and let `net.f().len()` through to a rustc error.
use std::path::Path;
use std::process::Command;

fn almide() -> &'static str { env!("CARGO_BIN_EXE_almide") }

const TOML: &str = "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\nedition = \"2026\"\n\n[permissions]\nallow = [\"IO\"]\n";
const MODULE: &str = "effect fn f() -> String = {\n  \"abc\"\n}\n";

fn project(dir: &Path, module: &str, main: &str) {
    std::fs::write(dir.join("almide.toml"), TOML).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join(format!("src/{module}.almd")), MODULE).unwrap();
    std::fs::write(dir.join("src/main.almd"), main.replace("MOD", module)).unwrap();
}

fn run(dir: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(almide()).current_dir(dir).args(args).output().expect("run almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn codes(text: &str) -> Vec<String> {
    text.lines().filter(|l| l.starts_with('{'))
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["code"].as_str().unwrap().to_string())
        .collect()
}

/// The same program under the colliding name and under a neutral one gets the
/// same verdict. Asserting parity rather than a fixed code keeps the test
/// honest if the diagnostic for the shape ever changes.
fn same_verdict(main: &str, expected: &[&str]) {
    let mut verdicts = Vec::new();
    for module in ["net", "svc"] {
        let dir = tempfile::tempdir().unwrap();
        project(dir.path(), module, main);
        let (_, text) = run(dir.path(), &["check", "--json", "src/main.almd"]);
        verdicts.push((module, codes(&text)));
    }
    assert_eq!(verdicts[0].1, verdicts[1].1, "`net` and `svc` judged differently: {verdicts:?}");
    assert_eq!(verdicts[0].1, expected, "{verdicts:?}");
}

#[test]
fn a_method_on_the_bare_result_is_rejected_for_a_module_named_like_a_stdlib_one() {
    same_verdict(
        "import self.MOD\n\neffect fn main() -> Unit = {\n  println(int.to_string(MOD.f().len()))\n}\n",
        &["E002"],
    );
}

#[test]
fn an_unannotated_binding_of_the_result_is_e041_for_a_module_named_like_a_stdlib_one() {
    same_verdict(
        "import self.MOD\n\neffect fn main() -> Unit = {\n  let s = MOD.f()\n  println(int.to_string(s.len()))\n}\n",
        &["E041"],
    );
}

#[test]
fn the_propagating_program_builds_and_runs_under_the_colliding_name() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), "net", "import self.net\n\neffect fn main() -> Unit = {\n  println(\"len=\" + int.to_string(string.len(net.f()!)))\n}\n");
    let (ok, text) = run(dir.path(), &["run", "src/main.almd"]);
    assert!(ok, "{text}");
    assert_eq!(text.trim(), "len=3");
}

#[test]
fn the_stdlib_module_of_the_same_name_is_still_the_stdlib() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("prog.almd"), "import net\n\neffect fn main() -> Unit = {\n  println(\"ok\")\n}\n").unwrap();
    let (ok, text) = run(dir.path(), &["check", "prog.almd"]);
    assert!(ok, "{text}");
}
