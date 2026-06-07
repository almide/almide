//! Regression guard for compile-time native-stack bounds (contract C-059).
//!
//! @contract: C-059
//!
//! A wide function body — thousands of sibling statements in one block — must
//! compile within a SMALL native stack. The Perceus RC-insertion pass walks the
//! function-body statement chain iteratively (`pass_perceus.rs`), so an
//! N-statement body is O(1) native stack, not O(N). It formerly recursed once
//! per statement and overflowed: ~5460 statements blew the 256 MiB driver
//! thread, and far fewer blew a small OS main stack (Windows' 1 MiB made it a
//! release-blocking crash).
//!
//! We pin a deliberately small driver stack via `ALMIDE_COMPILER_STACK` (the
//! analogue of rustc's `RUST_MIN_STACK`) and compile a body far wider than that
//! stack could ever hold under the old per-statement recursion. If anyone
//! reintroduces the recursion, this build overflows and the test fails.

use std::process::Command;

fn almide_bin() -> String {
    // The freshly-built binary under test.
    env!("CARGO_BIN_EXE_almide").to_string()
}

/// One block, `n` sibling statements, each binding + using a heap String so the
/// Perceus Inc/Dec chain is threaded across the whole width.
fn wide_program(n: usize) -> String {
    let mut s = String::from("effect fn main() -> Unit = {\n  var total: Int = 0\n");
    for k in 0..n {
        let rep = (k % 7) + 1;
        s.push_str(&format!("  let s{k} = string.repeat(\"ab\", {rep})\n"));
        s.push_str(&format!("  total = total + string.len(s{k})\n"));
    }
    s.push_str("  println(int.to_string(total))\n}\n");
    s
}

#[test]
fn wide_function_body_compiles_in_small_stack() {
    // 1000 sibling statements on a deliberately small 2 MiB driver stack. Under
    // the former per-statement recursion (~48 KiB/frame) a 2 MiB stack holds only
    // ~40 frames, so this body would overflow ~25x over; the iterative pass fits
    // it easily (a 1 MiB stack compiles 5000+ such statements). The margin is
    // wide enough that any regression to per-statement recursion fails here.
    const STACK_BYTES: usize = 2 * 1024 * 1024;
    let dir = std::env::temp_dir().join("almide_compiler_stack_test");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("wide.almd");
    let out = dir.join("wide.wasm");
    std::fs::write(&src, wide_program(1000)).unwrap();

    let result = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "-o", out.to_str().unwrap()])
        .env("ALMIDE_COMPILER_STACK", STACK_BYTES.to_string())
        .output()
        .expect("failed to spawn almide build");

    assert!(
        result.status.success(),
        "compiling a 1000-statement body on a {} MiB driver stack failed — the \
         Perceus chain walk likely regressed to per-statement recursion.\nstderr:\n{}",
        STACK_BYTES / (1024 * 1024),
        String::from_utf8_lossy(&result.stderr),
    );
    assert!(out.exists(), "wasm output was not produced");
}
