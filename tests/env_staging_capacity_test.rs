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
    // The ceiling is on the environ BLOCK, so the sizes are reached with
    // several variables rather than one: Linux caps a single environment
    // string at 128 KiB (MAX_ARG_STRLEN) and refuses the exec outright,
    // where macOS only bounds the total.
    //
    // 4 x 60,000 fits the old page; 4 x 100,000 is past its 261,120 cliff;
    // 8 x 100,000 is past it by three times.
    for (count, len) in [(1usize, 16usize), (4, 60_000), (4, 100_000), (8, 100_000)] {
        let value = marked(len);
        let mut env: Vec<(String, &str)> = vec![("BIGVAR".to_string(), value.as_str())];
        for i in 1..count {
            env.push((format!("FILLER{i}"), value.as_str()));
        }
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let want = format!("len={len} tail=TAIL\n");
        assert_eq!(run(&native, false, &[], &env), want, "native {count}x{len}");
        assert_eq!(run(&core, true, &[], &env), want, "core wasm {count}x{len}");
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
    // Same reason as above: argv's per-string cap is 128 KiB on Linux, so the
    // block is grown with several arguments instead of one giant one.
    for (count, len) in [(1usize, 16usize), (4, 100_000), (8, 100_000)] {
        let value = marked(len);
        let args: Vec<&str> = std::iter::repeat_n(value.as_str(), count).collect();
        let want = format!("count={count} len={len} tail=TAIL\n");
        assert_eq!(run(&native, false, &args, &[]), want, "native {count}x{len}");
        assert_eq!(run(&core, true, &args, &[]), want, "core wasm {count}x{len}");
    }
    // Several small arguments keep their order and their framing.
    let want = "count=3 len=3 tail=one\n";
    assert_eq!(run(&native, false, &["one", "two", "three"], &[]), want, "native multi");
    assert_eq!(run(&core, true, &["one", "two", "three"], &[]), want, "core wasm multi");
}
