//! A user variant case spelled like a stdlib newtype's constructor (#4275).
//!
//! `value` declares `mod type Path = List[(Bool, String, Int)]`, whose
//! constructor is `Path(..)`. A user's `type Arg = | Str(String) |
//! Path(String)` (almide-shell's D-Bus args) broke the native build: the
//! walker resolved a bare case name program-wide, so the stdlib's own
//! `match p { Path(steps) => .. }` was spelled `Arg::Path(steps)` against a
//! `Path` scrutinee (rustc E0308), and the user's `Path("/x")` was built as
//! the stdlib's tuple struct. The wasm leg's newtype erasure keyed on the
//! same bare name: it erased the user's `Path("/x")` and `Path(s)` to their
//! payloads and refused the program (E082 `ty-mismatch`). On both legs the
//! value's type now decides: a newtype's value is its own struct, an enum's
//! value its own case.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue4275-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    root
}

fn run(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(almide()).args(args).current_dir(dir).output().expect("spawn almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

const PROGRAM: &str = r#"type Arg = | Str(String) | Path(String)

fn name(a: Arg) -> String = match a {
  Path(s) => s,
  Str(s) => s,
}

fn main() -> Unit = {
  println(name(Path("/x")))
  println(name(Str("y")))
  let v = value.object([("a", value.int(7))])
  let got = value.get_path(v, value.key(value.root(), "a")) ?? value.int(0)
  println(int.to_string(value.as_int(got) ?? -1))
}

test "a user variant named Path" {
  assert_eq(name(Path("/x")), "/x")
  assert_eq(name(Str("y")), "y")
}
"#;

#[test]
fn a_user_case_named_like_a_stdlib_newtype_runs_on_both_targets() {
    let dir = scratch("run");
    std::fs::write(dir.join("p.almd"), PROGRAM).expect("write");
    for target in ["rust", "wasm"] {
        let (ok, stdout, stderr) = run(&dir, &["run", "p.almd", "--target", target]);
        assert!(ok, "{target} run failed:\nstdout: {stdout}\nstderr: {stderr}");
        assert_eq!(stdout, "/x\ny\n7\n", "wrong {target} output; stderr: {stderr}");
    }
}

#[test]
fn a_user_case_named_like_a_stdlib_newtype_tests_on_both_targets() {
    let dir = scratch("test");
    std::fs::write(dir.join("p_test.almd"), PROGRAM).expect("write");
    for target in [None, Some("wasm")] {
        let mut args = vec!["test", "p_test.almd"];
        if let Some(t) = target {
            args.extend(["--target", t]);
        }
        let (ok, stdout, stderr) = run(&dir, &args);
        assert!(ok, "almide test ({target:?}) failed:\nstdout: {stdout}\nstderr: {stderr}");
    }
}
