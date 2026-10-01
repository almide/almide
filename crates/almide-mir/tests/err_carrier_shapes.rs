//! #3121 / #3084: shapes the MIR rung used to refuse, which the structural leg
//! already served — the C-132 err carrier's call sites and raises, a Unit
//! effect fn's raising `if`, and a nested `mut`-param call after call operands.

fn assert_all_lower(name: &str, src: &str) {
    let dump = almide_mir::pipeline::debug_dump_mir(src)
        .unwrap_or_else(|e| panic!("{name}: did not reach the MIR lowering: {e:?}"));
    assert!(!dump.contains("LOWER-WALL"), "{name}: a fn walled:\n{dump}");
}

#[test]
fn err_carrier_raise_and_call_sites_lower() {
    // `check(v)!` inside a can-err `mut`-param fn raises `err((e, s))!`, a match
    // over an err literal; `forward` re-raises a carrier call's err; `main`
    // consumes the carrier through `??` and `match`.
    assert_all_lower(
        "err-carrier",
        "effect fn check(v: Int) -> Int = {\n  guard v >= 0 else err(\"negative\")\n  v\n}\n\
         effect fn append_then_check(mut s: String, v: Int) -> Int = {\n  s = s + \"<${v}>\"\n  let w = check(v)!\n  w + 1\n}\n\
         effect fn push_then_err(mut xs: List[Int], v: Int) -> Int = {\n  list.push(xs, v)\n  if v > 5 then err(\"too big\") else ok(v * 2)\n}\n\
         effect fn forward(mut xs: List[Int], v: Int) -> Int = {\n  list.push(xs, 0)\n  let r = push_then_err(xs, v)!\n  r + 1\n}\n\
         effect fn main() -> Unit = {\n  var xs: List[Int] = []\n  let a = forward(xs, 9) ?? -1\n  let m = match push_then_err(xs, 3) {\n    ok(v) => \"ok ${v}\",\n    err(e) => \"err ${e}\",\n  }\n  var s = \"s\"\n  let b = append_then_check(s, -3) ?? -1\n  println(\"${a} ${m} ${b} ${xs} ${s}\")\n}\n",
    );
}

#[test]
fn unit_effect_fn_raising_if_lowers() {
    assert_all_lower(
        "unit-raise",
        "effect fn visit(x: Int) -> Unit = if x < 0 then err(\"negative\")!\nelse ()\n\
         effect fn stop_at(x: Int) -> Unit = if x == 3 then err(\"stopped\")!\nelse if x > 3 then println(\"past\") else ()\n\
         effect fn show_unit(p: String) -> Unit = {\n  guard p != \"\" else err(\"empty\")!\n  ()\n}\n\
         effect fn main() -> Unit = {\n  visit(1)!\n  stop_at(5)!\n  show_unit(\"p\")!\n}\n",
    );
}

#[test]
fn nested_mut_call_after_call_operands_lowers() {
    // The inner call's write-back block follows two call operands; they are
    // bound first, in order, so they still run before it.
    assert_all_lower(
        "hoisted-args",
        "fn arg(s: List[Int], i: Int) -> Int = list.get(s, i) ?? 0\n\
         fn set2(mut s: List[Int], x: Int, y: Int) -> Int = {\n  list.push(s, x)\n  list.push(s, y)\n  x + y\n}\n\
         fn main() -> Unit = {\n  var s = [3, 4, 5]\n  let n = set2(s, arg(s, 2), set2(s, arg(s, 1), arg(s, 0)))\n  println(\"${n} ${s}\")\n}\n",
    );
}

#[test]
fn unit_main_guard_err_keeps_its_error_exit() {
    // A Unit `main`'s raise is its error exit, not a value: the known-constructor
    // reduction must leave it to the abort-line rewrite. Reducing it once let the
    // native render drop the err arm and exit 0.
    let src = "effect fn main() -> Unit = {\n  let n = string.len(\"\")\n  guard n >= 1 else err(\"guard tripped\")\n  println(\"unreachable\")\n}\n";
    if let Ok(code) = almide_mir::pipeline::try_render_rust_source(src) {
        assert!(code.contains("guard tripped"), "the err exit vanished from the render:\n{code}");
    }
}
