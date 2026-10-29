//! xtarget-fuzz seed=4023 index=34: `list.join[T: Text]` called with
//! `T = Option[Int]` and `list.product[A: Numeric]` with `A = List[Float]`
//! passed `almide check`, then the native build panicked in ResolveCalls on
//! the unresolvable `option.__join` / `list.__product` dispatch. The bound
//! check skipped every type without a conformance-table name, so a
//! container, tuple or function type satisfied any bound. Such a type never
//! conforms to a non-derivable built-in protocol; the checker now says so,
//! on both legs, while the primitive instantiations keep running.
use std::process::Command;

fn almide(args: &[&str], file: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg(args[0])
        .arg(file)
        .args(&args[1..])
        .output()
        .expect("almide")
}

const REFUSED: &[(&str, &str)] = &[
    (
        "fn main() -> Unit = {\n  let x: List[Option[Int]] = []\n  let r = list.join(x, some(10))\n  println(\"${r}\")\n}\n",
        "type 'Option[Int]' does not implement protocol 'Text'",
    ),
    (
        "fn main() -> Unit = {\n  let r = list.product([[1.0], [2.0]])\n  println(\"${r}\")\n}\n",
        "type 'List[Float]' does not implement protocol 'Numeric'",
    ),
    (
        "fn main() -> Unit = {\n  let r = list.sum([(1, 2)])\n  println(\"${r}\")\n}\n",
        "type '(Int, Int)' does not implement protocol 'Numeric'",
    ),
];

#[test]
fn a_container_type_does_not_satisfy_a_non_derivable_builtin_bound() {
    let dir = tempfile::tempdir().expect("scratch");
    for (i, (src, want)) in REFUSED.iter().enumerate() {
        let file = dir.path().join(format!("refused_{i}.almd"));
        std::fs::write(&file, src).expect("source");
        for target in ["rust", "wasm"] {
            let out = almide(&["run", "--target", target], &file);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert!(!out.status.success(), "{target} #{i}: accepted\n{stderr}");
            assert!(stderr.contains(want), "{target} #{i}: want `{want}`\n{stderr}");
            assert!(!stderr.contains("panicked"), "{target} #{i}: compiler panic\n{stderr}");
        }
    }
}

#[test]
fn primitive_instantiations_of_the_same_bounds_still_run_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("accepted.almd");
    std::fs::write(
        &file,
        "fn main() -> Unit = {\n  println(list.join([\"a\", \"b\"], \"-\"))\n  println(\"${list.product([2.0, 3.0])}\")\n  println(\"${list.sum([1, 2, 3])}\")\n}\n",
    )
    .expect("source");
    for target in ["rust", "wasm"] {
        let out = almide(&["run", "--target", target], &file);
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "a-b\n6\n6\n", "{target}");
    }
}
