//! Resolving git dependencies against their remotes (#4527, #4528, #4529).
//!
//! - #4527: with no lock pin, a floating ref (a branch, or no ref at all)
//!   resolves to the remote's CURRENT head, not to whatever checkout the
//!   cache holds from its first fetch. With a lock, the locked commit is
//!   used and the remote is not needed.
//! - #4528: a ref the remote does not have is an error naming the
//!   dependency, the ref, the chain that requires it and the tags that do
//!   exist; `almide add --dry-run` checks the tag too.
//! - #4529: concurrent `almide` processes fetching one dependency into one
//!   cache all succeed, and one of them clones.
//!
//! No network: every remote is a local repository reached through
//! `file://`, and `HOME` is a scratch directory, so the dependency cache is
//! per test.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Fixture {
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("almide-dep-resolve-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        Fixture { root }
    }

    /// A repository `remotes/<pkg>` holding package `pkg` on `main`, with
    /// `extra` appended to its manifest; tagged with each of `tags`.
    /// Answers its `file://` url.
    fn repo(&self, pkg: &str, extra: &str, tags: &[&str]) -> String {
        let work = self.root.join("remotes").join(pkg);
        write(work.join("almide.toml"), &format!("[package]\nname = \"{pkg}\"\nversion = \"0.1.0\"\n{extra}"));
        write(work.join("src/mod.almd"), "fn value() -> Int = 1\n");
        git(&work, &["init", "-q"]);
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "-qm", "one"]);
        for t in tags {
            git(&work, &["tag", t]);
        }
        format!("file://{}", work.display())
    }

    /// A new commit on `pkg`'s `main`; answers it.
    fn advance(&self, pkg: &str, n: i64) -> String {
        let work = self.root.join("remotes").join(pkg);
        write(work.join("src/mod.almd"), &format!("fn value() -> Int = {n}\n"));
        git(&work, &["commit", "-qam", "next"]);
        git(&work, &["rev-parse", "HEAD"])
    }

    fn head(&self, pkg: &str) -> String {
        git(&self.root.join("remotes").join(pkg), &["rev-parse", "HEAD"])
    }

    fn project(&self, name: &str, deps: &str) -> PathBuf {
        let dir = self.root.join(name);
        write(
            dir.join("almide.toml"),
            &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}"),
        );
        write(dir.join("src/main.almd"), "effect fn main() -> Unit = println(\"ok\")\n");
        dir
    }

    fn almide(&self, dir: &Path, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
        cmd.current_dir(dir)
            .env("HOME", self.root.join("home"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("ALMIDE_DEPS_CACHE_ONLY")
            .env("ALMIDE_RUN_PROJECT_DIR", self.root.join(".run"))
            .args(args);
        cmd
    }

    fn run(&self, dir: &Path, args: &[&str]) -> Output {
        self.almide(dir, args).output().expect("spawn almide")
    }
}

fn locked_commit(proj: &Path, name: &str) -> Option<String> {
    let path = proj.join("almide.lock");
    if !path.exists() {
        return None;
    }
    almide::project::parse_lock_file(&path)
        .expect("parse lock")
        .into_iter()
        .find(|l| l.name == name)
        .map(|l| l.commit)
}

/// #4527: delete almide.lock after the branch moved, and the next resolve
/// locks the branch's new head, for a default-branch dependency and a named
/// branch alike. Before, it re-locked the commit cached at the first fetch.
#[test]
fn deleting_the_lock_re_resolves_a_floating_ref_against_the_remote() {
    let f = Fixture::new("relock");
    let floating = f.repo("floating", "", &[]);
    let branched = f.repo("branched", "", &[]);
    let app = f.project(
        "app",
        &format!("floating = {{ git = \"{floating}\" }}\nbranched = {{ git = \"{branched}\", branch = \"main\" }}\n"),
    );

    let o = f.run(&app, &["deps"]);
    assert!(o.status.success(), "first resolve:\n{}", stderr(&o));
    let (a_floating, a_branched) = (f.head("floating"), f.head("branched"));
    assert_eq!(locked_commit(&app, "floating").as_deref(), Some(a_floating.as_str()));
    assert_eq!(locked_commit(&app, "branched").as_deref(), Some(a_branched.as_str()));

    // The branches move. With the lock in place nothing changes: the lock is
    // sticky until `almide update`.
    let b_floating = f.advance("floating", 2);
    let b_branched = f.advance("branched", 2);
    let o = f.run(&app, &["deps"]);
    assert!(o.status.success(), "locked resolve:\n{}", stderr(&o));
    assert_eq!(locked_commit(&app, "floating").as_deref(), Some(a_floating.as_str()));
    assert_eq!(locked_commit(&app, "branched").as_deref(), Some(a_branched.as_str()));

    // No lock: resolve against the remote, not the cache.
    std::fs::remove_file(app.join("almide.lock")).unwrap();
    let o = f.run(&app, &["deps"]);
    assert!(o.status.success(), "unlocked resolve:\n{}", stderr(&o));
    assert_eq!(
        locked_commit(&app, "floating").as_deref(),
        Some(b_floating.as_str()),
        "the default-branch dependency was re-locked to the cached commit, not the remote head:\n{}",
        stderr(&o)
    );
    assert_eq!(
        locked_commit(&app, "branched").as_deref(),
        Some(b_branched.as_str()),
        "the branch dependency was re-locked to the cached commit, not the remote head:\n{}",
        stderr(&o)
    );

    // A lock needs no network: with both remotes gone, the locked commits
    // still resolve from the cache.
    let remotes = f.root.join("remotes");
    let gone = f.root.join("remotes-gone");
    std::fs::rename(&remotes, &gone).unwrap();
    let o = f.run(&app, &["deps"]);
    assert!(o.status.success(), "a locked resolve asked the remote:\n{}", stderr(&o));

    // No lock and no remote: the cached checkout, with a warning that names
    // its commit, and no lock entry made from it.
    std::fs::remove_file(app.join("almide.lock")).unwrap();
    let o = f.run(&app, &["deps"]);
    let text = stderr(&o);
    assert!(o.status.success(), "the unreachable-remote fallback failed:\n{text}");
    assert!(text.contains("warning: could not reach"), "{text}");
    assert!(text.contains(&b_floating[..12]) && text.contains(&b_branched[..12]), "the warning must name the cached commits:\n{text}");
    assert!(!app.join("almide.lock").exists(), "a fallback to the cache was written into almide.lock");
    std::fs::rename(&gone, &remotes).unwrap();
}

/// #4529: several `almide` processes resolving the same dependency into one
/// cache at once all succeed, and one of them clones it.
#[test]
fn concurrent_processes_fetching_one_dependency_all_succeed_with_one_clone() {
    let f = Fixture::new("parallel");
    let shared = f.repo("shared", "", &["v0.3.0"]);
    let apps: Vec<PathBuf> = (0..6)
        .map(|i| f.project(&format!("app{i}"), &format!("shared = {{ git = \"{shared}\", tag = \"v0.3.0\" }}\n")))
        .collect();
    let children: Vec<_> = apps
        .iter()
        .map(|app| f.almide(app, &["deps"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().expect("spawn"))
        .collect();
    let outputs: Vec<Output> = children.into_iter().map(|c| c.wait_with_output().expect("wait")).collect();
    let mut clones = 0;
    for o in &outputs {
        let text = stderr(o);
        assert!(o.status.success(), "a concurrent fetch failed:\n{text}");
        clones += text.lines().filter(|l| l.starts_with("Fetching shared from")).count();
    }
    assert_eq!(clones, 1, "every racing process cloned the dependency");
    let tag_commit = f.head("shared");
    for app in &apps {
        assert_eq!(locked_commit(app, "shared").as_deref(), Some(tag_commit.as_str()));
    }
}

/// #4528: a tag the remote does not have — here asked for by a transitive
/// dependency — names the dependency, the tag, the chain from the root and
/// the newest tags the remote has, instead of git's raw stderr.
#[test]
fn a_missing_tag_names_the_dependency_the_chain_and_the_existing_tags() {
    let f = Fixture::new("missing-tag");
    let leaf = f.repo("leaf", "", &["v0.1.0", "v0.2.0"]);
    let mid = f.repo("mid", &format!("\n[dependencies]\nleaf = {{ git = \"{leaf}\", tag = \"v9.9.9\" }}\n"), &["v0.1.0"]);
    let app = f.project("app", &format!("mid = {{ git = \"{mid}\", tag = \"v0.1.0\" }}\n"));

    let o = f.run(&app, &["deps"]);
    let text = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{text}");
    assert!(
        text.contains(&format!("dependency `leaf` (required by app → mid → leaf) asks for tag v9.9.9, which {leaf} does not have")),
        "{text}"
    );
    assert!(text.contains("hint: the newest tags are v0.2.0, v0.1.0"), "{text}");
    assert!(!text.contains("Remote branch"), "git's raw stderr leaked:\n{text}");

    // A branch the remote does not have, on the floating path.
    let app2 = f.project("app2", &format!("leaf = {{ git = \"{leaf}\", branch = \"nope\" }}\n"));
    let o = f.run(&app2, &["deps"]);
    let text = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{text}");
    assert!(text.contains("dependency `leaf` (required by app2 → leaf) asks for branch nope"), "{text}");

    // `almide add --dry-run` looks the tag up before saying yes.
    let before = std::fs::read(app2.join("almide.toml")).unwrap();
    let o = f.run(&app2, &["add", "leaf2", "--git", &leaf, "--tag", "v9.9.9", "--dry-run"]);
    let text = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "add --dry-run accepted a tag that does not exist:\n{text}");
    assert!(text.contains("asks for tag v9.9.9") && text.contains("v0.2.0, v0.1.0"), "{text}");
    let o = f.run(&app2, &["add", "leaf2", "--git", &leaf, "--tag", "v0.2.0", "--dry-run"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains("the repository and the tag exist"));
    assert_eq!(std::fs::read(app2.join("almide.toml")).unwrap(), before, "a dry run wrote the manifest");
}
