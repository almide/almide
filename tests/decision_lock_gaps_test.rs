//! #4852: three lock gaps, each with a fresh package the checks never saw.
//! - `--locked` holds on every command that resolves dependencies (check,
//!   bench, emit as well as run / build / test): a dependency the lock does
//!   not name is refused before anything is fetched. Row `deps.locked-mode`.
//! - A changed source is refused unless THIS dependency's own resolution came
//!   from the locked source — another package fetching that URL does not lift
//!   it. Row `deps.changed-source`.
//! - A fetched package's own `path =` dependency may name only what lies
//!   inside that package. Row `trust.local-source-exempt`.

#[path = "support/git_host.rs"]
mod git_host;
#[path = "support/novel.rs"]
mod novel;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn host() -> &'static git_host::GitHost {
    static HOST: std::sync::OnceLock<git_host::GitHost> = std::sync::OnceLock::new();
    HOST.get_or_init(|| git_host::GitHost::start(&std::env::temp_dir().join(format!("almide-lockgaps-host-{}", std::process::id()))))
}

/// The host's request count is shared by every test in this binary, so the
/// tests run one at a time: a count read across a run then sees only it.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// Commit `files` (relative path, text) and tag `tag` in `repo`.
fn commit(repo: &Path, files: &[(&str, String)], tag: &str) {
    if !repo.join(".git").exists() {
        std::fs::create_dir_all(repo).unwrap();
        git(repo, &["init", "-q", "-b", "main"]);
    }
    for e in std::fs::read_dir(repo).unwrap().flatten() {
        if e.file_name() != ".git" {
            let p = e.path();
            if p.is_dir() { std::fs::remove_dir_all(p).unwrap() } else { std::fs::remove_file(p).unwrap() }
        }
    }
    for (rel, text) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", tag]);
    git(repo, &["tag", tag]);
}

fn package(name: &str, deps: &str) -> Vec<(&'static str, String)> {
    vec![
        ("almide.toml", format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}")),
        ("src/mod.almd", "fn hello() -> String = \"hi\"\n".to_string()),
    ]
}

/// A served repository at a fresh `https://github.com/almide/<name>` URL.
fn serve(root: &Path) -> (PathBuf, String) {
    let name = novel::ident("repo").replace('_', "-");
    let dir = root.join(&name);
    let url = format!("https://github.com/almide/{name}");
    host().serve(&url, &dir);
    (dir, url)
}

fn app(root: &Path, deps: &str) -> PathBuf {
    let proj = root.join("app");
    std::fs::create_dir_all(proj.join("src")).unwrap();
    std::fs::write(proj.join("almide.toml"), format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}")).unwrap();
    std::fs::write(proj.join("src/main.almd"), "effect fn main() -> Unit = println(\"app\")\n").unwrap();
    proj
}

fn almide(proj: &Path, home: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.args(args).current_dir(proj).env("HOME", home);
    host().env(&mut cmd).output().expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("exit {:?}\n{}{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn root() -> (tempfile::TempDir, PathBuf) {
    let td = tempfile::TempDir::new().unwrap();
    let home = td.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    (td, home)
}

#[test]
fn novel_locked_holds_on_check_bench_and_emit() {
    let _serial = serial();
    let (td, home) = root();
    let dep = novel::ident("dep");
    let (repo, url) = serve(td.path());
    commit(&repo, &package(&dep, ""), "v0.1.0");
    let proj = app(td.path(), &format!("{dep} = {{ git = \"{url}\", tag = \"v0.1.0\" }}\n"));
    for args in [&["check", "--locked", "src/main.almd"][..], &["emit", "--locked", "src/main.almd", "--target", "rust"][..], &["bench", "--locked", "src/main.almd"][..]] {
        let before = host().requests();
        let o = almide(&proj, &home, args);
        let t = text(&o);
        assert!(!o.status.success(), "`almide {}` resolved a dependency the lock does not name:\n{t}", args.join(" "));
        assert!(t.contains("--locked: not in almide.lock") && t.contains(&dep), "`almide {}`:\n{t}", args.join(" "));
        assert_eq!(host().requests(), before, "`almide {}` fetched under --locked", args.join(" "));
        assert!(!proj.join("almide.lock").exists(), "`almide {}` wrote a lock under --locked", args.join(" "));
    }
}

#[test]
fn novel_a_changed_source_is_not_lifted_by_another_package_fetching_the_old_url() {
    let _serial = serial();
    let (td, home) = root();
    let (a, c) = (novel::ident("a"), novel::ident("c"));
    let (old, old_url) = serve(td.path());
    let (new, new_url) = serve(td.path());
    // The old URL serves `a` at v0.1.0 and an unrelated package `c` at v0.2.0.
    commit(&old, &package(&a, ""), "v0.1.0");
    commit(&old, &package(&c, ""), "v0.2.0");
    // The new source of `a` itself depends on `c` from the OLD URL.
    commit(&new, &package(&a, &format!("{c} = {{ git = \"{old_url}\", tag = \"v0.2.0\" }}\n")), "v0.1.0");
    let proj = app(td.path(), &format!("{a} = {{ git = \"{old_url}\", tag = \"v0.1.0\" }}\n"));
    let o = almide(&proj, &home, &["check", "src/main.almd"]);
    assert!(o.status.success() && proj.join("almide.lock").exists(), "control: the first check locks `{a}`:\n{}", text(&o));
    std::fs::write(
        proj.join("almide.toml"),
        format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{a} = {{ git = \"{new_url}\", tag = \"v0.1.0\" }}\n"),
    )
    .unwrap();
    let o = almide(&proj, &home, &["check", "src/main.almd"]);
    let t = text(&o);
    assert!(!o.status.success(), "`{a}` changed source under the lock and was admitted because `{c}` fetched the old URL:\n{t}");
    assert!(t.contains(&format!("`{a}` is locked to")), "{t}");
}

#[test]
fn novel_a_fetched_packages_path_dependency_outside_it_is_refused() {
    let _serial = serial();
    let (td, home) = root();
    let (p, q) = (novel::ident("p"), novel::ident("q"));
    let outside = td.path().join(novel::path_segment());
    std::fs::create_dir_all(outside.join("src")).unwrap();
    std::fs::write(outside.join("almide.toml"), format!("[package]\nname = \"{q}\"\nversion = \"0.1.0\"\n")).unwrap();
    std::fs::write(outside.join("src/mod.almd"), "fn hello() -> String = \"outside\"\n").unwrap();
    let (repo, url) = serve(td.path());
    commit(&repo, &package(&p, &format!("{q} = {{ path = \"{}\" }}\n", outside.display())), "v0.1.0");
    let proj = app(td.path(), &format!("{p} = {{ git = \"{url}\", tag = \"v0.1.0\" }}\n"));
    let o = almide(&proj, &home, &["check", "src/main.almd"]);
    let t = text(&o);
    assert!(!o.status.success(), "a fetched package loaded a local directory outside itself:\n{t}");
    assert!(t.contains("outside its own package") && t.contains(&q), "{t}");
}
