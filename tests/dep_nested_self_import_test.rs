//! `import self.<X>` in a DEPENDENCY module that sits in a subdirectory.
//!
//! Importing a package's root (`import deplib`) loads every module under its
//! `src/` as a sub-namespace, recursing into subdirectories: `src/native/`
//! becomes `deplib.native`. The recursion passed that NAMESPACE on as the
//! package `self` resolves against, so a module in `src/native/` importing
//! `self.core` looked for a package called `deplib.native` — "package
//! 'deplib.native' not found in dependencies", and the consumer did not
//! build. `self` is the package, whichever directory the module sits in.
//!
//! The same module imported on its own (`import deplib.native.leaf`, without
//! the root) resolved correctly; only the root's recursive scan was wrong.

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

/// An app depending on `deplib` by path, whose `src/` holds `modules` (paths
/// relative to it, subdirectories included).
fn scratch(name: &str, modules: &[(&str, &str)], main: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-nested-self-{}", name));
    let _ = std::fs::remove_dir_all(&root);
    let dep = root.join("dep");
    let app = root.join("app");
    std::fs::create_dir_all(&app).expect("mkdir app");
    std::fs::create_dir_all(dep.join("src")).expect("mkdir dep");
    std::fs::write(dep.join("almide.toml"), "[package]\nname = \"deplib\"\nversion = \"0.1.0\"\n")
        .expect("write dep toml");
    for (file, body) in modules {
        let path = dep.join("src").join(file);
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir module dir");
        std::fs::write(path, body).expect("write dep module");
    }
    std::fs::write(
        app.join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndeplib = { path = \"../dep\" }\n",
    )
    .expect("write app toml");
    std::fs::write(app.join("main.almd"), main).expect("write main");
    app
}

fn run_in(dir: &Path) -> String {
    let output = Command::new(almide_bin())
        .args(["run", "main.almd"])
        .current_dir(dir)
        .output()
        .expect("failed to spawn almide");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

const MODULES: &[(&str, &str)] = &[
    ("mod.almd", "fn name() -> String = \"deplib\"\n"),
    ("core.almd", "fn value() -> Int = 41\n"),
    ("native/leaf.almd", "import self.core\nfn value() -> Int = core.value() + 1\n"),
    ("native/deep/twig.almd", "import self.core\nfn value() -> Int = core.value() + 2\n"),
];

#[test]
fn subdirectory_module_self_import_resolves_under_root_import() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "root",
        MODULES,
        concat!(
            "import deplib\n",
            "import deplib.native.leaf as leaf\n",
            "effect fn main() -> Unit = println(\"${deplib.name()} ${int.to_string(leaf.value())}\")\n",
        ),
    );
    let out = run_in(&dir);
    assert!(out.contains("deplib 42"), "self import in src/native/ did not resolve:\n{out}");
}

#[test]
fn nested_subdirectory_module_self_import_resolves_under_root_import() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "deep",
        MODULES,
        concat!(
            "import deplib\n",
            "import deplib.native.deep.twig as twig\n",
            "effect fn main() -> Unit = println(\"${deplib.name()} ${int.to_string(twig.value())}\")\n",
        ),
    );
    let out = run_in(&dir);
    assert!(out.contains("deplib 43"), "self import in src/native/deep/ did not resolve:\n{out}");
}

/// The path that always worked, pinned so the fix cannot trade one for the
/// other.
#[test]
fn subdirectory_module_self_import_resolves_without_root_import() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "leaf-only",
        MODULES,
        "import deplib.native.leaf as leaf\neffect fn main() -> Unit = println(int.to_string(leaf.value()))\n",
    );
    let out = run_in(&dir);
    assert!(out.contains("42"), "leaf-only import regressed:\n{out}");
}
