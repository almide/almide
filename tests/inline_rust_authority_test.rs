//! #4100: an `@inline_rust` template is foreign code. A dependency's template
//! is spliced into the native build as Rust text, so:
//!
//! - it is native code under `[permissions] native` (#4093): an ungranted
//!   dependency that carries one is refused;
//! - outside the stdlib it counts as every effect category, like `@extern`
//!   (epoch 8), so `[permissions].allow` and `check --effects` see it;
//! - E085 decides "this is the stdlib" by identity with the compiler's
//!   embedded sources, not by a `stdlib/` directory name.
//!
//! The dependency below writes a file from a plain `fn` through its template.
//! Before #4100 it built under `allow = ["Time"]` and the binary wrote the
//! file.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4100-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(root.join("tpl/almide.toml"), "[package]\nname = \"tpl\"\nversion = \"0.1.0\"\n");
    write(
        root.join("tpl/src/mod.almd"),
        "@inline_rust(\"{ let _ = std::fs::write({p}.as_str(), \\\"pwned\\\"); 1i64 }\")\nfn touch(p: String) -> Int = 1\n",
    );
    root
}

fn app(root: &Path, permissions: &str) -> PathBuf {
    let dir = root.join("app");
    write(
        dir.join("almide.toml"),
        &format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ntpl = {{ path = \"../tpl\" }}\n{permissions}"),
    );
    let marker = root.join("marker.txt");
    write(
        dir.join("src/main.almd"),
        &format!("import tpl\n\nfn main() -> Unit = println(int.to_string(tpl.touch(\"{}\")))\n", marker.display()),
    );
    dir
}

fn almide(dir: &Path, root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .env("ALMIDE_RUN_PROJECT_DIR", root.join(".run"))
        .env_remove("RUSTFLAGS")
        .args(args)
        .output()
        .expect("spawn almide")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn an_ungranted_dependency_template_is_native_code() {
    let root = scratch("ungranted");
    let dir = app(&root, "");
    let o = almide(&dir, &root, &["run", "src/main.almd"]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("error: dependency `tpl` brings native code"), "{err}");
    assert!(err.contains("@inline_rust fn `touch` (src/mod.almd:2)"), "{err}");
    assert!(!root.join("marker.txt").exists(), "the template ran");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_granted_template_is_every_capability() {
    let root = scratch("allow");
    let dir = app(&root, "\n[permissions]\nnative = [\"tpl\"]\nallow = [\"Time\"]\n");
    let o = almide(&dir, &root, &["run", "src/main.almd"]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "a file-writing template passed allow = [\"Time\"]:\n{err}");
    assert!(err.contains("capability violation") && err.contains("FS.write") && err.contains("not in [permissions].allow"), "{err}");
    assert!(!root.join("marker.txt").exists(), "the template ran past [permissions]");
    // Granted every category, it builds and does what it says.
    let dir = app(&root, "\n[permissions]\nnative = [\"tpl\"]\nallow = [\"FS\", \"IO\", \"Net\", \"Env\", \"Proc\", \"Time\", \"Rand\", \"Fan\", \"Abort\"]\n");
    let o = almide(&dir, &root, &["run", "src/main.almd"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(std::fs::read_to_string(root.join("marker.txt")).ok().as_deref(), Some("pwned"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_stdlib_directory_layout_is_not_stdlib_authority() {
    let root = scratch("layout");
    // Named like an embedded stdlib source, in a directory named `stdlib`,
    // with its own text: not the stdlib.
    let file = root.join("pkg/stdlib/string.almd");
    write(file.clone(), "@intrinsic(\"almide_rt_fs_write_text\")\nfn w(p: String, s: String) -> Unit\n");
    let o = almide(&root.join("pkg"), &root, &["check", "stdlib/string.almd"]);
    let err = stderr(&o);
    assert_ne!(o.status.code(), Some(0), "a copied stdlib layout was granted stdlib authority:\n{err}");
    assert!(err.contains("E085") && err.contains("@intrinsic on fn 'w' outside the stdlib"), "{err}");
    let _ = std::fs::remove_dir_all(&root);
}
