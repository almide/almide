//! `bytes.read_string_at` / `read_string_be` decode their window lossily on
//! both legs. The wasm self-host copied the window's raw bytes, so invalid
//! UTF-8 became a String that held invalid UTF-8. Printing hid it; every string
//! operation after it read wrong: `string.to_upper` of three 0xFF bytes gave
//! "\x01" plus three U+FFFD with length 1, where native's
//! `String::from_utf8_lossy` gives three U+FFFD with length 3. The stdlib
//! documents both readers as "lossy UTF-8". Found by the inspection's parity
//! campaign (`float32.to_int32`, seed 1791721660).
//!
//! The expected output is native's, which is unchanged; wasm must now print it.
use std::process::Command;

const SOURCE: &str = r#"fn show(s: String) -> String = "${string.len(s)}:${string.to_upper(s)}:${string.to_bytes(s)}"

effect fn main() -> Unit = {
  let bad = bytes.from_list([255, 254, 65, 0xE6, 0x97, 0xA5, 0xE6, 0x97])
  let good = bytes.from_list([0x61, 0xC3, 0xA9, 0xE6, 0x97, 0xA5])
  println(show(bytes.read_string_at(bad, 0, 3)))
  println(show(bytes.read_string_at(bad, 3, 5)))
  println(show(bytes.read_string_at(good, 0, 6)))
  println(show(bytes.read_string_at(good, 1, 1)))
  println(show(bytes.read_string_at(good, -1, 3)))
  println(show(bytes.read_string_at(good, 4, 5)))
  println(show(bytes.read_string_at(good, 9223372036854775807, 2)))
  println(show(bytes.read_string_at(good, 0, -1)))
  let pre = bytes.from_list([0, 0, 0, 3, 255, 0x41, 0x42])
  println(show(bytes.read_string_be(pre, 0)))
  println(show(bytes.read_string_be(pre, -1)))
  println(show(bytes.read_string_be(bytes.from_list([0, 0, 0, 9, 1]), 0)))
}
"#;

const EXPECTED: &str = r#"3:��A:[239, 191, 189, 239, 191, 189, 65]
2:日�:[230, 151, 165, 239, 191, 189]
3:AÉ日:[97, 195, 169, 230, 151, 165]
1:�:[239, 191, 189]
0::[]
0::[]
0::[]
0::[]
3:�AB:[239, 191, 189, 65, 66]
0::[]
0::[]
"#;

#[test]
fn windowed_byte_readers_decode_invalid_utf8_lossily_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("read_string_lossy.almd");
    std::fs::write(&file, SOURCE).expect("source");
    for wasm in [false, true] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file);
        if wasm {
            c.args(["--target", "wasm"]);
        }
        let out = c.output().expect("almide run");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "wasm={wasm}");
    }
}
