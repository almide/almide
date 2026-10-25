//! #4218: a generic body over `A` calls generic fns whose own letter is also
//! `A` (`option.is_none[A]`, `list.map[A, B]`). The two `A`s are different
//! variables that share a spelling: the callee's binds to the caller's `A?`
//! (or to the caller's `A` itself) as it binds to any other type. Unifying
//! them by name failed the occurs check, so the callee's parameter stayed
//! unbound and the call was refused (E025) where the same body over `T`
//! checked.

use std::process::Command;

const SRC: &str = r#"fn gather[A](xs: List[A?]) -> List[A]? =
  if list.any(xs, (o) => option.is_none(o)) then none
  else some(list.map(
    xs,
    (o) => match o {
      some(v) => v,
      none => panic("unreachable: every element was checked"),
    },
  ))

fn firsts[A, B](pairs: List[(A, B)]) -> List[A] = list.map(pairs, ((a, _)) => a)

test "the callee's A binds to the caller's A?" {
  assert_eq(gather([some(1), some(2)]), some([1, 2]))
  assert_eq(gather([some("a"), none]), none)
}

test "the callee's A and B bind to the caller's A" {
  assert_eq(firsts([(1, "x"), (2, "y")]), [1, 2])
}
"#;

#[test]
fn a_generic_body_over_a_calls_generics_over_a_on_both_targets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("shared_letter_test.almd");
    std::fs::write(&file, SRC).expect("write fixture");
    for target in ["wasm", "rust"] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(["test", file.to_str().expect("path"), "--target", target])
            .current_dir(dir.path())
            .output()
            .expect("run almide test");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "target {target}: almide test failed:\nstdout:\n{stdout}\nstderr:\n{stderr}");
        assert!(stdout.contains("2 tests in 1 file"), "target {target}: both tests must run:\n{stdout}");
    }
}
