//! `almide.lock` may only assert a (git, ref, commit) triple that was true
//! together — #2523 and #2522.
//!
//! The lock exists so a third party rebuilds the same bytes. Two defects made
//! it record a triple no build ever performed:
//!
//! * #2523 — the dependency cache was keyed `<name>/<ref>`, carrying no source
//!   URL, so the first repository fetched under a name answered for every
//!   other repository with that name and ref. The second project's lock then
//!   named ITS url beside the FIRST repository's commit, which no other
//!   machine can fetch.
//! * #2522 — the locked commit was looked up by name alone, so bumping a
//!   dependency's `tag` in the manifest reused the old tag's commit and wrote
//!   the manifest's NEW tag beside it. The lock read as bumped; the build
//!   compiled the old tag.
//!
//! Both are checked the way a user meets them: real git repositories reached
//! over `file://`, a real `almide check`, and the lock read back off disk. The
//! consumer calls a function that exists only in the source it asked for, so
//! a wrong resolution is a type error and not merely a different hash.

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

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir()
        .join(format!("almide-dep-source-identity-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    root
}

/// A package named `fizz_protocol` exporting exactly one function, `$marker`,
/// committed and tagged. Answers the commit the tag points at.
///
/// The marker is what makes a wrong resolution LOUD: a consumer that calls
/// `fizz_protocol.<marker>()` only type-checks against the revision that
/// declares it.
fn commit_marker(repo: &Path, marker: &str, tag: &str) -> String {
    std::fs::create_dir_all(repo.join("src")).expect("mkdir");
    std::fs::write(
        repo.join("almide.toml"),
        "[package]\nname = \"fizz_protocol\"\nversion = \"0.1.0\"\n",
    )
    .expect("write manifest");
    std::fs::write(
        repo.join("src").join("mod.almd"),
        format!("fn {marker}() -> String = \"{marker}\"\n"),
    )
    .expect("write module");
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", marker]);
    git(repo, &["tag", tag]);
    git(repo, &["rev-parse", tag])
}

fn init_repo(root: &Path, name: &str) -> PathBuf {
    let repo = root.join(name);
    std::fs::create_dir_all(&repo).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    repo
}

/// A consumer pinned to `git` at `tag`, calling `marker` on the dependency.
fn write_consumer(proj: &Path, git_url: &str, tag: &str, marker: &str) {
    std::fs::create_dir_all(proj.join("src")).expect("mkdir");
    std::fs::write(
        proj.join("almide.toml"),
        format!(
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\nfizz_protocol = {{ git = \"{git_url}\", tag = \"{tag}\" }}\n"
        ),
    )
    .expect("write manifest");
    std::fs::write(
        proj.join("src").join("main.almd"),
        format!(
            "import io\nimport fizz_protocol\n\n\
             effect fn main() -> Unit = {{\n  io.print(fizz_protocol.{marker}())\n}}\n"
        ),
    )
    .expect("write entry");
}

/// `almide check` with the dependency cache confined to `home`. Answers
/// (success, stderr).
fn check(proj: &Path, home: &Path) -> (bool, String) {
    let out = Command::new(almide_bin())
        .args(["check", "src/main.almd"])
        .current_dir(proj)
        .env("HOME", home)
        .output()
        .expect("spawn almide");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

/// The `fizz_protocol` entry of a project's lock, as (ref, commit).
fn locked(proj: &Path) -> (String, String) {
    let path = proj.join("almide.lock");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let entry = almide::project::parse_lock_file(&path)
        .unwrap_or_else(|e| panic!("parse lock:\n{text}\n{e}"))
        .into_iter()
        .find(|l| l.name == "fizz_protocol")
        .unwrap_or_else(|| panic!("no fizz_protocol entry in:\n{text}"));
    (entry.ref_name, entry.commit)
}

/// #2523: a package name is not a namespace. Two repositories publishing
/// `fizz_protocol v0.2.2` must each resolve to their OWN commit, however many
/// of them a single cache has seen.
#[test]
fn two_sources_sharing_a_name_and_tag_each_resolve_their_own_commit() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = scratch("two-sources");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("mkdir");

    let src_a = init_repo(&root, "srcA");
    let commit_a = commit_marker(&src_a, "from_a", "v0.2.2");
    let src_b = init_repo(&root, "srcB");
    let commit_b = commit_marker(&src_b, "from_b", "v0.2.2");
    assert_ne!(commit_a, commit_b, "the two sources must differ for this to test anything");

    // A first — it is A's checkout that used to answer for B.
    let proj_a = root.join("projA");
    write_consumer(&proj_a, &format!("file://{}", src_a.display()), "v0.2.2", "from_a");
    let (ok_a, stderr_a) = check(&proj_a, &home);
    assert!(ok_a, "projA should check against its own source:\n{stderr_a}");
    assert_eq!(locked(&proj_a), ("v0.2.2".into(), commit_a.clone()));

    let proj_b = root.join("projB");
    write_consumer(&proj_b, &format!("file://{}", src_b.display()), "v0.2.2", "from_b");
    let (ok_b, stderr_b) = check(&proj_b, &home);
    assert!(
        ok_b,
        "projB got a different source's checkout — `from_b` is undefined in srcA:\n{stderr_b}"
    );
    let (ref_b, got_b) = locked(&proj_b);
    assert_eq!(ref_b, "v0.2.2");
    assert_eq!(
        got_b, commit_b,
        "projB's lock names srcB but records {got_b}; srcA's commit is {commit_a}. \
         That triple is unresolvable anywhere but this machine's cache (#2523)"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// #2522: bumping the manifest's tag must move the COMMIT, not just the label.
#[test]
fn bumping_a_tag_records_the_new_tags_commit() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = scratch("tag-bump");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("mkdir");

    let src = init_repo(&root, "src");
    let old_commit = commit_marker(&src, "in_v020", "v0.2.0");
    let new_commit = commit_marker(&src, "in_v022", "v0.2.2");
    assert_ne!(old_commit, new_commit);

    let proj = root.join("proj");
    write_consumer(&proj, &format!("file://{}", src.display()), "v0.2.0", "in_v020");
    let (ok, stderr) = check(&proj, &home);
    assert!(ok, "the v0.2.0 pin should check:\n{stderr}");
    assert_eq!(locked(&proj), ("v0.2.0".into(), old_commit.clone()));

    // Bump the tag, and call a function only v0.2.2 declares.
    write_consumer(&proj, &format!("file://{}", src.display()), "v0.2.2", "in_v022");
    let (ok, stderr) = check(&proj, &home);
    assert!(
        ok,
        "the bumped manifest still built v0.2.0 — `in_v022` is undefined there:\n{stderr}"
    );
    let (ref_name, commit) = locked(&proj);
    assert_eq!(ref_name, "v0.2.2");
    assert_eq!(
        commit, new_commit,
        "the lock says ref v0.2.2 beside {commit}, which is v0.2.0's commit {old_commit} — \
         the entry was relabelled, not re-resolved (#2522)"
    );
    let _ = std::fs::remove_dir_all(&root);
}
