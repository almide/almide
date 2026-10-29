//! #4115: on wasm, a module global passed straight into the call that is a
//! function's result was planned as a frame-tail move (a once-used local is
//! dead there) and left empty for every later reader; native was correct and
//! both legs exited 0. A global outlives the frame, so it never moves there.
//! The callee forwards the global to a stdlib fn wasm runs from a bundled
//! Almide body, which is what put the call on the move plan.
use std::process::Command;

const SOURCE: &str = r#"var arena = bytes.from_list([1, 2, 3])

fn nx(b: Bytes, pos: Int) -> (Int?, Int) = {
  let (v, next) = bytes.next_u16_le(b, pos)
  (v, next)
}

fn show(pos: Int) -> String = "${nx(arena, pos).1}"

fn main() -> Unit = {
  println(show(0))
  println(show(1))
  println("${bytes.len(arena)}")
}
"#;

#[test]
fn a_global_passed_into_a_result_call_keeps_its_value_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("global_result_call.almd");
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
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "2\n3\n3\n",
            "{target}"
        );
    }
}
