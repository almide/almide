//! #4215: in a `test` block the fallible spelling `xs |> list.map((x) =>
//! f(x)!)!` stays the plain map (the callback's `!` unwraps), so its outer
//! `!` is a no-op. The pipe lowering built a `Result` around the collected
//! `Vec` instead, and the native build failed with
//! `no method named unwrap found for struct Vec` (rustc E0599).

use std::process::Command;

const SRC: &str = r#"import args

effect fn twice(n: Int) -> Int = n * 2

test "piped fallible map over a user effect fn" {
  let ys = [1, 2, 3] |> list.map((n) => twice(n)!)!
  assert_eq(ys, [2, 4, 6])
}

test "piped fallible map over an Almide-bodied stdlib effect fn" {
  let flags = ["--a", "--b", "--c"] |> list.map((n) => args.flag(n)!)!
  assert_eq(list.len(flags), 3)
}
"#;

#[test]
fn a_piped_fallible_map_in_a_test_block_builds_natively() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("pipe_map_test.almd");
    std::fs::write(&file, SRC).expect("write fixture");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["test", file.to_str().expect("path"), "--target", "rust"])
        .current_dir(dir.path())
        .output()
        .expect("run almide test");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "almide test failed:\nstdout:\n{stdout}\nstderr:\n{stderr}");
}
