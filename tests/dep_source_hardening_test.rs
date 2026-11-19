//! A dependency's source, lock entry and source tree are untrusted input
//! (#4810, #4811, #4812, #4813; ADR-0035 D1). Each case below was measured to
//! run a command, reach another host, use an arbitrary directory as the
//! package or load modules from outside the package before the change; each
//! is now refused (or ignored) before anything is fetched or loaded.
//!
//! Repositories are served by `support/git_host.rs` (no network).

#[path = "support/git_host.rs"]
mod git_host;

use git_host::GitHost;
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

/// `almide <args>` in `dir`, with the dependency cache under `root/home`,
/// pointed at `host` when there is one.
fn almide(dir: &Path, root: &Path, host: Option<&GitHost>, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.current_dir(dir).env("HOME", root.join("home")).args(args);
    if let Some(h) = host {
        h.env(&mut cmd);
    }
    cmd.output().expect("spawn almide")
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

/// Every form ADR-0035 D1 lists as refused, as a manifest `git = …` value,
/// with a word its diagnostic must hold.
fn refused_sources(marker: &Path) -> Vec<(String, &'static str)> {
    vec![
        (format!("--upload-pack=touch {};true", marker.display()), "begins with `-`"),
        ("https://other.invalid?@github.com/almide/json".into(), "holds a `?` or `#`"),
        ("https://other.invalid#@github.com/almide/json".into(), "holds a `?` or `#`"),
        ("https://user:pw@github.com/almide/json".into(), "password or token"),
        ("git://github.com/almide/json".into(), "scheme"),
        ("file:///srv/git/json".into(), "scheme"),
        ("ext::sh".into(), "transport helper"),
        ("https://github.com/-x/json".into(), "begins with `-`"),
        ("https://github.com/almide/js\u{202E}on".into(), "bidi"),
        ("https://github.com/almide/%E2%80%AEjson".into(), "percent-encoded"),
        ("https://github.com/almide/../json".into(), "path segment"),
        ("https://github.com/almide/aux".into(), "reserved"),
    ]
}

fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || ('\u{202A}'..='\u{202E}').contains(&c) => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// #4810, #4811, ADR-0035 D1: each refused form, in the root manifest, is
/// refused before anything is fetched, on `check`, `test` and `deps`, and
/// the command an option-shaped source smuggles in never runs.
#[test]
fn every_refused_source_form_in_the_root_manifest_runs_and_fetches_nothing() {
    let root = scratch("g1");
    let host = GitHost::start(&root);
    let marker = root.join("MARKER");
    for (i, (src, why)) in refused_sources(&marker).into_iter().enumerate() {
        let dir = root.join(format!("direct{i}"));
        app(&dir, &format!("x = {{ git = {}, tag = \"v0.1.0\" }}\n", toml_string(&src)), "effect fn main() -> Unit = println(\"hi\")\n\ntest \"t\" { assert(true) }\n");
        for args in [&["check", "src/main.almd"][..], &["test", "src/main.almd"], &["deps"]] {
            let o = almide(&dir, &root, Some(&host), args);
            assert!(!o.status.success(), "`almide {}` accepted `{}`:\n{}", args.join(" "), src.escape_debug(), text(&o));
            assert!(text(&o).contains(why), "`almide {}` on `{}`: expected `{why}`:\n{}", args.join(" "), src.escape_debug(), text(&o));
        }
    }
    assert!(!marker.exists(), "a source's command ran");
    assert_eq!(host.requests(), 0, "a refused source reached the network");
    let _ = std::fs::remove_dir_all(&root);
}

/// The same forms in a fetched dependency's own almide.toml: the
/// dependency is fetched, its manifest is refused, and nothing it names is.
#[test]
fn every_refused_source_form_in_a_dependency_manifest_is_refused() {
    let root = scratch("g1t");
    let host = GitHost::start(&root);
    let marker = root.join("MARKER");
    for (i, (src, why)) in refused_sources(&marker).into_iter().enumerate() {
        let url = format!("https://github.com/almide/hardening-depa-{i}");
        host.publish(
            &url,
            &root.join(format!("depa{i}")),
            &[
                ("src/mod.almd", "fn hello() -> String = \"hi\"\n"),
                ("almide.toml", &format!("[package]\nname = \"depa\"\nversion = \"0.1.0\"\n\n[dependencies]\nx = {{ git = {}, tag = \"v0.1.0\" }}\n", toml_string(&src))),
            ],
            &["v0.1.0"],
        );
        let trans = root.join(format!("trans{i}"));
        app(&trans, &format!("depa = {{ git = \"{url}\", tag = \"v0.1.0\" }}\n"), "import depa\neffect fn main() -> Unit = println(depa.hello())\n");
        let o = almide(&trans, &root, Some(&host), &["check", "src/main.almd"]);
        assert!(!o.status.success(), "a transitive `{}` was accepted:\n{}", src.escape_debug(), text(&o));
        assert!(text(&o).contains(why) && text(&o).contains("dependency `depa`"), "transitive `{}`:\n{}", src.escape_debug(), text(&o));
    }
    assert!(!marker.exists(), "the fetched dependency's source ran its command");
    let _ = std::fs::remove_dir_all(&root);
}

/// #4812: the lock's `commit` named the cache directory; an absolute path
/// there was joined in as the package directory. It is now a typed hash.
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
    for commit in [elsewhere.display().to_string(), "--upload-pack=x".to_string(), "ABCDEF".repeat(7)[..40].to_string(), "abc".to_string()] {
        write(
            dir.join("almide.lock"),
            &format!("foo = {{ git = \"https://github.com/almide/does-not-exist-hardening\", ref = \"v0.1.0\", commit = \"{commit}\" }}\n"),
        );
        let o = almide(&dir, &root, None, &["check", "src/main.almd"]);
        assert!(!o.status.success(), "`{commit}` as the lock's commit was accepted:\n{}", text(&o));
        assert!(text(&o).contains("is not a hex commit hash"), "{}", text(&o));
    }
    // A sum that is not a tree sum (a path, say) is refused the same way.
    write(
        dir.join("almide.lock"),
        &format!(
            "foo = {{ git = \"https://github.com/almide/does-not-exist-hardening\", ref = \"v0.1.0\", commit = \"{}\", sum = \"{}\" }}\n",
            "a".repeat(40),
            elsewhere.display()
        ),
    );
    let o = almide(&dir, &root, None, &["check", "src/main.almd"]);
    assert!(!o.status.success() && text(&o).contains("is not a tree sum"), "{}", text(&o));
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

/// ADR-0035 (owner ruling 2026-11-19): https is anonymous, so a repository
/// the host will not show without credentials is refused with the ssh form
/// to write instead.
#[test]
fn a_private_https_repository_is_refused_with_the_ssh_form() {
    let root = scratch("private");
    let host = GitHost::start(&root);
    host.answer_missing_with(401);
    let dir = root.join("app");
    app(&dir, "secret = { git = \"https://github.com/almide/secret-pkg.git\", tag = \"v0.1.0\" }\n", "import secret\neffect fn main() -> Unit = println(\"x\")\n");
    let o = almide(&dir, &root, Some(&host), &["check", "src/main.almd"]);
    assert!(!o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("fetch it over ssh: `git = \"git@github.com:almide/secret-pkg\"`"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&root);
}
