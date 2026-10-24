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
  show("link list", fs.root_list_dir(root, "link") |> result.map((xs) => list.join(xs, ",")))
  show("link exists", fs.root_exists(root, "flink") |> result.map((b) => "${b}"))
  show("link remove", fs.root_remove(root, "flink") |> result.map((_) => "removed"))
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
        "link list: err fs: path escapes root: link",
        "link exists: err fs: path escapes root: flink",
        "link remove: err fs: path escapes root: flink",
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
    assert!(proj.join("data/flink").is_symlink(), "a refused rooted remove took the link");
}

/// #4084: the stock-runtime artifact (`almide build --target wasm`, the p1 fs
/// service) refuses the same symlinks with the same bytes: its walk opens
/// each segment from the previous directory's descriptor with lookupflags 0.
/// Needs the `wasmtime` CLI; without it the leg is reported and skipped.
#[test]
fn the_stock_wasm_artifact_refuses_rooted_symlinks_alike() {
    let (_t, proj) = tree(None);
    let native = run(&proj, &[]);
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", "main.almd", "--target", "wasm", "-o", "main.wasm"])
        .current_dir(&proj)
        .output()
        .expect("spawn");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let base = proj.parent().unwrap().to_str().unwrap().to_string();
    let Ok(o) = Command::new("wasmtime")
        .args(["run", "--dir", &format!("{base}::{base}"), "--env", &format!("ALMIDE_CWD={}", proj.display()), "main.wasm"])
        .current_dir(&proj)
        .output()
    else {
        eprintln!("SKIP: no wasmtime CLI on PATH");
        return;
    };
    // The rooted lines only: a plain call through an absolute link is the
    // stock runtime's own preopen bound (cap-std refuses it), not the root's.
    let rooted = |s: &str| s.lines().filter(|l| !l.starts_with("plain ")).map(str::to_string).collect::<Vec<_>>();
    let stock = String::from_utf8_lossy(&o.stdout).to_string();
    assert_eq!(rooted(&stock), rooted(&native), "stderr: {}", String::from_utf8_lossy(&o.stderr));
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

const RACE: &str = r#"import fs

effect fn main() -> Unit = {
  let root = fs.open_root("data")!
  var rooted_leaks = 0
  var rooted_reads = 0
  var checked_leaks = 0
  for i in list.range(0, 3000) {
    match fs.root_read_text(root, "sub/a.txt") {
      ok(t) => if string.contains(t, "secret") then { rooted_leaks = rooted_leaks + 1 } else { rooted_reads = rooted_reads + 1 },
      err(_) => (),
    }
    // The pre-#4084 shape: check for a symlink, then the plain call.
    if not fs.is_symlink("data/sub") then {
      match fs.read_text("data/sub/a.txt") {
        ok(t) => if string.contains(t, "secret") then { checked_leaks = checked_leaks + 1 } else (),
        err(_) => (),
      }
    } else ()
  }
  println("rooted leaks: ${rooted_leaks}")
  println("rooted reads: ${rooted_reads}")
  println("checked leaks: ${checked_leaks}")
}
"#;

/// #4084: a thread flips `data/sub` between the directory and a symlink to
/// the outside while the program reads `sub/a.txt` through its root, 3000
/// times, on each target. The rooted call never answers the outside bytes:
/// it holds each directory by descriptor and refuses a symlink it meets. The
/// program also runs the pre-#4084 shape (an `is_symlink` check, then the
/// plain read) and prints how often THAT leaked — printed, not asserted: it
/// shows the flips landed inside the run, and depends on timing.
/// tests/fs_beneath_core.rs (almide-rt-core) holds the deterministic
/// version, with the swap placed between the walk's own steps.
#[test]
fn a_directory_flipped_to_a_symlink_during_rooted_reads_never_leaks_on_either_target() {
    let (_t, proj) = tree(None);
    let base = proj.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(base.join("outside/sub")).unwrap();
    std::fs::write(base.join("outside/sub/a.txt"), "secret\n").unwrap();
    std::fs::write(proj.join("main.almd"), RACE).unwrap();
    for args in [&[][..], &["--target", "wasm"][..]] {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flipper = {
            let (base, stop) = (base.clone(), stop.clone());
            std::thread::spawn(move || {
                let (sub, aside) = (base.join("proj/data/sub"), base.join("proj/data/sub.real"));
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    std::fs::rename(&sub, &aside).unwrap();
                    std::os::unix::fs::symlink(base.join("outside/sub"), &sub).unwrap();
                    std::fs::remove_file(&sub).unwrap();
                    std::fs::rename(&aside, &sub).unwrap();
                }
            })
        };
        let out = run(&proj, args);
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        flipper.join().unwrap();
        eprintln!("{args:?}:\n{out}");
        assert!(out.lines().any(|l| l == "rooted leaks: 0"), "{args:?}: a rooted read followed the flipped symlink:\n{out}");
    }
}
