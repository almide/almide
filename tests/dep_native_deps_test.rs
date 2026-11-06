//! A dependency package's `[native-deps]` reach the generated `Cargo.toml` of
//! the project that depends on it (#4521).
//!
//! The package `nd` declares a native dep (a local path crate, so no network)
//! and its `native/ndshim.rs` calls into that crate. The root app depends on
//! `nd` and declares no `[native-deps]` of its own, so the crate can only come
//! from `nd`'s manifest. v0.69.0 wrote the dependency crates into the manifest
//! and then overwrote the file with the root's text, so rustc failed with
//! E0433 on the crate path. Every native route is driven: `run`, `build`,
//! `test` and `build --cdylib`; and the manifest `run` leaves in its project
//! dir is read back as TOML.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PROBE: &str = "almide_issue4521_probe";

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&path, body).expect("write");
}

/// `root/probe` (the crate), `root/nd` (the package), `root/app` (the root).
fn fixture(tag: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("almide-issue4521-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(
        root.join("probe/Cargo.toml"),
        &format!("[package]\nname = \"{PROBE}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n"),
    );
    write(root.join("probe/src/lib.rs"), "pub fn value() -> i64 { 4521 }\n");
    let probe_path = root.join("probe").to_str().expect("utf-8 path").replace('\\', "/");
    write(
        root.join("nd/almide.toml"),
        &format!(
            "[package]\nname = \"nd\"\nversion = \"0.1.0\"\n\n[native-deps]\n{PROBE} = {{ path = \"{probe_path}\" }}\n"
        ),
    );
    write(root.join("nd/native/ndshim.rs"), &format!("pub fn number() -> i64 {{ {PROBE}::value() }}\n"));
    write(root.join("nd/src/mod.almd"), "@extern(rust, \"crate::ndshim\", \"number\")\nfn number() -> Int = _\n");
    let app = root.join("app");
    write(
        app.join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nnd = { path = \"../nd\" }\n\n\
         [permissions]\nnative = [\"nd\"]\n",
    );
    write(
        app.join("src/main.almd"),
        "import nd\n\neffect fn main() -> Unit = {\n  println(int.to_string(nd.number()))\n}\n\n\
         test \"value\" {\n  assert_eq(nd.number(), 4521)\n}\n",
    );
    (root, app)
}

fn almide(app: &Path, root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(app)
        .env("ALMIDE_RUN_PROJECT_DIR", root.join(".run"))
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args(args)
        .output()
        .expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn a_dependency_packages_native_deps_reach_every_native_route() {
    let (root, app) = fixture("routes");

    let o = almide(&app, &root, &["run", "src/main.almd"]);
    assert!(o.status.success(), "run failed (the dependency's native dep was dropped?):\n{}", text(&o));
    assert_eq!(String::from_utf8_lossy(&o.stdout), "4521\n");

    // What Cargo saw: the manifest the run left in its project dir, read as TOML.
    let manifest = std::fs::read_to_string(root.join(".run/Cargo.toml")).expect("run left no Cargo.toml");
    let doc: toml::Table = toml::from_str(&manifest).unwrap_or_else(|e| panic!("not TOML ({e}):\n{manifest}"));
    let deps = doc.get("dependencies").and_then(toml::Value::as_table);
    assert!(
        deps.is_some_and(|d| d.contains_key(PROBE)),
        "the generated Cargo.toml has no `{PROBE}` under [dependencies]:\n{manifest}"
    );

    let bin = root.join("out/app-bin");
    let o = almide(&app, &root, &["build", "src/main.almd", "-o", bin.to_str().unwrap()]);
    assert!(o.status.success() && bin.exists(), "build failed:\n{}", text(&o));
    let o = Command::new(&bin).output().expect("spawn built binary");
    assert_eq!(String::from_utf8_lossy(&o.stdout), "4521\n");

    let o = almide(&app, &root, &["test", "src/main.almd"]);
    assert!(o.status.success(), "test failed:\n{}", text(&o));

    let o = almide(&app, &root, &["build", "src/main.almd", "--cdylib"]);
    assert!(o.status.success(), "cdylib build failed:\n{}", text(&o));

    let _ = std::fs::remove_dir_all(&root);
}
