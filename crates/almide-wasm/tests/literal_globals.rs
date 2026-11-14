mod harness;
use harness::run_wasm;

fn emit(src: &str) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("literal_globals.almd", src).expect("front");
    almide_wasm::emit_program(&ir).expect("emit")
}
fn run(src: &str) -> harness::RunResult {
    run_wasm(&emit(src)).expect("run")
}

#[test]
fn constants_keep_signed_remainders_and_shadowed_locals() {
    let r = run(r#"let modulus = 2147483648
    let flag = true
    let scale = 1.5
    fn rem(x: Int) -> Int = x % modulus
    fn shadowed(modulus: Int) -> Int = 17 % modulus
    fn main() -> Unit = {
      for x in [0, -1, 2147483649, -2147483649, -9223372036854775808] { println("${rem(x)}") }
      println("${shadowed(3)} ${flag} ${scale * 2.0}")
    }"#);
    assert_eq!(r.exit, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "0\n-1\n1\n-1\n0\n2 true 3\n");
}

#[test]
fn mutable_globals_are_read_after_every_write() {
    let r = run(r#"let immutable = 8
    var divisor = 3
    fn read() -> Int = 17 % divisor + immutable
    effect fn main() -> Unit = {
      println("${read()}")
      divisor = 4
      println("${read()}")
    }"#);
    assert_eq!(r.exit, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "10\n9\n");
}

#[test]
fn zero_and_overflow_divisors_keep_runtime_abort() {
    for (value, message) in [("0", "division by zero"), ("-1", "integer overflow")] {
        let src = format!(
            "let divisor = {value}\nfn main() -> Unit = println(\"${{-9223372036854775808 % divisor}}\")"
        );
        let r = run(&src);
        assert_ne!(r.exit, 0);
        assert!(r.stderr.contains(message), "{}", r.stderr);
    }
}

#[test]
fn exported_division_keeps_preinitialization_abort_and_postinitialization_value() {
    let src = "let divisor = 8\nfn divide(x: Int) -> Int = x / divisor\nfn remainder(x: Int) -> Int = x % divisor\nfn main() -> Unit = ()";
    let bytes = emit(src);
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, bytes).expect("the module compiles");
    let mut linker = wasmtime::Linker::new(&engine);
    linker.define_unknown_imports_as_traps(&module).expect("the imports resolve to traps");
    let mut store = wasmtime::Store::new(&engine, ());
    let inst = linker.instantiate(&mut store, &module).expect("the module instantiates");
    let div = inst
        .get_typed_func::<i64, i64>(&mut store, "divide")
        .expect("the export has the expected signature");
    let rem = inst
        .get_typed_func::<i64, i64>(&mut store, "remainder")
        .expect("the export has the expected signature");
    // Before main the global is zero: each operation must still abort,
    // reaching the deliberately unbound diagnostic import here.
    assert!(div.call(&mut store, 17).is_err());
    assert!(rem.call(&mut store, 17).is_err());
    inst.get_typed_func::<(), ()>(&mut store, "main")
        .expect("the export has the expected signature")
        .call(&mut store, ())
        .expect("the call does not trap");
    for x in [i64::MIN, -17, -1, 0, 1, 17, i64::MAX] {
        assert_eq!(div.call(&mut store, x).expect("the call does not trap"), x / 8);
        assert_eq!(rem.call(&mut store, x).expect("the call does not trap"), x % 8);
    }
}

/// The dividends every divisor is tried against: the extremes, zero, the
/// sign boundary, and the divisor's own neighbourhood.
fn dividends(c: i64) -> Vec<i64> {
    let mut xs = vec![i64::MIN, i64::MIN + 1, -2147483649, -17, -2, -1, 0, 1, 2, 17, 2147483649, i64::MAX - 1, i64::MAX];
    for d in [-1i64, 0, 1] {
        xs.push(c.wrapping_add(d));
        xs.push(c.wrapping_neg().wrapping_add(d));
        xs.push(c.wrapping_mul(3).wrapping_add(d));
    }
    xs
}

fn division_program(binding: &str, c: i64) -> String {
    let xs: Vec<String> = dividends(c).iter().map(|x| x.to_string()).collect();
    format!(
        "{binding} divisor = {c}\nfn divide(x: Int) -> Int = x / divisor\nfn remainder(x: Int) -> Int = x % divisor\neffect fn main() -> Unit = {{\n  for x in [{}] {{ println(\"${{divide(x)}} ${{remainder(x)}}\") }}\n}}",
        xs.join(", ")
    )
}

/// Differential: a division by a literal immutable global answers what the
/// same program answers with the global declared `var` (the guarded runtime
/// division, which this lowering never touches), and both answer the
/// truncating quotient and remainder computed here.
#[test]
fn literal_global_divisors_agree_with_the_runtime_division() {
    let divisors = [
        1, 2, 3, 7, 8, 10, 13, 5000, 65536, 1_000_000_007, 2147483648, 4611686018427387904, i64::MAX, -2, -3, -8, -5000,
        -2147483648, i64::MIN + 1,
    ];
    for c in divisors {
        let folded = run(&division_program("let", c));
        let runtime = run(&division_program("var", c));
        assert_eq!(folded.exit, 0, "divisor {c}: {}", folded.stderr);
        let expected: String =
            dividends(c).iter().map(|x| format!("{} {}\n", x.wrapping_div(c), x.wrapping_rem(c))).collect();
        assert_eq!(folded.stdout, expected, "divisor {c}");
        assert_eq!(folded.stdout, runtime.stdout, "divisor {c}: literal and runtime division differ");
    }
}

/// The lowering fires on exactly the literal immutable Int global: the same
/// program with the divisor as a literal global and as a `var` must emit
/// different code (the control that the differential above compares two
/// lowerings, not one with itself), while a parameter shadowing the global
/// and a `var` global must emit what they emitted before any global was a
/// known divisor — the runtime division with both guards.
#[test]
fn only_a_literal_immutable_global_divides_as_its_literal() {
    let count_runtime_rem = |bytes: &[u8]| {
        // `i64.rem_s` whose divisor is not an `i64.const` right before it:
        // the guarded runtime remainder ends in `local.get r; i64.rem_s`.
        let mut n = 0;
        for w in bytes.windows(3) {
            if w[0] == 0x20 && w[2] == 0x81 {
                n += 1;
            }
        }
        n
    };
    let known = emit("let divisor = 5000\nfn remainder(x: Int) -> Int = x % divisor\nfn main() -> Unit = ()");
    let mutable = emit("var divisor = 5000\nfn remainder(x: Int) -> Int = x % divisor\nfn main() -> Unit = ()");
    let shadowed = emit("let divisor = 5000\nfn remainder(x: Int, divisor: Int) -> Int = x % divisor\nfn main() -> Unit = ()");
    assert_eq!(count_runtime_rem(&known), 0, "a literal immutable global still divides at run time");
    assert_eq!(count_runtime_rem(&mutable), 1, "a mutable global must keep the runtime division");
    assert_eq!(count_runtime_rem(&shadowed), 1, "a parameter shadowing the global must keep the runtime division");
    assert!(known.len() < mutable.len(), "the literal lowering must not be larger than the runtime division");
}

/// Before main has stored it the global is zero whatever its literal: every
/// division by it still aborts, for a divisor that is not a power of two
/// and for 1 (whose quotient needs no instruction at all) alike.
#[test]
fn every_known_divisor_keeps_the_preinitialization_abort() {
    for c in [1i64, 3, 5000, -7, 2147483648] {
        let src = format!(
            "let divisor = {c}\nfn divide(x: Int) -> Int = x / divisor\nfn remainder(x: Int) -> Int = x % divisor\nfn main() -> Unit = ()"
        );
        let bytes = emit(&src);
        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, bytes).expect("the module compiles");
        let mut linker = wasmtime::Linker::new(&engine);
        linker.define_unknown_imports_as_traps(&module).expect("the imports resolve to traps");
        let mut store = wasmtime::Store::new(&engine, ());
        let inst = linker.instantiate(&mut store, &module).expect("the module instantiates");
        let div = inst.get_typed_func::<i64, i64>(&mut store, "divide").expect("the export has the expected signature");
        let rem = inst.get_typed_func::<i64, i64>(&mut store, "remainder").expect("the export has the expected signature");
        assert!(div.call(&mut store, 17).is_err(), "divisor {c}: divide ran before main");
        assert!(rem.call(&mut store, 17).is_err(), "divisor {c}: remainder ran before main");
        inst.get_typed_func::<(), ()>(&mut store, "main")
            .expect("the export has the expected signature")
            .call(&mut store, ())
            .expect("the call does not trap");
        for x in [i64::MIN, -17, -1, 0, 1, 17, i64::MAX] {
            assert_eq!(div.call(&mut store, x).expect("the call does not trap"), x.wrapping_div(c), "{x} / {c}");
            assert_eq!(rem.call(&mut store, x).expect("the call does not trap"), x.wrapping_rem(c), "{x} % {c}");
        }
    }
}

/// The dividend is evaluated, effects and all, before the divisor is
/// looked at — the order the runtime division has.
#[test]
fn the_dividend_runs_before_the_known_divisor_is_tested() {
    let r = run(r#"let divisor = 5000
    fn loud(x: Int) -> Int = {
      println("dividend ${x}")
      x
    }
    fn main() -> Unit = {
      println("${loud(12345) % divisor}")
      println("${loud(-12345) / divisor}")
    }"#);
    assert_eq!(r.exit, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "dividend 12345\n2345\ndividend -12345\n-2\n");
}
