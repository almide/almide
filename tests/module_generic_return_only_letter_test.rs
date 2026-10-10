//! A module generic whose type parameter no value parameter carries is
//! specialized from the call's type or its explicit type args (#3494).
//!
//! `pub fn dekode[T](s: String) -> Option[T]` in a project module, called as
//! `util.dekode[Int]("5")`, panicked natively with an unresolved
//! `util.dekode` (ResolveCalls) and walled on wasm: the module mono bound
//! letters only from the args, so the instance was never discovered and the
//! generic was pruned under the call. The same fn in the entry file worked.

use std::path::{Path, PathBuf};
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

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

const MANIFEST: &str = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n";

/// A project with `src/util.almd` beside the entry `src/main.almd`.
fn project(tag: &str, util: &str, main: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3494-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    write(&root.join("almide.toml"), MANIFEST);
    write(&root.join("src").join("util.almd"), util);
    write(&root.join("src").join("main.almd"), main);
    root
}

/// An app depending on a path package `dep` whose `src/mod.almd` is `dep_mod`.
fn with_dependency(tag: &str, dep_mod: &str, main: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3494-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    write(&root.join("dep").join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
    write(&root.join("dep").join("src").join("mod.almd"), dep_mod);
    write(
        &root.join("app").join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    write(&root.join("app").join("src").join("main.almd"), main);
    root.join("app")
}

fn assert_both_targets(app: &Path, expected: &str) {
    for target in ["rust", "wasm"] {
        let run = Command::new(almide_bin())
            .args(["run", "src/main.almd", "--target", target])
            .current_dir(app)
            .output()
            .expect("spawn almide");
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{target} run failed:\nstdout: {stdout}\nstderr: {stderr}");
        assert_eq!(stdout, expected, "wrong {target} output: {stdout}");
    }
}

const DEKODE: &str = "pub fn dekode[T](s: String) -> Option[T] = none\n";

#[test]
fn explicit_type_arg_pins_a_return_only_letter() {
    if !tools_available() {
        return;
    }
    let main = "import self.util\n\n\
                effect fn main() -> Unit = {\n\
                \x20 let r: Option[Int] = util.dekode[Int](\"5\")\n\
                \x20 println(\"${r ?? 0}\")\n\
                \x20 println(\"${util.dekode[Int](\"6\") ?? 1}\")\n\
                }\n";
    assert_both_targets(&project("explicit", DEKODE, main), "0\n1\n");
}

#[test]
fn the_expected_type_pins_it_once_per_instance() {
    if !tools_available() {
        return;
    }
    let main = "import self.util\n\n\
                effect fn main() -> Unit = {\n\
                \x20 let a: Option[Int] = util.dekode(\"5\")\n\
                \x20 let b: Option[String] = util.dekode(\"5\")\n\
                \x20 let c: List[Option[Bool]] = [util.dekode(\"x\")]\n\
                \x20 println(\"${a ?? 1} ${b ?? \"s\"} ${list.len(c)}\")\n\
                }\n";
    assert_both_targets(&project("expected", DEKODE, main), "1 s 1\n");
}

#[test]
fn a_letter_in_a_list_return_and_one_beside_an_arg_letter() {
    if !tools_available() {
        return;
    }
    let util = "pub fn none_of[T](s: String) -> List[T] = []\n\n\
                pub fn conv[A, B](x: A) -> Option[B] = none\n";
    let main = "import self.util\n\n\
                effect fn main() -> Unit = {\n\
                \x20 let xs: List[Int] = util.none_of[Int](\"5\")\n\
                \x20 let r: Option[String] = util.conv[Int, String](3)\n\
                \x20 let q: Option[Int] = util.conv(\"x\")\n\
                \x20 println(\"${list.len(xs)} ${r ?? \"d\"} ${q ?? 7}\")\n\
                }\n";
    assert_both_targets(&project("list-and-pair", util, main), "0 d 7\n");
}

#[test]
fn sibling_and_generic_callers_reach_it() {
    if !tools_available() {
        return;
    }
    let util = "pub fn dekode[T](s: String) -> Option[T] = none\n\n\
                pub fn wrap(s: String) -> Int = dekode[Int](s) ?? 0\n\n\
                pub fn twice[T](s: String) -> List[Option[T]] = [dekode[T](s), dekode(s)]\n";
    let main = "import self.util\n\n\
                fn via[U](s: String) -> Option[U] = util.dekode[U](s)\n\n\
                effect fn main() -> Unit = {\n\
                \x20 let r: Option[Int] = via[Int](\"5\")\n\
                \x20 let xs: List[Option[Int]] = util.twice(\"5\")\n\
                \x20 println(\"${util.wrap(\"5\")} ${r ?? 2} ${list.len(xs)}\")\n\
                }\n";
    assert_both_targets(&project("callers", util, main), "0 2 2\n");
}

#[test]
fn a_letter_only_the_explicit_type_arg_names() {
    if !tools_available() {
        return;
    }
    let util = "pub fn tag[T](s: String) -> Int = string.len(s)\n";
    let main = "import self.util\n\n\
                effect fn main() -> Unit = {\n  println(\"${util.tag[Int](\"abc\")}\")\n}\n";
    assert_both_targets(&project("phantom", util, main), "3\n");
}

#[test]
fn a_dependency_package_generic() {
    if !tools_available() {
        return;
    }
    let main = "import dep\n\n\
                effect fn main() -> Unit = {\n\
                \x20 let r: Option[Int] = dep.dekode[Int](\"5\")\n\
                \x20 let s: Option[String] = dep.dekode(\"5\")\n\
                \x20 println(\"${r ?? 0} ${s ?? \"s\"}\")\n\
                }\n";
    assert_both_targets(&with_dependency("dep", DEKODE, main), "0 s\n");
}
