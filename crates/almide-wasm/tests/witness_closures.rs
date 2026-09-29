//! #2758 (#1696 step 4) — CLOSURES in the structural witness. A lambda
//! body is a frame of its own (its params callee-owned, its captures views
//! of the env block the closure holds). Creating a closure allocates the
//! env: each handle-typed capture shares into it (`am`, released by the
//! env's drop glue) and the env is an owned value. A literal lambda handed
//! to a native arm declines: the arm may inline its body in the caller's
//! frame (list.map / filter / fold). A closure CALL lends the Fn value,
//! hands its arguments over under the callee-owned convention and releases
//! a fresh callee after the call.

const PROGRAM: &str = r#"fn adder(xs: List[Int]) -> (List[Int]) -> Int = (ys) => list.len(ys) + list.len(xs)

fn apply(f: (List[Int]) -> Int, xs: List[Int]) -> Int = f(xs)

fn fresh_call(xs: List[Int]) -> Int = adder(xs)([3])

fn bump(xs: List[Int]) -> List[Int] = list.map(xs, (x) => x + 1)

fn pairer(n: Int) -> (Int) -> List[Int] = (k) => {
  let t = [k, n]
  t
}

effect fn main() -> Unit = {
  let plus = adder([1, 2])
  let pair = pairer(5)
  println("${plus([3])} ${list.len(bump([1]))} ${list.len(pair(4))}")
  println("${apply(plus, [1])} ${fresh_call([2])}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("closures.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn closure_frames_and_env_captures_witness_exactly() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    // The borrowed param shares into the env; the env moves out.
    assert_eq!(w.get("adder").map(String::as_str), Some("am\nim\n"));
    // `t` is bound INSIDE the lambda: the lambda's own local, never a
    // capture (the env carries only the Int `n`, no RC site).
    assert_eq!(w.get("pairer").map(String::as_str), Some("im\n"));
    // A closure call (`call_indirect`): the Fn value is lent; the argument
    // shares into the lifted body's owned param (`am`).
    assert_eq!(w.get("apply").map(String::as_str), Some("\nam\n"));
    // A fresh callee (a call result) is released after the call (`id`); a
    // fresh argument moves in (`im`).
    assert_eq!(w.get("fresh_call").map(String::as_str), Some("\nim\nid\n"));
    // A literal callback to a native arm may be inlined: declined.
    assert_eq!(w.get("bump").map(String::as_str), Some("!decline:call-arg:Lambda\n"));
    let lambdas: Vec<(&String, &String)> = w.iter().filter(|(k, _)| k.starts_with("<lambda#")).collect();
    assert!(!lambdas.is_empty(), "the lambda bodies are witnessed: {w:?}");
    for (name, cert) in lambdas {
        assert!(!cert.starts_with("!poison"), "{name}: {cert:?}");
        if !cert.starts_with('!') {
            assert!(accepted(cert), "{name}: the portable checker must accept {cert:?}");
        }
    }
    // `adder`'s lambda: its param is owned and released, its capture a view.
    assert!(w.values().any(|c| c == "id\n\n"), "{w:?}");
}
