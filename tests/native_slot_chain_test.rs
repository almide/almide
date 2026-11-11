//! #4661: consecutive writes of a module-level buffer keep it in a local
//! between them (`native_slot_chain`): each write went through the slot's
//! thread-local cell twice, where a local buffer is written in place.
//!
//! The program prints the same text on the native render (the ownership
//! certificate required) and on wasm, and the rendered function touches the
//! slot once per run of writes, not once per write. No wall-clock assertion.

use std::process::Command;

const BUFFER: &str = r#"var buf = bytes.zeros(64)
var seen = 0

fn peek() -> Int = bytes.len(buf) + seen

// Six writes in a row: one run.
fn fill(o: Int, x: Float) -> Unit = {
  bytes.put_f32_le(buf, o, x)
  bytes.put_f32_le(buf, o + 4, x * 2.0)
  bytes.put_f32_le(buf, o + 8, x * 3.0)
  bytes.put_u8(buf, o + 12, 7)
  bytes.put_u8(buf, o + 13, 9)
  bytes.put_f32_le(buf, o + 16, 0.5 - x)
}

// A call that reads the buffer between two writes: two runs.
fn split(o: Int) -> Unit = {
  bytes.put_u8(buf, o, 1)
  seen = seen + peek()
  bytes.put_u8(buf, o + 1, 2)
}

effect fn main() -> Unit = {
  let before = buf
  for i in 0..<3 { fill(i * 20, float.from(i) + 0.25) }
  split(62)
  var sum = 0
  for i in 0..<64 { sum = sum + (bytes.get(buf, i) ?? 0) * (i + 1) }
  let first = bytes.get_f32_le(buf, 4) ?? -1.0
  println("${sum} ${first} ${seen} ${bytes.len(before)} ${bytes.get(before, 12) ?? -1} ${bytes.get(buf, 12) ?? -1}")
}
"#;

fn run(src: &std::path::Path, target: Option<&str>) -> String {
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let mut cmd = Command::new(bin);
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("almide");
    assert!(out.status.success(), "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn writes_in_a_row_print_the_same_on_both_targets() {
    let dir = std::env::temp_dir().join(format!("almide-slot-chain-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, BUFFER).unwrap();
    let (native, wasm) = (run(&src, None), run(&src, Some("wasm")));
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(native, wasm);
    assert!(native.ends_with(" 64 0 7\n"), "{native}");
}

#[test]
fn a_run_of_writes_touches_the_slot_once() {
    let rust = almide_mir::pipeline::try_render_rust_source(BUFFER).expect("the certified render");
    let body = |name: &str| {
        let at = rust.find(&format!("fn almd_{name}(")).unwrap_or_else(|| panic!("no {name}: {rust}"));
        let end = rust[at..].find("\n}\n").expect("the fn's end");
        rust[at..at + end].to_string()
    };
    // `fill`: one take and one store for its six writes.
    assert_eq!(body("fill").matches("ALMD_HSLOT_").count(), 2, "{}", body("fill"));
    // `split`: the read between its two writes keeps both runs.
    assert!(body("split").matches("ALMD_HSLOT_").count() >= 4, "{}", body("split"));
}
