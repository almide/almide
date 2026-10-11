//! #3555: two native compile failures over `Bytes` / `Matrix` values, which
//! render as `AlmideRcCow`:
//! - a `list.fold` with such an accumulator, passed straight to a
//!   by-reference parameter, let rustc infer the accumulator from the raw
//!   `Vec<u8>` parameter type (E0308); the fold now names its accumulator type;
//! - `list.min` / `max` / `sort` over `List[Bytes]` needed `Ord` on
//!   `AlmideRcCow` (E0277); it now orders by the value, the same
//!   lexicographic, prefix-first order `bytes.cmp` documents.
//!
//! Both were found by the inspection harness's parity campaign once it built
//! `Bytes` values.
use std::process::Command;

const FOLD_ARG: &str = r#"effect fn main() -> Unit = {
  println("${bytes.len(list.fold([1, 2], bytes.new(3), (acc, x) => acc))}")
  println("${matrix.rows(list.fold([1, 2], matrix.zeros(3, 1), (acc, x) => acc))}")
  println("${bytes.len(list.fold([1, 2], bytes.new(3), (acc, x) => bytes.concat(acc, bytes.new(x))))}")
  println("${bytes.eof(list.fold([true, false], bytes.from_list([100, 100]), (b, t) => bytes.from_list([1])), 7)}")
}
"#;

const ORDER: &str = r#"effect fn main() -> Unit = {
  let xs = [bytes.from_list([2, 1]), bytes.from_list([2]), bytes.from_list([]), bytes.from_list([1, 255, 3]), bytes.from_list([2, 0, 9])]
  println("${bytes.to_list(list.min(xs) ?? bytes.new(0))} ${bytes.to_list(list.max(xs) ?? bytes.new(0))}")
  println("${list.sort(xs) |> list.map((b) => bytes.to_list(b))}")
  let ys: List[Bytes] = []
  println("${bytes.len(list.min(ys) ?? bytes.from_list([7, 7]))}")
}
"#;

fn run(name: &str, src: &str, wasm: bool) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_fold_over_a_bytes_or_matrix_accumulator_passes_straight_to_a_borrowed_parameter() {
    let expected = "3\n3\n6\ntrue\n";
    assert_eq!(run("fold_arg", FOLD_ARG, false), expected, "native");
    assert_eq!(run("fold_arg", FOLD_ARG, true), expected, "wasm");
}

#[test]
fn bytes_values_order_lexicographically_with_a_prefix_first_on_native() {
    // wasm refuses `list.min` / `sort` over Bytes at check time
    // (`list-cmp-elem:Scalar(Bytes)`), so this pins the native leg only.
    let expected = "[] [2, 1]\n[[], [1, 255, 3], [2], [2, 0, 9], [2, 1]]\n2\n";
    assert_eq!(run("order", ORDER, false), expected, "native");
}
