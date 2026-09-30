//! #3058: the MIR rung lowers what the deleted incumbent passes used to supply,
//! from facts derived from the IR on the product paths.
//!
//! Before, the registries that told the MIR lowering an effect fn's ABI were
//! filled only by passes that were deleted with the incumbent wasm pipeline,
//! so on every product path they were empty: a can-err `effect fn -> Int`
//! walled, and so did every caller that matched over or unwrapped a never-err
//! effect call. These shapes now lower; the native one renders end to end.

fn lowers(src: &str) -> String {
    almide_mir::pipeline::debug_dump_mir(src).expect("the program reaches the MIR lowering")
}

fn assert_all_lower(name: &str, src: &str) {
    let dump = lowers(src);
    assert!(!dump.contains("LOWER-WALL"), "{name}: a fn walled:\n{dump}");
}

#[test]
fn can_err_effect_fn_takes_the_auto_wrap_carrier() {
    assert_all_lower(
        "can-err",
        "effect fn parse_twice(s: String) -> Int = {\n  let n = int.parse(s)!\n  n * 2\n}\n\
         effect fn main() -> Unit = {\n  let v = parse_twice(\"21\")!\n  println(int.to_string(v))\n}\n",
    );
}

#[test]
fn never_err_call_sites_settle_against_the_raw_return() {
    // Statement `!`, a match with a wildcard Ok arm, and a `??` over the call.
    assert_all_lower(
        "never-err",
        "effect fn say() -> Unit = {\n  println(\"hi\")\n}\n\
         effect fn twice(n: Int) -> Int = n * 2\n\
         effect fn main() -> Unit = {\n  say()!\n  say()!\n  match twice(3) {\n    ok(_) => println(\"ok\"),\n    err(e) => println(e),\n  }\n  println(int.to_string(twice(4) ?? 0))\n}\n",
    );
}

#[test]
fn never_err_scalar_fn_renders_natively_with_its_raw_return() {
    // The native sig table follows the same fact: a never-err `-> Int` effect
    // fn returns `i64`, not the `Result<i64, String>` carrier, so the settled
    // caller's raw value types against it.
    let src = "effect fn f(n: Int) -> Int = n * 2\n\
               effect fn main() -> Unit = {\n  let x = f(3)!\n  println(int.to_string(x))\n}\n";
    let code = almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("walled: {}", e.reason()));
    assert!(code.contains("fn almd_f(v0: i64) -> i64"), "{code}");
}

#[test]
fn list_rest_and_as_arms_lower_on_the_mir_rung() {
    // #3058 class 2: a named rest binds `list.drop(xs, k)` under a `>=` length
    // test, an unnamed one only relaxes the test, and a top-level as-arm reads
    // the subject itself.
    assert_all_lower(
        "list-rest",
        "fn total(xs: List[Int]) -> Int = match xs {\n  [] => 0,\n  [h, ..t] => h + total(t),\n}\n\
         fn g(xs: List[Int]) -> Int = match xs {\n  [] => 0,\n  [a, b, ..] => a * 10 + b,\n  [7, ..r] => 700 + list.len(r),\n  _ => -1,\n}\n\
         fn pick(xs: List[Int]) -> Int = match xs {\n  all @ [x] => x + list.len(all),\n  _ => 0,\n}\n\
         fn main() -> Unit = {\n  println(int.to_string(total([1, 2]) + g([3]) + pick([4])))\n}\n",
    );
}
