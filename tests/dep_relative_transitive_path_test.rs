//! A dependency's own relative `path` dependency resolves against the
//! directory of the `almide.toml` that declares it, not against the process
//! working directory (#2844, the Cargo rule).
//!
//! Layout (the issue's repro, plus a diamond: b → d and c → d, both by `../d`):
//!
//!   deps/d/        [package] name = "d"
//!   deps/b/        d = { path = "../d" }      ← relative to deps/b/
//!   deps/c/        d = { path = "../d" }      ← relative to deps/c/
//!   app/           b = { path = "../deps/b" }, c = { path = "../deps/c" }
//!
//! Before the fix `../d` was looked up from `app/` and the build stopped with
//! "path dependency 'd' not found: ../d".

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

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

/// Returns the `app/` directory of a fresh scratch tree.
fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue2844-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(root.join("deps/d/almide.toml"), "[package]\nname = \"d\"\nversion = \"0.1.0\"\n");
    write(root.join("deps/d/src/mod.almd"), "fn shared() -> String = \"D\"\n");
    write(
        root.join("deps/b/almide.toml"),
        "[package]\nname = \"b\"\nversion = \"0.1.0\"\n\n[dependencies]\nd = { path = \"../d\" }\n",
    );
    write(root.join("deps/b/src/mod.almd"), "import d\n\nfn from_b() -> String = \"B+\" + d.shared()\n");
    write(
        root.join("deps/c/almide.toml"),
        "[package]\nname = \"c\"\nversion = \"0.1.0\"\n\n[dependencies]\nd = { path = \"../d\" }\n",
    );
    write(root.join("deps/c/src/mod.almd"), "import d\n\nfn from_c() -> String = \"C+\" + d.shared()\n");
    write(
        root.join("app/almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nb = { path = \"../deps/b\" }\nc = { path = \"../deps/c\" }\n",
    );
    write(
        root.join("app/src/main.almd"),
        "import b\nimport c\n\neffect fn main() -> Unit = {\n  println(b.from_b())\n  println(c.from_c())\n}\n",
    );
    root.join("app")
}

fn almide_in(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(almide_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to spawn almide");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), combined)
}

#[test]
fn transitive_relative_path_dep_resolves_against_its_own_manifest() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let app = scratch("run");
    let (ok, out) = almide_in(&app, &["run", "src/main.almd"]);
    assert!(ok && out.contains("B+D") && out.contains("C+D"), "native run:\n{out}");
    assert!(!out.contains("not found"), "native run:\n{out}");
}

#[test]
fn transitive_relative_path_dep_resolves_on_the_wasm_target() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let app = scratch("wasm");
    let (ok, out) = almide_in(&app, &["build", "src/main.almd", "--target", "wasm", "-o", "main.wasm"]);
    assert!(ok, "wasm build:\n{out}");
    assert!(!out.contains("not found"), "wasm build:\n{out}");
}

#[test]
fn dep_path_names_the_transitive_dependency_beside_its_declarer() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let app = scratch("dep-path");
    let (ok, out) = almide_in(&app, &["dep-path", "d"]);
    assert!(ok, "dep-path d:\n{out}");
    let printed = PathBuf::from(out.trim());
    assert!(printed.is_dir(), "dep-path d printed a non-directory: {out}");
    assert!(printed.ends_with("deps/d/src"), "dep-path d: {out}");
}
