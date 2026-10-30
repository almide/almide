//! A dependency's `mod type` record does not capture a same-named type the
//! depending package declares (#4352).
//!
//! On v0.69.0-rc1 the app's own `type ToolCall` resolved to the dependency's
//! private `mod type ToolCall` (in `dep.sse`), so building and reading the
//! app's own record failed with E033 "cannot build a literal of opaque type
//! 'ToolCall' outside its module 'dep.sse'". The app's type is declared both
//! in a submodule (the reported shape) and in the entry file itself. The
//! control keeps the boundary: a value of the dependency's opaque type, read
//! from the app, is still E033.

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

const DEP_SSE: &str = "mod type ToolCall = { id: String }\n\
                       mod fn first_id(calls: List[ToolCall]) -> String = (list.get(calls, 0) ?? { id: \"\" }).id\n\
                       mod fn make(id: String) -> ToolCall = { id: id }\n";

const DEP_MOD: &str = "import self.sse as sse\n\
                       \n\
                       fn count() -> String = sse.first_id([])\n\
                       \n\
                       fn call(id: String) -> sse.ToolCall = sse.make(id)\n";

const APP_PROV: &str = "import dep\n\
                        \n\
                        type ToolCall = { id: String }\n\
                        \n\
                        fn make(id: String) -> ToolCall = ToolCall { id: id }\n\
                        \n\
                        fn id_of(t: ToolCall) -> String = t.id\n\
                        \n\
                        fn uses_dep() -> String = dep.count()\n";

/// `entry`: the app's `ToolCall` is declared in the entry file; otherwise in
/// the `prov` submodule.
fn scratch(tag: &str, entry: bool, main_body: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4352-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(&root.join("dep").join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
    write(&root.join("dep").join("src").join("sse.almd"), DEP_SSE);
    write(&root.join("dep").join("src").join("mod.almd"), DEP_MOD);
    write(
        &root.join("app").join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    let main = if entry {
        format!("{APP_PROV}\n{main_body}")
    } else {
        write(&root.join("app").join("src").join("prov.almd"), APP_PROV);
        format!("import self.prov as prov\n\n{main_body}")
    };
    write(&root.join("app").join("src").join("main.almd"), &main);
    root.join("app")
}

fn run(app: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(almide_bin()).args(args).current_dir(app).output().expect("spawn almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn assert_runs_on_both_targets(app: &Path, expected: &str) {
    let (ok, out) = run(app, &["check", "src/main.almd"]);
    assert!(ok, "check refused the app's own ToolCall:\n{out}");
    for target in ["rust", "wasm"] {
        let out = Command::new(almide_bin())
            .args(["run", "src/main.almd", "--target", target])
            .current_dir(app)
            .output()
            .expect("spawn almide");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{target} run failed:\nstdout: {stdout}\nstderr: {stderr}");
        assert_eq!(stdout, expected, "wrong {target} output");
    }
}

#[test]
fn a_dependency_mod_type_beside_a_same_named_type_in_an_app_submodule() {
    if !tools_available() {
        return;
    }
    let app = scratch(
        "submodule",
        false,
        "effect fn main() -> Unit = {\n  println(prov.id_of(prov.make(\"x\")))\n  println(\"[${prov.uses_dep()}]\")\n}\n",
    );
    // The submodule checked on its own is the reported command.
    let (ok, out) = run(&app, &["check", "src/prov.almd"]);
    assert!(ok, "check refused the submodule's own ToolCall:\n{out}");
    assert_runs_on_both_targets(&app, "x\n[]\n");
}

#[test]
fn a_dependency_mod_type_beside_a_same_named_type_in_the_app_entry_file() {
    if !tools_available() {
        return;
    }
    let app = scratch(
        "entry",
        true,
        "effect fn main() -> Unit = {\n  println(id_of(make(\"y\")))\n  println(\"[${uses_dep()}]\")\n}\n",
    );
    assert_runs_on_both_targets(&app, "y\n[]\n");
}

/// The control: the dependency's `mod type` stays opaque to the app. A value
/// of it, handed out by the dependency, cannot be read in the app — next to
/// the app's own same-named type and without one.
#[test]
fn the_dependency_mod_type_is_still_opaque_to_the_app() {
    if !tools_available() {
        return;
    }
    for (tag, entry) in [("control-entry", true), ("control-submodule", false)] {
        // The entry file already imports `dep` (it holds the app's type).
        let main = if entry {
            "effect fn main() -> Unit = println(id_of(make(\"a\")) + dep.call(\"z\").id)\n".to_string()
        } else {
            "import dep\n\neffect fn main() -> Unit = println(prov.id_of(prov.make(\"a\")) + dep.call(\"z\").id)\n".to_string()
        };
        let app = scratch(tag, entry, &main);
        let (ok, out) = run(&app, &["check", "src/main.almd"]);
        assert!(!ok, "{tag}: reading the dependency's mod type was accepted:\n{out}");
        assert!(
            out.contains("cannot read field 'id' of opaque type 'ToolCall' outside its module 'dep.sse'"),
            "{tag}: expected the E033 field read refusal:\n{out}"
        );
    }
}
