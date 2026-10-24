//! A Bytes or Matrix runtime result is wrapped in `AlmideRcCow` with its value
//! type named. Inferred, `AlmideRcCow::from(x)` left `x`'s type to the
//! surrounding call, and a generic producer such as
//! `almide_rt_option_unwrap_or_else(…)` then had nothing to fix it (E0282 in
//! the generated Rust; the program type-checks and runs on wasm). Found by the
//! inspection's parity campaign (`bytes.get_u32_le`, seed 1791720899).
use std::process::Command;

const SOURCE: &str = r#"effect fn main() -> Unit = {
  let r3: Int = bytes.get_u32_le(
    option.unwrap_or_else(
      list.find(list.repeat(bytes.from_list([0, 2, 1]), 5), (s2) => false),
      () => bytes.from_list([7, 0, 0, 0]),
    ),
    0,
  ) ?? 0
  let r4: Int = bytes.len(option.unwrap_or_else(list.find([bytes.from_list([1, 2, 3])], (b) => true), () => bytes.zeros(0)))
  let r5: Int = matrix.rows(option.unwrap_or_else(list.find([matrix.zeros(2, 3)], (m) => false), () => matrix.zeros(4, 1)))
  println("${r3} ${r4} ${r5}")
}
"#;

#[test]
fn a_generic_bytes_or_matrix_producer_passed_straight_to_a_bytes_parameter_compiles_natively() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("rc_cow_glue.almd");
    std::fs::write(&file, SOURCE).expect("source");
    for wasm in [false, true] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if wasm {
            c.args(["--target", "wasm"]);
        }
        let out = c.output().expect("almide run");
        assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "7 3 4\n", "wasm={wasm}");
    }
}
