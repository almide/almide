//! #4165: a derived `Codec.decode` error inside a `List` element carries the
//! element's index in its path (`at cmds[0]`, `at cmds[0].text`). Native framed
//! it through `almide_rt_value_decode_list`; the wasm leg decodes through the
//! derived `T.__list_dec_go` worker, which dropped the `[i]` (`at cmds`). Error
//! text is observable output, so both legs must print the same bytes.
use std::process::Command;

const SOURCE: &str = r#"import json

type Cmd: Codec = | Move(Int, Int) | Say { text: String, loud: Bool } | Stop

type Batch: Codec = { cmds: List[Cmd], last: Option[Cmd] }

fn show(s: String) -> String = match json.parse(s) {
  ok(v) => match Batch.decode(v) {
    ok(_) => "ok",
    err(e) => "err ${e}",
  },
  err(e) => "parse ${e}",
}

fn main() -> Unit = {
  println(show("{\"cmds\":[{\"Say\":{}}]}"))
  println(show("{\"cmds\":[{\"Say\":{\"text\":1}}]}"))
  println(show("{\"cmds\":[{\"Nope\":[]}]}"))
}
"#;

const EXPECTED: &str = "err missing field 'text', at cmds[0]\n\
err expected Str, received Int, at cmds[0].text\n\
err unknown variant for Cmd, at cmds[0]\n";

#[test]
fn list_element_decode_errors_carry_the_index_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("codec_list_index_path.almd");
    std::fs::write(&file, SOURCE).expect("source");
    for target in ["rust", "wasm"] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("run")
            .arg(&file)
            .args(["--target", target])
            .output()
            .expect("run");
        assert!(
            out.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "{target}");
    }
}
