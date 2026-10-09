//! #3495: a call to a generic fn whose type parameter nothing determines —
//! no argument mentions it, the result does not pin it, no explicit type
//! argument names it — is E025 at `check`, on both targets.
//!
//! It passed `check` and then failed the build: native emitted Rust rustc
//! could not infer, wasm walled E082, and the cross-module call panicked in
//! ResolveCalls. The verdict is the checker's, so it is the same whichever
//! target is asked and whichever file the generic fn lives in.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// A package with the given `src/<name>.almd` files.
fn package(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"tp\"\nversion = \"0.1.0\"\n").expect("manifest");
    for (name, body) in files {
        std::fs::write(src.join(format!("{name}.almd")), body).expect("module");
    }
    dir
}

fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(almide()).current_dir(dir).args(args).output().expect("run almide");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn has_wasmtime() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

const TAG: &str = "fn tag[T](s: String) -> Int = string.len(s)\n";

fn single(call: &str) -> tempfile::TempDir {
    let main = format!("{TAG}\neffect fn main() -> Unit = {{\n  println(int.to_string({call}))\n}}\n");
    package(&[("main", &main)])
}

fn cross(call: &str) -> tempfile::TempDir {
    let main = format!("import self.util\n\neffect fn main() -> Unit = {{\n  println(int.to_string({call}))\n}}\n");
    package(&[("util", TAG), ("main", &main)])
}

/// `check`, `check --target wasm` and `run` on both legs all refuse with the
/// one E025 naming the parameter and the fn — never a rustc error, an E082
/// wall or a panic.
fn assert_refused(dir: &std::path::Path, callee: &str) {
    let want = format!("cannot infer type of the type parameter `T` declared on the function `{callee}`");
    let mut legs: Vec<Vec<&str>> = vec![
        vec!["check", "src/main.almd"],
        vec!["check", "src/main.almd", "--target", "wasm"],
        vec!["run", "src/main.almd"],
    ];
    if has_wasmtime() {
        legs.push(vec!["run", "src/main.almd", "--target", "wasm"]);
    }
    for args in legs {
        let (ok, text) = run(dir, &args);
        assert!(!ok, "{args:?} must refuse, got success:\n{text}");
        assert!(text.contains("error[E025]") && text.contains(&want), "{args:?}: expected the E025, got:\n{text}");
        assert_eq!(text.matches("error[").count(), 1, "{args:?}: exactly one error, got:\n{text}");
        assert!(text.contains(&format!("{callee}[Int](")), "{args:?}: the fix names the type argument, got:\n{text}");
        assert!(!text.contains("E082") && !text.contains("panicked"), "{args:?}: no build-stage failure, got:\n{text}");
    }
}

fn assert_runs(dir: &std::path::Path, want: &str) {
    let (ok, native) = run(dir, &["run", "src/main.almd"]);
    assert!(ok && native.ends_with(want), "native:\n{native}");
    if has_wasmtime() {
        let (ok, wasm) = run(dir, &["run", "src/main.almd", "--target", "wasm"]);
        assert!(ok && wasm.ends_with(want), "wasm:\n{wasm}");
    }
}

#[test]
fn an_undetermined_type_parameter_is_e025_in_the_same_file() {
    assert_refused(single("tag(\"abc\")").path(), "tag");
}

#[test]
fn an_undetermined_type_parameter_is_e025_across_a_module() {
    assert_refused(cross("util.tag(\"abc\")").path(), "util.tag");
}

#[test]
fn naming_the_type_argument_builds_on_both_legs() {
    assert_runs(single("tag[Int](\"abc\")").path(), "3\n");
}

/// `util.tag[Int](..)` reaches the module fn's instantiation: the explicit
/// type args were dropped on the `module.fn` path, so the fix-it the E025
/// offers would have been refused by the same E025. (The module-generic
/// monomorphizer still binds only from arguments, so this spelling does not
/// build yet — a separate defect; dropping the unused parameter does.)
#[test]
fn a_module_call_honours_its_explicit_type_arguments() {
    let dir = cross("util.tag[Int](\"abc\")");
    let (ok, text) = run(dir.path(), &["check", "src/main.almd"]);
    assert!(ok, "util.tag[Int] must check, got:\n{text}");
    let main = "import self.util\n\neffect fn main() -> Unit = {\n  println(int.to_string(util.tag(\"abc\")))\n}\n";
    let dropped = package(&[("util", "fn tag(s: String) -> Int = string.len(s)\n"), ("main", main)]);
    assert_runs(dropped.path(), "3\n");
}

/// A parameter that reaches the result is already the call-result E025's
/// (`let _ = util.dekode("5")`); the call gets that one error, not two.
#[test]
fn a_result_only_parameter_keeps_its_single_e025() {
    let dir = package(&[
        ("util", "fn dekode[T](s: String) -> Option[T] = none\n"),
        ("main", "import self.util\n\neffect fn main() -> Unit = {\n  let _ = util.dekode(\"5\")\n  println(\"x\")\n}\n"),
    ]);
    let (ok, text) = run(dir.path(), &["check", "src/main.almd"]);
    assert!(!ok, "must refuse, got success:\n{text}");
    assert_eq!(text.matches("error[E025]").count(), 1, "one E025, got:\n{text}");
    assert!(text.contains("cannot infer a concrete type for this expression"), "got:\n{text}");
}

/// Every way a parameter IS determined stays accepted: by an argument, by
/// the expected type through the result, by an explicit type argument, by a
/// rigid parameter of a generic caller, through a lambda, by a stdlib generic.
#[test]
fn a_determined_type_parameter_is_accepted() {
    let main = "fn tag[T](s: String) -> Int = string.len(s)\n\
                fn ident[T](x: T) -> T = x\n\
                fn mk[T]() -> List[T] = []\n\
                fn wrap[T](x: T) -> Int = tag[T](\"zz\")\n\
                fn apply[A, B](x: A, f: (A) -> B) -> B = f(x)\n\
                \n\
                effect fn main() -> Unit = {\n\
                \x20 let xs: List[Int] = mk()\n\
                \x20 let n = ident(3) + list.len(xs) + wrap(\"q\") + apply(4, (k) => k * 3)\n\
                \x20 println(int.to_string(n + list.len(list.map([1, 2], (x) => x + 1))))\n\
                }\n";
    let dir = package(&[("main", main)]);
    let (ok, text) = run(dir.path(), &["check", "src/main.almd"]);
    assert!(ok, "must check, got:\n{text}");
    assert_runs(dir.path(), "19\n");
}
