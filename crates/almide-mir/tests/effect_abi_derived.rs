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

#[test]
fn loop_value_exits_lower_beyond_while_in_result_string() {
    // #3058 class 3: a guard's value exit in a `for` body, and in a pure or
    // never-err fn returning its value raw, takes the same flag + value
    // rewrite `while` in a `Result[_, String]` fn had.
    assert_all_lower(
        "loop-exit",
        "fn first_neg(xs: List[Int]) -> Int = {\n  var n = 0\n  for x in xs {\n    guard x > 0 else x\n    n = n + x\n  }\n  n\n}\n\
         fn capped(limit: Int) -> Int = {\n  var i = 0\n  var s = 0\n  while i < 10 {\n    i = i + 1\n    guard i <= limit else s * 100\n    s = s + i\n  }\n  s\n}\n\
         effect fn eff(xs: List[Int]) -> Int = {\n  var n = 0\n  for x in xs {\n    guard x > 0 else x\n    n = n + x\n  }\n  n\n}\n\
         effect fn main() -> Unit = {\n  println(int.to_string(first_neg([1, -2]) + capped(3) + eff([3])!))\n}\n",
    );
}

#[test]
fn scalar_scalar_result_if_and_match_lower() {
    // #3058: `if c then ok(x) else err(y)` over a `Result[Int, Int]` builds the
    // len-as-tag block the bind position already builds, and a match over the
    // call binds the scalar err by value.
    assert_all_lower(
        "scalar-result",
        "fn f(n: Int) -> Result[Int, Int] = if n > 0 then ok(n * 2) else err(n - 400)\n\
         fn g(n: Int) -> Result[Bool, Int8] = if n > 0 then ok(true) else err(-3)\n\
         effect fn main() -> Unit = {\n  match f(3) {\n    ok(v) => println(\"ok ${v}\"),\n    err(e) => println(\"err ${e}\"),\n  }\n  match g(0) {\n    ok(_) => println(\"ok\"),\n    err(e) => println(\"err ${e}\"),\n  }\n}\n",
    );
}

#[test]
fn uint64_interpolation_and_move_mode_unwrap_lower() {
    // #3058: `${u}` of a UInt64 routes to its own unsigned printer, and a `!`
    // over a never-err `mut`-param call the move-mode rewrite already typed as
    // its raw `(result, buffer)` tuple is the identity.
    assert_all_lower(
        "u64-and-move-mode",
        "protocol Counter {\n  fn bump(mut self: Self, by: Int) -> Unit\n  fn read(self) -> Int\n}\n\
         type Tally: Counter = { n: Int }\n\
         fn Tally.bump(mut self: Tally, by: Int) -> Unit = {\n  self.n = self.n + by\n}\n\
         fn Tally.read(self) -> Int = self.n\n\
         effect fn go[C: Counter](mut c: C) -> Int = {\n  c.bump(2)\n  c.read()\n}\n\
         fn big() -> UInt64 = uint64.max_value()\n\
         effect fn main() -> Unit = {\n  var t = Tally { n: 1 }\n  let a = go(t)!\n  println(\"${a} ${t.n} ${big()}\")\n}\n",
    );
}
