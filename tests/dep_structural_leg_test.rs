//! The pkg_deps route flip: an external-dependency project lowers on the
//! STRUCTURAL wasm leg (the `has_pkg_deps → incumbent` routing exception is
//! retired — `wasm_leg::lower_to_ir_with_deps` resolves through the same
//! dep table as the incumbent driver, and dependency modules lower under
//! their versioned name). The gate asserts three things on a layered
//! two-package project: the structural leg emits the module under FORCED
//! structural routing (a wall would hard-error), the production routing
//! picks the structural leg (named by ALMIDE_VERIFIED_DEBUG), and the wasm
//! output is byte-identical to native.

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

/// The dep_fn_collision_test layering: an app over two path deps, one with
/// a submodule (`import self.path`) — the shape that exercises pkg_id
/// self-module naming AND the versioned-name registration in one project.
fn scratch() -> std::path::PathBuf {
    let root = std::env::temp_dir().join("almide-dep-structural");
    let _ = std::fs::remove_dir_all(&root);
    write(
        &root.join("shapes").join("almide.toml"),
        "[package]\nname = \"shapes\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root.join("shapes").join("src").join("path.almd"),
        "fn close() -> String = \"Z\"\n",
    );
    write(
        &root.join("shapes").join("src").join("mod.almd"),
        "import self.path\n\npub fn render() -> String = \"<\" + path.close() + \">\"\n\npub effect fn emit() -> String = path.close()\n",
    );
    write(
        &root.join("sqlib").join("almide.toml"),
        "[package]\nname = \"sqlib\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root.join("sqlib").join("src").join("mod.almd"),
        "pub fn tag() -> String = \"[sq]\"\n",
    );
    write(
        &root.join("app").join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nshapes = { path = \"../shapes\" }\nsqlib = { path = \"../sqlib\" }\n",
    );
    write(
        &root.join("app").join("src").join("main.almd"),
        "import shapes\nimport sqlib\n\neffect fn main() -> Unit = {\n  println(shapes.render())\n  println(shapes.emit()!)\n  println(sqlib.tag())\n}\n",
    );
    root.join("app")
}

#[test]
fn dep_project_runs_on_the_structural_leg() {
    if !tools_available() {
        return;
    }
    let app = scratch();

    let native = Command::new(almide_bin())
        .args(["run", "src/main.almd"])
        .current_dir(&app)
        .output()
        .expect("spawn almide");
    assert!(
        native.status.success(),
        "native run failed:\n{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let native_out = String::from_utf8_lossy(&native.stdout).to_string();
    assert_eq!(native_out, "<Z>\nZ\n[sq]\n", "native output: {native_out}");

    // Forced-structural: a wall is a hard error here, so success proves the
    // structural leg itself lowered the dep project (no silent reroute).
    let forced = Command::new(almide_bin())
        .args(["run", "src/main.almd", "--target", "wasm"])
        .env("ALMIDE_WASM_STRUCTURAL", "1")
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .current_dir(&app)
        .output()
        .expect("spawn almide");
    let forced_err = String::from_utf8_lossy(&forced.stderr);
    assert!(
        forced.status.success(),
        "structural leg walled on the dep project (the pkg_deps flip regressed):\n{forced_err}"
    );
    assert_eq!(
        String::from_utf8_lossy(&forced.stdout),
        native_out,
        "wasm/native divergence on the forced-structural leg"
    );

    // Production routing: no exception may send the dep project to the
    // incumbent — the debug env names the winning leg.
    let routed = Command::new(almide_bin())
        .args(["run", "src/main.almd", "--target", "wasm"])
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .current_dir(&app)
        .output()
        .expect("spawn almide");
    let routed_err = String::from_utf8_lossy(&routed.stderr);
    assert!(
        routed.status.success(),
        "routed wasm run failed:\n{routed_err}"
    );
    assert!(
        routed_err.contains("structural leg emitted the module"),
        "production routing did not pick the structural leg for a dep project:\n{routed_err}"
    );
    assert_eq!(
        String::from_utf8_lossy(&routed.stdout),
        native_out,
        "wasm/native divergence under production routing"
    );
}
