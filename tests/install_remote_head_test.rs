//! `almide install` of a branch (no tag) builds the remote's CURRENT head
//! (#1956). Before, the clone cached under `~/.almide/cache/<name>/main` was
//! reused as-is on every later install, so pushing new commits and running
//! `almide install` again silently reinstalled the stale version; only
//! `almide clean` fixed it. A branch install now resolves the remote head
//! first and builds that commit (`go install pkg@latest` semantics); a tag
//! keeps its immutable cache entry.

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
        && Command::new("git").arg("--version").output().is_ok_and(|o| o.status.success())
        && Command::new("rustc").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.email=test@example.com", "-c", "user.name=test"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?} failed:\n{}", String::from_utf8_lossy(&out.stderr));
}

fn commit_version(repo: &Path, version: &str) {
    std::fs::create_dir_all(repo.join("src")).expect("mkdir");
    std::fs::write(repo.join("almide.toml"), "[package]\nname = \"hello\"\nversion = \"0.1.0\"\n").expect("write");
    std::fs::write(
        repo.join("src").join("main.almd"),
        format!("effect fn main() -> Unit = println(\"{version}\")\n"),
    )
    .expect("write");
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", version]);
}

/// `almide install file://<repo>` with the dependency cache confined to
/// `home`; answers the installed binary's stdout and the install's stderr.
fn install_and_run(repo: &Path, home: &Path, bin_dir: &Path) -> (String, String) {
    let out = Command::new(almide_bin())
        .args(["install", &format!("file://{}", repo.display()), "--bin-dir", bin_dir.to_str().unwrap()])
        .env("HOME", home)
        .output()
        .expect("spawn almide");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "install failed:\n{stderr}");
    let run = Command::new(bin_dir.join("hello")).output().expect("run installed binary");
    (String::from_utf8_lossy(&run.stdout).to_string(), stderr)
}

fn scratch() -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("almide-issue1956-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("repo");
    let home = root.join("home");
    let bin_dir = root.join("bin");
    std::fs::create_dir_all(&repo).expect("mkdir");
    std::fs::create_dir_all(&home).expect("mkdir");
    std::fs::create_dir_all(&bin_dir).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    (repo, home, bin_dir)
}

#[test]
fn reinstalling_a_branch_builds_the_new_remote_head() {
    if !tools_available() {
        eprintln!("skipping: almide, git, or rustc not available");
        return;
    }
    let (repo, home, bin_dir) = scratch();
    commit_version(&repo, "v1");
    let (first, stderr) = install_and_run(&repo, &home, &bin_dir);
    assert_eq!(first, "v1\n");
    // The dependency name is the URL's last segment (`repo`), the binary
    // name comes from the manifest (`hello`).
    assert!(stderr.contains("Resolved repo HEAD -> "), "the built commit is not reported:\n{stderr}");

    commit_version(&repo, "v2");
    let (second, _) = install_and_run(&repo, &home, &bin_dir);
    assert_eq!(second, "v2\n", "the second install reused the stale clone");
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
}
