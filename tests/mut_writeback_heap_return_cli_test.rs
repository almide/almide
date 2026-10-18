//! A `mut` buffer call that also returns a heap value is linear in bytes on the
//! PRODUCT route (`almide run --target wasm`), at a rolled loop bound and at
//! one >= 1024, which goes through the unrolled fast lane (unroll.rs) — a lane
//! that once lowered its body statement by statement and skipped the block
//! walk's move-in (tests/selfhost_rebind_move_cli_test.rs). The in-process
//! gate is crates/almide-wasm/tests/mut_writeback_heap_return.rs.
//!
//! The write-back `let (r, s) = fill(out, …)` binds the call's result tuple and
//! shares the returned buffer into `out`; the tuple kept its own credit on the
//! buffer while its heap slot (`s`) was still read, so every next call met the
//! buffer at rc 2 and copied it — Σ len over the calls.
use std::process::Command;

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

/// stdout and the bytes the module requested from `$alloc`; nothing live at exit.
fn run_counted(src: &str) -> (String, u64) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("writeback.almd");
    std::fs::write(&file, src).expect("source");
    let out = almide()
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("wasm run");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{stderr}");
    let field = |k: &str| -> u64 {
        stderr
            .split_whitespace()
            .find_map(|w| w.strip_prefix(k)?.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {stderr}"))
    };
    assert_eq!(field("live="), 0, "{stderr}");
    (String::from_utf8_lossy(&out.stdout).to_string(), field("bytes="))
}

fn program(ret_ty: &str, ret: &str, bind: &str, calls: u32) -> String {
    format!(
        "fn fill(mut out: Bytes, n: Int) -> {ret_ty} = {{
  var k = 0
  while k < n {{
    bytes.push(out, 7)
    k = k + 1
  }}
  {ret}
}}

effect fn main() -> Unit = {{
  var out = bytes.new(0)
  var acc = 0
  var i = 0
  while i < {calls} {{
    {bind}
    i = i + 1
  }}
  println(\"${{bytes.len(out)}} ${{acc}}\")
}}
"
    )
}

/// Bytes per call over the zero-call program. 64 pushes a call into a
/// doubling buffer is a few hundred bytes; a copy of the buffer per call is
/// tens of kilobytes on average at these bounds.
fn per_call(ret_ty: &str, ret: &str, bind: &str, calls: u32, want: &str) -> u64 {
    let (out0, base) = run_counted(&program(ret_ty, ret, bind, 0));
    assert_eq!(out0, "0 0\n");
    let (out, looped) = run_counted(&program(ret_ty, ret, bind, calls));
    assert_eq!(out, want);
    looped.saturating_sub(base) / u64::from(calls)
}

#[test]
fn a_mut_buffer_call_returning_a_heap_value_is_linear_at_rolled_and_unrolled_bounds() {
    let shapes = [
        ("tuple", "(Int, String)", "(1, \"s\")", "let (r, s) = fill(out, 64)\n    acc = acc + r + string.len(s)"),
        ("String", "String", "\"abc\"", "let r = fill(out, 64)\n    acc = acc + string.len(r) - 1"),
        ("Int", "Int", "2", "acc = acc + fill(out, 64)"),
    ];
    let mut bad = Vec::new();
    for (name, ret_ty, ret, bind) in shapes {
        for calls in [1000u32, 4096] {
            let b = per_call(ret_ty, ret, bind, calls, &format!("{} {}\n", 64 * calls, 2 * calls));
            if b >= 1024 {
                bad.push(format!("{name}: {b} bytes per call at bound {calls}"));
            }
        }
    }
    assert!(bad.is_empty(), "the call copies the buffer:\n{}", bad.join("\n"));
}
