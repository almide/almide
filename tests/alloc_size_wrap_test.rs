//! #4396 (C-197): the structural leg sizes blocks in i32, and three size
//! computations WRAPPED into a small request where the program asked for one
//! wasm32 cannot hold. The block came back small, the caller copied the full
//! length into it, and the run ended in a raw `out of bounds memory access`
//! trap (or wrote over live blocks when the small block sat low in memory):
//!
//! - `$alloc`'s own `len + PAYLOAD + 3` for a len within 15 of 2^32;
//! - the sum of two lengths of ONE block (`s + s`) past 2^32, in `$concat`
//!   and in `$str_append`'s in-place window;
//! - the doubled capacity of a growing block at 2^31 (`$list_push`,
//!   `$bytes_push`, `$map_reserve`, `$str_append`).
//!
//! Each now saturates, and `$alloc` refuses the saturated request in C-197's
//! defined form: `Error: out of memory`, exit 1, the output before it kept.
//! Native satisfies these programs from host memory (it prints the length):
//! that is C-197's contracted divergence, the threshold being structural per
//! leg, so this pins the wasm leg's FORM alone, as alloc_bump_wrap_test does.
//! Each probe holds 2-4 GiB of linear memory, so the cases run one at a time.

use std::process::Command;
use std::sync::Mutex;

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let release = root.join("target/release/almide");
    if release.exists() {
        return release.to_str().unwrap().to_string();
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

fn assert_defined_oom(name: &str, program: &str) {
    let _serial = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join(format!("{name}.almd"));
    std::fs::write(&src, program).expect("write probe");
    let run = Command::new(almide_bin())
        .args(["run", "--target", "wasm"])
        .arg(&src)
        .output()
        .expect("spawn almide run --target wasm");
    let out = String::from_utf8_lossy(&run.stdout);
    let err = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "{name}: exit 1 expected; stdout={out} stderr={err}");
    assert_eq!(out, "before\n", "{name}: the output before the exhaustion survives");
    assert!(err.contains("Error: out of memory"), "{name}: the defined C-197 line expected, got: {err}");
    assert!(
        !err.contains("out of bounds") && !err.contains("wasm trap"),
        "{name}: a raw trap is not a permitted form: {err}"
    );
}

/// `$alloc(len)` for `len` within 15 of 2^32: two 2^31 - 3 byte halves sum to
/// 0xFFFF_FFFA, and `len + 15` wrapped to a 12-byte class-0 block.
#[test]
fn alloc_length_wrap_is_the_defined_oom_abort() {
    assert_defined_oom(
        "alloc_len_wrap",
        r#"fn main() -> Unit = {
  println("before")
  let s = string.repeat("a", 2147483645)
  let t = s + s
  println("unreachable ${string.len(t)}")
}
"#,
    );
}

/// `$concat`: `s + s` of a 2^31-byte string sums to exactly 2^32, which
/// wrapped to a 0-byte request.
#[test]
fn concat_length_sum_wrap_is_the_defined_oom_abort() {
    assert_defined_oom(
        "concat_sum_wrap",
        r#"fn main() -> Unit = {
  println("before")
  let s = string.repeat("a", 2147483648)
  let t = s + s
  println("unreachable ${string.len(t)}")
}
"#,
    );
}

/// `$str_append`: the accumulator form of the same sum (`s = s + s`).
#[test]
fn str_append_length_sum_wrap_is_the_defined_oom_abort() {
    assert_defined_oom(
        "str_append_sum_wrap",
        r#"fn main() -> Unit = {
  println("before")
  var s = string.repeat("a", 2147483648)
  s = s + s
  println("unreachable ${string.len(s)}")
}
"#,
    );
}

/// `$list_push`: a full 2^28-Int list has cap 2^31 bytes, whose doubling
/// wrapped to 0 (then the 4-slot minimum).
#[test]
fn list_push_capacity_doubling_wrap_is_the_defined_oom_abort() {
    assert_defined_oom(
        "list_push_doubling_wrap",
        r#"fn main() -> Unit = {
  println("before")
  var xs = list.repeat(7, 268435456)
  list.push(xs, 1)
  println("unreachable ${list.len(xs)}")
}
"#,
    );
}

/// `$bytes_push`: a full 2^31-byte buffer, the same doubling.
#[test]
fn bytes_push_capacity_doubling_wrap_is_the_defined_oom_abort() {
    assert_defined_oom(
        "bytes_push_doubling_wrap",
        r#"fn main() -> Unit = {
  println("before")
  let h = bytes.zeros(1073741824)
  var b = h + h
  bytes.push(b, 1)
  println("unreachable ${bytes.len(b)}")
}
"#,
    );
}
