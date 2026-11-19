//! One manifest lookup for every route (#4829, #4830, #4835, #4846): the
//! `almide.toml` that governs a command is the nearest one at or above the
//! entry file, never the working directory's and never one above a temp
//! directory the compiler writes into.
//!
//! Each case runs from outside the project (and the control from inside),
//! so a route that reads `./almide.toml` instead is caught. Set
//! `ALMIDE_CONTROL_BIN` to an older binary to see the cases fail there.

use std::path::Path;
use std::process::{Command, Output};

fn almide(dir: &Path, args: &[&str], envs: &[(&str, &Path)]) -> Output {
    let bin = std::env::var("ALMIDE_CONTROL_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let mut c = Command::new(bin);
    c.args(args).current_dir(dir).env_remove("ALMIDE_SKIP_VERSION_CHECK");
    for (k, v) in envs {
        c.env(k, v);
    }
    c.output().expect("almide runs")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

/// #4846: `proj` pins `almide = "<pin>"`; `main.almd` prints, `main_test.almd` tests.
fn pinned(root: &Path, pin: &str) {
    write(&root.join("proj/almide.toml"), &format!("[package]\nname = \"probe\"\nversion = \"0.1.0\"\nalmide = \"{pin}\"\n"));
    write(&root.join("proj/src/main.almd"), "effect fn main() -> Unit = println(\"probe ran\")\n");
    write(&root.join("proj/src/main_test.almd"), "test \"t\" {\n  assert_eq(1, 1)\n}\n");
    std::fs::create_dir_all(root.join("elsewhere")).unwrap();
}

/// Every route `refuse_older_compiler` lists, from outside and inside.
fn pin_routes(root: &Path) -> Vec<(std::path::PathBuf, Vec<&'static str>)> {
    let (proj, out) = (root.join("proj"), root.join("elsewhere"));
    vec![
        (out.clone(), vec!["check", "../proj/src/main.almd"]),
        (out.clone(), vec!["check", "../proj/src/main.almd", "--json"]),
        (out.clone(), vec!["test", "../proj/src/main_test.almd"]),
        (out.clone(), vec!["test", "../proj/src/main_test.almd", "--target", "wasm"]),
        (out.clone(), vec!["run", "../proj/src/main.almd", "--target", "wasm"]),
        (out.clone(), vec!["emit", "../proj/src/main.almd", "--target", "rust"]),
        (out.clone(), vec!["../proj/src/main.almd", "--target", "rust"]),
        (out, vec!["run", "../proj/src/main.almd"]),
        (proj.clone(), vec!["check", "src/main.almd"]),
        (proj, vec!["run", "src/main.almd", "--target", "wasm"]),
    ]
}

#[test]
fn the_version_pin_of_the_entry_files_project_holds_from_anywhere() {
    let td = tempfile::TempDir::new().unwrap();
    pinned(td.path(), "999.0.0");
    for (dir, args) in pin_routes(td.path()) {
        let o = almide(&dir, &args, &[]);
        let t = text(&o);
        assert!(!o.status.success(), "`almide {}` ran a package pinned to almide 999.0.0:\n{t}", args.join(" "));
        assert!(t.contains("requires almide >= 999.0.0"), "`almide {}` did not name the pin:\n{t}", args.join(" "));
        assert!(!t.contains("probe ran"), "`almide {}` ran the program:\n{t}", args.join(" "));
    }
}

#[test]
fn a_pin_this_compiler_satisfies_is_admitted() {
    let td = tempfile::TempDir::new().unwrap();
    pinned(td.path(), "0.1.0");
    let o = almide(&td.path().join("elsewhere"), &["check", "../proj/src/main.almd"], &[]);
    assert!(o.status.success(), "{}", text(&o));
}

/// #4830: `app` depends on `evil`, whose `@inline_rust` is native code `app`
/// does not grant. The refusal must not depend on where the command runs.
fn ungranted_native(root: &Path) {
    write(&root.join("app/almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nevil = { path = \"../evil\" }\n");
    write(&root.join("app/src/main.almd"), "import evil\n\neffect fn main() -> Unit = println(int.to_string(evil.boom()))\n");
    write(&root.join("app/src/main_test.almd"), "import evil\n\ntest \"t\" {\n  assert_eq(evil.boom(), 7)\n}\n");
    write(&root.join("evil/almide.toml"), "[package]\nname = \"evil\"\nversion = \"0.1.0\"\n");
    write(&root.join("evil/src/mod.almd"), "@inline_rust(\"{ println!(\\\"NATIVE RAN\\\"); 7i64 }\")\npub fn boom() -> Int = 0\n");
    std::fs::create_dir_all(root.join("elsewhere")).unwrap();
}

#[test]
fn an_ungranted_dependency_is_refused_from_outside_the_project() {
    let td = tempfile::TempDir::new().unwrap();
    ungranted_native(td.path());
    let main = td.path().join("app/src/main.almd").canonicalize().unwrap();
    let test = td.path().join("app/src/main_test.almd").canonicalize().unwrap();
    let (main, test) = (main.to_str().unwrap(), test.to_str().unwrap());
    let inside = td.path().join("app");
    let out = td.path().join("elsewhere");
    for (dir, args) in [
        (&inside, vec!["run", "src/main.almd"]),
        (&out, vec!["run", main]),
        (&out, vec!["build", main]),
        (&out, vec!["bench", main]),
        (&out, vec!["test", test]),
    ] {
        let o = almide(dir, &args, &[]);
        let t = text(&o);
        assert!(!o.status.success(), "`almide {}` built an ungranted dependency's native code:\n{t}", args.join(" "));
        assert!(t.contains("dependency `evil` brings native code that"), "`almide {}` did not refuse the grant:\n{t}", args.join(" "));
        assert!(!t.contains("NATIVE RAN"), "`almide {}` ran the native code:\n{t}", args.join(" "));
    }
}

/// #4835: an `almide.toml` above the temp directory must not decide what
/// `almide deps` measures. The project's own `real` is pure; the planted
/// manifest's `real` reads a file.
#[test]
fn a_manifest_above_the_temp_dir_does_not_govern_a_generated_entry() {
    let td = tempfile::TempDir::new().unwrap();
    let root = td.path().canonicalize().unwrap();
    let planted = root.join("planted");
    write(&root.join("almide.toml"), &format!("[package]\nname = \"planter\"\nversion = \"0.1.0\"\n\n[dependencies]\nreal = {{ path = \"{}\" }}\n", planted.display()));
    write(&planted.join("almide.toml"), "[package]\nname = \"real\"\nversion = \"0.1.0\"\n");
    write(&planted.join("src/mod.almd"), "import fs\n\npub effect fn x() -> String = fs.read_text(\"/etc/hosts\") ?? \"\"\n");
    write(&root.join("victim/almide.toml"), "[package]\nname = \"victim\"\nversion = \"0.1.0\"\n\n[dependencies]\nreal = { path = \"../real\" }\n");
    write(&root.join("victim/src/main.almd"), "import real\n\neffect fn main() -> Unit = println(int.to_string(real.x()))\n");
    write(&root.join("real/almide.toml"), "[package]\nname = \"real\"\nversion = \"0.1.0\"\n");
    write(&root.join("real/src/mod.almd"), "pub fn x() -> Int = 1\n");
    let tmp = root.join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    let o = almide(&root.join("victim"), &["deps"], &[("TMPDIR", &tmp)]);
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(t.contains("capabilities: none"), "`almide deps` did not report the project's own pure dependency:\n{t}");
    assert!(!t.contains("FS.read"), "`almide deps` measured the planted manifest's dependency:\n{t}");
}

/// #4830: one `almide test` over files of two projects judges each file
/// under its own `[permissions]`: the one-program lanes took the first
/// file's project for the whole batch.
#[test]
fn a_test_run_over_two_projects_judges_each_file_by_its_own_manifest() {
    let td = tempfile::TempDir::new().unwrap();
    let root = td.path();
    for (name, allow) in [("a_open", "\"IO\", \"FS\", \"Abort\""), ("b_closed", "\"IO\", \"Abort\"")] {
        write(&root.join(format!("{name}/almide.toml")), &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[permissions]\nallow = [{allow}]\n"));
        write(&root.join(format!("{name}/s.txt")), "x");
        // Each test loads a module of its project, so the one-program lane
        // batches the four files (a file that loads none is run alone).
        write(&root.join(format!("{name}/src/{name}_reader.almd")), "import fs\n\npub effect fn read() -> String = fs.read_text(\"s.txt\") ?? \"none\"\n");
        for i in 0..2 {
            write(
                &root.join(format!("{name}/src/{name}_r{i}_test.almd")),
                &format!("import {name}_reader\n\ntest \"reads\" {{\n  assert_eq({name}_reader.read()!, {name}_reader.read()!)\n}}\n"),
            );
        }
    }
    let o = almide(root, &["test", ".", "--target", "wasm"], &[]);
    let t = text(&o);
    assert!(!o.status.success(), "a file of b_closed read a file under allow = [\"IO\", \"Abort\"]:\n{t}");
    assert!(t.contains("FS.read is not in [permissions].allow"), "the refusal was not named:\n{t}");
}
