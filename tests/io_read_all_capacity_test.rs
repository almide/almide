//! #2116: `io.read_all` is bounded by the guest heap, not by the host's
//! park span. Before this, stock WASI refused at 257,024 bytes wearing the
//! unsupported-op message and a p2 component died at 326,657 with no
//! diagnostic at all, against a native leg with no bound.
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn run(program: &Path, wasm: bool, input: &[u8]) -> String {
    let mut command = if wasm { Command::new("wasmtime") } else { Command::new(program) };
    if wasm {
        command.arg("run").arg(program);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run fixture");
    let mut stdin = child.stdin.take().expect("stdin");
    // Megabyte inputs exceed the pipe buffer: feed and drain concurrently.
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child.wait_with_output().expect("wait");
    writer.join().expect("writer");
    assert!(
        output.status.success(),
        "{} exited {:?}: {}",
        program.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "{}: {}", program.display(), String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("utf-8 stdout")
}

fn build(dir: &Path, source: &Path) -> Vec<(&'static str, std::path::PathBuf)> {
    let mut artifacts = Vec::new();
    for (name, flags) in [
        ("native", vec![]),
        ("core.wasm", vec!["--target", "wasm"]),
        ("component.wasm", vec!["--target", "wasm", "--component"]),
    ] {
        let artifact = dir.join(name);
        let output = Command::new(env!("CARGO_BIN_EXE_almide"))
            .arg("build")
            .arg(source)
            .args(flags)
            .arg("-o")
            .arg(&artifact)
            .output()
            .expect("build");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        artifacts.push((name, artifact));
    }
    artifacts
}

#[test]
fn read_all_has_no_park_sized_cap_on_native_core_or_component() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("all.almd");
    std::fs::write(
        &source,
        r#"import io
effect fn main() -> Unit = {
  let s = io.read_all()
  println("len=${string.len(s)}")
  println("head=${string.slice(s, 0, 3)}")
  println("tail=${string.slice(s, string.len(s) - 3, string.len(s))}")
}
"#,
    )
    .expect("source");
    let artifacts = build(dir.path(), &source);

    // 4096 is the per-take chunk; 257,024 and 326,657 were the two park
    // cliffs; 1 MiB is past both by an order of magnitude. Head and tail
    // markers catch a chunk delivered out of order, which a length alone
    // would not see.
    for size in [6usize, 4095, 4096, 4097, 8192, 257_023, 257_024, 326_657, 1_000_000] {
        let mut input = vec![b'a'; size];
        input[..3].copy_from_slice(b"HEA");
        let end = size - 3;
        input[end..].copy_from_slice(b"TIL");
        let expected = format!("len={size}\nhead=HEA\ntail=TIL\n");
        for (name, artifact) in &artifacts {
            assert_eq!(run(artifact, *name != "native", &input), expected, "{name}: {size} bytes");
        }
    }
}

#[test]
fn read_all_answers_an_empty_stream_on_every_leg() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("empty.almd");
    std::fs::write(
        &source,
        r#"import io
effect fn main() -> Unit = println("len=${string.len(io.read_all())}")
"#,
    )
    .expect("source");
    let artifacts = build(dir.path(), &source);
    for (size, expected) in [(0usize, "len=0\n"), (1, "len=1\n"), (5, "len=5\n")] {
        for (name, artifact) in &artifacts {
            assert_eq!(run(artifact, *name != "native", &vec![b'a'; size]), expected, "{name}: {size}");
        }
    }
}

#[test]
fn read_all_agrees_across_legs_on_multibyte_text_spanning_chunks() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("utf8.almd");
    std::fs::write(
        &source,
        r#"import io
effect fn main() -> Unit = println("chars=${string.len(io.read_all())}")
"#,
    )
    .expect("source");
    let artifacts = build(dir.path(), &source);
    // 3-byte characters never align with the 4096-byte take, so every
    // chunk boundary falls inside one of them.
    let input = "あ".repeat(20_000);
    let mut answers = Vec::new();
    for (name, artifact) in &artifacts {
        answers.push(run(artifact, *name != "native", input.as_bytes()));
    }
    assert_eq!(answers[0], "chars=20000\n");
    assert!(answers.windows(2).all(|w| w[0] == w[1]), "legs disagree: {answers:?}");
}

#[test]
fn read_all_continues_the_cursor_a_line_read_left() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("cursor.almd");
    std::fs::write(
        &source,
        r#"import io
effect fn main() -> Unit = {
  println("first=${io.read_line()}")
  println("rest=${string.len(io.read_all())}")
}
"#,
    )
    .expect("source");
    let artifacts = build(dir.path(), &source);
    let input = format!("head\n{}", "b".repeat(300_000));
    let expected = "first=head\nrest=300000\n";
    for (name, artifact) in &artifacts {
        assert_eq!(run(artifact, *name != "native", input.as_bytes()), expected, "{name}");
    }
}
