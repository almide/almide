//! Unary minus on a sized signed integer wraps at the type's width (#4163,
//! C-180). `-MIN` is MIN on Int8, Int16 and Int32. Before the fix, the wasm
//! leg lowered `-x` as `0 - x` with no re-wrap, so an Int8 held 128. The
//! constant folder negated at i64 width and emitted `128i8`, which rustc
//! refuses, so the native build failed. Run-time and constant operands are
//! both covered, on both targets.
use std::process::Command;

const SOURCE: &str = r#"fn n8(x: Int8) -> Int8 = -x

fn n16(x: Int16) -> Int16 = -x

fn n32(x: Int32) -> Int32 = -x

effect fn main() -> Unit = {
  let a: Int8 = int8.from_wrapping(int.parse("-128")!)
  println(int8.to_string(n8(-128)))
  println(int16.to_string(n16(-32768)))
  println(int32.to_string(n32(-2147483648)))
  let k8: Int8 = -128
  println(int8.to_string(-k8))
  let k16: Int16 = -32768
  println(int16.to_string(-k16))
  let k32: Int32 = -2147483648
  println(int32.to_string(-k32))
  println(int8.to_string(n8(5)))
  println(int8.to_string(-a))
}
"#;

const WANT: &str = "-128\n-32768\n-2147483648\n-128\n-32768\n-2147483648\n-5\n-128\n";

#[test]
fn unary_minus_wraps_at_the_declared_width_on_both_targets() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("unary_minus_narrow.almd");
    std::fs::write(&file, SOURCE).expect("source");
    for wasm in [false, true] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if wasm {
            c.args(["--target", "wasm"]);
        }
        let out = c.output().expect("almide run");
        assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), WANT, "wasm={wasm}");
    }
}
