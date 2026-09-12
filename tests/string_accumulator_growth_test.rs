//! #2117: a string accumulator past the largest size class keeps growing.
//! Above `16 << 15` the allocator stops rounding requests up to a class, so
//! `$str_append`'s in-place window could never fire again: every append
//! reallocated the whole string and `$free` abandons blocks that big, which
//! walked an 8 MB accumulator into C-197 on a machine with gigabytes free.
//! Native has no such bound, so the ceiling was also a divergence.
use std::path::Path;
use std::process::Command;

fn build(dir: &Path, source: &Path, name: &str, flags: &[&str]) -> std::path::PathBuf {
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
    artifact
}

fn run(program: &Path, wasm: bool) -> String {
    let mut command = if wasm { Command::new("wasmtime") } else { Command::new(program) };
    if wasm {
        command.arg("run").arg(program);
    }
    let output = command.output().expect("run");
    assert!(
        output.status.success(),
        "{} exited {:?}: {}",
        program.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8")
}

#[test]
fn an_accumulator_past_the_class_ceiling_completes_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("acc.almd");
    // Doubling reaches the 16 << 15 ceiling in nineteen appends, then two
    // thousand small appends run past it — the shape that used to abandon
    // a gigabyte of outgrown blocks and abort. Reaching the size by
    // doubling keeps the native leg (whose `+` copies) fast enough to be
    // the oracle.
    std::fs::write(
        &source,
        r#"effect fn main() -> Unit = {
  var acc = "a"
  var d = 0
  while d < 19 {
    acc = acc + acc
    d = d + 1
  }
  var n = 2000
  while n > 0 {
    acc = acc + "aaaaaaaaaaaaaaaa"
    n = n - 1
  }
  println("len=${string.len(acc)}")
  println("tail=${string.slice(acc, string.len(acc) - 3, string.len(acc))}")
}
"#,
    )
    .expect("source");
    // A 32 MiB ceiling is the assertion: the live accumulator never passes
    // 2 MiB, so only the abandoned copies of the old shape can reach it
    // (they summed to about a gigabyte). --heap-cap is honoured by both
    // legs, so one number states the promise for both.
    let cap = ["--heap-cap", "33554432"];
    let native = build(dir.path(), &source, "native", &cap);
    let core = build(dir.path(), &source, "core.wasm", &[&cap[..], &["--target", "wasm"]].concat());
    let expected = "len=556288\ntail=aaa\n";
    assert_eq!(run(&native, false), expected, "native");
    assert_eq!(run(&core, true), expected, "core wasm");
}
