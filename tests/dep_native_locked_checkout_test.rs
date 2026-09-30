//! A git dependency's `native/` modules come from the SAME checkout as its
//! `.almd` sources: the commit `almide.lock` pins (#3094).
//!
//! Module resolution fetched the locked commit, while native injection
//! re-fetched the manifest entry without the lock and got the branch head.
//! On a fresh machine (or after `almide clean`) a locked dependency whose
//! branch had moved built the head's Rust modules against the locked
//! commit's `@extern` declarations, with exit 0.
//!
//! The dependency is a local git repository reached via `file://`, and `HOME`
//! is redirected so the dependency cache is this test's own.

use std::path::{Path, PathBuf};
use std::process::Command;

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
}

fn native_rs(n: i64) -> String {
    format!("pub fn number() -> i64 {{ {n} }}\n")
}

fn real_home_dir(var: &str, below_home: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(below_home)
    })
}

/// `almide run src/main.almd` in `app`, with the dependency cache under
/// `home` and cargo/rustup still found where they really are.
fn run(root: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(root.join("app"))
        .env("CARGO_HOME", real_home_dir("CARGO_HOME", ".cargo"))
        .env("RUSTUP_HOME", real_home_dir("RUSTUP_HOME", ".rustup"))
        .env("HOME", root.join("home"))
        .env("ALMIDE_RUN_PROJECT_DIR", root.join(".run"))
        .args(["run", "src/main.almd"])
        .output()
        .expect("spawn almide");
    assert!(out.status.success(), "almide run failed\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_locked_git_dependency_builds_its_natives_from_the_locked_commit() {
    let root = std::env::temp_dir().join(format!("almide-issue3094-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("natrepo");
    write(repo.join("almide.toml"), "[package]\nname = \"nat\"\nversion = \"0.1.0\"\n");
    write(repo.join("src/mod.almd"), "@extern(rust, \"crate::natval\", \"number\")\nfn number() -> Int = _\n");
    write(repo.join("native/natval.rs"), &native_rs(1));
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);

    let url = format!("file://{}", repo.display());
    write(
        root.join("app/almide.toml"),
        &format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nnat = {{ git = \"{url}\" }}\n"),
    );
    write(
        root.join("app/src/main.almd"),
        "import nat\n\neffect fn main() -> Unit = println(int.to_string(nat.number()))\n",
    );

    // First build: resolves `main` to commit one and writes the lock.
    assert_eq!(run(&root), "1\n", "baseline");
    assert!(root.join("app/almide.lock").exists(), "the first build writes almide.lock");

    // The branch moves on; only the native module changes.
    write(repo.join("native/natval.rs"), &native_rs(2));
    git(&repo, &["commit", "-q", "-am", "two"]);

    // A fresh machine: no dependency cache, no build cache, the lock kept.
    let _ = std::fs::remove_dir_all(root.join("home"));
    let _ = std::fs::remove_dir_all(root.join(".run"));
    assert_eq!(run(&root), "1\n", "the natives came from the branch head, not the locked commit");

    let _ = std::fs::remove_dir_all(&root);
}
