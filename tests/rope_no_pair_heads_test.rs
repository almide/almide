//! #3575: with `head_dim` 0 or 1 a rope has no pair to rotate, so the result
//! is the input copied through, whatever the head count. The head-geometry
//! rule admits any head count when `head_dim` is 0, and both legs walked every
//! head: the wasm self-host recursed 2^63 times (no output in 20 s), while
//! native finished only because LLVM deleted the empty loop. Found by the
//! stdlib inspection's parity campaign (`float32.to_uint8`, seed 1791740389).
//!
//! Attention has the same shape: more heads than columns leaves a head
//! dimension of 0, every head writes nothing, and both legs still built and
//! normalised a score matrix per head (neither leg finished). The answer is the
//! zero matrix.
//!
//! Each run is given a deadline, so a regression fails instead of hanging.
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SOURCE: &str = r#"fn main() -> Unit = {
  let m = matrix.from_lists([[0.2, -42.5, 3.0], [1.0, 2.0, -0.5]])
  println("${matrix.to_lists(matrix.rope_rotate_at(m, 9223372036854775807, 0, -1.0, 1))}")
  println("${matrix.to_lists(matrix.rope_rotate_at(m, 9223372036854775807, 0, 10000.0, 0))}")
  println("${matrix.to_lists(matrix.rope_rotate_neox_at(m, 9223372036854775807, 0, 10000.0, 7))}")
  println("${matrix.to_lists(matrix.rope_rotate(m, 9223372036854775807, 0, 10000.0))}")
  println("${matrix.to_lists(matrix.rope_rotate_at(m, 3, 1, 10000.0, 2))}")
  println("${matrix.to_lists(matrix.multi_head_attention(m, m, m, 9223372036854775807))}")
  println("${matrix.to_lists(matrix.masked_multi_head_attention(m, m, m, 9223372036854775807))}")
}
"#;

const EXPECTED: &str = "[[0.2, -42.5, 3], [1, 2, -0.5]]
[[0.2, -42.5, 3], [1, 2, -0.5]]
[[0.2, -42.5, 3], [1, 2, -0.5]]
[[0.2, -42.5, 3], [1, 2, -0.5]]
[[0.2, -42.5, 3], [1, 2, -0.5]]
[[0, 0, 0], [0, 0, 0]]
[[0, 0, 0], [0, 0, 0]]
";

#[test]
fn a_rope_with_no_pair_copies_through_on_both_legs_without_walking_the_heads() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("rope_no_pairs.almd");
    std::fs::write(&file, SOURCE).expect("source");
    for wasm in [false, true] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.arg("run").arg(&file).stdout(Stdio::piped()).stderr(Stdio::piped());
        if wasm {
            c.args(["--target", "wasm"]);
        }
        let mut child = c.spawn().expect("almide run");
        let start = Instant::now();
        while child.try_wait().expect("wait").is_none() {
            if start.elapsed() > Duration::from_secs(120) {
                let _ = child.kill();
                panic!("wasm={wasm}: still running after 120 s (the heads are being walked)");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let out = child.wait_with_output().expect("output");
        assert!(out.status.success(), "wasm={wasm}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "wasm={wasm}");
    }
}
