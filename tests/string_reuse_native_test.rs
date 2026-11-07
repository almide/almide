//! Declaration-driven reuse must move owned strings, preserve borrowed
//! sources, and keep Unicode/alias observations on both compiler legs.
use std::process::Command;

const SOURCE: &str = r#"
fn pipeline(n: Int) -> String =
  string.split(" a${n} , b ", ",") |> list.map((s) => string.to_upper(string.trim(s))) |> list.join("|")
fn borrowed(s: String) -> String = {
  let changed = string.to_upper(string.trim(s))
  changed + ":" + s
}
fn loop_read(xs: List[String]) -> String =
  xs |> list.map((s) => string.to_lower(string.trim(s))) |> list.join("|")
fn recurse(s: String, n: Int) -> String =
  if n <= 0 then s else recurse(string.trim(s), n - 1)
effect fn main() -> Unit = {
  println(pipeline(7))
  let text = "  keep${8}  "
  let alias = [text, text]
  println(borrowed(text))
  println(list.join(alias, ";"))
  println(loop_read([" A ", " B "]))
  println(recurse("  done  ", 3))
  var v = "  a${9}  "
  v = string.trim(v)
  v = string.to_upper(v)
  println(v)
  println(string.to_upper(string.trim("\u{2003}éßıΣ🦀\u{2009}")))
  println(string.to_lower(string.trim("  ΟΣ İK  ")))
  println("[${string.trim_start("  x  ")}] [${string.trim_end("  x  ")}]")
  println("[${string.to_upper(string.trim(" \t\n"))}] [${string.to_lower("")}]")
}
"#;
const EXPECTED: &str = "A7|B\nKEEP8:  keep8  \n  keep8  ;  keep8  \na|b\ndone\nA9\nÉßıΣ🦀\nος İK\n[x  ] [  x]\n[] []\n";

#[test]
fn reuse_moves_dying_strings_and_preserves_every_observation() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("reuse.almd");
    std::fs::write(&file, SOURCE).unwrap();
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let out = Command::new(&bin).arg("emit").arg(&file).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8(out.stdout).unwrap();
    let pipeline = rust.split("pub fn pipeline(").nth(1).unwrap().split("\n}").next().unwrap();
    assert!(pipeline.contains("almide_rt_string_to_upper_owned("), "{pipeline}");
    assert!(pipeline.contains("almide_rt_string_trim_owned("), "{pipeline}");
    let borrowed = rust.split("pub fn borrowed(").nth(1).unwrap().split("\n}").next().unwrap();
    assert!(!borrowed.contains("almide_rt_string_trim_owned(s)"), "{borrowed}");
    for lane in ["native", "v0", "wasm"] {
        let mut cmd = Command::new(&bin);
        cmd.arg("run").arg(&file);
        if lane == "v0" { cmd.arg("--no-verified").env("ALMIDE_NO_VERIFIED_OK", "1"); }
        if lane == "wasm" { cmd.args(["--target", "wasm"]); }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{lane}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "{lane}");
        if lane == "v0" {
            assert_eq!(String::from_utf8_lossy(&out.stderr), "[almide] ALMIDE_NO_VERIFIED_OK is set: re-enable the retired `--no-verified` legs (the v0 fallback) instead of refusing the flag\n");
        } else {
            assert!(out.stderr.is_empty(), "{lane}: {}", String::from_utf8_lossy(&out.stderr));
        }
    }
}
