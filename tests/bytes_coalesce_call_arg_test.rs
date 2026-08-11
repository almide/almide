//! `Bytes?/Matrix? ?? fallback` inline in a call argument must compile (#1210).
//!
//! On released 0.56.0 this emitted INVALID Rust: the `??` lowers to a `match`
//! whose arms are RcCow-typed (the Bytes/Matrix value convention, #617), and
//! when that match is borrowed directly as a raw-runtime call argument
//! (`almide_rt_bytes_len(&match …)`), rustc propagates the expected `&Vec<u8>`
//! into the arms — deref coercion never gets a chance — and the build dies with
//! E0308 "expected `Vec<u8>`, found `RcCow<Vec<u8>>`". A let-bound `??` and an
//! explicit `match` compiled fine, and List/String/Map fallbacks were
//! unaffected, which is why nothing caught it: the trap is exactly
//! (RcCow-valued type) × (type-propagating rvalue) × (call-arg borrow).
//!
//! The fix renders those borrows as `&*(…)` — the explicit spelling of the
//! deref layer coercion strips from a plain `&var`. Lives at the compiler
//! level (not spec/) because the wasm leg walls honestly on heap-result `??`
//! over an Option, so a corpus file would fall off the wasm leg and grow the
//! fallback ratchet to assert a RUST-target property.

use std::io::Write;
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

/// Compile + run `src` on the Rust target; assert it prints `expected`.
fn run_prints(name: &str, src: &str, expected: &str) {
    let dir = std::env::temp_dir().join(format!("almd_1210_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.almd");
    let mut f = std::fs::File::create(&file).unwrap();
    f.write_all(src.as_bytes()).unwrap();
    drop(f);
    let out = Command::new(almide_bin())
        .args(["run", file.to_str().unwrap()])
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        out.status.success(),
        "[{name}] almide run failed (the #1210 invalid-Rust shape?):\n{stderr}"
    );
    assert_eq!(stdout.trim_end(), expected, "[{name}] wrong output");
}

#[test]
fn bytes_coalesce_inline_in_call_arg_compiles() {
    run_prints(
        "bytes_len",
        r#"import bytes
fn pick(flag: Bool) -> Bytes? = if flag then some(bytes.from_list([7])) else none
fn main() -> Unit = println("${bytes.len(pick(false) ?? bytes.new(0))}")
"#,
        "0",
    );
}

#[test]
fn bytes_coalesce_taken_side_still_unwraps() {
    // The fix must not break the Some side: `&*(match …)` derefs the SAME
    // RcCow layer either arm produces.
    run_prints(
        "bytes_some",
        r#"import bytes
fn pick(flag: Bool) -> Bytes? = if flag then some(bytes.from_list([7, 8])) else none
fn main() -> Unit = println("${bytes.len(pick(true) ?? bytes.new(0))}")
"#,
        "2",
    );
}

#[test]
fn matrix_coalesce_inline_in_call_arg_compiles() {
    run_prints(
        "matrix_rows",
        r#"import matrix
fn pick(flag: Bool) -> Matrix? = if flag then some(matrix.ones(2, 2)) else none
fn main() -> Unit = println("${matrix.rows(pick(false) ?? matrix.zeros(1, 3))}")
"#,
        "1",
    );
}
