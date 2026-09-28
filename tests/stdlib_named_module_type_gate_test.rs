//! #2843 gate: a package module named after a stdlib module (`url`) that
//! declares a type the stdlib module also declares (`Url`) is the package's
//! own module and its own type — in-package exactly as it is as a dependency
//! (`dep.url`). The in-package module used to share the stdlib module's key,
//! so its `type Url` collided with the stdlib's pre-registered `url.Url`
//! (E020) and was treated as the stdlib's own declaration.
//!
//! The cells are enumerated from the registry — every bundled module, every
//! `type` its bundled source declares — never from a hand list, and each
//! runs in both shapes. A cell passes when the check reports no error; the
//! in-package cell and the dependency cell must agree.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// `ALMIDE_BIN` runs the cells against another build; the default is this
/// workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn check(dir: &Path) -> (bool, String) {
    let out = Command::new(almide()).current_dir(dir).args(["check", "src/main.almd"]).output().expect("run almide");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

/// `(module, type)` for every `type` a bundled module's source declares.
fn bundled_module_types() -> BTreeSet<(String, String)> {
    let mut cells = BTreeSet::new();
    for module in almide_lang::stdlib_info::BUNDLED_MODULES {
        let Some(source) = almide_lang::stdlib_info::bundled_source(module) else { continue };
        let Some(program) = almide_lang::parse_cached(source) else { continue };
        for decl in &program.decls {
            if let almide_lang::ast::Decl::Type { name, .. } = decl {
                cells.insert((module.to_string(), name.as_str().to_string()));
            }
        }
    }
    assert!(cells.len() >= 10, "the bundled type registry looks truncated: {cells:?}");
    cells
}

fn shadow_module(ty: &str) -> String {
    format!("type {ty} = {{ text: String }}\n\nfn build(t: String) -> {ty} = {ty} {{ text: t }}\n")
}

fn check_in_package(module: &str, ty: &str) -> (bool, String) {
    let root = tempfile::tempdir().expect("tempdir");
    let pkg = root.path();
    write(&pkg.join("almide.toml"), "[package]\nname = \"cellpkg\"\nversion = \"0.1.0\"\n");
    write(&pkg.join(format!("src/{module}.almd")), &shadow_module(ty));
    write(
        &pkg.join("src/main.almd"),
        &format!("import self.{module}\n\neffect fn main() -> Unit = {{\n  println({module}.build(\"ok\").text)\n}}\n"),
    );
    check(pkg)
}

fn check_as_dependency(module: &str, ty: &str) -> (bool, String) {
    let root = tempfile::tempdir().expect("tempdir");
    let dep = root.path().join("cellpkg");
    let app = root.path().join("app");
    write(&dep.join("almide.toml"), "[package]\nname = \"cellpkg\"\nversion = \"0.1.0\"\n");
    write(&dep.join(format!("src/{module}.almd")), &shadow_module(ty));
    write(
        &dep.join("src/mod.almd"),
        &format!("import self.{module}\n\nfn run() -> String = {module}.build(\"ok\").text\n"),
    );
    write(&app.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ncellpkg = { path = \"../cellpkg\" }\n");
    write(&app.join("src/main.almd"), "import cellpkg\n\neffect fn main() -> Unit = {\n  println(cellpkg.run())\n}\n");
    check(&app)
}

#[test]
fn a_package_module_named_after_a_stdlib_module_owns_its_types_in_both_shapes() {
    let mut bad = Vec::new();
    for (module, ty) in bundled_module_types() {
        for (shape, (ok, out)) in [("in-package", check_in_package(&module, &ty)), ("dependency", check_as_dependency(&module, &ty))] {
            if !ok {
                bad.push(format!("{shape} module `{module}` declaring `type {ty}`:\n{out}"));
            }
        }
    }
    assert!(bad.is_empty(), "{} cell(s) rejected:\n{}", bad.len(), bad.join("\n"));
}

/// The reported cell end to end: it builds and runs in-package as it does as
/// a dependency.
#[test]
fn a_package_module_url_declaring_url_runs_in_package() {
    let root = tempfile::tempdir().expect("tempdir");
    let pkg = root.path();
    write(&pkg.join("almide.toml"), "[package]\nname = \"cellpkg\"\nversion = \"0.1.0\"\n");
    write(&pkg.join("src/url.almd"), &shadow_module("Url"));
    write(&pkg.join("src/main.almd"), "import self.url\n\neffect fn main() -> Unit = {\n  println(url.build(\"ok\").text)\n}\n");
    let out = Command::new(almide()).current_dir(pkg).args(["run", "src/main.almd"]).output().expect("run almide");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success() && text.trim_end().ends_with("ok"), "expected `ok`:\n{text}");
}
