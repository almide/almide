//! A dependency's type referenced by its QUALIFIED spelling while the entry
//! program declares a same-named type (#1955).
//!
//! `almide check` was green and the wasm leg ran, but the native build
//! constructed the entry program's `Box` for `sh.Box { w: 1 }`: the struct
//! literal's module part was the spelling the source WROTE (an import alias
//! `sh`, or the short last segment `shape`) while the type table keyed the
//! dependency's struct under `dep.shape.Box`, so the lookup missed and the
//! ctor name fell to the bare `Box` — the local one. With a local `Box` in
//! scope the same miss made the checker read `shape.Box` annotations as the
//! local type. Every alias spelling now resolves to the canonical key
//! (`register_alias_type_keys`, dotted-suffix resolution), in literals,
//! patterns, and annotations, on both legs.

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

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

const DEP_SHAPE: &str = "type Box = { w: Int }\nfn render(b: Box) -> String = \"dep:${int.to_string(b.w)}\"\n";

/// The entry program: a local `Box` that differs in shape from the
/// dependency's, and the dependency's `Box` reached through `import_line`
/// with module spelling `m` — in a literal, a parameter annotation, a
/// `let` annotation, and a record pattern.
fn app_main(import_line: &str, m: &str) -> String {
    format!(
        "{import_line}\n\
         type Box = {{ w: Int, tag: String }}\n\
         fn render(b: Box, n: Int) -> String = \"app:${{b.tag}}:${{int.to_string(n)}}\"\n\
         fn take(b: {m}.Box) -> String = {m}.render(b)\n\
         fn width(b: {m}.Box) -> Int = match b {{\n  {m}.Box {{ w }} => w,\n}}\n\
         fn via_dep() -> String = {{\n  let b: {m}.Box = {m}.Box {{ w: 3 }}\n  take(b) + \"/\" + int.to_string(width({m}.Box {{ w: 7 }}))\n}}\n\
         effect fn main() -> Unit = println(render(Box {{ w: 1, tag: \"x\" }}, 2) + \" \" + via_dep())\n\
         test \"local names win\" {{ assert_eq(render(Box {{ w: 1, tag: \"x\" }}, 2), \"app:x:2\") }}\n\
         test \"dep names through the module spelling\" {{ assert_eq(via_dep(), \"dep:3/7\") }}\n"
    )
}

fn scratch(tag: &str, import_line: &str, m: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue1955-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    write(&root.join("dep").join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
    write(&root.join("dep").join("src").join("shape.almd"), DEP_SHAPE);
    write(
        &root.join("app").join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    write(&root.join("app").join("src").join("main.almd"), &app_main(import_line, m));
    root.join("app")
}

fn assert_both_legs(app: &Path) {
    let run = Command::new(almide_bin())
        .args(["run", "src/main.almd"])
        .current_dir(app)
        .output()
        .expect("spawn almide");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "native run failed:\nstdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stdout, "app:x:2 dep:3/7\n", "wrong native output: {stdout}");

    let test = Command::new(almide_bin())
        .arg("test")
        .current_dir(app)
        .output()
        .expect("spawn almide");
    let stdout = String::from_utf8_lossy(&test.stdout);
    let stderr = String::from_utf8_lossy(&test.stderr);
    assert!(test.status.success(), "almide test failed:\nstdout: {stdout}\nstderr: {stderr}");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("test file(s) passed"),
        "a test block failed:\nstdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn dependency_type_through_an_import_alias_beside_a_same_named_local_type() {
    if !tools_available() {
        return;
    }
    assert_both_legs(&scratch("alias", "import dep.shape as sh", "sh"));
}

#[test]
fn dependency_type_through_the_short_module_name_beside_a_same_named_local_type() {
    if !tools_available() {
        return;
    }
    assert_both_legs(&scratch("short", "import dep.shape", "shape"));
}
