//! #2405: the parity manifest generators refuse to record goldens from a tree
//! the rows would not reproduce on — and nothing is written when they do.
//!
//! The three generators (`scripts/gen-{ast,check,run}-manifest.sh`) wrote
//! happily from any tree: an ORACLE older than the sources, a released binary
//! off PATH, a worktree 22 commits behind develop with two emitter changes in
//! between, an untracked fixture. The only trace was the `# oracle:` header,
//! which the gate deliberately skips. These tests forge each of those trees
//! and read the refusal text, the way a developer meets it.
//!
//! Two kinds of fixture. The BINARY forgeries run the real generators in this
//! repository with a fake `almide` (a shell script answering `--version`),
//! and assert the committed goldens are byte-identical afterwards — the
//! refusal must come before the generator truncates its outputs. The TREE
//! forgeries (an untracked fixture, a branch behind its upstream) cannot be
//! staged in this repository without disturbing it, so they run the shared
//! check (`scripts/lib/oracle-header.sh`, `refuse_stale_tree`) in a
//! throwaway git repository built to shape.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    almide_corpus::workspace_root(env!("CARGO_MANIFEST_DIR"))
}

const GOLDENS: &[&str] = &[
    "crates/almide-syntax/tests/golden/spec-ast-manifest.txt",
    "crates/almide-syntax/tests/golden/spec-ast-exclusions.txt",
    "crates/almide-spine/tests/golden/spec-check-manifest.txt",
    "crates/almide-spine/tests/golden/spec-check-exclusions.txt",
    "crates/almide-spine/tests/golden/spec-run-manifest.txt",
    "crates/almide-spine/tests/golden/spec-run-exclusions.txt",
];

const GENERATORS: &[&str] = &["gen-ast-manifest", "gen-check-manifest", "gen-run-manifest"];

/// A fake `almide` that answers `--version` with `line` and nothing else.
fn fake_oracle(at: &Path, line: &str) -> PathBuf {
    fs::create_dir_all(at.parent().unwrap()).unwrap();
    fs::write(at, format!("#!/bin/sh\n[ \"$1\" = --version ] && echo '{line}'\n")).unwrap();
    fs::set_permissions(at, fs::Permissions::from_mode(0o755)).unwrap();
    at.to_path_buf()
}

fn snapshot(root: &Path) -> Vec<Vec<u8>> {
    GOLDENS.iter().map(|g| fs::read(root.join(g)).unwrap()).collect()
}

struct Run {
    code: Option<i32>,
    err: String,
    out: String,
}

fn run_generator(generator: &str, oracle: &Path, mode: Option<&str>) -> Run {
    let mut cmd = Command::new("bash");
    cmd.arg(format!("scripts/{generator}.sh")).current_dir(root()).env("ORACLE", oracle);
    match mode {
        Some(m) => cmd.env("ALMIDE_MANIFEST_TREE_CHECK", m),
        None => cmd.env_remove("ALMIDE_MANIFEST_TREE_CHECK"),
    };
    let o = cmd.output().expect("spawn bash");
    Run {
        code: o.status.code(),
        err: String::from_utf8_lossy(&o.stderr).into_owned(),
        out: String::from_utf8_lossy(&o.stdout).into_owned(),
    }
}

/// Every generator refuses with `needle` in its refusal, and the goldens are
/// untouched afterwards.
fn every_generator_refuses(oracle: &Path, needle: &str) {
    let root = root();
    let before = snapshot(&root);
    for generator in GENERATORS {
        let r = run_generator(generator, oracle, None);
        assert_eq!(r.code, Some(1), "{generator}: must refuse (exit 1); stderr:\n{}\nstdout:\n{}", r.err, r.out);
        assert!(r.err.contains(needle), "{generator}: refusal must say `{needle}`; stderr:\n{}", r.err);
        assert!(
            r.err.contains("refusing to record goldens from this tree (#2405); nothing was written"),
            "{generator}: the refusal must say nothing was written; stderr:\n{}",
            r.err
        );
        assert!(!r.out.contains("manifest:"), "{generator}: must not reach the summary line; stdout:\n{}", r.out);
    }
    assert_eq!(before, snapshot(&root), "a refused generator changed a committed golden");
}

fn tree_version() -> String {
    almide_corpus::tree_version(&root())
}

#[test]
fn a_released_binary_is_refused_by_every_generator() {
    let dir = tempfile::tempdir().unwrap();
    let oracle = fake_oracle(&dir.path().join("almide"), &format!("almide {} (release, 000000000)", tree_version()));
    every_generator_refuses(&oracle, "a release binary comes from the release workflow, never from this tree");
}

#[test]
fn a_binary_of_another_version_is_refused_by_every_generator() {
    let dir = tempfile::tempdir().unwrap();
    let oracle = fake_oracle(&dir.path().join("almide"), "almide 0.0.1 (dev)");
    every_generator_refuses(&oracle, &format!("ORACLE is `almide 0.0.1 (dev)` but this tree is version {}", tree_version()));
}

#[test]
fn a_binary_stamped_with_another_commit_is_refused_by_every_generator() {
    let dir = tempfile::tempdir().unwrap();
    let oracle = fake_oracle(&dir.path().join("almide"), &format!("almide {} (dev, 000000000)", tree_version()));
    every_generator_refuses(&oracle, "ORACLE was built at 000000000");
}

#[test]
fn an_unstamped_binary_from_outside_this_tree_is_refused_by_every_generator() {
    let dir = tempfile::tempdir().unwrap();
    let oracle = fake_oracle(&dir.path().join("almide"), &format!("almide {} (dev)", tree_version()));
    every_generator_refuses(&oracle, "carries no build sha");
}

#[test]
fn an_unstamped_binary_older_than_the_sources_is_refused_by_every_generator() {
    // Built in this tree (under target/), unstamped, and dated before every
    // source: the mtime fallback — what `cargo build` itself keys on.
    let at = root().join(format!("target/almide-corpus-stale-oracle-{}/almide", std::process::id()));
    let oracle = fake_oracle(&at, &format!("almide {} (dev)", tree_version()));
    let touched = Command::new("touch").args(["-t", "200001010000"]).arg(&oracle).status().unwrap();
    assert!(touched.success());
    every_generator_refuses(&oracle, "ORACLE predates the sources");
    fs::remove_dir_all(at.parent().unwrap()).ok();
}

#[test]
fn the_override_is_explicit_and_says_so() {
    // `off` records anyway (a deliberate act) and leaves a warning as the
    // trace; a misspelled mode is not an override.
    let dir = tempfile::tempdir().unwrap();
    let oracle = fake_oracle(&dir.path().join("almide"), "almide 0.0.1 (dev)");
    let r = run_generator("gen-ast-manifest", &oracle, Some("of"));
    assert_eq!(r.code, Some(2), "a misspelled mode must not pass; stderr:\n{}", r.err);
    assert!(r.err.contains("ALMIDE_MANIFEST_TREE_CHECK=of: expected strict, gate or off"), "{}", r.err);
    // Under `off` a generator would proceed past the check and truncate the
    // committed goldens (the fake oracle emits nothing), so the override is
    // asserted on the shared check alone: it passes, and it says so.
    let root = root();
    let o = Command::new("bash")
        .args(["-c", ". scripts/lib/oracle-header.sh; refuse_stale_tree; echo rc=$?"])
        .current_dir(&root)
        .env("ORACLE", &oracle)
        .env("ALMIDE_MANIFEST_TREE_CHECK", "off")
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(out.contains("rc=0"), "off must pass the check; stdout:\n{out}\nstderr:\n{err}");
    assert!(err.contains("::warning::ALMIDE_MANIFEST_TREE_CHECK=off"), "off must leave a trace; stderr:\n{err}");
}

// ── tree forgeries: a throwaway repository built to shape ───────────────────

/// A git repository with the layout the check reads: a root Cargo.toml whose
/// `[workspace.package]` version differs from `[package]`'s (the trap), a
/// committed spec fixture, a compiler source, the shared library copied in,
/// and a fake oracle built in-tree (under target/) after every source.
struct Forge {
    _dir: tempfile::TempDir,
    root: PathBuf,
    oracle: PathBuf,
}

impl Forge {
    fn new() -> Forge {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("scripts/lib")).unwrap();
        fs::copy(self::root().join("scripts/lib/oracle-header.sh"), root.join("scripts/lib/oracle-header.sh")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.12.2\"\n\n[package]\nname = \"almide\"\nversion = \"9.9.9\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("spec/lang")).unwrap();
        fs::write(root.join("spec/lang/committed.almd"), "fn f() -> Int = 1\n").unwrap();
        fs::create_dir_all(root.join("crates/almide-wasm/src")).unwrap();
        fs::write(root.join("crates/almide-wasm/src/emit.rs"), "// emitter\n").unwrap();
        fs::write(root.join(".gitignore"), "target/\n").unwrap();
        let f = Forge { _dir: dir, root: root.clone(), oracle: PathBuf::new() };
        f.git(&["init", "-q", "-b", "main"]);
        f.git(&["add", "-A"]);
        f.git(&["commit", "-q", "-m", "seed"]);
        let oracle = fake_oracle(&root.join("target/release/almide"), "almide 9.9.9 (dev)");
        // Strictly newer than every source: `find -newer` compares whole seconds.
        let t = Command::new("touch").args(["-t", "203001010000"]).arg(&oracle).status().unwrap();
        assert!(t.success());
        Forge { oracle, ..f }
    }

    fn git(&self, args: &[&str]) -> String {
        let o = Command::new("git")
            .args(["-c", "user.name=forge", "-c", "user.email=forge@example.invalid", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    fn check(&self, mode: Option<&str>) -> Run {
        let mut cmd = Command::new("bash");
        cmd.args(["-c", ". scripts/lib/oracle-header.sh; refuse_stale_tree"])
            .current_dir(&self.root)
            .env("ORACLE", &self.oracle);
        match mode {
            Some(m) => cmd.env("ALMIDE_MANIFEST_TREE_CHECK", m),
            None => cmd.env_remove("ALMIDE_MANIFEST_TREE_CHECK"),
        };
        let o = cmd.output().unwrap();
        Run {
            code: o.status.code(),
            err: String::from_utf8_lossy(&o.stderr).into_owned(),
            out: String::from_utf8_lossy(&o.stdout).into_owned(),
        }
    }
}

#[test]
fn a_clean_tree_with_its_own_fresh_binary_passes() {
    let f = Forge::new();
    let r = f.check(None);
    assert_eq!(r.code, Some(0), "stderr:\n{}\nstdout:\n{}", r.err, r.out);
    assert!(r.err.is_empty(), "a passing check says nothing; stderr:\n{}", r.err);
}

#[test]
fn the_tree_version_is_the_package_one_not_the_workspace_one() {
    let f = Forge::new();
    let o = Command::new("bash")
        .args(["-c", ". scripts/lib/oracle-header.sh; tree_version"])
        .current_dir(&f.root)
        .env("ORACLE", &f.oracle)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "9.9.9");
    // …and the Rust reader agrees.
    assert_eq!(almide_corpus::tree_version(&f.root), "9.9.9");
}

#[test]
fn an_untracked_fixture_is_refused_until_it_is_added() {
    let f = Forge::new();
    fs::write(f.root.join("spec/lang/forgotten.almd"), "fn g() -> Int = 2\n").unwrap();
    let r = f.check(None);
    assert_eq!(r.code, Some(1), "stderr:\n{}", r.err);
    assert!(r.err.contains("untracked .almd fixture(s) under spec/"), "{}", r.err);
    assert!(r.err.contains("  spec/lang/forgotten.almd"), "the fixture is named; stderr:\n{}", r.err);
    // The gate keeps this check even while vouching for the binary.
    let g = f.check(Some("gate"));
    assert_eq!(g.code, Some(1), "gate mode must still refuse an untracked fixture; stderr:\n{}", g.err);
    assert!(g.err.contains("spec/lang/forgotten.almd"), "{}", g.err);
    f.git(&["add", "spec/lang/forgotten.almd"]);
    let r = f.check(None);
    assert_eq!(r.code, Some(0), "staged is enough; stderr:\n{}", r.err);
}

#[test]
fn a_worktree_behind_its_upstream_is_refused_and_the_emitter_commits_are_named() {
    let f = Forge::new();
    f.git(&["checkout", "-q", "-b", "fix"]);
    f.git(&["checkout", "-q", "main"]);
    fs::write(f.root.join("crates/almide-wasm/src/emit.rs"), "// emitter, changed\n").unwrap();
    f.git(&["commit", "-q", "-am", "Change the emitter"]);
    fs::write(f.root.join("README.md"), "prose\n").unwrap();
    f.git(&["add", "README.md"]);
    f.git(&["commit", "-q", "-m", "Prose only"]);
    f.git(&["checkout", "-q", "fix"]);
    f.git(&["branch", "-q", "--set-upstream-to=main"]);
    // The checkout back to `fix` restored emit.rs with a fresh mtime; the
    // oracle is dated 2030, so the binary check stays quiet and the refusal
    // is the tree's alone.
    let r = f.check(None);
    assert_eq!(r.code, Some(1), "stderr:\n{}", r.err);
    assert!(r.err.contains("this worktree is 2 commit(s) behind main"), "{}", r.err);
    assert!(r.err.contains("1 of them touch compiler sources:"), "{}", r.err);
    assert!(r.err.contains("Change the emitter"), "the emitter commit is named; stderr:\n{}", r.err);
    assert!(!r.err.contains("Prose only"), "a prose-only commit is not named as an emitter change; stderr:\n{}", r.err);
    // The gate vouches for the binary and judges a detached commit: no upstream
    // comparison there.
    let g = f.check(Some("gate"));
    assert_eq!(g.code, Some(0), "gate mode does not compare against upstream; stderr:\n{}", g.err);
}

#[test]
fn a_stale_in_tree_binary_is_refused_and_the_newer_source_is_named() {
    let f = Forge::new();
    let t = Command::new("touch").args(["-t", "200001010000"]).arg(&f.oracle).status().unwrap();
    assert!(t.success());
    let r = f.check(None);
    assert_eq!(r.code, Some(1), "stderr:\n{}", r.err);
    assert!(r.err.contains("ORACLE predates the sources"), "{}", r.err);
    assert!(r.err.contains("crates/almide-wasm/src/emit.rs"), "the newer source is named; stderr:\n{}", r.err);
}

#[test]
fn the_committed_header_is_verified_before_a_gate_regenerates_over_it() {
    let f = Forge::new();
    let m = f.root.join("manifest.txt");
    let verify = |text: &str| -> Run {
        fs::write(&m, text).unwrap();
        let o = Command::new("bash")
            .args(["-c", ". scripts/lib/oracle-header.sh; verify_committed_header manifest.txt"])
            .current_dir(&f.root)
            .env("ORACLE", &f.oracle)
            .output()
            .unwrap();
        Run {
            code: o.status.code(),
            err: String::from_utf8_lossy(&o.stderr).into_owned(),
            out: String::from_utf8_lossy(&o.stdout).into_owned(),
        }
    };
    assert_eq!(verify("# oracle: almide 9.9.9 (dev) at 000000000 — x\nrow\n").code, Some(0));
    assert_eq!(verify("# oracle: almide 9.9.9 (dev, 000000000) at 000000000 — x\nrow\n").code, Some(0));
    let r = verify("# oracle: almide 9.9.8 (dev) at 000000000 — x\nrow\n");
    assert_eq!(r.code, Some(1));
    assert!(r.err.contains("does not name the CLI built from this tree (almide 9.9.9 (dev…))"), "{}", r.err);
    let r = verify("# oracle: almide 9.9.9 (release, 000000000) at 000000000 — x\nrow\n");
    assert_eq!(r.code, Some(1), "a release binary's rows are refused; stderr:\n{}", r.err);
    let r = verify("row\n");
    assert_eq!(r.code, Some(1), "no header at all is refused; stderr:\n{}", r.err);
    // The Rust readers apply the same predicate.
    assert!(almide_corpus::verify_oracle_header(&f.root, "# oracle: almide 9.9.9 (dev) at 0 — x\n").is_ok());
    assert!(almide_corpus::verify_oracle_header(&f.root, "# oracle: almide 9.9.8 (dev) at 0 — x\n").is_err());
    assert!(almide_corpus::verify_oracle_header(&f.root, "# oracle: almide 9.9.9 (release, 0) at 0 — x\n").is_err());
}
