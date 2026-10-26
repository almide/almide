//! #3750: building a `value.Path` step by step is linear on the wasm leg.
//!
//! The self-hosted path is a `List[String]` of segments and `value.key` /
//! `value.index` return `path + [seg]`. With the var shared into the call, every
//! step copied every segment: 100 000 steps ran out of memory on wasm while
//! native took milliseconds. The rebind now moves the dying var into the call
//! (selfhost_rebind_move_cli_test.rs), so the append lands in place.
//!
//! Measured through the product route (`almide run --target wasm`): 4x the
//! steps must not take the bytes requested past 6x (linear lands near 4,
//! the per-step copy near 16), and both legs print the same.
use std::process::Command;

fn run(src: &str, target: Option<&str>) -> (String, Option<u64>) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("path.almd");
    std::fs::write(&file, src).expect("source");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&file);
    if let Some(t) = target {
        cmd.args(["--target", t]).env("ALMIDE_WASM_ALLOC_COUNT", "1");
    }
    let out = cmd.output().expect("run");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{stderr}");
    let bytes = stderr.split_whitespace().find_map(|w| w.strip_prefix("bytes=")?.parse().ok());
    (String::from_utf8_lossy(&out.stdout).to_string(), bytes)
}

fn program(step: &str, steps: u32) -> String {
    format!(
        "effect fn main() -> Unit = {{
  var x: value.Path = value.root()
  var i = 0
  while i < {steps} {{
    x = {step}
    i = i + 1
  }}
  println(\"${{option.is_some(value.get_path(value.object([(\\\"a\\\", value.int(1))]), x))}}\")
}}
"
    )
}

#[test]
fn a_path_built_step_by_step_is_linear_in_bytes_on_wasm() {
    for step in ["value.key(x, \"a\")", "value.index(x, 0)"] {
        let (small_out, small) = run(&program(step, 2048), Some("wasm"));
        let (large_out, large) = run(&program(step, 8192), Some("wasm"));
        let (native_out, _) = run(&program(step, 8192), None);
        assert_eq!(small_out, "false\n", "{step}");
        assert_eq!(large_out, native_out, "{step}: native and wasm differ");
        let (small, large) = (small.expect("bytes= on stderr"), large.expect("bytes= on stderr"));
        assert!(
            large < 6 * small,
            "{step}: 4x the steps requested {large} bytes against {small} ({:.1}x) — each step copies the path",
            large as f64 / small as f64
        );
    }
}
