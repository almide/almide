//! #4022: file access bounded by directory, on both targets.
//!
//! - The rooted family (`fs.open_root`, `fs.root_*`, C-392) refuses a path
//!   that leaves its root through a symlink anywhere along it with the same
//!   `fs: path escapes root: <path>` on native and on the embedded wasm host.
//!   The stdlib cannot make a symlink, so the tree is built here;
//!   `spec/wasm_cross/fs_root_escape.almd` holds the `..` and absolute cases.
//! - `[permissions] fs` (C-393) bounds every fs call of the program: a path
//!   whose resolved place (symlinks followed) lies outside the granted roots
//!   is refused with `fs: "<path>" is outside [permissions] fs`, the
//!   predicates answer false, and both targets print the same bytes. Without
//!   the key nothing is bounded, as before.
//! - The p3 component host (a program naming the full-response http family)
//!   preopens only the granted roots, not `/`.
use std::path::Path;
use std::process::Command;

const ROOTED: &str = r#"import fs

fn show(label: String, r: Result[String, String]) -> Unit = match r {
  ok(v) => println("${label}: ok ${string.trim(v)}"),
  err(e) => println("${label}: err ${e}"),
}

effect fn main() -> Unit = {
  let root = fs.open_root("data")!
  show("nested", fs.root_read_text(root, "sub/a.txt"))
  show("dir link", fs.root_read_text(root, "link/s.txt"))
  show("file link", fs.root_read_text(root, "flink"))
  show("link write", fs.root_write_text(root, "link/new.txt", "x") |> result.map((_) => "written"))
  show("dotdot", fs.root_read_text(root, "sub/../../outside/s.txt"))
  show("plain outside", fs.read_text("../outside/s.txt"))
  show("plain link", fs.read_text("data/link/s.txt"))
  show("plain dangling", fs.write("data/dangling", "x") |> result.map((_) => "written"))
  println("exists outside: ${fs.exists("../outside/s.txt")}")
  println("is_symlink link: ${fs.is_symlink("data/link")}")
}
"#;

/// A project dir `proj/` with `data/sub/a.txt`, `data/link -> ../../outside`,
/// `data/flink -> outside/s.txt`, a dangling `data/dangling`, and the probe;
/// `outside/s.txt` beside it.
fn tree(manifest: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
    let t = tempfile::tempdir().expect("scratch");
    let base = std::fs::canonicalize(t.path()).expect("canonical scratch");
    let proj = base.join("proj");
    let outside = base.join("outside");
    std::fs::create_dir_all(proj.join("data/sub")).expect("mkdir");
    std::fs::create_dir_all(&outside).expect("mkdir");
    std::fs::write(outside.join("s.txt"), "secret\n").expect("write");
    std::fs::write(proj.join("data/sub/a.txt"), "hello\n").expect("write");
    std::os::unix::fs::symlink(&outside, proj.join("data/link")).expect("symlink");
    std::os::unix::fs::symlink(outside.join("s.txt"), proj.join("data/flink")).expect("symlink");
    std::os::unix::fs::symlink(outside.join("made-through-link.txt"), proj.join("data/dangling")).expect("symlink");
    std::fs::write(proj.join("main.almd"), ROOTED).expect("write");
    if let Some(m) = manifest {
        std::fs::write(proj.join("almide.toml"), m).expect("write");
    }
    (t, proj)
}

fn run(dir: &Path, args: &[&str]) -> String {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg("main.almd")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn");
    assert!(o.status.success(), "{args:?}: {}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8")
}

#[test]
fn a_symlink_inside_a_root_pointing_outside_is_refused_alike_on_both_targets() {
    let (_t, proj) = tree(None);
    let native = run(&proj, &[]);
    let wasm = run(&proj, &["--target", "wasm"]);
    assert_eq!(native, wasm, "native and wasm differ");
    for line in [
        "nested: ok hello",
        "dir link: err fs: path escapes root: link/s.txt",
        "file link: err fs: path escapes root: flink",
        "link write: err fs: path escapes root: link/new.txt",
        "dotdot: err fs: path escapes root: sub/../../outside/s.txt",
        // No `fs` key: the plain calls are not bounded, as before.
        "plain outside: ok secret",
        "plain link: ok secret",
        "exists outside: true",
        "is_symlink link: true",
    ] {
        assert!(native.lines().any(|l| l == line), "missing `{line}` in:\n{native}");
    }
    assert!(!proj.join("../outside/new.txt").exists(), "a rooted write went through the link");
}

#[test]
fn permissions_fs_bounds_every_fs_call_alike_on_both_targets() {
    let (_t, proj) = tree(Some("[package]\nname = \"scoped\"\n\n[permissions]\nfs = [\"./data\"]\n"));
    let native = run(&proj, &[]);
    let wasm = run(&proj, &["--target", "wasm"]);
    assert_eq!(native, wasm, "native and wasm differ");
    for line in [
        "nested: ok hello",
        "plain outside: err fs: \"../outside/s.txt\" is outside [permissions] fs",
        "plain link: err fs: \"data/link/s.txt\" is outside [permissions] fs",
        "plain dangling: err fs: \"data/dangling\" is outside [permissions] fs",
        "exists outside: false",
        // Every call is judged by where its path leads, the no-follow
        // predicate included: a link that points out cannot be asked about.
        "is_symlink link: false",
    ] {
        assert!(native.lines().any(|l| l == line), "missing `{line}` in:\n{native}");
    }
    assert!(!proj.join("../outside/made-through-link.txt").exists(), "a write followed a dangling link out");
}

const COMPONENT: &str = r#"import fs
import http

effect fn main() -> Unit = {
  let r: Result[HttpResponse, String] = http.get_response("http://127.0.0.1:9/x")
  println("${result.is_ok(r)}")
  println(fs.read_text("data/sub/a.txt") ?? "unreadable")
  println(fs.read_text("../outside/s.txt") ?? "unreadable")
  println("${string.len(fs.read_text("/etc/hosts") ?? "") > 0}")
}
"#;

#[test]
fn the_component_host_preopens_only_the_granted_roots() {
    let (_t, proj) = tree(Some("[package]\nname = \"scoped\"\n\n[permissions]\nfs = [\"./data\"]\n"));
    std::fs::write(proj.join("main.almd"), COMPONENT).expect("write");
    let wasm = run(&proj, &["--target", "wasm"]);
    assert_eq!(wasm, "false\nhello\n\nunreadable\nfalse\n", "the component host reached outside its preopens");
    // Without the key the component host preopens the cwd and the temp dir —
    // not `/`. (Whether the scratch tree's sibling is reachable depends on
    // whether the platform's temp dir spells the cwd's prefix, so only the
    // cwd read and the `/etc` read are asserted.)
    std::fs::remove_file(proj.join("almide.toml")).expect("rm manifest");
    let wasm = run(&proj, &["--target", "wasm"]);
    let lines: Vec<&str> = wasm.lines().collect();
    assert_eq!((lines[1], lines.last().copied()), ("hello", Some("false")), "{wasm}");
}

const SCOPED_TEST: &str = r#"import fs

test "a path outside the scope is refused" {
  assert_eq(fs.read_text("../outside/s.txt"), err("fs: \"../outside/s.txt\" is outside [permissions] fs"))
  assert_eq(fs.read_text("data/link/s.txt"), err("fs: \"data/link/s.txt\" is outside [permissions] fs"))
  assert(not fs.exists("../outside/s.txt"))
}

test "a path inside the scope is read" {
  assert_eq(fs.read_text("data/sub/a.txt"), ok("hello\n"))
}
"#;

fn test_run(dir: &Path, args: &[&str]) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("test")
        .arg("scoped_test.almd")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn");
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

/// #4084: a test harness build has no `main`, and `[permissions] fs` used to
/// hang on it — `almide test` ran unbounded. The scope is now resolved on the
/// first fs call (native) and set for the test run (the embedded wasm host),
/// so a test file is bounded like `almide run`, on both legs. Without the
/// key the same file fails: the outside read succeeds.
#[test]
fn almide_test_is_bounded_by_permissions_fs_on_both_targets() {
    let (_t, proj) = tree(Some("[package]\nname = \"scoped\"\n\n[permissions]\nfs = [\"./data\"]\n"));
    std::fs::write(proj.join("scoped_test.almd"), SCOPED_TEST).expect("write");
    for args in [&["--target", "native"][..], &["--target", "wasm"][..], &[][..]] {
        let (ok, out) = test_run(&proj, args);
        assert!(ok, "{args:?}: the scoped test file failed:\n{out}");
    }
    std::fs::remove_file(proj.join("almide.toml")).expect("rm manifest");
    for args in [&["--target", "native"][..], &["--target", "wasm"][..]] {
        let (ok, out) = test_run(&proj, args);
        assert!(!ok, "{args:?}: without the key the outside read should succeed and the assertion fail:\n{out}");
    }
}
