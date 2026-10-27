//! A Bytes argument spelled as an `if` / `match` passed to a function compiled
//! from Almide (a user fn, or a self-hosted stdlib fn such as `hash.crc32`)
//! failed rustc E0308 natively after `almide check` accepted it. Native
//! `BorrowLowering` wrote `&*(if …)` for every propagating Bytes rvalue: that
//! deref is right at a runtime intrinsic's `&Vec<u8>` slot, but an
//! Almide-compiled callee takes `&AlmideRcCow<Vec<u8>>`.
//!
//! Found by the counted stdlib inspection's parity campaign (8 functions:
//! `hex.decode`, `regex.*`, `url.decode_component`, `zlib.*`). The expected
//! outputs are the wasm leg's, which was right; the crc32 value is
//! `zlib.crc32(bytes([1]))` in Python's zlib module.
use std::process::Command;

const USER_FN: &str = r#"fn total(b: Bytes) -> Int = bytes.len(b)

fn main() -> Unit = {
  let c = list.len([1, 2]) > 1
  println(int.to_string(total(if c then bytes.from_list([1]) else bytes.from_list([2, 3]))))
  println(int.to_string(bytes.len(if c then bytes.from_list([1]) else bytes.from_list([2, 3]))))
  let m = match c { true => bytes.from_list([4]), false => bytes.zeros(0) }
  println(int.to_string(total(match c { true => bytes.from_list([4]), false => bytes.zeros(0) }) + bytes.len(m)))
}
"#;

const SELF_HOSTED: &str = r#"import hash

fn main() -> Unit = {
  let c = list.len([1, 2]) > 1
  println(int.to_string(hash.crc32(if c then bytes.from_list([1]) else bytes.from_list([2]))))
}
"#;

fn run(src: &str, wasm: bool) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("bytes_arg.almd");
    std::fs::write(&file, src).expect("source");
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn propagating_bytes_argument_builds_natively_and_agrees_with_wasm() {
    for (src, want) in [(USER_FN, "1\n1\n2\n"), (SELF_HOSTED, "2768625435\n")] {
        assert_eq!(run(src, false), want, "native");
        assert_eq!(run(src, true), want, "wasm");
    }
}
