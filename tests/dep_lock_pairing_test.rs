//! Every `almide.lock` entry records the commit of the dependency it names
//! — #2529.
//!
//! `update_lock_file` paired the manifest's DIRECT dependency list to the
//! walk's result by position. The walk is the flattened graph and pushes a
//! dependency before recursing into its own, so the two only lined up while
//! nothing had transitive dependencies. As soon as one did, every later direct
//! dependency was shifted by one and had another package's commit written
//! under its own url.
//!
//! The lock then named a real repository beside a commit that repository does
//! not contain, so the very next build failed for everyone — the same false
//! `(git, ref, commit)` triple as #2523 and #2522, but produced where the
//! entry is WRITTEN rather than where it is looked up. #2522 tightened the
//! lookup to `(name, git, ref)`, and all three match here: only the value is
//! another package's. A lookup key cannot catch a wrong value.

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
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.email=test@example.com", "-c", "user.name=test"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?} failed:\n{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn dep_line(name: &str, repo: &Path) -> String {
    format!("{name} = {{ git = \"file://{}\", tag = \"v1.0.0\" }}", repo.display())
}

/// A package exporting one function, optionally depending on other packages,
/// committed and tagged `v1.0.0`. Answers the tagged commit.
fn commit_package(root: &Path, pkg: &str, deps: &[String]) -> (PathBuf, String) {
    let repo = root.join(pkg);
    std::fs::create_dir_all(repo.join("src")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    let mut manifest = format!("[package]\nname = \"{pkg}\"\nversion = \"0.1.0\"\n");
    if !deps.is_empty() {
        manifest.push_str("\n[dependencies]\n");
        for d in deps {
            manifest.push_str(d);
            manifest.push('\n');
        }
    }
    std::fs::write(repo.join("almide.toml"), manifest).expect("write manifest");
    std::fs::write(
        repo.join("src").join("mod.almd"),
        format!("fn {pkg}_fn() -> String = \"{pkg}\"\n"),
    )
    .expect("write module");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", pkg]);
    git(&repo, &["tag", "v1.0.0"]);
    let commit = git(&repo, &["rev-parse", "v1.0.0"]);
    (repo, commit)
}

fn lock_entries(proj: &Path) -> Vec<almide::project::LockedDep> {
    let path = proj.join("almide.lock");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    almide::project::parse_lock_file(&path)
        .unwrap_or_else(|e| panic!("parse lock:\n{text}\n{e}"))
}

fn commit_of(entries: &[almide::project::LockedDep], name: &str) -> String {
    entries
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no '{name}' entry in the lock"))
        .commit
        .clone()
}

fn check(proj: &Path, home: &Path) -> (bool, String) {
    let out = Command::new(almide_bin())
        .args(["check", "src/main.almd"])
        .current_dir(proj)
        .env("HOME", home)
        .output()
        .expect("spawn almide");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

/// Two direct dependencies where the FIRST has a transitive dependency of its
/// own — the shape that shifts the positional pairing by one.
#[test]
fn a_direct_dep_after_one_with_transitive_deps_records_its_own_commit() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = std::env::temp_dir().join(format!("almide-issue2529-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("mkdir");

    let (lib, lib_commit) = commit_package(&root, "fizz_lib", &[]);
    let (mid, mid_commit) = commit_package(&root, "fizz_mid", &[dep_line("fizz_lib", &lib)]);
    let (other, other_commit) = commit_package(&root, "fizz_other", &[]);
    // The walk is [fizz_mid, fizz_lib, fizz_other]; the manifest list is
    // [fizz_mid, fizz_other]. Position puts fizz_lib's commit under fizz_other.
    assert_ne!(lib_commit, other_commit);

    let proj = root.join("proj");
    std::fs::create_dir_all(proj.join("src")).expect("mkdir");
    std::fs::write(
        proj.join("almide.toml"),
        format!(
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\n\n[dependencies]\n{}\n{}\n",
            dep_line("fizz_mid", &mid),
            dep_line("fizz_other", &other),
        ),
    )
    .expect("write manifest");
    std::fs::write(
        proj.join("src").join("main.almd"),
        "import io\nimport fizz_mid\nimport fizz_other\n\n\
         effect fn main() -> Unit = {\n  io.print(fizz_mid.fizz_mid_fn() + fizz_other.fizz_other_fn())\n}\n",
    )
    .expect("write entry");

    let (ok, stderr) = check(&proj, &home);
    assert!(ok, "the first check should succeed:\n{stderr}");

    let entries = lock_entries(&proj);
    assert_eq!(commit_of(&entries, "fizz_mid"), mid_commit);
    let got_other = commit_of(&entries, "fizz_other");
    assert_eq!(
        got_other, other_commit,
        "fizz_other's lock entry names its own url beside {got_other}, which is \
         fizz_lib's commit ({lib_commit}) — the transitive dependency shifted the \
         positional pairing (#2529)"
    );
    // A transitive dependency is not a direct one: the lock carries the two
    // the manifest names, not the package fizz_mid pulled in.
    assert_eq!(entries.len(), 2, "lock should carry exactly the direct deps: {entries:?}");

    // The consequence, not just the record: a lock naming a commit its
    // repository does not contain cannot be fetched by anyone, including the
    // machine that wrote it.
    let (ok, stderr) = check(&proj, &home);
    assert!(
        ok,
        "the second check could not resolve the lock this build just wrote:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
