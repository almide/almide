//! A record literal keeps the struct of the type it was checked against when a
//! dependency package and the depending package declare record types with the
//! same fields (#3473).
//!
//! On 0.66.0 the native leg emitted such a literal as the other package's
//! struct, in both directions: the dependency's `{ r: v, g: v }` came out as
//! the app's `Swatch`, and the app's literals as `almide_rt_dep_Color`. rustc
//! rejected the generated code with E0308 while `--target wasm` ran correctly.
//! Develop resolves both; this pins the cross-package case that the in-package
//! fixes (#3189, #3283) do not cover, on both targets.

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

fn scratch(tag: &str, dep_mod: &str, app_main: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3473-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    write(&root.join("dep").join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
    write(&root.join("dep").join("src").join("mod.almd"), dep_mod);
    write(
        &root.join("app").join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    write(&root.join("app").join("src").join("main.almd"), app_main);
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

#[test]
fn a_dependency_literal_beside_a_same_shaped_app_record() {
    if !tools_available() {
        return;
    }
    let dep = "type Color = { r: Float, g: Float }\n\npub fn gray(v: Float) -> Color = { r: v, g: v }\n";
    let app = "import dep\n\n\
               type Swatch = { r: Float, g: Float }\n\n\
               fn half(c: dep.Color) -> Swatch = { r: c.r / 2.0, g: c.g / 2.0 }\n\n\
               effect fn main() -> Unit = {\n  println(float.to_string(half(dep.gray(1.0)).r))\n}\n";
    assert_both_targets(&scratch("dep-literal", dep, app), "0.5\n");
}

#[test]
fn app_literals_of_both_same_shaped_records() {
    if !tools_available() {
        return;
    }
    let dep = "type Color = { r: Float, g: Float, b: Float, a: Float }\n\n\
               pub fn gray(v: Float) -> Color = { r: v, g: v, b: v, a: 1.0 }\n\n\
               pub fn lum(c: Color) -> Float = c.r + c.g + c.b\n";
    let app = "import dep\n\n\
               type Swatch = { r: Float, g: Float, b: Float, a: Float }\n\n\
               fn mk(v: Float) -> Swatch = { r: v, g: v, b: v, a: 0.5 }\n\n\
               fn back(s: Swatch) -> dep.Color = { r: s.r, g: s.g, b: s.b, a: s.a }\n\n\
               effect fn main() -> Unit = {\n\
               \x20 let s = mk(0.25)\n\
               \x20 println(float.to_string(s.a))\n\
               \x20 println(float.to_string(dep.lum(back(s))))\n\
               \x20 println(float.to_string(dep.gray(2.0).a))\n\
               }\n";
    assert_both_targets(&scratch("app-literals", dep, app), "0.5\n0.75\n1.0\n");
}
