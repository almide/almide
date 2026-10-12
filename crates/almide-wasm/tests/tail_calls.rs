//! C-292: deep (incl. mutual) tail recursion must run in CONSTANT stack —
//! `return_call` on tail-position direct calls. The stage-8 CI caught the
//! regression this guards: the fixture passed on a laptop's generous stack
//! and overflowed on the runner. This referee pins a SMALL stack so depth
//! bugs cannot hide behind the host environment.

use std::sync::{Arc, Mutex};

fn run_small_stack(bytes: &[u8]) -> anyhow::Result<String> {
    wasmparser::validate(bytes)?;
    let mut cfg = wasmtime::Config::new();
    cfg.max_wasm_stack(64 * 1024); // deliberately tiny: depth must be O(1)
    let engine = wasmtime::Engine::new(&cfg)?;
    let module = wasmtime::Module::new(&engine, bytes)?;
    let out = Arc::new(Mutex::new(String::new()));
    let out2 = out.clone();
    let mut store = wasmtime::Store::new(&engine, ());
    let mut linker = wasmtime::Linker::new(&engine);
    let mem_out = out2.clone();
    linker.func_wrap(
        "almide",
        "println",
        move |mut caller: wasmtime::Caller<'_, ()>, ptr: i32, len: i32| {
            let mem = caller
                .get_export("memory")
                .and_then(|e| e.into_memory())
                .expect("exported memory");
            let mut buf = vec![0u8; len as usize];
            mem.read(&caller, ptr as usize, &mut buf).expect("in-bounds read");
            let mut o = mem_out.lock().expect("lock");
            o.push_str(&String::from_utf8_lossy(&buf));
            o.push('\n');
        },
    )?;
    linker.func_wrap("almide", "eprintln", |_: wasmtime::Caller<'_, ()>, _: i32, _: i32| {})?;
    fn exit_host(_: wasmtime::Caller<'_, ()>, code: i32) -> wasmtime::Result<()> {
        Err(wasmtime::Error::msg(format!("almide.exit({code}) in a tail-call fixture")))
    }
    linker.func_wrap("almide", "exit", exit_host)?;
    // fs is never reached by this fixture — inert stubs satisfy the imports.
    linker.func_wrap(
        "almide",
        "fs_call",
        |_: wasmtime::Caller<'_, ()>, _: i32, _: i32, _: i32, _: i32, _: i32| -> i64 { 0 },
    )?;
    linker.func_wrap("almide", "host_read", |_: wasmtime::Caller<'_, ()>, _: i32| {})?;
    let instance = linker.instantiate(&mut store, &module)?;
    let main = instance.get_typed_func::<(), ()>(&mut store, "main")?;
    main.call(&mut store, ())?;
    let s = out.lock().expect("lock").clone();
    Ok(s)
}

#[test]
fn deep_tail_recursion_is_constant_stack() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("root");
    let rel = "spec/wasm_cross/ref_gleam_tail_deep.almd";
    let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture");
    let ir = almide_spine::s5::lower_to_ir(rel, &text).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    let wasm_out = run_small_stack(&bytes).expect("must run in a tiny stack (return_call)");
    let interp = almide_spine::s5::run_file(rel, &text).expect("interp");
    assert_eq!(interp.exit, 0);
    assert_eq!(wasm_out, interp.stdout);
}

/// Build `src` through the product leg (the C-132 move-mode rewrite
/// included), run it on the tiny stack, and compare with `want` — native's
/// output (`almide run`). The interpreter is not a referee here: its own
/// recursion runs out of host stack at this depth.
fn assert_constant_stack(name: &str, src: &str, want: &str) {
    let ir = almide_spine::s5::lower_to_ir(name, src).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    let wasm_out = run_small_stack(&bytes).unwrap_or_else(|e| panic!("{name}: must run in a tiny stack: {e:#}"));
    assert_eq!(wasm_out, want, "{name}: wasm");
}

/// #3662: a self tail call that forwards a `mut` parameter of a
/// value-returning fn. The move-mode rewrite makes the callee return
/// `(value, buffer)` and the caller write the buffer back after the call, so
/// the call was a plain `call` and 100k levels exhausted the stack; the
/// write-back now folds into the tail call (C-178's unbounded depth).
#[test]
fn self_tail_call_forwarding_a_mut_param_is_constant_stack() {
    let src = r#"fn go(mut s: String, i: Int) -> String =
  if i == 0 then s
  else {
    string.push(s, "b")
    go(s, i - 1)
  }

effect fn main() -> Unit = {
  var s = string.repeat("a", 10)
  let r = go(s, 100000)
  println(int.to_string(string.len(r)))
  println(int.to_string(string.len(s)))
}
"#;
    assert_constant_stack("mut_tail_self.almd", src, "100010\n100010\n");
}

/// #3662, the mutual twin: two fns hand the same `mut` buffer back and forth
/// in tail position.
#[test]
fn mutual_tail_calls_forwarding_a_mut_param_are_constant_stack() {
    let src = r#"fn ping(mut xs: List[Int], i: Int) -> Int =
  if i == 0 then list.len(xs)
  else {
    list.push(xs, i)
    pong(xs, i - 1)
  }

fn pong(mut xs: List[Int], i: Int) -> Int =
  if i == 0 then list.len(xs) + 1000000
  else ping(xs, i - 1)

effect fn main() -> Unit = {
  var xs: List[Int] = []
  let n = ping(xs, 100000)
  println(int.to_string(n))
  println(int.to_string(list.len(xs)))
}
"#;
    assert_constant_stack("mut_tail_mutual.almd", src, "50000\n50000\n");
}
