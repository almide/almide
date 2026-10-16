//! #3753: a small-shape `matrix.mul` / `matrix.mul_f32` (literal dims <= 8,
//! unrolled by `MatrixShapeSpec` into an `InlineRust` block) used as the
//! argument of another matrix runtime call failed the native build with
//! rustc E0308 (`expected AlmideMatrix, found AlmideRcCow<AlmideMatrix>`):
//! the block rendered as `&{ ..; AlmideRcCow::from(..) }`, and rustc checks a
//! block tail against the raw type the `&AlmideMatrix` param expects. Wasm
//! ran it. Each line below is one argument position of the shape class.
use std::process::Command;

const SRC: &str = r#"fn main() -> Unit = {
  let r0: Matrix[Float32] = matrix.mul_f32(
    matrix.mul_f32(
      matrix.ones_f32(2, 2),
      matrix.mul_f32(matrix.ones_f32(2, 2), matrix.ones_f32(2, 2)),
    ),
    matrix.mul_f32_scaled(matrix.ones_f32(2, 2), 0.5, matrix.ones_f32(2, 2)),
  )
  println("${matrix.to_lists(r0)}")
  let r1 = matrix.mul(matrix.ones(2, 2), matrix.mul(matrix.ones(2, 2), matrix.ones(2, 2)))
  println("${matrix.to_lists(r1)}")
  let r2 = matrix.mul(matrix.mul(matrix.ones(2, 3), matrix.ones(3, 2)), matrix.ones(2, 1))
  println("${matrix.to_lists(r2)}")
  let r3 = matrix.transpose(matrix.mul(matrix.ones(2, 3), matrix.ones(3, 1)))
  println("${matrix.to_lists(r3)}")
  let r4 = matrix.add(matrix.mul_f32(matrix.ones_f32(2, 3), matrix.ones_f32(3, 2)), matrix.ones_f32(2, 2))
  println("${matrix.to_lists(r4)}")
}
"#;

fn run(target: Option<&str>) -> (bool, String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("unrolled_mul_arg.almd");
    std::fs::write(&file, SRC).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if let Some(t) = target {
        c.args(["--target", t]);
    }
    let out = c.output().expect("run");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn an_unrolled_small_matmul_as_a_matrix_call_argument_runs_on_both_legs() {
    let expected = "[[8, 8], [8, 8]]\n[[4, 4], [4, 4]]\n[[6], [6]]\n[[3, 3]]\n[[4, 4], [4, 4]]\n";
    for target in [None, Some("wasm")] {
        let (ok, stdout, stderr) = run(target);
        assert!(ok, "{target:?}: {stderr}");
        assert_eq!(stdout, expected, "{target:?}");
    }
}
