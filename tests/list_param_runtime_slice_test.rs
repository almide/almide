//! A `List[T]` parameter the native leg borrow-infers renders as `&[T]`; a
//! runtime fn that takes `&Vec<T>` then fails rustc (E0308, "codegen
//! produced invalid Rust"). The stdlib inspection's diamond criterion found
//! it for `io.write_bytes` (2026-10-14): `pub fn call(a0: List[Int]) -> Unit
//! = io.write_bytes(a0)` in a module did not build natively, where the same
//! call in `main` did. `random.choice` had the same signature. Both runtime
//! fns now take a slice, which a `&Vec<T>` argument coerces to.
use std::process::Command;

const SOURCE: &str = r#"import io
import random

fn emit(xs: List[Int]) -> Unit = io.write_bytes(xs)

effect fn pick(xs: List[String]) -> String? = random.choice(xs)

effect fn main() -> Unit = {
  emit([72, 105, 10])
  let one = pick(["only"])!
  println("${one}")
}
"#;

#[test]
fn a_borrowed_list_param_reaches_the_slice_taking_runtime_fns() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("slice.almd");
    std::fs::write(&file, SOURCE).expect("source");
    for wasm in [false, true] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if wasm {
            c.args(["--target", "wasm"]);
        }
        let out = c.output().expect("almide run");
        assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "Hi\nsome(\"only\")\n", "wasm={wasm}");
    }
}
