//! What a non-`self` import LOADS must not depend on where it is written
//! (#884).
//!
//! `import pkg` loads the package entry point and, through it, the package's
//! sibling sub-namespaces. `import pkg.sub` loads only that sub-module and its
//! transitive imports. The entry program's own imports always followed that
//! rule; every other import position — a `self.` submodule, a module's imports,
//! a dependency submodule's imports — routed a DOTTED import through
//! `load_module(path[0])`, i.e. loaded the whole package.
//!
//! The consequence was not merely extra work: a consumer that imported
//! `ceangal.view` from a `self.` submodule got the dependency's demo app and
//! internal modules linked in, and their calls — into modules the consumer never
//! imports, whose generics monomorphization had already dropped as unreachable —
//! failed IR verification. The build produced no wasm at all, while the same
//! import from the root module built fine.

use std::path::{Path, PathBuf};

/// A two-package tree: a dependency with a root module that pulls in a heavy
/// sibling, and a light sub-module a consumer can import on its own.
fn write_dependency(root: &Path) -> PathBuf {
    let dep = root.join("dep");
    std::fs::create_dir_all(dep.join("src")).unwrap();
    std::fs::write(
        dep.join("almide.toml"),
        "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    // The light sub-module: no imports of its own.
    std::fs::write(
        dep.join("src").join("view.almd"),
        "type View = { kind: String, text: String }\n\nfn text(s: String) -> View = View { kind: \"text\", text: s }\n",
    )
    .unwrap();
    // A sibling the ROOT pulls in, and that a consumer of `dep.view` must not get.
    std::fs::write(
        dep.join("src").join("demo.almd"),
        "fn banner() -> String = \"demo\"\n",
    )
    .unwrap();
    std::fs::write(
        dep.join("src").join("mod.almd"),
        "import self.demo as demo\n\nfn headline() -> String = demo.banner()\n",
    )
    .unwrap();
    dep
}

fn resolve(entry: &Path, dep: &Path) -> Vec<String> {
    let src = std::fs::read_to_string(entry).unwrap();
    let mut parser = almide::parser::Parser::new(almide::lexer::Lexer::tokenize(&src));
    let program = parser.parse().expect("entry parses");
    let pkg_id = almide::project::PkgId { name: "dep".into(), major: 1 };
    let resolved = almide::resolve::resolve_imports_with_deps(
        entry.to_str().unwrap(),
        &program,
        &[(pkg_id, dep.join("src"))],
    )
    .expect("resolution succeeds");
    resolved.modules.into_iter().map(|(n, _, _, _)| n).collect()
}

fn consumer(root: &Path, files: &[(&str, &str)]) -> PathBuf {
    let app = root.join("app");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(
        app.join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    for (name, body) in files {
        std::fs::write(app.join("src").join(name), body).unwrap();
    }
    app.join("src").join("mod.almd")
}

#[test]
fn importing_a_submodule_from_the_root_loads_only_that_submodule() {
    let td = tempfile::TempDir::new().unwrap();
    let dep = write_dependency(td.path());
    let entry = consumer(
        td.path(),
        &[("mod.almd", "import dep.view as v\n\nfn main() -> Unit = println(v.text(\"hi\").text)\n")],
    );
    let mods = resolve(&entry, &dep);
    assert!(mods.iter().any(|m| m == "dep.view"), "got {mods:?}");
    assert!(!mods.iter().any(|m| m == "dep"), "the package ROOT must not be loaded: {mods:?}");
    assert!(!mods.iter().any(|m| m == "dep.demo"), "an unreached sibling must not be loaded: {mods:?}");
}

#[test]
fn importing_a_submodule_from_a_self_submodule_loads_the_same_set() {
    // The shape that failed: the dependency import lives in `src/app.almd`,
    // not in `src/mod.almd`.
    let td = tempfile::TempDir::new().unwrap();
    let dep = write_dependency(td.path());
    let entry = consumer(
        td.path(),
        &[
            ("app.almd", "import dep.view as v\n\nfn label() -> String = v.text(\"hi\").text\n"),
            ("mod.almd", "import self.app as app\n\nfn main() -> Unit = println(app.label())\n"),
        ],
    );
    let mods = resolve(&entry, &dep);
    assert!(mods.iter().any(|m| m == "dep.view"), "got {mods:?}");
    assert!(
        !mods.iter().any(|m| m == "dep"),
        "the package ROOT must not be loaded from a self submodule either: {mods:?}"
    );
    assert!(
        !mods.iter().any(|m| m == "dep.demo"),
        "an unreached sibling must not be loaded from a self submodule either: {mods:?}"
    );
}

#[test]
fn importing_the_whole_package_still_loads_its_siblings() {
    // The other half of the rule: `import dep` DOES pull the package in.
    let td = tempfile::TempDir::new().unwrap();
    let dep = write_dependency(td.path());
    let entry = consumer(
        td.path(),
        &[("mod.almd", "import dep\n\nfn main() -> Unit = println(dep.headline())\n")],
    );
    let mods = resolve(&entry, &dep);
    assert!(mods.iter().any(|m| m == "dep"), "got {mods:?}");
    assert!(mods.iter().any(|m| m == "dep.demo"), "the root's sibling must come along: {mods:?}");
}
