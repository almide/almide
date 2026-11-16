//! #4782 — `io.read_to_end` / `io.read_exact` are linear in the input on the
//! native render, as on v0.69.1.
//!
//! Both gather their chunks into a `List[Bytes]` and finish with
//! `list.join(chunks, bytes.new())`, which reaches `bytes.__join`. Its
//! declaration is `@intrinsic("almide_rt_bytes_join")`, but the native floor
//! had no per-type rule for a `List[Bytes]` param, so the certified v1 render
//! linked the self-hosted body instead: a fold of `bytes.__concat(acc, ..)`
//! that copies the whole accumulator on every chunk. 32 MiB of stdin took
//! 0.037 s on v0.69.1 and 4.45 s after (×4 per doubling).
//!
//! Two checks:
//! - STRUCTURE: the shipped render of a `list.join` over `List[Bytes]` — an
//!   owned list and one borrowed through a param — calls the runtime body and
//!   builds no per-element concatenation.
//! - GROWTH: the built `io.read_to_end` / `io.read_exact` programs, timed at
//!   N and 8N bytes of stdin. Linear work grows ×8; the quadratic one grew
//!   ×50 and more. The bound is the ratio, never a wall-clock number.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn shipped_v1(name: &str, src: &str) -> String {
    almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("{name}: the v1 render walled: {e:?}\n{src}"))
}

#[test]
fn a_bytes_list_join_calls_the_runtime_body() {
    let owned = r#"effect fn main() -> Unit = {
  let xs = [bytes.from_list([1, 2]), bytes.from_list([3]), bytes.from_list([4, 5])]
  let j = list.join(xs, bytes.from_list([0]))
  println("${bytes.len(j)}")
}
"#;
    let borrowed = r#"fn glue(xs: List[Bytes]) -> Bytes = list.join(xs, bytes.new())

effect fn main() -> Unit = {
  let xs = [bytes.from_list([1, 2]), bytes.from_list([3])]
  println("${bytes.len(glue(xs))} ${list.len(xs)}")
}
"#;
    for (name, src) in [("owned", owned), ("borrowed", borrowed)] {
        let rust = shipped_v1(name, src);
        assert!(rust.contains("almide_rt_bytes_join("), "{name}: the join does not reach the runtime body:\n{rust}");
        assert!(!rust.contains("almide_rt_bytes_concat("), "{name}: the join concatenates per element:\n{rust}");
    }
}

/// Build `src` with the `almide` under test into `dir`.
fn build(dir: &std::path::Path, name: &str, src: &str) -> std::path::PathBuf {
    let file = dir.join(format!("{name}.almd"));
    std::fs::write(&file, src).unwrap();
    let exe = dir.join(name);
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", file.to_str().unwrap(), "-o", exe.to_str().unwrap()])
        .output()
        .expect("almide build");
    assert!(out.status.success(), "{name}: build failed: {}", String::from_utf8_lossy(&out.stderr));
    exe
}

/// The fastest of three runs of `exe` over `n` bytes of stdin; checks it
/// printed `n`.
fn best_of_three(exe: &std::path::Path, n: usize) -> Duration {
    let input = vec![b'a'; n];
    (0..3)
        .map(|_| {
            let start = Instant::now();
            let mut child = Command::new(exe).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("spawn");
            child.stdin.take().unwrap().write_all(&input).unwrap();
            let out = child.wait_with_output().unwrap();
            let took = start.elapsed();
            assert!(out.status.success(), "{}: exited {:?}", exe.display(), out.status);
            assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), n.to_string());
            took
        })
        .min()
        .unwrap()
}

#[test]
fn reading_stdin_to_the_end_grows_linearly() {
    const SMALL: usize = 2 << 20;
    const LARGE: usize = 8 * SMALL;
    let dir = std::env::temp_dir().join(format!("almd_bytes_join_linear_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let read_exact = |n: usize| {
        format!("import io\neffect fn main() -> Unit = {{\n  let b = io.read_exact(io.stdin()!, {n})!\n  println(\"${{bytes.len(b)}}\")\n}}\n")
    };
    let to_end = "import io\neffect fn main() -> Unit = {\n  let b = io.read_to_end(io.stdin()!)!\n  println(\"${bytes.len(b)}\")\n}\n";
    let to_end = build(&dir, "to_end", to_end);
    let cases = [
        ("read_to_end", to_end.clone(), to_end),
        ("read_exact", build(&dir, "exact_small", &read_exact(SMALL)), build(&dir, "exact_large", &read_exact(LARGE))),
    ];
    for (name, small, large) in &cases {
        let (s, l) = (best_of_three(small, SMALL), best_of_three(large, LARGE));
        let ratio = l.as_secs_f64() / s.as_secs_f64().max(1e-4);
        // ×8 input: linear is ≈×8, quadratic ≈×64. 24 leaves room for noise
        // on a small input and still refuses the quadratic shape.
        assert!(ratio < 24.0, "{name}: 8× the input took {ratio:.1}× the time ({s:?} → {l:?}) — not linear");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
