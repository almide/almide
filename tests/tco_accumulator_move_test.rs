//! The TCO accumulator MOVES instead of cloning (native O(n²) fix).
//!
//! CloneInsertion's blanket inside-a-loop-always-clone rule used to clone a
//! tail-recursive accumulator on every iteration: `build(n - 1, acc + [x])`
//! emitted `concat(acc.clone(), …)` — O(len) per pass, O(n²) for the loop —
//! while the wasm renderer's in-place reuse ran O(n). The nightly fuzzer
//! caught it as a Hang divergence (seed 1785824938231857375 index 303:
//! `build(1000000)` — 787s native vs 0.01s wasm). TailCallOptPass now owns
//! the clone/move decisions for its loop params (`tco_owned_params`): the
//! provably-final read moves, everything else clones explicitly, and rustc's
//! borrow checker re-proves every move.
//!
//! These tests pin the emitted Rust: the accumulator loop must contain no
//! `.clone()` of the accumulator param. If one reappears, the quadratic
//! regression is back — fix the pass, don't relax the assertion.
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in
//! the build step; locally run `cargo build --release` first).

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// Emit Rust for `source` and return the body of `fn <name>` (from its
/// `pub fn <name>` line to the first column-zero `}`).
fn emitted_fn(source: &str, name: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide-tco-move-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();

    let output = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("failed to spawn almide");
    let rust = String::from_utf8_lossy(&output.stdout).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        output.status.success(),
        "--target rust emit failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let needle = format!("fn {name}(");
    let start = rust
        .find(&needle)
        .unwrap_or_else(|| panic!("emitted Rust has no `{needle}`:\n{rust}"));
    let rest = &rust[start..];
    let end = rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn list_accumulator_loop_moves_instead_of_cloning() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    let body = emitted_fn(
        "fn build(n: Int, acc: List[Int]) -> List[Int] = if n <= 0 then acc\n\
         else build(n - 1, acc + [7])\n\
         \n\
         fn main() -> Unit = {\n\
           println(int.to_string(list.len(build(10, []))))\n\
         }\n",
        "build",
        "list",
    );
    assert!(
        !body.contains(".clone()"),
        "the list accumulator loop clones again (O(n²) regression):\n{body}"
    );
    assert!(
        body.contains("AlmideConcat::concat(acc,"),
        "expected the accumulator to MOVE into the concat:\n{body}"
    );
}

#[test]
fn string_accumulator_loop_moves_instead_of_cloning() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    let body = emitted_fn(
        "fn repeat(n: Int, acc: String) -> String = if n <= 0 then acc\n\
         else repeat(n - 1, acc + \"x\")\n\
         \n\
         fn main() -> Unit = {\n\
           println(int.to_string(string.len(repeat(10, \"\"))))\n\
         }\n",
        "repeat",
        "string",
    );
    assert!(
        !body.contains(".clone()"),
        "the string accumulator loop clones again (O(n²) regression):\n{body}"
    );
    // The String concat must also route through AlmideConcat (push_str, keeps
    // capacity) — a format!("{}{}") reallocates every step and stays O(n²)
    // even with the move.
    assert!(
        body.contains("AlmideConcat::concat(acc,"),
        "expected the string accumulator to MOVE into an in-place concat:\n{body}"
    );
}
