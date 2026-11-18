//! A dependency's source, lock entry and source tree are untrusted input
//! (#4810, #4811, #4812, #4813). Each case below was measured to run a
//! command, reach another host, use an arbitrary directory as the package or
//! load modules from outside the package before the change; each is now
//! refused (or ignored) before git or the loader acts on it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-dep-hardening-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home")).expect("mkdir home");
    root
}

/// `almide <args>` in `dir`, with the dependency cache under `root/home`.
fn almide(dir: &Path, root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .env("HOME", root.join("home"))
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .output()
        .expect("spawn almide")
}

/// [`almide`], killed after `secs` (`None`: it did not finish).
fn almide_within(dir: &Path, root: &Path, args: &[&str], secs: u64) -> Option<Output> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .env("HOME", root.join("home"))
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn almide");
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(secs) {
        if child.try_wait().expect("wait").is_some() {
            return Some(child.wait_with_output().expect("output"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn app(dir: &Path, deps: &str, main: &str) {
    write(dir.join("almide.toml"), &format!("[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}"));
    write(dir.join("src/main.almd"), main);
}

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .expect("spawn git");
    assert!(o.status.success(), "git {args:?}: {}", text(&o));
}

/// #4810: an option-shaped git source — in the root manifest, or in a
/// fetched dependency's own manifest — is refused before git runs; the
/// command it smuggles in never runs, on `check`, `test` or `deps`.
#[test]
fn an_option_shaped_git_source_runs_nothing() {
    let root = scratch("g1");
    let marker = root.join("MARKER");
    let evil = format!("x = {{ git = \"--upload-pack=touch {};true\" }}\n", marker.display());
    let direct = root.join("direct");
    app(&direct, &evil, "effect fn main() -> Unit = println(\"hi\")\n\ntest \"t\" { assert(true) }\n");
    for args in [&["check", "src/main.almd"][..], &["test", "src/main.almd"], &["deps"]] {
        let o = almide(&direct, &root, args);
        assert!(!o.status.success(), "`almide {}` accepted an option-shaped source:\n{}", args.join(" "), text(&o));
        assert!(text(&o).contains("begins with `-`"), "`almide {}`: {}", args.join(" "), text(&o));
        assert!(!marker.exists(), "`almide {}` ran the source's command", args.join(" "));
    }
    // The same source in a fetched dependency's own almide.toml.
    let depa = root.join("depa");
    write(depa.join("src/mod.almd"), "fn hello() -> String = \"hi\"\n");
    write(depa.join("almide.toml"), &format!("[package]\nname = \"depa\"\nversion = \"0.1.0\"\n\n[dependencies]\n{evil}"));
    git(&depa, &["init", "-q"]);
    git(&depa, &["add", "-A"]);
    git(&depa, &["commit", "-qm", "init"]);
    git(&depa, &["tag", "v0.1.0"]);
    let trans = root.join("trans");
    app(
        &trans,
        &format!("depa = {{ git = \"file://{}\", tag = \"v0.1.0\" }}\n", depa.display()),
        "import depa\neffect fn main() -> Unit = println(depa.hello())\n",
    );
    let o = almide(&trans, &root, &["check", "src/main.almd"]);
    assert!(!o.status.success(), "a transitive option-shaped source was accepted:\n{}", text(&o));
    assert!(!marker.exists(), "the fetched dependency's source ran its command:\n{}", text(&o));
    let _ = std::fs::remove_dir_all(&root);
}

/// #4811: a `?` or `#` ahead of an `@` made the owner check read
/// github.com/almide while git contacts the host before it.
#[test]
fn a_query_or_fragment_in_a_git_source_is_refused() {
    let root = scratch("g2");
    for sep in ["?", "#"] {
        let dir = root.join(format!("app{}", sep.len()));
        let src = format!("https://other.invalid{sep}@github.com/almide/json");
        app(&dir, &format!("json = {{ git = \"{src}\", tag = \"v0.1.0\" }}\n"), "effect fn main() -> Unit = println(\"hi\")\n");
        let o = almide(&dir, &root, &["check", "src/main.almd"]);
        assert!(!o.status.success(), "{src} was accepted:\n{}", text(&o));
        assert!(text(&o).contains("holds a `?` or `#`"), "{src}: {}", text(&o));
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// #4812: the lock's `commit` names the cache directory; an absolute path
/// there was joined in as the package directory.
#[test]
fn a_lock_commit_that_is_not_a_hash_is_refused() {
    let root = scratch("g4");
    let elsewhere = root.join("elsewhere");
    write(elsewhere.join("almide.toml"), "[package]\nname = \"foo\"\nversion = \"0.1.0\"\n");
    write(elsewhere.join("src/mod.almd"), "fn hello() -> String = \"from-elsewhere\"\n");
    let dir = root.join("app");
    app(
        &dir,
        "foo = { git = \"https://github.com/almide/does-not-exist-hardening\", tag = \"v0.1.0\" }\n",
        "import foo\neffect fn main() -> Unit = println(foo.hello())\n",
    );
    write(
        dir.join("almide.lock"),
        &format!("foo = {{ git = \"https://github.com/almide/does-not-exist-hardening\", ref = \"v0.1.0\", commit = \"{}\" }}\n", elsewhere.display()),
    );
    let o = almide(&dir, &root, &["check", "src/main.almd"]);
    assert!(!o.status.success(), "a path as the lock's commit was accepted:\n{}", text(&o));
    assert!(text(&o).contains("is not a hex commit hash"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&root);
}

/// #4813: a dependency's source tree is walked without following symlinks:
/// one to a directory outside the package loaded its modules, one to `/`
/// hung `almide check`.
#[cfg(unix)]
#[test]
fn a_dependency_source_symlink_is_not_followed() {
    let root = scratch("s2");
    let outside = root.join("outside");
    write(outside.join("evil.almd"), "fn broken( = \n");
    for (k, target) in [outside.clone(), PathBuf::from("/")].into_iter().enumerate() {
        let dir = root.join(format!("app{k}"));
        write(dir.join("dep/almide.toml"), "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n");
        write(dir.join("dep/src/mod.almd"), "fn hello() -> String = \"hi\"\n");
        std::os::unix::fs::symlink(&target, dir.join("dep/src/ext")).expect("symlink");
        app(&dir, "dep = { path = \"dep\" }\n", "import dep\neffect fn main() -> Unit = println(dep.hello())\n");
        let o = almide_within(&dir, &root, &["check", "src/main.almd"], 60)
            .unwrap_or_else(|| panic!("`almide check` with src/ext -> {} did not finish in 60 s", target.display()));
        assert!(o.status.success(), "src/ext -> {}: {}", target.display(), text(&o));
        assert!(!text(&o).contains("evil"), "a module outside the package was loaded:\n{}", text(&o));
    }
    let _ = std::fs::remove_dir_all(&root);
}
