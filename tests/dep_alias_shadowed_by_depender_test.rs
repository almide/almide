//! A type alias declared in a dependency package keeps its own meaning when the
//! depending package declares a type of the same name (#3472).
//!
//! On 0.66.0 the dependency's `type Rgba = Color` resolved to the app's
//! `tint.Rgba`, so the dependency's own body failed to type-check with
//! `Solid() argument 1 expects dep.Color but got tint.Rgba`. Develop resolves
//! names where they are declared (dialect epoch 10, #3439), which fixes it; this
//! pins the cross-package, alias-shaped case that #3439's in-package fixture
//! does not cover. The app's same-named type is declared both in a submodule
//! (the reported shape) and in the entry file itself.

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

const DEP_MOD: &str = "type Color = { r: Float, g: Float }\n\
                       type Rgba = Color\n\
                       type Fill = Solid(Color) | Empty\n\
                       \n\
                       let black: Rgba = { r: 0.0, g: 0.0 }\n\
                       \n\
                       pub fn fill() -> Fill = Solid(black)\n\
                       \n\
                       pub fn tint(c: Rgba) -> Fill = Solid(c)\n";

const APP_TINT: &str = "type Rgba = { r: Float, g: Float }\n\npub fn zero() -> Rgba = { r: 0.0, g: 0.0 }\n";

const APP_BODY: &str = "effect fn main() -> Unit = {\n\
                        \x20 let f = dep.fill()\n\
                        \x20 println(match f { Solid(c) => float.to_string(c.r), Empty => \"none\" })\n\
                        \x20 println(match dep.tint({ r: 0.5, g: 0.25 }) { Solid(c) => float.to_string(c.g), Empty => \"none\" })\n\
                        \x20 println(float.to_string(tint.zero().g))\n\
                        }\n";

/// `local_rgba`: also declare `type Rgba` in the entry file itself.
fn scratch(tag: &str, local_rgba: bool) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3472-{tag}"));
    let _ = std::fs::remove_dir_all(&root);
    write(&root.join("dep").join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
    write(&root.join("dep").join("src").join("mod.almd"), DEP_MOD);
    write(
        &root.join("app").join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = { path = \"../dep\" }\n",
    );
    write(&root.join("app").join("src").join("tint.almd"), APP_TINT);
    let local = if local_rgba { "type Rgba = { r: Float, g: Float, b: Float }\n\n" } else { "" };
    write(
        &root.join("app").join("src").join("main.almd"),
        &format!("import dep\nimport self.tint\n\n{local}{APP_BODY}"),
    );
    root.join("app")
}

fn assert_both_targets(app: &Path) {
    for target in ["rust", "wasm"] {
        let run = Command::new(almide_bin())
            .args(["run", "src/main.almd", "--target", target])
            .current_dir(app)
            .output()
            .expect("spawn almide");
        let stdout = String::from_utf8_lossy(&run.stdout);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{target} run failed:\nstdout: {stdout}\nstderr: {stderr}");
        assert_eq!(stdout, "0.0\n0.25\n0.0\n", "wrong {target} output: {stdout}");
    }
}

#[test]
fn a_dependency_alias_beside_a_same_named_type_in_an_app_submodule() {
    if !tools_available() {
        return;
    }
    assert_both_targets(&scratch("submodule", false));
}

#[test]
fn a_dependency_alias_beside_a_same_named_type_in_the_app_entry_file() {
    if !tools_available() {
        return;
    }
    assert_both_targets(&scratch("entry", true));
}
