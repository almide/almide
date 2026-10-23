//! #4009 item 2 — the checked-allocator switch (checked_alloc.rs).
//!
//! - OFF: the module is byte-identical to a build that never heard of it.
//! - ON, honest program: the same output, though freed blocks are reused —
//!   the poison `$free` writes is intact when `$alloc` takes a block back.
//! - ON, a write after free through a dangling `ffi` address: the reuse
//!   finds the poison broken, prints the heap-check message and exits 70.
//!   The ordinary build runs the same program to exit 0 — the silent
//!   corruption the switch exists to expose.
//! - ON, a 16-byte write into a 4-byte buffer: the block's canary is broken
//!   when `$free` files it; the overrun message, exit 70. Ordinarily: exit 0.

mod harness;
use harness::run_wasm;

fn show(r: &harness::RunResult) -> String {
    format!("exit {} stdout {:?} stderr {:?}", r.exit, r.stdout, r.stderr)
}

fn emit(src: &str, checked: bool) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _checked = checked.then(almide_wasm::checked_alloc::CheckedGuard::set);
    almide_wasm::emit_program(&ir).expect("emit")
}

/// Small blocks: a fresh string per iteration, released at its end, so the
/// size-class lists are filed and popped a thousand times.
const SMALL_CHURN: &str = r#"fn main() -> Unit = {
  var acc = 0
  for i in 0..<1000 {
    let s = "row " + int.to_string(i)
    acc = acc + string.len(s)
  }
  println("${acc}")
}
"#;

const UAF_WRITE: &str = include_str!("../../../proofs/fixtures/heap-check/uaf_write.almd");
const OVERRUN_WRITE: &str = include_str!("../../../proofs/fixtures/heap-check/overrun_write.almd");

#[test]
fn off_is_byte_identical_and_on_changes_only_the_allocator() {
    let plain = emit(SMALL_CHURN, false);
    let checked = emit(SMALL_CHURN, true);
    assert_ne!(plain, checked, "the switch must change the module");
    assert_eq!(emit(SMALL_CHURN, false), plain, "the guard must restore the shipped module");
}

#[test]
fn an_honest_program_runs_the_same_under_the_check() {
    let plain = run_wasm(&emit(SMALL_CHURN, false)).expect("run");
    let checked = run_wasm(&emit(SMALL_CHURN, true)).expect("run");
    assert_eq!((plain.exit, &plain.stdout), (0, &"6890\n".to_string()));
    assert_eq!((checked.exit, &checked.stdout, &checked.stderr), (0, &plain.stdout, &plain.stderr));
}

#[test]
fn a_write_after_free_is_caught_on_reuse() {
    let plain = run_wasm(&emit(UAF_WRITE, false)).expect("run");
    assert_eq!(plain.exit, 0, "the ordinary build corrupts silently: {}", show(&plain));
    let checked = run_wasm(&emit(UAF_WRITE, true)).expect("run");
    assert_eq!(checked.exit, almide_wasm::checked_alloc::EXIT, "{}", show(&checked));
    assert!(checked.stderr.contains(almide_wasm::checked_alloc::TEXT), "{}", show(&checked));
    assert!(checked.stdout.is_empty(), "the check fires before the program prints: {}", show(&checked));
}

#[test]
fn a_write_past_the_end_is_caught_on_free() {
    let plain = run_wasm(&emit(OVERRUN_WRITE, false)).expect("run");
    assert_eq!((plain.exit, plain.stdout.as_str()), (0, "wrote 16\n"), "the ordinary build corrupts silently: {}", show(&plain));
    let checked = run_wasm(&emit(OVERRUN_WRITE, true)).expect("run");
    assert_eq!(checked.exit, almide_wasm::checked_alloc::EXIT, "{}", show(&checked));
    assert!(checked.stderr.contains(almide_wasm::checked_alloc::OVERRUN_TEXT), "{}", show(&checked));
}
