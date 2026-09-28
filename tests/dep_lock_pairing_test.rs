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

// ── #2532: the lock records what MVS selected, not what the manifest asked ──
//
// The root asks for `raise_foo@v1.0.0`; `raise_mid@v1.0.0` asks for
// `raise_foo@v1.1.0`. MVS builds v1.1.0. The lock used to record the root's
// REQUEST (`ref = "v1.0.0"` and v1.0.0's commit): a true triple, but not the
// one that was built, so reading the lock could not say what was compiled.
// Decision (2026-09-28): the lock records the resolved result — the selected
// version's ref and that version's commit, as Cargo.lock does. The request
// lives only in almide.toml.

/// `raise_foo` with one commit per version, each tagged `v<ver>`. Version
/// 1.1.0 and later export `since_1_1`, so a build that type-checks against it
/// proves the raised version is the one compiled. Answers each tag's commit.
fn commit_versioned_foo(root: &Path, versions: &[&str]) -> (PathBuf, Vec<String>) {
    let repo = root.join("raise_foo");
    std::fs::create_dir_all(repo.join("src")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    let mut commits = Vec::new();
    for ver in versions {
        std::fs::write(
            repo.join("almide.toml"),
            format!("[package]\nname = \"raise_foo\"\nversion = \"{ver}\"\n"),
        )
        .expect("write manifest");
        let extra = if *ver == "1.0.0" { "" } else { "fn since_1_1() -> String = \"raised\"\n" };
        std::fs::write(
            repo.join("src").join("mod.almd"),
            format!("fn foo_version() -> String = \"{ver}\"\n{extra}"),
        )
        .expect("write module");
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", ver]);
        git(&repo, &["tag", &format!("v{ver}")]);
        commits.push(git(&repo, &["rev-parse", "HEAD"]));
    }
    (repo, commits)
}

fn tagged(name: &str, repo: &Path, tag: &str) -> String {
    format!("{name} = {{ git = \"file://{}\", tag = \"{tag}\" }}", repo.display())
}

fn write_consumer(proj: &Path, foo_line: &str, mid_line: &str) {
    std::fs::write(
        proj.join("almide.toml"),
        format!("[package]\nname = \"consumer\"\nversion = \"0.1.0\"\n\n[dependencies]\n{foo_line}\n{mid_line}\n"),
    )
    .expect("write manifest");
}

fn entry<'a>(entries: &'a [almide::project::LockedDep], name: &str) -> &'a almide::project::LockedDep {
    entries
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no '{name}' entry in the lock: {entries:?}"))
}

#[test]
fn a_dependency_raised_by_mvs_is_locked_at_the_version_that_was_built() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = std::env::temp_dir().join(format!("almide-issue2532-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("mkdir");

    let (foo, commits) = commit_versioned_foo(&root, &["1.0.0", "1.1.0", "1.2.0"]);
    let (c10, c11, c12) = (commits[0].clone(), commits[1].clone(), commits[2].clone());
    let (mid, mid_commit) =
        commit_package(&root, "raise_mid", &[tagged("raise_foo", &foo, "v1.1.0")]);

    let proj = root.join("proj");
    std::fs::create_dir_all(proj.join("src")).expect("mkdir");
    // `since_1_1` exists only from 1.1.0: the check passing is itself the
    // proof that the raised version is what gets compiled.
    std::fs::write(
        proj.join("src").join("main.almd"),
        "import io\nimport raise_foo\nimport raise_mid\n\n\
         effect fn main() -> Unit = {\n  io.print(raise_foo.since_1_1() + raise_mid.raise_mid_fn())\n}\n",
    )
    .expect("write entry");
    write_consumer(&proj, &tagged("raise_foo", &foo, "v1.0.0"), &dep_line("raise_mid", &mid));

    // 1. The raise: the lock names v1.1.0 and v1.1.0's commit.
    let (ok, stderr) = check(&proj, &home);
    assert!(ok, "the first check should succeed:\n{stderr}");
    let entries = lock_entries(&proj);
    let foo_entry = entry(&entries, "raise_foo");
    assert_eq!(
        (foo_entry.ref_name.as_str(), foo_entry.commit.as_str()),
        ("v1.1.0", c11.as_str()),
        "the lock must record the version MVS built (v1.1.0 = {c11}), not the root's \
         request (v1.0.0 = {c10}) — #2532"
    );
    assert_eq!(foo_entry.git, format!("file://{}", foo.display()));
    // An unraised dependency is locked at its own request, as before.
    let mid_entry = entry(&entries, "raise_mid");
    assert_eq!((mid_entry.ref_name.as_str(), mid_entry.commit.as_str()), ("v1.0.0", mid_commit.as_str()));
    assert_eq!(entries.len(), 2, "the lock carries the direct deps only: {entries:?}");
    let first = std::fs::read(proj.join("almide.lock")).expect("read lock");

    // 2. A lock whose ref is ABOVE the manifest's request is not drift: a warm
    //    re-run accepts it and leaves it byte-identical…
    let (ok, stderr) = check(&proj, &home);
    assert!(ok, "a warm re-run must accept the resolved lock:\n{stderr}");
    assert_eq!(std::fs::read(proj.join("almide.lock")).expect("read lock"), first, "warm re-run changed the lock");
    //    …and so does a cold cache, which must fetch the locked commit.
    let cold = root.join("home-cold");
    std::fs::create_dir_all(&cold).expect("mkdir");
    let (ok, stderr) = check(&proj, &cold);
    assert!(ok, "a cold-cache re-run must resolve the lock it was given:\n{stderr}");
    assert_eq!(std::fs::read(proj.join("almide.lock")).expect("read lock"), first, "cold re-run changed the lock");

    // 3. A lock written before #2532 recorded the request. It still reads
    //    (the entry simply does not pin the fetch that is built) and the next
    //    run rewrites it to the resolved record.
    std::fs::write(
        proj.join("almide.lock"),
        format!(
            "# almide.lock — auto-generated, do not edit\n\n\
             raise_foo = {{ git = \"file://{}\", ref = \"v1.0.0\", commit = \"{c10}\" }}\n\
             raise_mid = {{ git = \"file://{}\", ref = \"v1.0.0\", commit = \"{mid_commit}\" }}\n",
            foo.display(),
            mid.display()
        ),
    )
    .expect("write old lock");
    let (ok, stderr) = check(&proj, &home);
    assert!(ok, "a pre-#2532 lock must still be accepted:\n{stderr}");
    assert_eq!(std::fs::read(proj.join("almide.lock")).expect("read lock"), first, "the old lock was not rewritten to the resolved record");

    // 4. Changing the manifest's request re-resolves. Above the transitive
    //    requirement there is no raise, and the lock names the request itself.
    write_consumer(&proj, &tagged("raise_foo", &foo, "v1.2.0"), &dep_line("raise_mid", &mid));
    let (ok, stderr) = check(&proj, &home);
    assert!(ok, "the check after raising the request should succeed:\n{stderr}");
    let entries = lock_entries(&proj);
    let foo_entry = entry(&entries, "raise_foo");
    assert_eq!((foo_entry.ref_name.as_str(), foo_entry.commit.as_str()), ("v1.2.0", c12.as_str()));
    //    And back down: the request drops below the transitive one, so MVS
    //    raises it again and the lock returns to the resolved v1.1.0 record.
    write_consumer(&proj, &tagged("raise_foo", &foo, "v1.0.0"), &dep_line("raise_mid", &mid));
    let (ok, stderr) = check(&proj, &home);
    assert!(ok, "the check after lowering the request should succeed:\n{stderr}");
    assert_eq!(std::fs::read(proj.join("almide.lock")).expect("read lock"), first);

    let _ = std::fs::remove_dir_all(&root);
}
