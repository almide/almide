//! An error-operator heap bind (`let x = f()!` / the auto-`?` `Try`) NESTED in
//! a loop/if arm is outside `desugar_effect_unwrap`'s reach — the transform
//! walks only the fn body's top-level statements, because a nested Err arm
//! would need a mid-loop early return the MIR has no Op for. Such a bind used
//! to fall to the terminal deferred `Alloc{Opaque}`: an EMPTY block bound in
//! place of the unwrapped payload, which the program then read (minesweeper's
//! 81-cell minefield read as `[]` — the #810 census's non-accumulator
//! producer, a silent wrong value on the verified default).
//!
//! The promise this test pins is direction-proof:
//!
//! - TODAY the wasm leg must DECLINE the program (an honest wall — "a wall is
//!   never a miscompile").
//! - The DAY the desugar learns nested positions, the build starts succeeding
//!   and this test's other arm takes over: the wasm output must byte-match
//!   native. A change that re-admits the silent empty bind fails either way.

use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// A Try bind nested in `while { if … }` whose payload is read afterwards —
/// the minimal minesweeper shape. Native prints `3`.
const NESTED_TRY: &str = r#"
effect fn make(n: Int) -> Result[List[Int], String] =
  if n < 0 then err("neg") else ok([n, n + 1, n + 2])

effect fn main() -> Unit = {
  var i = 0
  var total = 0
  while i < 2 {
    if i == 1 then {
      let xs = make(i)!
      total = total + list.len(xs)
    } else ()
    i = i + 1
  }
  println(int.to_string(total))
}
"#;

fn run_native(src: &Path) -> String {
    let out = Command::new(almide())
        .args(["run", src.to_str().unwrap()])
        .output()
        .expect("native run");
    assert!(
        out.status.success(),
        "native run failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn nested_effect_unwrap_never_binds_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("nested_try.almd");
    std::fs::write(&src, NESTED_TRY).expect("write fixture");

    // Native is the reference: the loop's second iteration unwraps a 3-element
    // list and reads its length.
    let native = run_native(&src);
    assert_eq!(native, "3", "native reference output changed: {native:?}");

    // The wasm leg: an honest wall today, a byte-matching build the day the
    // desugar learns nested positions. Never a silent empty bind.
    let wasm = dir.path().join("nested_try.wasm");
    let build = Command::new(almide())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "-o", wasm.to_str().unwrap()])
        .output()
        .expect("wasm build");
    if !build.status.success() {
        let msg = String::from_utf8_lossy(&build.stderr);
        assert!(
            msg.contains("wall") || msg.contains("Unsupported") || msg.contains("subset"),
            "wasm build failed for a reason other than an honest wall:\n{msg}"
        );
        return; // the honest-wall arm — today's expected state
    }
    // The build succeeded — the capability landed. The output must match native.
    let wt = match Command::new("wasmtime").arg("--dir=/").arg(&wasm).output() {
        Ok(o) if o.status.code() != Some(127) => o,
        _ => return, // wasmtime unavailable — skip the run leg, as the parity tests do
    };
    let wasm_out = String::from_utf8_lossy(&wt.stdout).trim().to_string();
    assert_eq!(
        wasm_out, native,
        "wasm builds this shape now but its output diverges from native — \
         the nested-unwrap lowering is binding a wrong value"
    );
}
