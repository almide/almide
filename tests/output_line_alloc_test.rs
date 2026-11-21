//! ADR-0038: an output line of admitted holes allocates nothing per line, on
//! both targets. E071 admits `println("…${x}…")` inside a counted loop of a
//! `@bounded` fn only because its lowering builds no String; this test is the
//! measurement behind that admission.
//!
//! One program prints every admitted shape (a single Int, several Int holes,
//! each sized int the predicate admits, Bool, a line longer than the native
//! line buffer, a stderr line) inside a counted loop of a `@bounded` fn, and
//! again in `main`'s own loop. It runs at two trip counts; the counters
//! (native `ALMIDE_ALLOC_COUNT`, wasm `ALMIDE_WASM_ALLOC_COUNT`) must read the
//! same at both, so the per-line allocation is exactly 0. On native the one
//! allocation is stdout's line buffer, made once at the first write.
//!
//! The control: a Float hole on wasm, and a UInt64 hole on native, still
//! build a String per line, which is why the predicate refuses them (stage 1).
//! When their writers land this control turns red and the predicate widens in
//! the same change.
use std::process::Command;

fn program(n: usize) -> String {
    let long = "L".repeat(300);
    format!(
        r#"@bounded
fn trace(a8: Int8, a16: Int16, a32: Int32, a64: Int64, b8: UInt8, b16: UInt16, b32: UInt32) -> Unit = {{
  for i in 0..<{n} {{
    println("${{i}}")
    println("t=${{i}} v=${{i * 3}} w=${{0 - i}}")
    println("i8=${{a8}} i16=${{a16}} i32=${{a32}} i64=${{a64}}")
    println("u8=${{b8}} u16=${{b16}} u32=${{b32}}")
    println("ok=${{i % 2 == 0}} ${{i > 1}}")
    println("{long} ${{i}}")
    eprintln("e=${{i}} ${{i == 0}}")
  }}
}}

effect fn main() -> Unit = {{
  trace(int8.min_value(), int16.min_value(), int32.min_value(), int64.min_value(), uint8.max_value(), uint16.max_value(), uint32.max_value())
  for i in 0..<{n} {{
    println("main ${{i}} ${{i % 3 == 0}}")
  }}
}}
"#
    )
}

/// `(allocs, stdout lines)` of one run, from the counter line on stderr.
fn run(src: &str, wasm: bool) -> (u64, usize) {
    let td = tempfile::TempDir::new().unwrap();
    let f = td.path().join("main.almd");
    std::fs::write(&f, src).unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("run").arg(&f).current_dir(td.path());
    let tag = if wasm {
        c.args(["--target", "wasm"]).env("ALMIDE_WASM_ALLOC_COUNT", "1");
        "__ALMD_WASM_ALLOC "
    } else {
        c.env("ALMIDE_ALLOC_COUNT", "1");
        "__ALMD_ALLOC "
    };
    let o = c.output().expect("almide runs");
    let (out, err) = (String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(o.status.success(), "the program failed:\n{src}\n{out}\n{err}");
    let line = err.lines().find(|l| l.starts_with(tag)).unwrap_or_else(|| panic!("no `{tag}` line on stderr:\n{err}"));
    let allocs = line
        .split_whitespace()
        .find_map(|w| w.strip_prefix("allocs="))
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("no allocs= in `{line}`"));
    (allocs, out.lines().count())
}

fn check(src: &str, critical: bool) -> (bool, String) {
    let td = tempfile::TempDir::new().unwrap();
    let f = td.path().join("main.almd");
    std::fs::write(&f, src).unwrap();
    let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
    c.arg("check").arg(&f).current_dir(td.path());
    if critical {
        c.args(["--profile", "critical"]);
    }
    let o = c.output().expect("almide runs");
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

#[test]
fn an_admitted_output_line_allocates_nothing_per_line_on_either_target() {
    let (few, many) = (4, 404);
    let (ok, out) = check(&program(few), false);
    assert!(ok, "the measured lines must be the ones E071 admits:\n{out}");
    for wasm in [false, true] {
        let target = if wasm { "wasm" } else { "native" };
        let (a, la) = run(&program(few), wasm);
        let (b, lb) = run(&program(many), wasm);
        assert_eq!((la, lb), (few * 7, many * 7), "{target}: every line printed");
        assert_eq!(
            a, b,
            "{target}: {} more lines cost {} more allocations (an output line must allocate 0 per line)",
            lb - la,
            b as i64 - a as i64
        );
        assert!(a <= 1, "{target}: at most the one stdout buffer is allocated, got {a}");
    }
}

#[test]
fn the_holes_the_predicate_refuses_still_allocate_per_line() {
    let line = |ty: &str, init: &str, n: usize| {
        format!("fn trace(x: {ty}) -> Unit = {{\n  for i in 0..<{n} {{\n    println(\"x=${{x}} ${{i}}\")\n  }}\n}}\n\neffect fn main() -> Unit = trace({init})\n")
    };
    // wasm: the Float printer returns a String block per line
    for (ty, init) in [("Float", "float.from_bits(4611686018427387904)"), ("Float32", "float32.from_bits(1073741824)")] {
        let (a, _) = run(&line(ty, init, 4), true);
        let (b, _) = run(&line(ty, init, 404), true);
        assert!(b > a, "wasm: a {ty} hole no longer allocates per line ({a} -> {b}); admit it in ty_has_sink_writer");
    }
    // native: a UInt64's text is built as a String per line
    let (a, _) = run(&line("UInt64", "uint64.max_value()", 4), false);
    let (b, _) = run(&line("UInt64", "uint64.max_value()", 404), false);
    assert!(b > a, "native: a UInt64 hole no longer allocates per line ({a} -> {b}); admit it in ty_has_sink_writer");
}
