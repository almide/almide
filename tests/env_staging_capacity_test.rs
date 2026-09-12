//! #2120: `env.get` and `env.args` answer the value, not a wrong one, when the
//! result outgrows the park's staging page. Before this, a stock-WASI artifact
//! answered `none` for a variable that was set and `[]` for a process that was
//! given arguments — both ordinary, valid-looking answers, at a cliff native
//! does not have (the park's data room, 261,120 bytes).
use std::path::Path;
use std::process::Command;

fn build(dir: &Path, source: &Path, name: &str, flags: &[&str]) -> std::path::PathBuf {
    let artifact = dir.join(name);
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("build")
        .arg(source)
        .args(flags)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("build");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    artifact
}

fn run(program: &Path, wasm: bool, args: &[&str], env: &[(&str, &str)]) -> String {
    let mut command = if wasm { Command::new("wasmtime") } else { Command::new(program) };
    if wasm {
        command.arg("run");
        for (k, v) in env {
            command.arg("--env").arg(format!("{k}={v}"));
        }
        command.arg(program);
    } else {
        for (k, v) in env {
            command.env(k, v);
        }
    }
    command.args(args);
    let out = command.output().expect("run");
    assert!(
        out.status.success(),
        "{} exited {:?}: {}",
        program.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8")
}

fn marked(len: usize) -> String {
    format!("{}TAIL", "a".repeat(len - 4))
}

#[test]
fn env_get_answers_a_value_larger_than_the_staging_page() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("get.almd");
    std::fs::write(
        &source,
        r#"import env
effect fn main() -> Unit = {
  let v = env.get("BIGVAR") ?? "MISSING"
  println("len=${string.len(v)} tail=${string.slice(v, string.len(v) - 4, string.len(v))}")
}
"#,
    )
    .expect("source");
    let native = build(dir.path(), &source, "native", &[]);
    let core = build(dir.path(), &source, "core.wasm", &["--target", "wasm"]);
    // 260,000 fits the old page; 262,000 is just past its 261,120 cliff;
    // 900,000 is past it by an order of magnitude.
    for len in [16usize, 260_000, 262_000, 900_000] {
        let value = marked(len);
        let want = format!("len={len} tail=TAIL\n");
        assert_eq!(run(&native, false, &[], &[("BIGVAR", &value)]), want, "native {len}");
        assert_eq!(run(&core, true, &[], &[("BIGVAR", &value)]), want, "core wasm {len}");
    }
}

#[test]
fn env_args_answers_arguments_larger_than_the_staging_page() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("args.almd");
    std::fs::write(
        &source,
        r#"import env
effect fn main() -> Unit = {
  let a = env.args()
  let first = list.get(a, 0) ?? ""
  println("count=${list.len(a)} len=${string.len(first)} tail=${string.slice(first, string.len(first) - 4, string.len(first))}")
}
"#,
    )
    .expect("source");
    let native = build(dir.path(), &source, "native", &[]);
    let core = build(dir.path(), &source, "core.wasm", &["--target", "wasm"]);
    for len in [16usize, 262_000, 900_000] {
        let value = marked(len);
        let want = format!("count=1 len={len} tail=TAIL\n");
        assert_eq!(run(&native, false, &[&value], &[]), want, "native {len}");
        assert_eq!(run(&core, true, &[&value], &[]), want, "core wasm {len}");
    }
    // Several small arguments keep their order and their framing.
    let want = "count=3 len=3 tail=one\n";
    assert_eq!(run(&native, false, &["one", "two", "three"], &[]), want, "native multi");
    assert_eq!(run(&core, true, &["one", "two", "three"], &[]), want, "core wasm multi");
}
