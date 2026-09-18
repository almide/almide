//! #2114: compare complete lines, UTF-8, CRLF, EOF, and the shared stdin cursor.
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn run(program: &Path, wasm: bool, input: &[u8]) -> Vec<u8> {
    let mut command = if wasm { Command::new("wasmtime") } else { Command::new(program) };
    if wasm { command.arg("run").arg(program); }
    let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().expect("run fixture");
    let mut stdin = child.stdin.take().expect("stdin");
    // Drain stdout while sending input: long echoed lines exceed pipe capacity.
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input).expect("write input"));
    let output = child.wait_with_output().expect("wait");
    writer.join().expect("writer");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}

#[test]
fn read_line_has_no_fixed_byte_cap_on_native_core_or_component() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("lines.almd");
    std::fs::write(&source, r#"import io
effect fn main() -> Unit = {
  println(io.read_line())
  println(io.read_line())
  println(int.to_string(io.read_byte()))
  println(io.read_all())
}
"#).expect("source");
    let mut artifacts = Vec::new();
    for (name, flags) in [
        ("native", vec![]),
        ("core.wasm", vec!["--target", "wasm"]),
        ("component.wasm", vec!["--target", "wasm", "--component"]),
    ] {
        let artifact = dir.path().join(name);
        let output = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("build").arg(&source).args(flags).arg("-o").arg(&artifact)
            .output().expect("build");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        artifacts.push(artifact);
    }
    let mut lines = vec![String::new(), "a\rb".into(), "end\r\r".into()];
    for n in [4094, 4095, 4096, 4097, 8193, 65537] { lines.push("x".repeat(n)); }
    lines.push(format!("{}日本語🙂", "x".repeat(4095)));
    for line in lines {
        for ending in ["", "\n", "\r\n"] {
            let input = if ending.is_empty() { line.clone() }
                else { format!("{line}{ending}second\nZtail") };
            let expected = if ending.is_empty() {
                format!("{}\n\n-1\n\n", line.trim_end_matches('\r'))
            } else {
                format!("{}\nsecond\n90\ntail\n", line.trim_end_matches('\r'))
            };
            for (index, artifact) in artifacts.iter().enumerate() {
                assert_eq!(run(artifact, index != 0, input.as_bytes()), expected.as_bytes(),
                    "{}: {} bytes, ending {ending:?}", artifact.display(), line.len());
            }
        }
    }
}
