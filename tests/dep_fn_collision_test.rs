//! A bare fn name declared in BOTH a dependency (as an EFFECT fn) and an
//! unrelated module (as a PURE fn) — the #1597 poisoning class, the fn
//! twin of dep_type_collision_test's #1501.
//!
//! ResultPropagation used one bare-name map for everything: the dependency's
//! lifted `close` selected the OTHER package's pure `path.close` for the
//! body wrap (pure signature + Ok-wrapped body, rustc E0308), and — the
//! inverse — a tail call to the pure `close` from a lifted effect fn was
//! exempted from its Ok wrap as "already returns Result". Identity is now
//! structural: Phase 2 transforms exactly the fns Phase 1 lifted (by
//! position, no names), and the tail-call exemption looks up the callee
//! module-QUALIFIED, so a bare name can only ever mean a root fn.

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

fn scratch() -> std::path::PathBuf {
    let root = std::env::temp_dir().join("almide-issue1597");
    let _ = std::fs::remove_dir_all(&root);
    // The svg stand-in: a package whose SUBMODULE has a PURE `close`.
    write(
        &root.join("shapes").join("almide.toml"),
        "[package]\nname = \"shapes\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root.join("shapes").join("src").join("path.almd"),
        "fn close() -> String = \"Z\"\n",
    );
    // `render` (pure) reads the pure close; `emit` (effect) TAIL-CALLS the
    // pure close — the inverse cell: its Ok wrap must NOT be exempted by
    // the dependency's same-named lifted fn.
    write(
        &root.join("shapes").join("src").join("mod.almd"),
        "import self.path\n\npub fn render() -> String = \"<\" + path.close() + \">\"\n\npub effect fn emit() -> String = path.close()\n",
    );
    // The sqlite stand-in: an effect fn with the SAME bare name.
    write(
        &root.join("sqlib").join("almide.toml"),
        "[package]\nname = \"sqlib\"\nversion = \"0.1.0\"\n",
    );
    write(
        &root.join("sqlib").join("src").join("mod.almd"),
        "pub effect fn close(db: Int) -> Unit =\n  if db < 0 then err(\"bad\") else ok(())\n",
    );
    write(
        &root.join("app").join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nshapes = { path = \"../shapes\" }\nsqlib = { path = \"../sqlib\" }\n",
    );
    // sqlib is imported but never called — the PRESENCE of its `close`
    // was the trigger.
    write(
        &root.join("app").join("src").join("main.almd"),
        "import shapes\nimport sqlib as db\n\neffect fn main() -> Unit = {\n  println(shapes.render())\n  println(shapes.emit()!)\n}\n",
    );
    root.join("app")
}

#[test]
fn colliding_bare_fn_names_do_not_cross_wrap_across_packages() {
    if !tools_available() {
        return;
    }
    let app = scratch();
    let out = Command::new(almide_bin())
        .args(["run", "src/main.almd"])
        .current_dir(&app)
        .output()
        .expect("spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("E0308"),
        "the bare-name effect map cross-wrapped an unrelated fn (#1597):\n{stderr}"
    );
    assert!(out.status.success(), "run failed:\nstdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stdout, "<Z>\nZ\n", "wrong output: {stdout}");
}
