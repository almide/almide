//! #3281: `@export(wasm, "sym")` on a fn of a NON-ROOT module — a sibling
//! `import self.x` module or a dependency package — names an export of the
//! artifact exactly as it does in the root module (#2752). It used to be
//! dropped silently: the build succeeded and the host found no entry point.
//!
//! - the export exists under `sym`, called from main or not (a DCE root);
//! - a module's un-annotated pub fns stay internal (only the declaration
//!   exports, unlike the entry file's pub surface);
//! - one export namespace for the whole artifact: two modules (or a module
//!   and the root) claiming one name are the duplicate wall, naming both.
use std::path::Path;
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

/// Build `<root>/<app>/src/main.almd` to wasm: (ok, bytes, stderr).
fn build(app: &Path) -> (bool, Vec<u8>, String) {
    let wasm = app.join("out.wasm");
    let out = Command::new(almide())
        .current_dir(app)
        .args(["build", "src/main.almd", "--target", "wasm", "-o", wasm.to_str().unwrap()])
        .output()
        .expect("spawn almide");
    let bytes = if out.status.success() { std::fs::read(&wasm).unwrap_or_default() } else { Vec::new() };
    (out.status.success(), bytes, String::from_utf8_lossy(&out.stderr).into_owned())
}

fn exports(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::ExportSection(section) = payload.expect("parse") {
            for export in section {
                names.push(export.expect("export").name.to_string());
            }
        }
    }
    names
}

fn invoke(bytes: &[u8], sym: &str, arg: &str) -> Option<String> {
    if !Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success()) {
        return None;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let wasm = dir.path().join("m.wasm");
    std::fs::write(&wasm, bytes).unwrap();
    let out = Command::new("wasmtime").args(["run", "--invoke", sym]).arg(&wasm).arg(arg).output().expect("wasmtime");
    assert!(out.status.success(), "invoking `{sym}` trapped:\n{}", String::from_utf8_lossy(&out.stderr));
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

const TOML: &str = "[package]\nname = \"exp\"\nversion = \"0.0.1\"\n";

#[test]
fn a_sibling_module_export_is_in_the_artifact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("exp");
    write(&app.join("almide.toml"), TOML);
    // `sub_entry` is called from main; `on_event` is never called — the
    // host is its only caller, so it must be a DCE root. `helper` is a
    // plain pub fn and stays internal.
    write(
        &app.join("src/sub.almd"),
        "@export(wasm, \"sub_entry\")\npub fn sub_entry(x: Int) -> Int = x + 1\n\n@export(wasm, \"framework_event\")\npub fn on_event(x: Int) -> Int = helper(x) * 3\n\npub fn helper(x: Int) -> Int = x + 2\n",
    );
    write(
        &app.join("src/main.almd"),
        "import self.sub\neffect fn main() -> Unit = println(int.to_string(sub.sub_entry(1)))\n",
    );
    let (ok, bytes, err) = build(&app);
    assert!(ok, "build failed:\n{err}");
    let names = exports(&bytes);
    assert!(names.iter().any(|n| n == "sub_entry"), "`sub_entry` must be exported: {names:?}");
    assert!(names.iter().any(|n| n == "framework_event"), "an uncalled declared export is a DCE root: {names:?}");
    assert!(!names.iter().any(|n| n == "on_event" || n == "helper"), "only the declared symbols export: {names:?}");
    if let Some(out) = invoke(&bytes, "framework_event", "4") {
        assert_eq!(out, "18");
    }
}

#[test]
fn a_dependency_package_export_is_in_the_artifact() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(&dir.path().join("fw/almide.toml"), "[package]\nname = \"fw\"\nversion = \"0.1.0\"\n");
    write(
        &dir.path().join("fw/src/mod.almd"),
        "@export(wasm, \"framework_event\")\npub fn on_event(x: Int) -> Int = x * 10\n\npub fn start() -> Int = 1\n",
    );
    let app = dir.path().join("exp");
    write(&app.join("almide.toml"), &format!("{TOML}\n[dependencies]\nfw = {{ path = \"../fw\" }}\n"));
    write(&app.join("src/main.almd"), "import fw\neffect fn main() -> Unit = println(int.to_string(fw.start()))\n");
    let (ok, bytes, err) = build(&app);
    assert!(ok, "build failed:\n{err}");
    let names = exports(&bytes);
    assert!(names.iter().any(|n| n == "framework_event"), "the dependency's export must ship: {names:?}");
    assert!(!names.iter().any(|n| n == "on_event" || n == "start"), "only the declared symbol exports: {names:?}");
    if let Some(out) = invoke(&bytes, "framework_event", "7") {
        assert_eq!(out, "70");
    }
}

#[test]
fn two_modules_exporting_one_name_are_the_duplicate_wall() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("exp");
    write(&app.join("almide.toml"), TOML);
    write(&app.join("src/a.almd"), "@export(wasm, \"tick\")\npub fn ta(x: Int) -> Int = x + 1\n");
    write(&app.join("src/b.almd"), "@export(wasm, \"tick\")\npub fn tb(x: Int) -> Int = x + 2\n");
    write(
        &app.join("src/main.almd"),
        "import self.a\nimport self.b\neffect fn main() -> Unit = println(int.to_string(a.ta(1) + b.tb(1)))\n",
    );
    let (ok, bytes, err) = build(&app);
    assert!(!ok && bytes.is_empty(), "a duplicate export name across modules must refuse the build:\n{err}");
    assert!(err.contains("duplicate wasm export name `tick`"), "{err}");
    assert!(err.contains("a.ta") && err.contains("b.tb"), "the wall names both claimants:\n{err}");
}

#[test]
fn a_module_export_colliding_with_the_root_is_the_duplicate_wall() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("exp");
    write(&app.join("almide.toml"), TOML);
    write(&app.join("src/sub.almd"), "@export(wasm, \"render\")\npub fn draw(x: Int) -> Int = x + 1\n");
    write(
        &app.join("src/main.almd"),
        "import self.sub\n\n@export(wasm, \"render\")\nfn paint(x: Int) -> Int = x * 2\n\neffect fn main() -> Unit = println(int.to_string(sub.draw(paint(1))))\n",
    );
    let (ok, _, err) = build(&app);
    assert!(!ok, "a module export colliding with a root export must refuse the build:\n{err}");
    assert!(err.contains("duplicate wasm export name `render`"), "{err}");
    assert!(err.contains("paint") && err.contains("sub.draw"), "the wall names both claimants:\n{err}");
}

#[test]
fn a_module_export_that_does_not_lower_refuses_the_module() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("exp");
    write(&app.join("almide.toml"), TOML);
    write(
        &app.join("src/sub.almd"),
        "@export(wasm, \"grow\")\npub fn grow(n: Int) -> Int = if n > 100 then todo(\"big\") else n + 1\n\npub fn one() -> Int = 1\n",
    );
    write(&app.join("src/main.almd"), "import self.sub\neffect fn main() -> Unit = println(int.to_string(sub.one()))\n");
    let (ok, bytes, err) = build(&app);
    assert!(!ok && bytes.is_empty(), "a declared module export that does not lower must refuse the build:\n{err}");
    assert!(err.contains("error[E082]") && err.contains("exported function `sub.grow`"), "{err}");
}
