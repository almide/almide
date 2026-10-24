//! `[trust] owners` (#4093 option a): a git dependency is fetched only from
//! an owner the root manifest trusts — github.com/almide by default — on
//! `almide add` and on every fetch, direct, transitive or locked.
//!
//! No network: each "host" is a directory of bare repositories, and git's
//! own `url.<base>.insteadOf` (passed through `GIT_CONFIG_*`, never written
//! to any config file) maps `https://github.com/` onto it. The manifests and
//! locks therefore name the real `https://github.com/<owner>/<repo>` URLs,
//! which is what the trust check reads, while git clones from disk. `HOME`
//! is a scratch directory, so the dependency cache is per test.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Hosts {
    root: PathBuf,
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

impl Hosts {
    fn new(tag: &str) -> Hosts {
        let root = std::env::temp_dir().join(format!("almide-issue4093a-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("mkdir");
        let h = Hosts { root };
        h.repo("almide", "svg", "svg", "fn value() -> Int = 1\n", "");
        h.repo("evil", "svg", "svg", "fn value() -> Int = 666\n", "");
        h.repo("almide", "csv", "csv", "fn value() -> Int = 3\n", "");
        h.repo(
            "alice",
            "mylib",
            "mylib",
            "import util\n\nfn value() -> Int = util.value() * 2\n",
            "\n[dependencies]\nutil = { git = \"https://github.com/bob/util\", tag = \"v0.1.0\" }\n",
        );
        h.repo("bob", "util", "util", "fn value() -> Int = 5\n", "");
        h.repo("carol", "solo", "solo", "fn value() -> Int = 7\n", "");
        h
    }

    /// A bare repository `<owner>/<repo>` holding package `pkg`, tagged v0.1.0.
    fn repo(&self, owner: &str, repo: &str, pkg: &str, src: &str, extra: &str) {
        let work = self.root.join("work").join(owner).join(repo);
        write(work.join("almide.toml"), &format!("[package]\nname = \"{pkg}\"\nversion = \"0.1.0\"\n{extra}"));
        write(work.join("src/mod.almd"), src);
        git(&work, &["init", "-q"]);
        git(&work, &["add", "-A"]);
        git(&work, &["commit", "-qm", "init"]);
        git(&work, &["tag", "v0.1.0"]);
        let bare = self.root.join("hosts").join(owner);
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["clone", "-q", "--bare", work.to_str().unwrap(), repo]);
    }

    /// A project directory `name` with `manifest` and a `main` printing `expr`.
    fn project(&self, name: &str, manifest: &str, imports: &[&str], expr: &str) -> PathBuf {
        let dir = self.root.join(name);
        write(dir.join("almide.toml"), &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n{manifest}"));
        let imports: String = imports.iter().map(|i| format!("import {i}\n")).collect();
        write(
            dir.join("src/main.almd"),
            &format!(
                "{imports}\neffect fn main() -> Unit = println(int.to_string({expr}))\n\n\
                 test \"value\" {{\n  assert({expr} > 0)\n}}\n"
            ),
        );
        dir
    }

    fn almide(&self, dir: &Path, args: &[&str]) -> Output {
        let base = format!("file://{}/", self.root.join("hosts").display());
        Command::new(env!("CARGO_BIN_EXE_almide"))
            .current_dir(dir)
            .env("HOME", self.root.join("home"))
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", format!("url.{base}.insteadOf"))
            .env("GIT_CONFIG_VALUE_0", "https://github.com/")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("ALMIDE_RUN_PROJECT_DIR", self.root.join(".run"))
            .args(args)
            .output()
            .expect("spawn almide")
    }
}

impl Drop for Hosts {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn read(path: PathBuf) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

const RUN: &[&str] = &["run", "src/main.almd", "--target", "wasm"];

#[test]
fn an_untrusted_add_is_refused_with_the_source_and_writes_nothing() {
    let h = Hosts::new("refuse");
    let dir = h.project("app", "", &[], "1");
    let before = read(dir.join("almide.toml"));
    let o = h.almide(&dir, &["add", "carol/solo@v0.1.0"]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    for needle in [
        "error: refusing to add `solo`: its owner is not trusted by this project",
        "source: https://github.com/carol/solo",
        "owner: github.com/carol (not in [trust] owners)",
        "models often invent repository names",
        "almide add carol/solo@v0.1.0 --trust github.com/carol",
        "if you chose this name yourself, do not trust it",
    ] {
        assert!(err.contains(needle), "missing `{needle}`:\n{err}");
    }
    assert_eq!(read(dir.join("almide.toml")), before, "a refused add edited almide.toml");
    assert!(read(dir.join("almide.lock")).is_none(), "a refused add wrote a lock");
    assert!(!h.root.join("home/.almide/cache/.sources/https/github.com/carol").exists(), "a refused add fetched");
}

#[test]
fn add_with_the_matching_trust_writes_the_grant_and_builds() {
    let h = Hosts::new("trust");
    let dir = h.project("app", "", &["solo"], "solo.value()");
    let o = h.almide(&dir, &["add", "carol/solo@v0.1.0", "--trust", "github.com/carol"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let manifest = read(dir.join("almide.toml")).unwrap();
    assert!(manifest.contains("solo = { git = \"https://github.com/carol/solo\", tag = \"v0.1.0\" }"), "{manifest}");
    assert!(manifest.contains("[trust]\nowners = [\"github.com/carol\"]"), "{manifest}");
    assert!(read(dir.join("almide.lock")).unwrap().contains("https://github.com/carol/solo"));
    let o = h.almide(&dir, RUN);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), "7\n");
}

#[test]
fn a_trust_flag_naming_another_owner_is_refused() {
    let h = Hosts::new("mismatch");
    let dir = h.project("app", "", &[], "1");
    let before = read(dir.join("almide.toml"));
    for wrong in ["github.com/caro1", "caro1", "gitlab.com/carol"] {
        let o = h.almide(&dir, &["add", "carol/solo@v0.1.0", "--trust", wrong]);
        assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
        assert!(stderr(&o).contains("does not match the owner of https://github.com/carol/solo (github.com/carol)"), "{}", stderr(&o));
    }
    assert_eq!(read(dir.join("almide.toml")), before);
}

#[test]
fn a_transitive_untrusted_owner_is_refused_naming_the_chain() {
    let h = Hosts::new("transitive");
    let manifest = "\n[dependencies]\nmylib = { git = \"https://github.com/alice/mylib\", tag = \"v0.1.0\" }\n\n[trust]\nowners = [\"github.com/alice\"]\n";
    let dir = h.project("app", manifest, &["mylib"], "mylib.value()");
    for args in [RUN, &["build", "src/main.almd", "--target", "wasm", "-o", "out.wasm"], &["test", "src/main.almd"]] {
        let o = h.almide(&dir, args);
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "`almide {}`:\n{err}", args.join(" "));
        assert!(err.contains("dependency `util` (required by mylib → util) is fetched from an owner this project does not trust"), "{err}");
        assert!(err.contains("run `almide trust github.com/bob`"), "{err}");
    }
    // A dependency's own [trust] is never read: grant bob inside mylib? It
    // cannot; only the root can. `almide trust` is the one step.
    let o = h.almide(&dir, &["trust", "github.com/bob"]);
    assert!(o.status.success(), "{}", stderr(&o));
    let o = h.almide(&dir, RUN);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), "10\n");
}

#[test]
fn a_locked_dependency_from_an_untrusted_owner_is_refused() {
    let h = Hosts::new("locked-untrusted");
    // Trusted once: build, so the lock is written.
    let manifest = "\n[dependencies]\nsolo = { git = \"https://github.com/carol/solo\", tag = \"v0.1.0\" }\n";
    let dir = h.project("app", &format!("{manifest}\n[trust]\nowners = [\"github.com/carol\"]\n"), &["solo"], "solo.value()");
    let o = h.almide(&dir, RUN);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(read(dir.join("almide.lock")).unwrap().contains("carol/solo"));
    // The grant removed: the lock and the warm cache are not a grant.
    write(dir.join("almide.toml"), &format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n{manifest}"));
    let o = h.almide(&dir, RUN);
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("dependency `solo` is fetched from an owner this project does not trust"), "{}", stderr(&o));
}

#[test]
fn a_source_changed_under_the_lock_is_refused_until_update() {
    let h = Hosts::new("source-changed");
    let deps = |owner: &str| format!("\n[dependencies]\nsvg = {{ git = \"https://github.com/{owner}/svg\", tag = \"v0.1.0\" }}\n\n[trust]\nowners = [\"github.com/evil\"]\n");
    let dir = h.project("app", &deps("almide"), &["svg"], "svg.value()");
    let o = h.almide(&dir, RUN);
    assert_eq!(stdout(&o), "1\n", "{}", stderr(&o));
    write(dir.join("almide.toml"), &format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n{}", deps("evil")));
    let lock_before = read(dir.join("almide.lock"));
    let o = h.almide(&dir, RUN);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("`svg` is locked to https://github.com/almide/svg but almide.toml now says https://github.com/evil/svg")
            && err.contains("run `almide update svg`"),
        "{err}"
    );
    assert_eq!(read(dir.join("almide.lock")), lock_before, "the refusal rewrote the lock");
    let o = h.almide(&dir, &["update", "svg"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(read(dir.join("almide.lock")).unwrap().contains("https://github.com/evil/svg"));
    let o = h.almide(&dir, RUN);
    assert_eq!(stdout(&o), "666\n", "{}", stderr(&o));
}

#[test]
fn the_cache_is_keyed_by_source_so_a_same_named_package_cannot_answer() {
    let h = Hosts::new("cache");
    let evil = h.project(
        "first",
        "\n[dependencies]\nsvg = { git = \"https://github.com/evil/svg\", tag = \"v0.1.0\" }\n\n[trust]\nowners = [\"github.com/evil\"]\n",
        &["svg"],
        "svg.value()",
    );
    let o = h.almide(&evil, RUN);
    assert_eq!(stdout(&o), "666\n", "{}", stderr(&o));
    // Same HOME, same package name and tag, the official source.
    let honest = h.project("second", "\n[dependencies]\nsvg = { git = \"https://github.com/almide/svg\", tag = \"v0.1.0\" }\n", &["svg"], "svg.value()");
    let o = h.almide(&honest, RUN);
    assert_eq!(stdout(&o), "1\n", "the evil/svg checkout answered for almide/svg:\n{}", stderr(&o));
    let sources = h.root.join("home/.almide/cache/.sources/https/github.com");
    assert!(sources.join("evil/svg/@").is_dir() && sources.join("almide/svg/@").is_dir());
}

#[test]
fn a_missing_repository_is_not_found_with_a_runnable_suggestion() {
    let h = Hosts::new("notfound");
    let dir = h.project("app", "", &[], "1");
    let before = read(dir.join("almide.toml"));
    let o = h.almide(&dir, &["add", "csvv"]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("error: https://github.com/almide/csvv does not exist"), "{err}");
    assert!(err.contains("did you mean `almide add csv`?"), "{err}");
    // An official name under an invented owner: refused, and pointed home.
    let o = h.almide(&dir, &["add", "almide-lang/csv"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("did you mean `almide add csv`?"), "{}", stderr(&o));
    assert_eq!(read(dir.join("almide.toml")), before);
    assert!(read(dir.join("almide.lock")).is_none());
}

#[test]
fn a_failed_fetch_leaves_the_manifest_untouched() {
    let h = Hosts::new("failed-fetch");
    let dir = h.project("app", "", &[], "1");
    let before = read(dir.join("almide.toml"));
    // The repository exists; the tag does not.
    let o = h.almide(&dir, &["add", "svg@v9.9.9"]);
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert_eq!(read(dir.join("almide.toml")), before);
    // The fetch succeeds but locking it fails (an untrusted transitive):
    // both files go back.
    let o = h.almide(&dir, &["add", "alice/mylib@v0.1.0", "--trust", "alice"]);
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("almide.toml and almide.lock are unchanged"), "{}", stderr(&o));
    assert_eq!(read(dir.join("almide.toml")), before);
    assert!(read(dir.join("almide.lock")).is_none());
}

#[test]
fn dry_run_reports_the_verdict_and_writes_nothing() {
    let h = Hosts::new("dry-run");
    let dir = h.project("app", "", &[], "1");
    let before = read(dir.join("almide.toml"));
    let o = h.almide(&dir, &["add", "carol/solo@v0.1.0", "--trust", "carol", "--dry-run"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("would add solo = https://github.com/carol/solo at v0.1.0"), "{}", stdout(&o));
    let o = h.almide(&dir, &["add", "carol/solo", "--dry-run"]);
    assert_eq!(o.status.code(), Some(1), "an untrusted dry run must still say no");
    assert_eq!(read(dir.join("almide.toml")), before);
    assert!(!h.root.join("home/.almide/cache/.sources").exists(), "a dry run fetched");
}

#[test]
fn locked_refuses_what_the_lock_does_not_name() {
    let h = Hosts::new("locked");
    let dir = h.project("app", "\n[dependencies]\nsvg = { git = \"https://github.com/almide/svg\", tag = \"v0.1.0\" }\n", &["svg"], "svg.value()");
    let run_locked: &[&str] = &["run", "src/main.almd", "--target", "wasm", "--locked"];
    let o = h.almide(&dir, run_locked);
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("error: --locked: not in almide.lock: `svg`"), "{}", stderr(&o));
    assert!(read(dir.join("almide.lock")).is_none(), "--locked wrote a lock");
    assert!(!h.root.join("home/.almide/cache/.sources").exists(), "--locked fetched");
    // Locked once, it builds under --locked.
    assert!(h.almide(&dir, RUN).status.success());
    let o = h.almide(&dir, run_locked);
    assert_eq!(stdout(&o), "1\n", "{}", stderr(&o));
}
