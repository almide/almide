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
    let module = wasmtime::Module::new(&engine, bytes).unwrap();
    let mut linker = wasmtime::Linker::new(&engine);
    linker.define_unknown_imports_as_traps(&module).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let inst = linker.instantiate(&mut store, &module).unwrap();
    let div = inst
        .get_typed_func::<i64, i64>(&mut store, "divide")
        .unwrap();
    let rem = inst
        .get_typed_func::<i64, i64>(&mut store, "remainder")
        .unwrap();
    // Before main the global is zero: each operation must still abort,
    // reaching the deliberately unbound diagnostic import here.
    assert!(div.call(&mut store, 17).is_err());
    assert!(rem.call(&mut store, 17).is_err());
    inst.get_typed_func::<(), ()>(&mut store, "main")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    for x in [i64::MIN, -17, -1, 0, 1, 17, i64::MAX] {
        assert_eq!(div.call(&mut store, x).unwrap(), x / 8);
        assert_eq!(rem.call(&mut store, x).unwrap(), x % 8);
    }
}
