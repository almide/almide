//! ADR-0035 D3 / D4 negative controls: a package is a tree of regular files,
//! and the lock's tree sum is checked on every use.
//!
//! - D3: a fetched tree holding a symlink, a submodule, a `..` or other
//!   out-of-grammar name, a bidi name, a case collision or an over-budget
//!   file is refused as a package-format error naming the entry, and nothing
//!   is written to the cache.
//! - D3: a link planted in a cache entry afterwards is refused.
//! - D4: a cached file changed after the fetch is refused (cache-only) or
//!   fetched again (online), and the changed content is never built.
//! - D4: a lock whose sum is not the content's is refused; `--locked`
//!   refuses a lock written before sums.
//!
//! Repositories are served by `support/git_host.rs` (no network).

#[path = "support/git_host.rs"]
mod git_host;

use git_host::{GitHost, git};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-pkgfmt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home")).expect("mkdir home");
    root
}

fn almide(dir: &Path, root: &Path, host: &GitHost, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.current_dir(dir).env("HOME", root.join("home")).args(args);
    host.env(&mut cmd);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn app(dir: &Path, url: &str) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("almide.toml"), format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nlib = {{ git = \"{url}\", tag = \"v0.1.0\" }}\n")).unwrap();
    std::fs::write(dir.join("src/main.almd"), "import lib\neffect fn main() -> Unit = println(lib.word())\n").unwrap();
}

const LIB_TOML: &str = "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n";

fn trees(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root.join("home/.almide/cache/trees"))
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| !p.file_name().unwrap().to_string_lossy().starts_with('.')).collect())
        .unwrap_or_default()
}

/// A valid package repository with `make` applied on top of its first
/// commit, served at a fresh URL, tagged v0.1.0. `make` either changes the
/// working tree (committed here) or makes its own commit (plumbing).
fn served_with(host: &GitHost, root: &Path, name: &str, make: impl FnOnce(&Path)) -> String {
    let dir = root.join("repos").join(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("almide.toml"), LIB_TOML).unwrap();
    std::fs::write(dir.join("src/mod.almd"), "fn word() -> String = \"ok\"\n").unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    let base = git(&dir, &["rev-parse", "HEAD"]);
    make(&dir);
    if git(&dir, &["rev-parse", "HEAD"]) == base {
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-qm", "c", "--allow-empty"]);
    }
    git(&dir, &["tag", "-f", "v0.1.0"]);
    let url = format!("https://github.com/almide/pkgfmt-{name}");
    host.serve(&url, &dir);
    url
}

/// A commit on top of HEAD whose tree adds `name` at the top level — a
/// one-byte blob with `mode`, or for mode 160000 a submodule commit that
/// does not exist — an entry git's porcelain will not create, written with
/// plumbing (`mktree --missing`, `commit-tree`).
fn plumb_entry(dir: &Path, mode: &str, name: &str) {
    use std::io::Write;
    let listing = git(dir, &["ls-tree", "HEAD"]);
    let tmp = dir.with_extension("blob");
    std::fs::write(&tmp, "x").unwrap();
    let blob = git(dir, &["hash-object", "-w", tmp.to_str().unwrap()]);
    std::fs::remove_file(&tmp).unwrap();
    let mut child = Command::new("git")
        .current_dir(dir)
        .args(["mktree", "--missing"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let entry = match mode {
        "160000" => format!("{mode} commit {}\t{name}", "1".repeat(40)),
        _ => format!("{mode} blob {blob}\t{name}"),
    };
    child.stdin.take().unwrap().write_all(format!("{listing}\n{entry}\n").as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "git mktree with `{name}`");
    let tree = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let commit = git(dir, &["commit-tree", &tree, "-p", "HEAD", "-m", "plumbed"]);
    git(dir, &["update-ref", "HEAD", &commit]);
}

/// D3: each refused tree is refused naming its entry, and the cache holds
/// no tree afterwards.
#[test]
fn a_tree_that_is_not_regular_files_in_the_grammar_is_refused_naming_the_entry() {
    let root = scratch("d3");
    let host = GitHost::start(&root);
    #[allow(clippy::type_complexity)]
    let mut cases: Vec<(&str, Box<dyn FnOnce(&Path)>, &str, &str)> = vec![
        ("submodule", Box::new(|d: &Path| plumb_entry(d, "160000", "vendor")), "vendor", "submodule"),
        ("bidi", Box::new(|d: &Path| std::fs::write(d.join("src/a\u{202E}b.almd"), "fn z() -> Int = 1\n").unwrap()), "src/a", "grammar"),
        ("case", Box::new(|d: &Path| {
            std::fs::write(d.join("README"), "a").unwrap();
            std::fs::write(d.join("readme"), "b").unwrap();
        }), "readme", "case"),
        ("big", Box::new(|d: &Path| std::fs::write(d.join("big.bin"), vec![0u8; 16 * 1024 * 1024 + 1]).unwrap()), "big.bin", "per file"),
        ("dotgit", Box::new(|d: &Path| plumb_entry(d, "100644", ".git")), ".git", "git metadata"),
        ("dotdot", Box::new(|d: &Path| plumb_entry(d, "100644", "..")), "..", "grammar"),
        ("dash", Box::new(|d: &Path| std::fs::write(d.join("-rf"), "x").unwrap()), "-rf", "grammar"),
    ];
    #[cfg(unix)]
    cases.push(("symlink", Box::new(|d: &Path| std::os::unix::fs::symlink("/etc/passwd", d.join("src/ext")).unwrap()), "src/ext", "symbolic link"));
    for (name, make, entry, why) in cases {
        let url = served_with(&host, &root, name, make);
        let dir = root.join(format!("app-{name}"));
        app(&dir, &url);
        let o = almide(&dir, &root, &host, &["check", "src/main.almd"], &[]);
        assert!(!o.status.success(), "{name}: the tree was accepted:\n{}", text(&o));
        assert!(text(&o).contains("package format") && text(&o).contains(entry) && text(&o).contains(why), "{name}:\n{}", text(&o));
        assert!(trees(&root).is_empty(), "{name}: a refused tree was written to the cache: {:?}", trees(&root));
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// D4 (and D3 for a planted link): a cache entry that no longer hashes to
/// its sum is refused in cache-only mode, and fetched again online — the
/// changed content is never what is built.
#[test]
fn a_changed_cache_entry_is_refused_and_never_built() {
    let root = scratch("d4");
    let host = GitHost::start(&root);
    let url = served_with(&host, &root, "good", |_| {});
    let dir = root.join("app");
    app(&dir, &url);
    let run = |env: &[(&str, &str)]| almide(&dir, &root, &host, &["run", "src/main.almd", "--target", "wasm"], env);
    let o = run(&[]);
    assert!(o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "ok", "{}", text(&o));
    let lock = std::fs::read_to_string(dir.join("almide.lock")).unwrap();
    assert!(lock.contains("sum = \"h1:"), "the lock has no sum:\n{lock}");
    let all = trees(&root);
    let [tree] = &all[..] else { panic!("one tree expected: {:?}", trees(&root)) };
    let tree = tree.clone();

    let tampered = "fn word() -> String = \"TAMPERED\"\n";
    let changes: Vec<(&str, Box<dyn Fn(&Path)>)> = vec![
        ("an edited file", Box::new(move |t: &Path| std::fs::write(t.join("src/mod.almd"), tampered).unwrap())),
        ("an added file", Box::new(|t: &Path| std::fs::write(t.join("src/extra.almd"), "fn e() -> Int = 1\n").unwrap())),
        ("a removed file", Box::new(|t: &Path| std::fs::remove_file(t.join("almide.toml")).unwrap())),
        #[cfg(unix)]
        ("a planted link", Box::new(|t: &Path| std::os::unix::fs::symlink("/", t.join("src/ext")).unwrap())),
    ];
    for (what, change) in changes {
        change(&tree);
        let o = run(&[("ALMIDE_DEPS_CACHE_ONLY", "1")]);
        assert!(!o.status.success(), "{what}: cache-only use of a changed entry succeeded:\n{}", text(&o));
        assert!(text(&o).contains("does not match its sum"), "{what}:\n{}", text(&o));
        assert!(!String::from_utf8_lossy(&o.stdout).contains("TAMPERED"));
        let before = host.requests();
        let o = run(&[]);
        assert!(o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "ok", "{what}: online use did not refetch:\n{}", text(&o));
        assert!(text(&o).contains("fetching it again") && host.requests() > before, "{what}:\n{}", text(&o));
        // Back to a clean entry: a third use is a pure cache hit.
        let before = host.requests();
        let o = run(&[]);
        assert!(o.status.success() && host.requests() == before, "{what}: a clean entry was fetched again:\n{}", text(&o));
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// D4: the lock's sum is checked against fetched content (a remote that
/// serves other content for the locked commit is refused), and `--locked`
/// refuses a lock written before sums, while an unlocked build adds them.
#[test]
fn the_lock_sum_is_enforced_and_locked_needs_one() {
    let root = scratch("d4b");
    let host = GitHost::start(&root);
    let url = served_with(&host, &root, "locked", |_| {});
    let dir = root.join("app");
    app(&dir, &url);
    let run = |args: &[&str]| almide(&dir, &root, &host, args, &[]);
    let o = run(&["run", "src/main.almd", "--target", "wasm"]);
    assert!(o.status.success(), "{}", text(&o));
    let lock = std::fs::read_to_string(dir.join("almide.lock")).unwrap();
    let sum_at = lock.find("sum = \"h1:").unwrap() + 10;
    let sum = &lock[sum_at..sum_at + 44];

    // Another valid sum: the remote's (unchanged) content no longer matches.
    let other = if sum.starts_with('A') { format!("B{}", &sum[1..]) } else { format!("A{}", &sum[1..]) };
    let wrong = lock.replace(sum, &other);
    std::fs::write(dir.join("almide.lock"), &wrong).unwrap();
    let _ = std::fs::remove_dir_all(root.join("home/.almide/cache"));
    let o = run(&["run", "src/main.almd", "--target", "wasm"]);
    assert!(!o.status.success() && text(&o).contains("but almide.lock records"), "{}", text(&o));
    assert!(trees(&root).is_empty(), "content that failed the lock's sum was cached");

    // A lock without sums: --locked refuses it, a plain build writes them.
    let old = lock.replace(&format!(", sum = \"h1:{sum}\""), "");
    assert!(!old.contains("sum ="), "{old}");
    std::fs::write(dir.join("almide.lock"), &old).unwrap();
    let o = run(&["run", "--locked", "src/main.almd", "--target", "wasm"]);
    assert!(!o.status.success() && text(&o).contains("has no `sum`"), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(dir.join("almide.lock")).unwrap(), old, "--locked wrote the lock");
    let o = run(&["run", "src/main.almd", "--target", "wasm"]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(dir.join("almide.lock")).unwrap(), lock, "read old, write new");
    let o = run(&["run", "--locked", "src/main.almd", "--target", "wasm"]);
    assert!(o.status.success(), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&root);
}
