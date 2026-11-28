//! #4959: the default `almide test` lane (wasm first, native for what wasm
//! cannot serve) stopped the whole run on a file whose wasm certificate is
//! INCOMPLETE (C-462 refuses to run that module), where v0.69.1 passed it.
//! Now the module is still not run, and the file goes to the native lane with
//! a note, as any file the wasm leg cannot serve does. An explicit `--target
//! wasm` still refuses it without `--allow-incomplete-verification`.
//!
//! The program slices a `Bytes` by a range, which the wasm producer does not
//! certify today (`rhs:RuntimeCall:almide_rt_bytes_slice`, the
//! spec/lang/index_access_test.almd row of proofs/incomplete-verification.toml).
//! When the producer certifies it, pick another shape from that ledger.

use std::process::Command;

const PROGRAM: &str = r#"test "bytes range slice" {
  var buf: Bytes = bytes.from_list([10, 20, 30, 40, 50])
  let sub = buf[1..<3]
  assert_eq(bytes.len(sub), 2)
  assert_eq(sub[0], 20)
  assert_eq(sub[1], 30)
}
"#;

const NOTE: &str = "its wasm certificate is INCOMPLETE, so the wasm module was not run; the file runs on the native lane";

fn almide(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).args(args).current_dir(dir).output().expect("almide");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn an_incomplete_wasm_certificate_sends_the_default_lane_to_native_and_explicit_wasm_still_refuses() {
    let dir = tempfile::tempdir().expect("scratch");
    std::fs::write(dir.path().join("slice_test.almd"), PROGRAM).expect("the program is written");

    let (ok, out) = almide(dir.path(), &["test", "slice_test.almd"]);
    assert!(ok, "the default lane stopped on the INCOMPLETE file:\n{out}");
    assert!(out.contains(NOTE), "the default lane must say why the file ran on native:\n{out}");
    assert!(out.contains("UNCERTIFIED"), "the note names the uncertified function(s):\n{out}");
    assert!(out.contains("1 via native fallback"), "the file must run on the native lane:\n{out}");
    assert!(!out.contains("error[certificate-incomplete]"), "{out}");

    let (ok, out) = almide(dir.path(), &["test", "--target", "wasm", "slice_test.almd"]);
    assert!(!ok, "an explicit --target wasm must still refuse the INCOMPLETE module:\n{out}");
    assert!(out.contains("error[certificate-incomplete]") && out.contains("the module was not run"), "{out}");
    assert!(!out.contains(NOTE), "{out}");

    let (ok, out) = almide(dir.path(), &["test", "--target", "wasm", "--allow-incomplete-verification", "slice_test.almd"]);
    assert!(ok, "the flag lets the module run on wasm:\n{out}");
    assert!(out.contains("warning[verification-incomplete]"), "{out}");
}
