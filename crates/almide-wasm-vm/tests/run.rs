//! The interpreter and the runner contract, one requirement at a time, on
//! small modules written as text.

use almide_wasm_vm::{run_program, Limits};

struct Ran {
    exit: i32,
    out: String,
    err: String,
}

fn run_with(wat: &str, limits: Limits, input: &[u8]) -> Ran {
    let bytes = wat::parse_str(wat).expect("test module assembles");
    let (mut input, mut out, mut err) = (input, Vec::new(), Vec::new());
    let exit = run_program(&bytes, limits, &mut input, &mut out, &mut err).expect("module loads");
    let text = |b: Vec<u8>| String::from_utf8(b).expect("the test programs write UTF-8");
    Ran { exit, out: text(out), err: text(err) }
}

fn run(wat: &str) -> Ran {
    run_with(wat, Limits::default(), b"")
}

/// A module with the five WASI imports, one page of memory, a string at 16,
/// a `$print(ptr, len, fd)` helper, and the given body for `_start`.
fn program(data: &str, extra: &str, start: &str) -> String {
    let data = data.replace('\n', "\\n");
    format!(
        r#"(module
  (type $fd_rw (func (param i32 i32 i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_write" (func $fd_write (type $fd_rw)))
  (import "wasi_snapshot_preview1" "proc_exit" (func $proc_exit (param i32)))
  (import "wasi_snapshot_preview1" "random_get" (func $random_get (param i32 i32) (result i32)))
  (import "wasi_snapshot_preview1" "clock_time_get" (func $clock (param i32 i64 i32) (result i32)))
  (import "wasi_snapshot_preview1" "fd_read" (func $fd_read (type $fd_rw)))
  (memory 1)
  (data (i32.const 16) "{data}")
  (func $print (param $ptr i32) (param $len i32) (param $fd i32)
    (i32.store (i32.const 0) (local.get $ptr))
    (i32.store (i32.const 4) (local.get $len))
    (drop (call $fd_write (local.get $fd) (i32.const 0) (i32.const 1) (i32.const 8))))
  {extra}
  (func (export "_start") {start}))"#
    )
}

const TRAP: &str = "Error: wasm trap: ";

#[test]
fn output_and_a_normal_return_exit_zero() {
    let r = run(&program("hi\n", "", "(call $print (i32.const 16) (i32.const 3) (i32.const 1))"));
    assert_eq!((r.exit, r.out.as_str(), r.err.as_str()), (0, "hi\n", ""));
}

#[test]
fn proc_exit_is_the_exit_code() {
    let r = run(&program("", "", "(call $proc_exit (i32.const 3)) (unreachable)"));
    assert_eq!((r.exit, r.err.as_str()), (3, ""), "no instruction runs after proc_exit");
}

#[test]
fn a_trap_exits_one_with_one_named_line() {
    let r = run(&program("", "", "(unreachable)"));
    assert_eq!(r.exit, 1);
    assert_eq!(r.err, format!("{TRAP}wasm `unreachable` instruction executed\n"));
}

#[test]
fn a_guard_that_names_its_abort_gets_no_second_line() {
    let r = run(&program("Error: boom\n", "", "(call $print (i32.const 16) (i32.const 12) (i32.const 2)) (unreachable)"));
    assert_eq!((r.exit, r.err.as_str()), (1, "Error: boom\n"));
}

#[test]
fn a_named_line_does_not_hide_a_different_trap() {
    let body = "(call $print (i32.const 16) (i32.const 12) (i32.const 2)) (drop (i32.div_u (i32.const 1) (i32.const 0)))";
    let r = run(&program("Error: boom\n", "", body));
    assert_eq!(r.err, format!("Error: boom\n{TRAP}integer divide by zero\n"));
}

#[test]
fn numeric_traps_are_the_spec_traps() {
    let cases = [
        ("(drop (i64.div_s (i64.const 1) (i64.const 0)))", "integer divide by zero"),
        ("(drop (i64.div_s (i64.const -9223372036854775808) (i64.const -1)))", "integer overflow"),
        ("(drop (i64.rem_u (i64.const 1) (i64.const 0)))", "integer divide by zero"),
    ];
    for (body, reason) in cases {
        let r = run(&program("", "", body));
        assert_eq!((r.exit, r.err.clone()), (1, format!("{TRAP}{reason}\n")), "{body}");
    }
    let r = run(&program("", "", "(drop (i64.rem_s (i64.const -9223372036854775808) (i64.const -1)))"));
    assert_eq!(r.exit, 0, "rem_s MIN -1 is 0, not a trap");
}

#[test]
fn memory_is_bounded_by_its_current_size() {
    let r = run(&program("", "", "(drop (i32.load (i32.const 65533)))"));
    assert_eq!(r.err, format!("{TRAP}out of bounds memory access\n"));
    let r = run(&program("", "", "(drop (i64.load offset=65528 (i32.const 0)))"));
    assert_eq!(r.exit, 0, "the last 8 bytes are in bounds");
    let r = run(&program("", "", "(drop (i32.load8_u offset=1 (i32.const -1)))"));
    assert_eq!(r.err, format!("{TRAP}out of bounds memory access\n"), "address + offset is not wrapped");
    let r = run(&program("", "", "(memory.fill (i32.const 65535) (i32.const 0) (i32.const 2))"));
    assert_eq!(r.err, format!("{TRAP}out of bounds memory access\n"));
    let r = run(&program("", "", "(memory.copy (i32.const 65536) (i32.const 0) (i32.const 0))"));
    assert_eq!(r.exit, 0, "a zero-length copy at the end is in bounds");
}

#[test]
fn memory_grows_to_its_cap_and_no_further() {
    let start = r#"
      (if (i32.ne (memory.grow (i32.const 2)) (i32.const 1)) (then (unreachable)))
      (if (i32.ne (memory.size) (i32.const 3)) (then (unreachable)))
      (i32.store (i32.const 196604) (i32.const 7))
      (if (i32.ne (memory.grow (i32.const 1)) (i32.const -1)) (then (unreachable)))
      (if (i32.ne (memory.size) (i32.const 3)) (then (unreachable)))"#;
    let limits = Limits { memory_pages: 3, ..Limits::default() };
    let r = run_with(&program("", "", start), limits, b"");
    assert_eq!((r.exit, r.err.as_str()), (0, ""));
}

#[test]
fn blocks_loops_and_branches_carry_values() {
    let extra = r#"
      (func $sum (param $n i64) (result i64) (local $acc i64)
        (block $done
          (loop $next
            (br_if $done (i64.eqz (local.get $n)))
            (local.set $acc (i64.add (local.get $acc) (local.get $n)))
            (local.set $n (i64.sub (local.get $n) (i64.const 1)))
            (br $next)))
        (local.get $acc))
      (func $pick (param $c i32) (result i32)
        (block $out (result i32)
          (drop (br_if $out (i32.const 7) (local.get $c)))
          (i32.const 9)))"#;
    let start = r#"
      (if (i64.ne (call $sum (i64.const 100)) (i64.const 5050)) (then (unreachable)))
      (if (i32.ne (call $pick (i32.const 1)) (i32.const 7)) (then (unreachable)))
      (if (i32.ne (call $pick (i32.const 0)) (i32.const 9)) (then (unreachable)))
      (if (i32.ne (select (i32.const 1) (i32.const 2) (i32.const 0)) (i32.const 2)) (then (unreachable)))"#;
    let r = run(&program("", extra, start));
    assert_eq!((r.exit, r.err.as_str()), (0, ""));
}

#[test]
fn a_tail_call_reuses_its_frame() {
    let extra = r#"
      (func $count (param $n i64) (result i64)
        (if (result i64) (i64.eqz (local.get $n))
          (then (i64.const 42))
          (else (return_call $count (i64.sub (local.get $n) (i64.const 1))))))"#;
    let start = "(if (i64.ne (call $count (i64.const 1000000)) (i64.const 42)) (then (unreachable)))";
    let limits = Limits { call_depth: 4, ..Limits::default() };
    let r = run_with(&program("", extra, start), limits, b"");
    assert_eq!((r.exit, r.err.as_str()), (0, ""), "a million tail calls in four frames");
}

#[test]
fn recursion_past_the_fixed_depth_traps() {
    let extra = "(func $down (param $n i32) (result i32) (i32.add (i32.const 1) (call $down (local.get $n))))";
    let start = "(drop (call $down (i32.const 0)))";
    let r = run_with(&program("", extra, start), Limits { call_depth: 1000, ..Limits::default() }, b"");
    assert_eq!(r.err, format!("{TRAP}call stack exhausted\n"));
    let r = run_with(&program("", extra, start), Limits { stack_cells: 1000, ..Limits::default() }, b"");
    assert_eq!(r.err, format!("{TRAP}call stack exhausted\n"), "the value stack is bounded too");
}

#[test]
fn fuel_bounds_every_run() {
    let r = run_with(&program("", "", "(loop $spin (br $spin))"), Limits { fuel: 10_000, ..Limits::default() }, b"");
    assert_eq!(r.err, format!("{TRAP}all fuel consumed by WebAssembly\n"));
}

#[test]
fn indirect_calls_check_the_entry_and_the_signature() {
    let extra = r#"
      (type $unit (func))
      (type $to_i32 (func (result i32)))
      (table 3 3 funcref)
      (elem (i32.const 0) $nothing)
      (func $nothing)"#;
    let cases = [
        ("(call_indirect (type $unit) (i32.const 0))", None),
        ("(call_indirect (type $unit) (i32.const 1))", Some("uninitialized element")),
        ("(call_indirect (type $unit) (i32.const 3))", Some("undefined element: out of bounds table access")),
        ("(drop (call_indirect (type $to_i32) (i32.const 0)))", Some("indirect call type mismatch")),
    ];
    for (start, trap) in cases {
        let r = run(&program("", extra, start));
        match trap {
            None => assert_eq!((r.exit, r.err.as_str()), (0, ""), "{start}"),
            Some(reason) => assert_eq!(r.err, format!("{TRAP}{reason}\n"), "{start}"),
        }
    }
}

#[test]
fn stdin_is_served_and_the_unserved_calls_trap_by_name() {
    let start = r#"
      (i32.store (i32.const 0) (i32.const 32))
      (i32.store (i32.const 4) (i32.const 8))
      (drop (call $fd_read (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 8)))
      (call $print (i32.const 32) (i32.load (i32.const 8)) (i32.const 1))"#;
    let r = run_with(&program("", "", start), Limits::default(), b"echo");
    assert_eq!((r.exit, r.out.as_str()), (0, "echo"));
    let r = run(&program("", "", "(drop (call $random_get (i32.const 0) (i32.const 4)))"));
    assert_eq!(r.err, format!("{TRAP}host call `random_get` is not served by this VM\n"));
}

#[test]
fn f32_exists_only_between_a_demote_and_a_reinterpret() {
    let start = r#"
      (if (i32.ne (i32.reinterpret_f32 (f32.demote_f64 (f64.const 1.5))) (i32.const 0x3fc00000))
        (then (unreachable)))
      (if (f64.ne (f64.promote_f32 (f32.reinterpret_i32 (i32.const 0x3fc00000))) (f64.const 1.5))
        (then (unreachable)))
      (if (f64.ne (f64.promote_f32 (f32.convert_i64_s (i64.const 16777217))) (f64.const 16777216))
        (then (unreachable)))"#;
    let r = run(&program("", "", start));
    assert_eq!((r.exit, r.err.as_str()), (0, ""));
}
