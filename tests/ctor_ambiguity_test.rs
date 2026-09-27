//! The intra-module constructor-name collision (E019 extension, almide#1426,
//! edit-locality hunt V3): two variant types in the SAME module declaring the
//! same case name used to register both candidates silently — bare resolution
//! became registration-order-dependent, and the newer case was unreachable.
//! Now a hard error at registration. The cross-module qualified polarity is
//! pinned by spec/integration/modules/qualified_ctor_test.almd.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("e019.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
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
fn same_module_duplicate_ctor_name_is_e019() {
    let (ok, text) = check(
        r#"type Light = | Red | Green
type Alert = | Red | Amber

effect fn main() -> Unit = {
  println("x")
}
"#,
    );
    assert!(!ok, "intra-module duplicate ctor must be a hard error, got success:\n{text}");
    assert!(text.contains("E019"), "duplicate ctor must be E019, got:\n{text}");
    assert!(
        text.contains("'Light' and 'Alert'"),
        "E019 must name both declaring types, got:\n{text}"
    );
}

#[test]
fn distinct_ctor_names_in_one_module_stay_accepted() {
    let (ok, text) = check(
        r#"type Light = | Red | Green
type Alert = | Amber | Clear

effect fn main() -> Unit = {
  println("x")
}
"#,
    );
    assert!(ok, "distinct case names must stay accepted, got:\n{text}");
    assert!(!text.contains("E019"), "E019 must not overfire, got:\n{text}");
}

/// Write a package (`almide.toml` + `src/<name>.almd` for each file) and run
/// `almide check src/main.almd` in it.
fn check_project(files: &[(&str, &str)]) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"leak\"\nversion = \"0.1.0\"\n")
        .expect("write manifest");
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir src");
    for (name, source) in files {
        std::fs::write(dir.path().join("src").join(format!("{name}.almd")), source).expect("write module");
    }
    let out = Command::new(almide())
        .current_dir(dir.path())
        .arg("check")
        .arg("src/main.almd")
        .output()
        .expect("run almide check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

const FINISH: &str = "type Finish = | Stop | Length\n\
fn name(f: Finish) -> String = match f { Stop => \"stop\", Length => \"length\" }\n";

/// #2636: `main` never imports `finish`; its `| Stop` must not be visible
/// there, let alone override `main`'s own `type Stop`.
#[test]
fn a_transitive_modules_case_does_not_override_the_files_own_type() {
    let (ok, text) = check_project(&[
        ("finish", FINISH),
        ("middle", "import self.finish as finish\nfn describe() -> String = finish.name(finish.Stop)\n"),
        ("main", "import self.middle\n\ntype Stop = { message: String }\n\n\
effect fn main() -> Unit = {\n  let s = Stop { message: \"halt\" }\n  println(s.message + \" \" + middle.describe())\n}\n"),
    ]);
    assert!(ok, "the file's own record must win over a transitive case, got:\n{text}");
}

/// #2636: a transitive module's case is no second meaning of a bare name, so
/// it does not make the directly imported module's case ambiguous.
#[test]
fn a_transitive_modules_case_does_not_make_a_bare_name_ambiguous() {
    let (ok, text) = check_project(&[
        ("finish", FINISH),
        ("signal", "type Signal = | Stop(String) | Go\nfn show(s: Signal) -> String = match s { Stop(m) => m, Go => \"go\" }\n"),
        ("middle", "import self.finish as finish\nfn describe() -> String = finish.name(finish.Stop)\n"),
        ("main", "import self.middle\nimport self.signal\n\n\
effect fn main() -> Unit = println(signal.show(Stop(\"x\")) + middle.describe())\n"),
    ]);
    assert!(ok, "only the imported module's case is visible, got:\n{text}");
    assert!(!text.contains("E019"), "E019 must not count an invisible case, got:\n{text}");
}

/// Two IMPORTED modules declaring the case stay ambiguous, and the hint
/// names the qualified spellings that pick one.
#[test]
fn two_imported_modules_sharing_a_case_name_is_e019_with_the_qualified_fix() {
    let (ok, text) = check_project(&[
        ("finish", FINISH),
        ("signal", "type Signal = | Stop(String) | Go\nfn show(s: Signal) -> String = match s { Stop(m) => m, Go => \"go\" }\n"),
        ("main", "import self.finish as fin\nimport self.signal\n\n\
effect fn main() -> Unit = println(signal.show(Stop(\"x\")) + fin.name(fin.Length))\n"),
    ]);
    assert!(!ok, "a case name two imports declare must be an error, got success:\n{text}");
    assert!(text.contains("E019"), "expected E019, got:\n{text}");
    assert!(
        text.contains("`fin.Stop`") && text.contains("`signal.Stop`"),
        "the hint must name the qualified spellings under the written aliases, got:\n{text}"
    );
}
