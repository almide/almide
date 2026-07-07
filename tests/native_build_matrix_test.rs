//! Native-build gate for matrix programs — the #739 class.
//!
//! `almide check` green must imply `almide build` (native) green. Matrix programs
//! are the one stdlib family excluded from the rlib fast path, so they always take
//! the self-contained cargo build — a path the spec corpus (wasm-first) and the
//! rlib-built examples never compile. #739 lived there: the generated crate
//! embedded almide-kernel's `bridge` module (a stale `Vec<Vec<f64>>` adapter), and
//! the burn splicer keyed on the alias line INSIDE it, producing invalid Rust for
//! every native matrix build while check stayed green. This gate builds and RUNS a
//! matrix program natively — any rustc failure on generated code is a compiler bug
//! and fails CI.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") { return bin; }
    let release = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if release.exists() { return release.to_str().unwrap().to_string(); }
    let debug = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/almide");
    if debug.exists() { return debug.to_str().unwrap().to_string(); }
    "almide".to_string()
}

/// The #739 repro shape: a Matrix built from a nested `list.map` via
/// `matrix.from_lists`, read back element-wise. Uses stdout only, so the test
/// needs no preopens or scratch files beyond the build dir.
const MATRIX_ALMD: &str = r#"fn build(t: Int, d: Int) -> Matrix =
  matrix.from_lists(list.range(0, t) |> list.map((i) =>
    list.range(0, d) |> list.map((j) => int.to_float(i * d + j))))

effect fn main() -> () = {
  let m = build(3, 2)
  var s = ""
  var r = 0
  while r < 3 {
    var c = 0
    while c < 2 {
      s = s + float.to_string(matrix.get(m, r, c)) + " "
      c = c + 1
    }
    r = r + 1
  }
  println(s)
  println(float.to_string(matrix.get(matrix.transpose(m), 0, 2)))
}
"#;

#[test]
fn native_build_of_matrix_program_succeeds_and_runs() {
    let dir = std::env::temp_dir().join(format!("almide-739-gate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mk temp dir");
    let src = dir.join("m.almd");
    let bin = dir.join(if cfg!(windows) { "m.exe" } else { "m" });
    std::fs::write(&src, MATRIX_ALMD).expect("write source");

    let build = Command::new(almide_bin())
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("spawn almide build");
    assert!(
        build.status.success(),
        "`almide build` (native) failed on a check-green matrix program — \
         a rustc error on generated code is a compiler bug (#739 class).\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new(&bin).output().expect("run built binary");
    assert!(run.status.success(), "built matrix binary exited nonzero");
    let out = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        out, "0.0 1.0 2.0 3.0 4.0 5.0 \n4.0\n",
        "matrix output mismatch"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
