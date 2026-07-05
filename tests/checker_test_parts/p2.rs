// ---- Effect isolation (Layer 1 security) ----

#[test]
fn effect_isolation_pure_cannot_call_effect() {
    let errs = errors(
        "effect fn load() -> Result[String, String] = ok(\"data\")\nfn f() -> String = load()"
    );
    assert!(!errs.is_empty(), "pure fn calling effect fn should error");
    assert!(errs[0].contains("effect"), "error should mention effect, got: {}", errs[0]);
}

#[test]
fn effect_isolation_effect_can_call_effect() {
    has_no_errors(
        "effect fn load() -> Result[String, String] = ok(\"data\")\neffect fn f() -> Result[String, String] = load()"
    );
}

#[test]
fn effect_isolation_test_can_call_effect() {
    has_no_errors(
        "effect fn load() -> Result[String, String] = ok(\"data\")\ntest \"use effect\" {\n  let _ = load()\n  assert(true)\n}"
    );
}

#[test]
fn effect_isolation_pure_can_call_pure() {
    has_no_errors(
        "fn double(x: Int) -> Int = x * 2\nfn f() -> Int = double(5)"
    );
}

#[test]
fn effect_isolation_fan_in_pure_fn() {
    let errs = errors(
        "effect fn a() -> Result[Int, String] = ok(1)\neffect fn b() -> Result[Int, String] = ok(2)\nfn f() -> (Int, Int) = fan { a(); b() }"
    );
    assert!(!errs.is_empty(), "fan in pure fn should error");
    assert!(errs[0].contains("fan") || errs[0].contains("effect"), "error should mention fan or effect, got: {}", errs[0]);
}

#[test]
fn effect_isolation_fan_in_effect_fn() {
    has_no_errors(
        "effect fn a() -> Result[Int, String] = ok(1)\neffect fn b() -> Result[Int, String] = ok(2)\neffect fn f() -> Result[Unit, String] = {\n  let _ = fan { a(); b() }\n  ok(())\n}"
    );
}

#[test]
fn effect_isolation_fan_var_capture_rejected() {
    let errs = errors(
        "effect fn a() -> Result[Int, String] = ok(1)\neffect fn f() -> Result[Unit, String] = {\n  var x = 0\n  let _ = fan { a(); a() }\n  ok(())\n}"
    );
    // No error — var x is not captured inside fan
    assert!(errs.is_empty(), "var not captured should be fine, got: {:?}", errs);
}

#[test]
fn effect_isolation_fan_var_capture_error() {
    let errs = errors(
        "effect fn a(n: Int) -> Result[Int, String] = ok(n)\neffect fn f() -> Result[Unit, String] = {\n  var x = 0\n  let _ = fan { a(x); a(x) }\n  ok(())\n}"
    );
    assert!(!errs.is_empty(), "capturing var in fan should error");
    assert!(errs[0].contains("mutable") || errs[0].contains("var"), "error should mention mutable/var, got: {}", errs[0]);
}

#[test]
fn effect_isolation_stdlib_effect_fn() {
    let errs = errors(
        "import fs\nfn f(path: String) -> String = fs.read_text(path)"
    );
    assert!(!errs.is_empty(), "pure fn calling stdlib effect fn should error");
    assert!(errs[0].contains("effect"), "error should mention effect, got: {}", errs[0]);
}

// ---- Escape analysis: var mutation in lambdas ----

#[test]
fn escape_var_local_no_lambda() {
    // var local to function, mutated in same scope (no lambda) — OK
    has_no_errors("fn sum(xs: List[Int]) -> Int = { var total = 0; for x in xs { total = total + x }; total }");
}

#[test]
fn escape_var_read_only_in_lambda() {
    // var read (not mutated) in lambda — OK
    has_no_errors("fn offset_all(xs: List[Int]) -> List[Int] = { var base = 10; list.map(xs, (x) => x + base) }");
}

#[test]
fn escape_var_mutated_in_lambda_pure_fn() {
    // var mutated inside lambda in pure fn — ERROR
    let errs = errors(
        "fn bad() -> fn() -> Unit = { var count = 0; () => { count = count + 1 } }"
    );
    assert!(!errs.is_empty(), "should error on var mutation in lambda inside pure fn");
    assert!(errs.iter().any(|e| e.contains("mutated inside a closure")),
        "error should mention closure mutation, got: {:?}", errs);
}

#[test]
fn escape_var_mutated_in_lambda_effect_fn() {
    // var mutated inside lambda in effect fn — OK
    has_no_errors(
        "effect fn counter() -> Result[Int, String] = { var count = 0; let inc = () => { count = count + 1 }; inc(); ok(count) }"
    );
}

#[test]
fn escape_var_declared_inside_lambda() {
    // var declared AND mutated inside same lambda — OK (same scope)
    has_no_errors(
        "fn transform(xs: List[Int]) -> List[Int] = list.map(xs, (x) => { var temp = x * 2; temp = temp + 1; temp })"
    );
}

#[test]
fn escape_nested_lambda_mutation() {
    // var from outer scope mutated in deeply nested lambda — ERROR
    let errs = errors(
        "fn bad() -> fn() -> fn() -> Unit = { var x = 0; () => { () => { x = x + 1 } } }"
    );
    assert!(!errs.is_empty(), "should error on var mutation in nested lambda inside pure fn");
    assert!(errs.iter().any(|e| e.contains("mutated inside a closure")),
        "error should mention closure mutation, got: {:?}", errs);
}

#[test]
fn escape_multiple_vars_mutated() {
    // Multiple vars mutated in lambda — should report errors for each
    let errs = errors(
        "fn bad() -> fn() -> Unit = { var a = 0; var b = 0; () => { a = 1; b = 2 } }"
    );
    assert!(errs.len() >= 2, "should report errors for both vars, got: {:?}", errs);
}

// ---- Exhaustiveness: nested patterns ----

#[test]
fn exhaust_nested_option() {
    // Missing some(none)
    let errs = errors(
        "fn f(x: Option[Option[Int]]) -> Int = match x {\n  some(some(n)) => n\n  none => 0\n}"
    );
    assert!(errs.iter().any(|e| e.contains("some(none)")), "should report some(none), got: {:?}", errs);
}

#[test]
fn exhaust_nested_option_complete() {
    has_no_errors(
        "fn f(x: Option[Option[Int]]) -> Int = match x {\n  some(some(n)) => n\n  some(none) => -1\n  none => 0\n}"
    );
}

#[test]
fn exhaust_nested_result_in_option() {
    // Missing some(err(_))
    let errs = errors(
        "fn f(x: Option[Result[Int, String]]) -> String = match x {\n  some(ok(n)) => \"ok\"\n  none => \"none\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("some(err(_))")), "should report some(err(_)), got: {:?}", errs);
}

#[test]
fn exhaust_deep_variant() {
    // type Expr = | Lit(Int) | Neg(Expr)
    // Missing Neg(Neg(_))
    let errs = errors(
        "type Expr =\n  | Lit(Int)\n  | Neg(Expr)\nfn eval(e: Expr) -> Int = match e {\n  Lit(n) => n\n  Neg(Lit(n)) => 0 - n\n}"
    );
    assert!(errs.iter().any(|e| e.contains("Neg(Neg(_))")), "should report Neg(Neg(_)), got: {:?}", errs);
}

#[test]
fn exhaust_variant_with_wildcard_nested() {
    has_no_errors(
        "type Expr =\n  | Lit(Int)\n  | Neg(Expr)\nfn eval(e: Expr) -> Int = match e {\n  Lit(n) => n\n  Neg(x) => 0\n}"
    );
}

// ---- Exhaustiveness: tuple patterns ----

#[test]
fn exhaust_tuple_bool_pair() {
    // Missing (true, false), (false, true)
    let errs = errors(
        "fn f(p: (Bool, Bool)) -> String = match p {\n  (true, true) => \"tt\"\n  (false, false) => \"ff\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("(true, false)")), "should report (true, false), got: {:?}", errs);
    assert!(errs.iter().any(|e| e.contains("(false, true)")), "should report (false, true), got: {:?}", errs);
}

#[test]
fn exhaust_tuple_bool_pair_complete() {
    has_no_errors(
        "fn f(p: (Bool, Bool)) -> String = match p {\n  (true, true) => \"tt\"\n  (true, false) => \"tf\"\n  (false, true) => \"ft\"\n  (false, false) => \"ff\"\n}"
    );
}

#[test]
fn exhaust_tuple_with_wildcard() {
    has_no_errors(
        "fn f(p: (Bool, Bool)) -> String = match p {\n  (true, true) => \"tt\"\n  _ => \"other\"\n}"
    );
}

// ---- Exhaustiveness: infinite domain ----

#[test]
fn exhaust_int_without_wildcard() {
    let errs = errors(
        "fn f(x: Int) -> String = match x {\n  0 => \"zero\"\n  1 => \"one\"\n}"
    );
    assert!(!errs.is_empty(), "should require _ for Int match");
}

#[test]
fn exhaust_int_with_wildcard() {
    has_no_errors(
        "fn f(x: Int) -> String = match x {\n  0 => \"zero\"\n  _ => \"other\"\n}"
    );
}

#[test]
fn exhaust_string_without_wildcard() {
    let errs = errors(
        "fn f(x: String) -> Int = match x {\n  \"a\" => 1\n  \"b\" => 2\n}"
    );
    assert!(!errs.is_empty(), "should require _ for String match");
}

// ---- Exhaustiveness: existing flat cases still work ----

#[test]
fn exhaust_variant_missing_case() {
    let errs = errors(
        "type Color =\n  | Red\n  | Green\n  | Blue\nfn name(c: Color) -> String = match c {\n  Red => \"red\"\n  Green => \"green\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("Blue")), "should report Blue, got: {:?}", errs);
}

#[test]
fn exhaust_option_missing_none() {
    let errs = errors(
        "fn f(x: Option[Int]) -> Int = match x {\n  some(v) => v\n}"
    );
    assert!(errs.iter().any(|e| e.contains("none")), "should report none, got: {:?}", errs);
}

#[test]
fn exhaust_result_missing_err() {
    let errs = errors(
        "fn f(x: Result[Int, String]) -> Int = match x {\n  ok(v) => v\n}"
    );
    assert!(errs.iter().any(|e| e.contains("err")), "should report err, got: {:?}", errs);
}

#[test]
fn exhaust_bool_missing_false() {
    let errs = errors(
        "fn f(x: Bool) -> Int = match x {\n  true => 1\n}"
    );
    assert!(errs.iter().any(|e| e.contains("false")), "should report false, got: {:?}", errs);
}

#[test]
fn exhaust_guard_not_counted() {
    // Guard arms don't guarantee coverage
    let errs = errors(
        "type AB = | A | B\nfn f(x: AB) -> Int = match x {\n  A => 1\n  B if true => 2\n}"
    );
    assert!(errs.iter().any(|e| e.contains("B")), "guarded B should not count, got: {:?}", errs);
}

// ── Sized Numeric Types Stage 1c: mixed-width arithmetic rejection ──

#[test]
fn sized_mixed_width_int8_int32_rejected() {
    let errs = errors(
        "fn f(a: Int8, b: Int32) -> Int32 = a + b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject Int8 + Int32, got: {:?}", errs);
}

#[test]
fn sized_mixed_width_float32_int32_rejected() {
    let errs = errors(
        "fn f(a: Float32, b: Int32) -> Float32 = a + b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject Float32 + Int32, got: {:?}", errs);
}

#[test]
fn sized_mixed_width_uint16_int16_rejected() {
    let errs = errors(
        "fn f(a: UInt16, b: Int16) -> Int16 = a * b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject UInt16 * Int16 (even same width, different signedness), got: {:?}", errs);
}

#[test]
fn sized_same_width_arith_ok() {
    has_no_errors("fn f(a: Int32, b: Int32) -> Int32 = a + b");
    has_no_errors("fn f(a: UInt8, b: UInt8) -> UInt8 = a - b");
    has_no_errors("fn f(a: Float32, b: Float32) -> Float32 = a * b");
}

#[test]
fn sized_literal_coercion_ok() {
    has_no_errors("fn f(a: Int32) -> Int32 = a + 5");
    has_no_errors("fn f(a: Int32) -> Int32 = 10 + a");
    has_no_errors("fn f(a: Float32) -> Float32 = a + 1.5");
}

#[test]
fn sized_canonical_int_plus_sized_ok() {
    // `Int` / `Float` canonical types stay permissive to preserve the
    // literal-coercion story. `Int + Int32` is therefore accepted (the
    // right-hand side collapses to the sized variant at emit time).
    has_no_errors("fn f(a: Int, b: Int32) -> Int32 = a + b");
}

#[test]
fn sized_mixed_all_ops_rejected() {
    for op in ["-", "*", "/", "%", "^"] {
        let src = format!("fn f(a: Int8, b: Int32) -> Int32 = a {} b", op);
        let errs = errors(&src);
        assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
            "operator '{}' should reject mixed sized types, got: {:?}", op, errs);
    }
}

// ── Numeric protocol (P3 of Matrix[T] dtype arc) ──

#[test]
fn numeric_protocol_accepts_int() {
    has_no_errors("fn double[T: Numeric](x: T) -> T = x + x\nfn use_it() -> Int = double(21)");
}

#[test]
fn numeric_protocol_accepts_float() {
    has_no_errors("fn double[T: Numeric](x: T) -> T = x + x\nfn use_it() -> Float = double(1.5)");
}

#[test]
fn numeric_protocol_accepts_int32() {
    has_no_errors("fn double[T: Numeric](x: T) -> T = x + x\nfn use_it(x: Int32) -> Int32 = double(x)");
}

#[test]
fn numeric_protocol_rejects_string() {
    let errs = errors(
        "fn double[T: Numeric](x: T) -> T = x + x\nfn use_it() -> String = double(\"x\")"
    );
    assert!(errs.iter().any(|e| e.contains("does not implement protocol 'Numeric'")),
        "should reject String, got: {:?}", errs);
}

#[test]
fn numeric_protocol_rejects_bool() {
    let errs = errors(
        "fn double[T: Numeric](x: T) -> T = x + x\nfn use_it() -> Bool = double(true)"
    );
    assert!(errs.iter().any(|e| e.contains("does not implement protocol 'Numeric'")),
        "should reject Bool, got: {:?}", errs);
}

#[test]
fn sized_int64_explicit_vs_int32_rejected() {
    let errs = errors(
        "fn mix(a: Int32, b: Int64) -> Int32 = a + b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject Int32 + Int64, got: {:?}", errs);
}

#[test]
fn sized_float64_explicit_vs_float32_rejected() {
    let errs = errors(
        "fn mix(a: Float32, b: Float64) -> Float64 = a + b"
    );
    assert!(errs.iter().any(|e| e.contains("mixes sized numeric")),
        "should reject Float32 + Float64, got: {:?}", errs);
}

#[test]
fn sized_int_and_int64_interop_ok() {
    // Canonical `Int` stays the literal-coercion slot; it interops
    // with `Int64` freely at the same width.
    has_no_errors("fn f(a: Int, b: Int64) -> Int64 = b");
    has_no_errors("fn f(a: Int64) -> Int = a");
}

// ── Strict Matrix[T] discrimination (post-C) ──

#[test]
fn strict_matrix_f32_rejects_bare_matrix() {
    // A fn that asks for `Matrix[Float32]` MUST NOT accept a bare
    // `Matrix` value — bare carries no f32 guarantee.
    let errs = errors(
        "fn needs_f32(m: Matrix[Float32]) -> Int = matrix.rows(m)\nfn use_bare() -> Int = needs_f32(matrix.zeros(3, 3))"
    );
    assert!(errs.iter().any(|e| e.contains("expects")),
        "should reject bare Matrix passed to Matrix[Float32] param, got: {:?}", errs);
}

#[test]
fn strict_matrix_bare_accepts_typed() {
    // `matrix.shape(m: Matrix)` still accepts `Matrix[Float32]`
    // (bare widens to typed downstream via the runtime tag).
    has_no_errors(
        "fn row_count_f32(m: Matrix[Float32]) -> Int = matrix.rows(m)"
    );
}

#[test]
fn strict_matrix_float_alias_interop() {
    // `Matrix[Float]` is the legacy alias for bare `Matrix` — both
    // directions stay compatible at the checker layer.
    has_no_errors(
        "fn f(m: Matrix[Float]) -> Int = matrix.rows(m)\nfn g() -> Int = f(matrix.zeros(3, 3))"
    );
    has_no_errors(
        "fn f(m: Matrix) -> Int = matrix.rows(m)\nfn g() -> Int = {\n  let m: Matrix[Float] = matrix.zeros(3, 3)\n  f(m)\n}"
    );
}

#[test]
fn set_of_closures_rejected() {
    // A closure has no equality/hash, so a `Set` of closures is meaningless. The
    // two targets disagreed (native rustc E0277, WASM silently dropped inserts and
    // printed 0), so reject it at typecheck on both. (E016)
    let errs = errors(
        "effect fn main() -> Unit = {\n  var s: Set[() -> Unit] = set.new()\n}"
    );
    assert!(errs.iter().any(|e| e.contains("Set") && e.contains("function")),
        "should reject Set[() -> Unit], got: {:?}", errs);
}

#[test]
fn map_with_closure_key_rejected() {
    // Same reason for a `Map` *key*. (E016)
    let errs = errors(
        "effect fn main() -> Unit = {\n  var m: Map[() -> Unit, Int] = map.new()\n}"
    );
    assert!(errs.iter().any(|e| e.contains("Map") && e.contains("key") && e.contains("function")),
        "should reject Map[() -> Unit, Int], got: {:?}", errs);
}

#[test]
fn map_with_closure_value_allowed() {
    // A closure is fine as a `Map` *value* — only the key must be comparable.
    has_no_errors(
        "effect fn main() -> Unit = {\n  var m: Map[String, () -> Unit] = map.new()\n  map.insert(m, \"a\", () => {})\n}"
    );
}

#[test]
fn enum_name_record_construction_rejected() {
    // Constructing a record literal via the ENUM TYPE name (not a case name) is a
    // category error. The two targets disagreed (native rustc leaked E0574, WASM
    // accepted it and mis-constructed the value with an empty field), so reject it
    // at typecheck on both with a proper diagnostic. (E017)
    let errs = errors(
        "type V = Tag(Float) | Named { who: String }\nfn main() -> Unit = {\n  let v = V { who: \"x\" }\n  println(\"${v}\")\n}"
    );
    assert!(errs.iter().any(|e| e.contains("enum type 'V'") && e.contains("record syntax")),
        "should reject `V {{ who: ... }}` on the enum type name, got: {:?}", errs);
}

#[test]
fn enum_name_record_construction_hint_lists_cases() {
    // The hint must name the available record-variant case so the fix is obvious.
    let hints = error_hints(
        "type V = Tag(Float) | Named { who: String }\nfn main() -> Unit = {\n  let v = V { who: \"x\" }\n  println(\"${v}\")\n}"
    );
    assert!(hints.iter().any(|h| h.contains("Named")),
        "hint should mention the `Named` case, got: {:?}", hints);
}

#[test]
fn record_type_construction_still_allowed() {
    // A legitimate record TYPE (not an enum) must still construct fine.
    has_no_errors(
        "type Point = { x: Int, y: Int }\nfn main() -> Unit = {\n  let p = Point { x: 1, y: 2 }\n  println(\"${p.x}\")\n}"
    );
}

#[test]
fn record_variant_case_construction_still_allowed() {
    // Constructing the record-bearing CASE (not the enum type) is the correct form.
    has_no_errors(
        "type V = Tag(Float) | Named { who: String }\nfn main() -> Unit = {\n  let v = Named { who: \"x\" }\n  println(\"${v}\")\n}"
    );
}

// ── E014 reachability with literal sub-patterns (A2 regression-lock) ──
//
// A literal nested in a constructor pattern (`some(1)`, `ok(0)`, `some("a")`)
// is a REFINEMENT of that constructor, not a full cover of it. So:
//   • a literal arm must NOT shadow a later binder arm of the same ctor, and
//   • two DISTINCT literals must NOT shadow each other.
// The `is_useful` infinite-domain guard (`enumerable`) realizes this: after
// `some(1)`, the value `some(2)` is still uncovered (Int is infinite), so
// `some(x)` stays reachable. These positive cases must check clean.

#[test]
fn literal_some_then_binder_is_reachable() {
    // some(1)/some(2)/some(x)/none — the binder catches every other Int.
    has_no_errors(
        "fn f(o: Option[Int]) -> String = match o {\n  some(1) => \"a\"\n  some(2) => \"b\"\n  some(x) => \"o\"\n  none => \"n\"\n}"
    );
}

#[test]
fn literal_some_with_none_between_binder_is_reachable() {
    // The outer Some/None space is complete BEFORE the binder, so this drives
    // the `enumerable && is_complete` arm of `is_useful` — the binder is still
    // reachable because the inner Int domain is infinite.
    has_no_errors(
        "fn f(o: Option[Int]) -> String = match o {\n  some(1) => \"a\"\n  none => \"n\"\n  some(x) => \"o\"\n}"
    );
}

#[test]
fn literal_result_then_binder_is_reachable() {
    has_no_errors(
        "fn f(r: Result[Int, String]) -> String = match r {\n  ok(0) => \"z\"\n  err(e) => e\n  ok(n) => \"n\"\n}"
    );
}

#[test]
fn string_literal_some_then_binder_is_reachable() {
    has_no_errors(
        "fn f(o: Option[String]) -> String = match o {\n  some(\"a\") => \"A\"\n  none => \"N\"\n  some(x) => x\n}"
    );
}

#[test]
fn distinct_int_literals_do_not_shadow() {
    has_no_errors(
        "fn f(n: Int) -> String = match n {\n  1 => \"a\"\n  2 => \"b\"\n  3 => \"c\"\n  x => \"o\"\n}"
    );
}

// Negative direction: the loosening must NOT swallow genuine dead arms. A
// binder BEFORE a same-ctor literal covers it (the binder already matches
// `some(1)`), and a duplicate literal is dead — both stay E014.

#[test]
fn binder_before_literal_is_still_unreachable() {
    let errs = errors(
        "fn f(o: Option[Int]) -> String = match o {\n  some(x) => \"o\"\n  some(1) => \"a\"\n  none => \"n\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("unreachable match arm")),
        "some(1) after some(x) must report E014, got: {:?}", errs);
}

#[test]
fn duplicate_int_literal_is_still_unreachable() {
    let errs = errors(
        "fn f(o: Option[Int]) -> String = match o {\n  some(1) => \"a\"\n  some(1) => \"d\"\n  some(x) => \"o\"\n  none => \"n\"\n}"
    );
    assert!(errs.iter().any(|e| e.contains("unreachable match arm")),
        "duplicate some(1) must report E014, got: {:?}", errs);
}

#[test]
fn ambiguous_constructor_reports_e019() {
    // The same ctor `Pong` in two variant types is ambiguous when used bare (#413).
    let errs = errors(
        "type Cmd = | Pong | Move(Int)\n\
         type Resp = | Pong | Ack(Int)\n\
         fn cmd(c: Cmd) -> Int = match c { Pong => 1, Move(x) => x }\n\
         fn main() -> Unit = { println(int.to_string(cmd(Pong))) }"
    );
    assert!(errs.iter().any(|e| e.contains("ambiguous constructor 'Pong'")),
        "ambiguous ctor must report E019, got: {:?}", errs);
}

#[test]
fn unambiguous_constructor_is_not_flagged() {
    // A ctor name in exactly one type must NOT trip the ambiguity check.
    has_no_errors(
        "type Cmd = | Stop | Move(Int)\n\
         fn cmd(c: Cmd) -> Int = match c { Stop => 0, Move(x) => x }\n\
         fn main() -> Unit = { println(int.to_string(cmd(Stop))) }"
    );
}

// ── #652: ordering operators reject compound operands (check ⇄ codegen parity) ──

#[test]
fn ordering_on_tuple_rejected() {
    let errs = errors("fn main() -> Unit = println(if (1, 2) < (1, 3) then \"lt\" else \"ge\")");
    assert!(errs.iter().any(|e| e.contains("not defined for")),
        "tuple `<` should be rejected, got: {:?}", errs);
}

#[test]
fn ordering_on_record_rejected() {
    let errs = errors(
        "type P = { x: Int, y: Int }\n\
         fn main() -> Unit = { let a: P = { x: 1, y: 2 }\n let b: P = { x: 1, y: 3 }\n println(if a > b then \"y\" else \"n\") }"
    );
    assert!(errs.iter().any(|e| e.contains("not defined for")),
        "record `>` should be rejected, got: {:?}", errs);
}

#[test]
fn ordering_on_list_rejected() {
    let errs = errors("fn main() -> Unit = println(if [1, 2] <= [1, 3] then \"a\" else \"b\")");
    assert!(errs.iter().any(|e| e.contains("not defined for")),
        "list `<=` should be rejected, got: {:?}", errs);
}

#[test]
fn ordering_on_scalars_still_accepted() {
    // Int / Float / String / Bool ordering must keep working — no false positives.
    for src in [
        "fn main() -> Unit = println(if 1 < 2 then \"a\" else \"b\")",
        "fn main() -> Unit = println(if 1.5 >= 2.0 then \"a\" else \"b\")",
        "fn main() -> Unit = println(if \"a\" < \"b\" then \"a\" else \"b\")",
        "fn main() -> Unit = println(if false < true then \"a\" else \"b\")",
    ] {
        let errs = errors(src);
        assert!(!errs.iter().any(|e| e.contains("not defined for")),
            "scalar ordering should be accepted: {} -> {:?}", src, errs);
    }
}

// ── #662: undecidable (unconstrained) type slot is rejected (E025), not ICE'd ──

#[test]
fn check_e025_unconstrained_error_type() {
    let errs = errors(
        "fn main() -> Unit = {\n  let r0: Result[Int, String] = ok(7)\n  let r = result.or_else(r0, (e) => ok(0))\n  println(\"done\")\n}"
    );
    assert!(errs.iter().any(|e| e.contains("cannot infer a concrete type")),
        "expected E025 unconstrained-type error, got: {:?}", errs);
}

#[test]
fn check_e025_annotated_ok() {
    let errs = errors(
        "fn main() -> Unit = {\n  let r0: Result[Int, String] = ok(7)\n  let r: Result[Int, String] = result.or_else(r0, (e) => ok(0))\n  println(\"done\")\n}"
    );
    assert!(!errs.iter().any(|e| e.contains("cannot infer a concrete type")),
        "annotated binding should resolve cleanly, got: {:?}", errs);
}
