//! #1869: `list.range` and `list.len` are in the native v1 runtime floor.
//! A `let`-bound range that is indexed or measured materializes through
//! `CallFn list.range`; before the floor carried it, EVERY such program
//! left the certified trust-spine render for the v3 codegen — silently
//! (`ALMIDE_VERIFIED_DEBUG` was the only witness). The probe pins the
//! route (the debug note names the v1 render) and the output on both
//! native and wasm, with the heap-cap splice on top: the cap runtime lands
//! AFTER the v1 render's inner attribute (it did not, while no capped
//! program reached v1).

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

const PROBE: &str = r#"effect fn main() -> Unit = {
  let idx = 0..<5
  println("len=" + int.to_string(list.len(idx)) + " at2=" + int.to_string(idx[2]))
  let r = list.range(3, 7)
  println(int.to_string(list.len(r)) + " " + int.to_string(r[0]) + " " + int.to_string(r[3]))
  let e = list.range(5, 5)
  let neg = list.range(4, 1)
  println(int.to_string(list.len(e)) + " " + int.to_string(list.len(neg)))
}
"#;
const EXPECTED: &str = "len=5 at2=2\n4 3 6\n0 0";

#[test]
fn a_measured_range_stays_on_the_v1_native_render() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("range_floor.almd");
    std::fs::write(&src, PROBE).expect("write probe");
    let bin = dir.path().join("range_floor");
    let build = Command::new(almide_bin())
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .args(["build", "--heap-cap", "1048576"])
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("spawn almide build");
    let note = String::from_utf8_lossy(&build.stderr);
    assert!(build.status.success(), "build failed: {note}");
    assert!(
        note.contains("native: v1 trust-spine render"),
        "the measured range left the v1 render for the fallback — `list.range` / `list.len` fell out of the floor: {note}"
    );
    let run = Command::new(&bin).output().expect("spawn probe");
    assert_eq!(run.status.code(), Some(0), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), EXPECTED, "native output");

    let wasm = Command::new(almide_bin())
        .args(["run"])
        .arg(&src)
        .args(["--target", "wasm"])
        .output()
        .expect("spawn almide run --target wasm");
    assert!(wasm.status.success(), "{}", String::from_utf8_lossy(&wasm.stderr));
    assert_eq!(String::from_utf8_lossy(&wasm.stdout).trim(), EXPECTED, "wasm output must match native");
}
