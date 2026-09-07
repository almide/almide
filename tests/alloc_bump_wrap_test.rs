//! #1908 (C-197): the structural leg's bump head is an i32. A request
//! whose end lies past 4 GiB used to WRAP the frontier below its base;
//! the grow guard then saw a small in-range frontier and the header store
//! landed past the end of memory as a raw `out of bounds memory access`
//! trap — every other unsatisfiable allocation dies `Error: out of
//! memory` + exit 1. A 2 GiB buffer's append growth (copy-grow into a
//! second block) is the natural way to reach it. Native satisfies the
//! same program from host memory: that is C-197's contracted divergence
//! (the threshold is structural per leg), so this pins the wasm leg's
//! FORM alone — the defined abort, never a trap.

use std::process::Command;

// The second append is what wraps: after the first copy-grow the bump
// head sits past 2 GiB, and the next 2 GiB block's end lies past 4 GiB.
// (One append fits — wasmtime grows the memory to 4 GiB — and answers
// identically on both legs.) Native answers 2143289356 from host memory.
const PROBE: &str = r#"fn main() -> Unit = {
  println("before")
  var b = bytes.new(2143289344)
  bytes.append_f64_le(b, 1.5)
  bytes.append_f32_le(b, 1.5)
  println("unreachable ${bytes.len(b)}")
}
"#;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let release = root.join("target/release/almide");
    if release.exists() {
        return release.to_str().unwrap().to_string();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

#[test]
fn bump_head_wrap_is_the_defined_oom_abort() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("wrap.almd");
    std::fs::write(&src, PROBE).expect("write probe");
    let run = Command::new(almide_bin())
        .args(["run", "--target", "wasm"])
        .arg(&src)
        .output()
        .expect("spawn almide run --target wasm");
    let out = String::from_utf8_lossy(&run.stdout);
    let err = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "exit 1 expected; stdout={out} stderr={err}");
    assert_eq!(out, "before\n", "output before the exhaustion survives");
    assert!(
        err.contains("Error: out of memory"),
        "the defined C-197 line expected, got: {err}"
    );
    assert!(
        !err.contains("out of bounds") && !err.contains("wasm trap"),
        "a raw trap is not a permitted form: {err}"
    );
}
