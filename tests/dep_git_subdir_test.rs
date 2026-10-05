//! A git dependency can name a package in a subdirectory of the repository
//! — #3381.
//!
//! `ceangal = { git = "…/ceangal2", tag = "v0.1.0", subdir = "ceangal" }`
//! selects the package directory inside the clone, for the fetch and for the
//! lock. Two packages of one repository at one ref share one clone (one
//! fetch) and are still two packages: two lock entries, two imports.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// A repository holding two packages, `ceangal/` and `snaidhm/`, and no
/// package at its root; tagged `v0.1.0`. Answers the tagged commit.
fn commit_monorepo(repo: &Path) -> String {
    std::fs::create_dir_all(repo).expect("mkdir");
    git(repo, &["init", "-q", "-b", "main"]);
    for pkg in ["ceangal", "snaidhm"] {
        let dir = repo.join(pkg).join("src");
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(
            repo.join(pkg).join("almide.toml"),
            format!("[package]\nname = \"{pkg}\"\nversion = \"0.1.0\"\n"),
        )
        .expect("write manifest");
        std::fs::write(dir.join("mod.almd"), format!("fn hello() -> String = \"{pkg}\"\n")).expect("write module");
    }
    std::fs::write(repo.join("README.md"), "two packages\n").expect("write readme");
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "packages"]);
    git(repo, &["tag", "v0.1.0"]);
    git(repo, &["rev-parse", "v0.1.0"])
}

fn dep(name: &str, repo: &Path, subdir: &str) -> String {
    format!("{name} = {{ git = \"file://{}\", tag = \"v0.1.0\", subdir = \"{subdir}\" }}", repo.display())
}

fn write_project(proj: &Path, deps: &[String], main: &str) {
    std::fs::create_dir_all(proj.join("src")).expect("mkdir");
    let _ = std::fs::remove_file(proj.join("almide.lock"));
    std::fs::write(
        proj.join("almide.toml"),
        format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{}\n", deps.join("\n")),
    )
    .expect("write manifest");
    std::fs::write(proj.join("src").join("main.almd"), main).expect("write entry");
}

fn almide(proj: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(almide_bin())
        .args(args)
        .current_dir(proj)
        .env("HOME", home)
        .output()
        .expect("spawn almide")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

/// The checkout directories under the shared clone root(s).
fn shared_checkouts(home: &Path) -> Vec<PathBuf> {
    let repos = home.join(".almide").join("cache").join(".repos");
    let mut out = Vec::new();
    for src in std::fs::read_dir(&repos).map(|r| r.flatten().collect::<Vec<_>>()).unwrap_or_default() {
        for co in std::fs::read_dir(src.path()).map(|r| r.flatten().collect::<Vec<_>>()).unwrap_or_default() {
            if !co.file_name().to_string_lossy().starts_with('.') {
                out.push(co.path());
            }
        }
    }
    out
}

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3381-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home")).expect("mkdir");
    root
}

const MAIN_BOTH: &str = "import io\nimport ceangal\nimport snaidhm\n\n\
     effect fn main() -> Unit = {\n  io.print(ceangal.hello() + snaidhm.hello())\n}\n";

#[test]
fn two_subdir_packages_of_one_repository_resolve_lock_and_share_a_clone() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = scratch("ok");
    let home = root.join("home");
    let repo = root.join("ceangal2");
    let commit = commit_monorepo(&repo);
    let proj = root.join("proj");
    write_project(&proj, &[dep("ceangal", &repo, "ceangal"), dep("snaidhm", &repo, "./snaidhm/")], MAIN_BOTH);

    let out = almide(&proj, &home, &["check", "src/main.almd"]);
    assert!(out.status.success(), "check should succeed:\n{}", stderr(&out));
    // One fetch for the two packages: they share the clone.
    let fetches = stderr(&out).matches("Fetching").count();
    assert_eq!(fetches, 1, "two packages of one repository at one ref must be fetched once:\n{}", stderr(&out));
    assert_eq!(shared_checkouts(&home).len(), 1, "one shared checkout: {:?}", shared_checkouts(&home));

    // The lock names each package exactly: source, ref, commit AND subdir
    // (normalized: `./snaidhm/` is recorded as `snaidhm`).
    let lock = almide::project::parse_lock_file(&proj.join("almide.lock")).expect("lock");
    assert_eq!(lock.len(), 2, "{lock:?}");
    for (name, sub) in [("ceangal", "ceangal"), ("snaidhm", "snaidhm")] {
        let e = lock.iter().find(|l| l.name == name).unwrap_or_else(|| panic!("no {name} entry: {lock:?}"));
        assert_eq!(e.subdir.as_deref(), Some(sub));
        assert_eq!(e.commit, commit);
        assert_eq!(e.ref_name, "v0.1.0");
    }

    // The locked re-run: still one checkout for both (the commit-keyed one).
    let out = almide(&proj, &home, &["check", "src/main.almd"]);
    assert!(out.status.success(), "the locked check should succeed:\n{}", stderr(&out));
    assert!(stderr(&out).matches("Fetching").count() <= 1, "{}", stderr(&out));
    assert_eq!(shared_checkouts(&home).len(), 2, "ref checkout + one commit checkout: {:?}", shared_checkouts(&home));

    // `dep-path` prints the package's own directory inside the clone.
    let out = almide(&proj, &home, &["dep-path", "snaidhm"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let printed = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(printed.ends_with("snaidhm/src"), "dep-path printed {printed}");
    assert!(Path::new(&printed).join("mod.almd").is_file(), "{printed}");

    // `deps` shows the subdir.
    let out = almide(&proj, &home, &["deps"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("(v0.1.0) subdir ceangal"), "{}", String::from_utf8_lossy(&out.stdout));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_subdir_that_is_not_this_package_is_refused_with_what_was_found() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = scratch("bad");
    let home = root.join("home");
    let repo = root.join("ceangal2");
    commit_monorepo(&repo);
    let proj = root.join("proj");
    let main_one = "import io\nimport ceangal\n\neffect fn main() -> Unit = {\n  io.print(ceangal.hello())\n}\n";
    let check = |deps: &[String]| {
        write_project(&proj, deps, main_one);
        let out = almide(&proj, &home, &["check", "src/main.almd"]);
        assert!(!out.status.success(), "check must fail for {deps:?}");
        stderr(&out)
    };

    // The subdir holds another package: the key is the import name.
    let e = check(&[dep("ceangal", &repo, "snaidhm")]);
    assert!(e.contains("almide.toml:6: dependency `ceangal` names subdir `snaidhm`, whose package is `snaidhm`"), "{e}");
    assert!(e.contains("rename the key to `snaidhm`"), "{e}");

    // A subdir the repository does not have lists the packages it does.
    let e = check(&[dep("ceangal", &repo, "pkgs/ceangal")]);
    assert!(e.contains("subdir `pkgs/ceangal` of dependency `ceangal` does not exist"), "{e}");
    assert!(e.contains("`ceangal`, `snaidhm`"), "{e}");

    // No subdir, and the root is no package: the issue's original report.
    let e = check(&[format!("ceangal = {{ git = \"file://{}\", tag = \"v0.1.0\" }}", repo.display())]);
    assert!(e.contains("needs a `subdir`") && e.contains("`subdir = \"ceangal\"`"), "{e}");

    // Refused before anything is fetched, on the line that writes it.
    let e = check(&[dep("ceangal", &repo, "../ceangal")]);
    assert!(e.contains("almide.toml:6: invalid `subdir = \"../ceangal\"`") && e.contains("`..` is not allowed"), "{e}");
    let e = check(&[dep("ceangal", &repo, "/abs")]);
    assert!(e.contains("it is an absolute path"), "{e}");
    let e = check(&["ceangal = { path = \"../ceangal2\", subdir = \"ceangal\" }".to_string()]);
    assert!(e.contains("`subdir` applies to git dependencies only"), "{e}");
    assert!(e.contains("write `path = \"../ceangal2/ceangal\"`"), "{e}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn add_with_subdir_writes_the_entry_only_after_the_package_resolves() {
    if !tools_available() {
        eprintln!("skipping: almide or git not available");
        return;
    }
    let root = scratch("add");
    let home = root.join("home");
    let repo = root.join("ceangal2");
    commit_monorepo(&repo);
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).expect("mkdir");
    let manifest = "[package]\nname = \"app\"\nversion = \"0.1.0\"\n";
    std::fs::write(proj.join("almide.toml"), manifest).expect("write");
    let url = format!("file://{}", repo.display());

    // A key that is not the subdir's package: refused, manifest untouched.
    let out = almide(&proj, &home, &["add", "cg", "--git", &url, "--tag", "v0.1.0", "--subdir", "ceangal"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("whose package is `ceangal`"), "{}", stderr(&out));
    assert_eq!(std::fs::read_to_string(proj.join("almide.toml")).unwrap(), manifest);

    let out = almide(&proj, &home, &["add", "ceangal", "--git", &url, "--tag", "v0.1.0", "--subdir", "ceangal/"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let written = std::fs::read_to_string(proj.join("almide.toml")).unwrap();
    assert!(written.contains(&format!("ceangal = {{ git = \"{url}\", tag = \"v0.1.0\", subdir = \"ceangal\" }}")), "{written}");

    let _ = std::fs::remove_dir_all(&root);
}

/// The manifest reader's half, without git: the normalized spelling and the
/// refusals, through the public parser every command uses.
#[test]
fn the_manifest_reader_normalizes_and_refuses_subdirs() {
    let dir = std::env::temp_dir().join(format!("almide-issue3381-parse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let toml = dir.join("almide.toml");
    let parse = |dep: &str| {
        std::fs::write(&toml, format!("[package]\nname = \"app\"\n\n[dependencies]\n{dep}\n")).unwrap();
        almide::project::parse_toml(&toml)
    };
    let p = parse("c = { git = \"https://x/r\", subdir = \"./pkgs//c/\" }").expect("parse");
    assert_eq!(p.dependencies[0].subdir.as_deref(), Some("pkgs/c"));
    let p = parse("c = { git = \"https://x/r\" }").expect("parse");
    assert_eq!(p.dependencies[0].subdir, None);
    for (bad, why) in [
        ("\"\"", "it is empty"),
        ("\".\"", "names the repository root"),
        ("\"a/../b\"", "`..` is not allowed"),
        ("\"a\\\\b\"", "uses `/` on every platform"),
        ("\"C:/x\"", "absolute path"),
        ("3", "must be a string"),
    ] {
        let e = parse(&format!("c = {{ git = \"https://x/r\", subdir = {bad} }}")).expect_err(bad);
        assert!(e.contains(":5:") && e.contains(why), "{bad}: {e}");
        // `check_manifest` (run before every command) refuses it the same way.
        let content = std::fs::read_to_string(&toml).unwrap();
        assert!(almide::project::check_manifest(&toml, &content).is_err(), "{bad}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
