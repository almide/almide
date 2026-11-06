//! A file that imports only `self.<sub>` of its own package, where `<sub>`
//! itself writes a bare `import self` (#4551).
//!
//! The resolver skipped a submodule's bare `import self`, so the package root
//! (`src/mod.almd`) was never loaded unless the entry file imported it too.
//! With no root loaded, the submodule's import table bound its `self` alias to
//! the submodule itself: `pkg.g()` resolved to the submodule's own `g`, and the
//! forwarder `local fn g() -> Int = pkg.g()` compiled to a call to itself.
//! `almide check` said "No errors found"; `almide test` and `almide run` spun
//! silently until the CI timeout, on both the native and the wasm route.
//!
//! Every run here is under a per-run deadline, so a hang fails the test with
//! a message instead of stalling CI.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

fn available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

/// A package `selfsub` whose submodule forwards to the root through
/// `import self as selfsub`. `root_imports_sub` makes the root import the
/// submodule back, the shape whose load order must not read as a cycle.
fn package(tag: &str, with_root: bool, root_imports_sub: bool) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4551-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(root.join("almide.toml"), "[package]\nname = \"selfsub\"\nversion = \"0.1.0\"\n");
    if with_root {
        let back = if root_imports_sub { "import self.sub\n\nfn via_sub() -> Int = sub.f() + 1\n\n" } else { "" };
        write(root.join("src/mod.almd"), &format!("{back}fn g() -> Int = 42\n"));
    }
    write(
        root.join("src/sub.almd"),
        "import self as selfsub\n\nlocal fn g() -> Int = selfsub.g()\n\nfn f() -> Int = g()\n",
    );
    write(
        root.join("ci/use_test.almd"),
        "import self.sub\n\ntest \"the submodule reaches the package root\" {\n  assert_eq(sub.f(), 42)\n}\n",
    );
    write(
        root.join("ci/main.almd"),
        "import self.sub\n\neffect fn main() -> Unit = {\n  println(int.to_string(sub.f()))\n}\n",
    );
    root
}

/// Run `almide <args>` in `root` with a deadline: (success, stdout+stderr).
/// A run past the deadline is killed and fails the test as the #4551 hang.
fn run(root: &Path, args: &[&str], deadline: Duration) -> (bool, String) {
    let mut child = Command::new(almide_bin())
        .current_dir(root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide");
    // Drained on their own threads, so a chatty child never blocks on a full
    // pipe and reads as a hang.
    let drain = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    };
    let out = drain(Box::new(child.stdout.take().unwrap()));
    let err = drain(Box::new(child.stderr.take().unwrap()));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait almide") {
            break status;
        }
        if start.elapsed() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("`almide {}` did not finish in {:?}: the #4551 hang", args.join(" "), deadline);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let text = out.join().unwrap_or_default() + &err.join().unwrap_or_default();
    (status.success(), text)
}

const DEADLINE: Duration = Duration::from_secs(300);

#[test]
fn a_file_importing_only_a_submodule_reaches_the_root_through_it() {
    if !available() {
        return;
    }
    let root = package("plain", true, false);
    let (ok, out) = run(&root, &["test", "ci/use_test.almd"], DEADLINE);
    assert!(ok, "almide test failed:\n{out}");
    let (ok, out) = run(&root, &["run", "ci/main.almd"], DEADLINE);
    assert!(ok && out.contains("42"), "native run did not print 42:\n{out}");
    let (ok, out) = run(&root, &["run", "ci/main.almd", "--target", "wasm"], DEADLINE);
    assert!(ok && out.contains("42"), "wasm run did not print 42:\n{out}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_root_that_imports_the_submodule_back_is_not_a_cycle() {
    if !available() {
        return;
    }
    let root = package("mutual", true, true);
    let (ok, out) = run(&root, &["test", "ci/use_test.almd"], DEADLINE);
    assert!(ok, "almide test failed:\n{out}");
    assert!(!out.contains("circular import"), "deferred root load read as a cycle:\n{out}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_bare_import_self_without_a_package_root_is_a_diagnostic() {
    if !available() {
        return;
    }
    let root = package("noroot", false, false);
    for args in [&["check", "ci/use_test.almd"][..], &["run", "ci/main.almd"][..]] {
        let (ok, out) = run(&root, args, DEADLINE);
        assert!(!ok, "`almide {}` accepted a `self` with no package root:\n{out}", args.join(" "));
        assert!(out.contains("src/mod.almd"), "the diagnostic does not name src/mod.almd:\n{out}");
        assert!(out.contains("self.sub"), "the diagnostic does not name the importing module:\n{out}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
