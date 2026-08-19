//! Unit 6 first light: source → typed IR → structural wasm → wasmtime,
//! compared against the interpreter (the definition, §6.6 authority order).
//! Every emitted module passes the wasmparser wall before instantiation.

mod harness;
use harness::run_wasm;

#[test]
fn hello_matches_the_interpreter() {
    let src = "effect fn main() -> Unit = {\n  println(\"hello, wasm\")\n  println(\"第二行 🦀\")\n}\n";
    let ir = almide_spine::s5::lower_to_ir("hello.almd", src).expect("front end");
    let bytes = almide_wasm::emit_program(&ir).expect("stage-1 shape");
    let run = run_wasm(&bytes).expect("wasmtime run");
    let interp = almide_spine::s5::run_file("hello.almd", src).expect("interp run");
    assert_eq!(interp.exit, 0);
    assert_eq!(run.exit, 0);
    assert_eq!(run.stdout, interp.stdout, "wasm output must equal the definition (interp)");
    assert_eq!(run.stdout, "hello, wasm\n第二行 🦀\n");
}
