//! A key written twice in `almide.toml` is refused where it is written — #2583.
//!
//! The manifest reader is line-based, so it accepted
//!
//! ```toml
//! [dependencies]
//! almai = { git = "https://github.com/almide/almai" }
//! almai = { git = "https://github.com/almide/almai.git" }
//! ```
//!
//! and kept BOTH dependencies. The lock writer then recorded `almai` twice,
//! and every later run died on the lock — a generated file — with a bare TOML
//! "duplicate key", so one duplicated manifest line bricked the project (it
//! broke almide-dojo on the next compiler). TOML forbids a key twice in one
//! table; the manifest now says so, naming both lines, before anything is
//! fetched or written. The lock writer never emits a name twice, and a lock an
//! affected compiler already wrote gets a message saying what to do.

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

/// A scratch root unique to this test, emptied first.
fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue2583-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home")).expect("mkdir");
    root
}

/// A package `dup_lib` exporting one function, committed and tagged `v1.0.0`.
/// Answers the repository and the tagged commit.
fn commit_package(root: &Path) -> (PathBuf, String) {
    let repo = root.join("dup_lib");
    std::fs::create_dir_all(repo.join("src")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("almide.toml"), "[package]\nname = \"dup_lib\"\nversion = \"0.1.0\"\n")
        .expect("write manifest");
    std::fs::write(repo.join("src").join("mod.almd"), "fn hello() -> String = \"hello\"\n")
        .expect("write module");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "dup_lib"]);
    git(&repo, &["tag", "v1.0.0"]);
    let commit = git(&repo, &["rev-parse", "v1.0.0"]);
    (repo, commit)
}

/// A consumer project whose `[dependencies]` holds `dep_lines` verbatim.
fn write_project(root: &Path, dep_lines: &[String]) -> PathBuf {
    let proj = root.join("proj");
    std::fs::create_dir_all(proj.join("src")).expect("mkdir");
    std::fs::write(
        proj.join("almide.toml"),
        format!(
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\n\n[dependencies]\n{}\n",
            dep_lines.join("\n")
        ),
    )
    .expect("write manifest");
    std::fs::write(
        proj.join("src").join("main.almd"),
        "import io\nimport dup_lib\n\neffect fn main() -> Unit = {\n  io.print(dup_lib.hello())\n}\n",
    )
    .expect("write entry");
    proj
}

fn run(proj: &Path, home: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(almide_bin())
        .args(args)
        .current_dir(proj)
        .env("HOME", home)
        .output()
        .expect("spawn almide");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

/// (a) The dojo shape: one dependency declared twice under two spellings of
/// the same url. Refused on the manifest's own line, before any lock exists.
#[test]
fn a_dependency_declared_twice_is_refused_on_the_manifest_line() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = scratch("manifest");
    let home = root.join("home");
    let (lib, _) = commit_package(&root);
    // Lines 6 and 7 of the manifest (after the 3-line [package], a blank, the header).
    let proj = write_project(
        &root,
        &[
            format!("dup_lib = {{ git = \"file://{}\", tag = \"v1.0.0\" }}", lib.display()),
            format!("dup_lib = {{ git = \"file://{}/\", tag = \"v1.0.0\" }}", lib.display()),
        ],
    );

    for args in [&["check", "src/main.almd"][..], &["run", "src/main.almd"][..], &["test"][..]] {
        let (ok, stderr) = run(&proj, &home, args);
        assert!(!ok, "`almide {}` accepted a manifest declaring dup_lib twice:\n{stderr}", args.join(" "));
        assert!(
            stderr.contains("almide.toml:7")
                && stderr.contains("dependency `dup_lib` is declared twice")
                && stderr.contains("first at line 6")
                && stderr.contains("keep one of lines 6 and 7"),
            "`almide {}` should name both manifest lines and the fix:\n{stderr}",
            args.join(" ")
        );
        assert!(
            !proj.join("almide.lock").exists(),
            "`almide {}` wrote a lock from a manifest it should have refused:\n{}",
            args.join(" "),
            std::fs::read_to_string(proj.join("almide.lock")).unwrap_or_default()
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// (b) A lock an affected compiler already wrote: the manifest is fixed, but
/// the lock still holds the entry twice. The refusal says which lines and
/// what to do rather than a bare "duplicate key".
#[test]
fn a_lock_holding_an_entry_twice_says_what_to_do() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = scratch("lock");
    let home = root.join("home");
    let (lib, commit) = commit_package(&root);
    let url = format!("file://{}", lib.display());
    let proj = write_project(&root, &[format!("dup_lib = {{ git = \"{url}\", tag = \"v1.0.0\" }}")]);
    std::fs::write(
        proj.join("almide.lock"),
        format!(
            "# almide.lock — auto-generated, do not edit\n\n\
             dup_lib = {{ git = \"{url}\", ref = \"v1.0.0\", commit = \"{commit}\" }}\n\
             dup_lib = {{ git = \"{url}/\", ref = \"v1.0.0\", commit = \"{commit}\" }}\n"
        ),
    )
    .expect("write lock");

    let (ok, stderr) = run(&proj, &home, &["check", "src/main.almd"]);
    assert!(!ok, "a lock holding dup_lib twice was accepted:\n{stderr}");
    assert!(
        stderr.contains("almide.lock:4")
            && stderr.contains("lock entry `dup_lib` appears twice")
            && stderr.contains("first at line 3")
            && stderr.contains("delete one of lines 3 and 4")
            && stderr.contains("or delete"),
        "the refusal should name both lock lines and the way out:\n{stderr}"
    );

    // Following the advice works: drop the lock, and the next run rewrites it
    // with one entry.
    std::fs::remove_file(proj.join("almide.lock")).expect("rm lock");
    let (ok, stderr) = run(&proj, &home, &["check", "src/main.almd"]);
    assert!(ok, "check after deleting the lock should succeed:\n{stderr}");
    let entries = almide::project::parse_lock_file(&proj.join("almide.lock")).expect("parse lock");
    assert_eq!(entries.len(), 1, "{entries:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// (c) A normal project is unaffected: one declaration, one lock entry, and
/// the second run reads the lock the first one wrote.
#[test]
fn a_dependency_declared_once_still_checks_and_locks_once() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = scratch("normal");
    let home = root.join("home");
    let (lib, commit) = commit_package(&root);
    let proj = write_project(
        &root,
        &[format!("dup_lib = {{ git = \"file://{}\", tag = \"v1.0.0\" }}", lib.display())],
    );
    for _ in 0..2 {
        let (ok, stderr) = run(&proj, &home, &["check", "src/main.almd"]);
        assert!(ok, "a single declaration should check:\n{stderr}");
    }
    let entries = almide::project::parse_lock_file(&proj.join("almide.lock")).expect("parse lock");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].commit, commit);
    let _ = std::fs::remove_dir_all(&root);
}

/// The writer half, without a network: handed the same name twice, the lock
/// still carries it once and reads back.
#[test]
fn the_lock_writer_never_emits_a_name_twice() {
    let dir = std::env::temp_dir().join(format!("almide-issue2583-writer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let lock = dir.join("almide.lock");
    let dep = |git: &str| almide::project::LockedDep {
        name: "almai".into(),
        git: git.into(),
        ref_name: "main".into(),
        commit: "0123456789abcdef".into(),
    };
    almide::project::write_lock_file(
        &lock,
        &[dep("https://github.com/almide/almai"), dep("https://github.com/almide/almai.git")],
    )
    .expect("write lock");
    let entries = almide::project::parse_lock_file(&lock).expect("the lock must read back");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].git, "https://github.com/almide/almai", "the first declaration wins");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every table the manifest reader reads is held to the rule, a repeated
/// table header is one too, and a multi-line value is not mistaken for keys.
#[test]
fn the_manifest_reader_refuses_every_duplicate_it_reads() {
    let dir = std::env::temp_dir().join(format!("almide-issue2583-tables-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("almide.toml");
    let parse = |text: &str| {
        std::fs::write(&path, text).expect("write");
        almide::project::parse_toml(&path)
    };

    let e = parse("[package]\nname = \"a\"\nname = \"b\"\n").err().expect("duplicate name");
    assert!(e.contains(":3: key `name` is declared twice in [package] (first at line 2)"), "{e}");

    let e = parse("[package]\nname = \"a\"\n\n[permissions]\nallow = [\"IO\"]\nallow = [\"Net\"]\n")
        .err()
        .expect("duplicate allow");
    assert!(e.contains("key `allow` is declared twice in [permissions]"), "{e}");

    let e = parse("[package]\nname = \"a\"\n\n[native-deps]\nfoo = \"1\"\n\"foo\" = \"2\"\n")
        .err()
        .expect("duplicate native dep, bare and quoted");
    assert!(e.contains("key `foo` is declared twice in [native-deps]"), "{e}");

    let e = parse(
        "[package]\nname = \"a\"\n\n[dependencies]\nx = { path = \"../x\" }\n\n[dependencies]\ny = { path = \"../y\" }\n",
    )
    .err()
    .expect("duplicate header");
    assert!(e.contains(":7: table [dependencies] is declared twice (first at line 4)"), "{e}");

    // The same key in two DIFFERENT tables is fine, as is a multi-line array
    // whose items contain `=`, and a table the reader never reads.
    let ok = parse(
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\n\n[native-deps]\nversion = \"1\"\n\n\
         [permissions]\nallow = [\n  \"IO\",\n  \"a=b\",\n  \"a=b\",\n]\n\n[tool]\nx = 1\nx = 2\n",
    );
    assert!(ok.is_ok(), "{:?}", ok.err());
    let _ = std::fs::remove_dir_all(&dir);
}
