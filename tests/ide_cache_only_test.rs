//! `almide ide` looks at a project without being asked to build it, so it
//! resolves dependencies cache-only (#4128): `ide outline` and `ide doc` on a
//! file of a project never clone the dependencies its manifest names and never
//! write `almide.lock`. The LSP's `didOpen` has the same rule
//! (`tests/lsp_test.rs`); this pins the CLI.
//!
//! The dependency is a local git repository a fetch WOULD clone successfully,
//! so a fetch is visible: a checkout under `$HOME/.almide/cache` and a lock.
//! Control: a plain `almide check` in the same project fetches and locks.

use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// A project in `app` whose one dependency is a tagged git repository in `repo`.
fn project(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let repo = root.join("repo");
    let home = root.join("home");
    let app = root.join("app");
    for d in [&repo.join("src"), &home, &app] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(repo.join("almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(repo.join("src/mod.almd"), "fn f() -> Int = 1\n").unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&repo)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    git(&["tag", "v0.1.0"]);
    std::fs::write(
        app.join("almide.toml"),
        format!(
            "[package]\nname = \"ide_offline_probe\"\nversion = \"0.1.0\"\n\n[dependencies]\ndep = {{ git = \"file://{}\", tag = \"v0.1.0\" }}\n",
            repo.display()
        ),
    )
    .unwrap();
    std::fs::write(app.join("main.almd"), "fn greet() -> String = \"hi\"\n\neffect fn main() -> Unit = println(greet())\n").unwrap();
    (app, home)
}

fn cloned(home: &Path) -> bool {
    std::fs::read_dir(home.join(".almide").join("cache")).map(|mut d| d.next().is_some()).unwrap_or(false)
}

#[test]
fn ide_outline_and_doc_never_fetch_or_write_the_lock() {
    let root = tempfile::tempdir().unwrap();
    let (app, home) = project(root.path());
    for args in [&["ide", "outline", "main.almd"][..], &["ide", "doc", "greet", "--file", "main.almd"][..]] {
        let o = Command::new(almide()).args(args).current_dir(&app).env("HOME", &home).output().expect("run almide ide");
        let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        assert!(text.contains("greet"), "`almide {}` did not describe the file:\n{text}", args.join(" "));
        assert!(!app.join("almide.lock").exists(), "`almide {}` wrote almide.lock:\n{text}", args.join(" "));
        assert!(!cloned(&home), "`almide {}` cloned into the dependency cache:\n{text}", args.join(" "));
    }

    let control = Command::new(almide())
        .args(["check", "main.almd"])
        .current_dir(&app)
        .env("HOME", &home)
        .output()
        .expect("run almide check");
    assert!(
        app.join("almide.lock").exists() && cloned(&home),
        "control: a plain `almide check` must fetch and lock (stderr: {})",
        String::from_utf8_lossy(&control.stderr)
    );
}
