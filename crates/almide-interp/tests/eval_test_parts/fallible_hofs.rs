// ── eval_test.rs, part 3: the __fallible_* carriers and effect-call matching ──
//
// include!-spliced into `tests/eval_test.rs` (#1856); the `lower` / `run` /
// `expect_out` helpers are the test binary's own.

// ── the __fallible_* carriers (ADR-0006's fallibility-polymorphic HOFs) ──
//
// These are what `list.map(xs, (x) => f(x)!)` INSTANTIATES: the checker sees
// the callback propagate and swaps in the fallible form, whose contract is
// first-err short-circuit. Every case is written the way a user writes it (the
// plain HOF name with a `!` inside), so these pin the rewrite as well as the
// carrier.
//
// EVERY expectation below was MEASURED from the native backend first — the
// interp must MATCH the backends, not be "correct" in the abstract (see
// crates/almide-interp/CLAUDE.md). That is why the err payload reads
// `invalid digit found in string`: it is Rust's own `parse::<i64>` message,
// surfaced verbatim, not a wrapper the interp is free to invent.
//
// Each member gets BOTH polarities — a full pass and a first-err — because the
// short-circuit is the only thing separating these from their plain siblings
// (CLAUDE.md: extend a family by matrix, never point-wise).

/// The renderers the cases share: a `Result` has no bare `repr`, so each shape
/// is matched and printed explicitly.
const SHOW: &str = "\
fn si(r: Result[List[Int], String]) -> String = match r {
  ok(xs) => \"ok[\" + list.join(list.map(xs, (n) => int.to_string(n)), \",\") + \"]\",
  err(e) => \"err:\" + e,
}
fn ss(r: Result[List[String], String]) -> String = match r {
  ok(xs) => \"ok[\" + list.join(xs, \",\") + \"]\",
  err(e) => \"err:\" + e,
}
fn so(r: Result[String?, String]) -> String = match r {
  ok(o) => \"ok:\" + (o ?? \"<none>\"),
  err(e) => \"err:\" + e,
}
fn sn(r: Result[Int, String]) -> String = match r {
  ok(n) => \"ok:\" + int.to_string(n),
  err(e) => \"err:\" + e,
}
fn su(r: Result[Unit, String]) -> String = match r {
  ok(_) => \"ok:unit\",
  err(e) => \"err:\" + e,
}
";

const PARSE_ERR: &str = "err:invalid digit found in string\n";

fn expect_try(body: &str, expected: &str) {
    expect_out(&format!("{}{}", SHOW, main_print(body)), expected);
}

#[test]
fn try_map_ok_and_first_err() {
    expect_try(
        "  println(si(list.map([\"1\", \"2\"], (s) => int.parse(s)!)))",
        "ok[1,2]\n",
    );
    expect_try(
        "  println(si(list.map([\"1\", \"zz\", \"3\"], (s) => int.parse(s)!)))",
        PARSE_ERR,
    );
}

#[test]
fn try_filter_ok_and_first_err() {
    expect_try(
        "  println(ss(list.filter([\"1\", \"2\"], (s) => int.parse(s)! > 1)))",
        "ok[2]\n",
    );
    expect_try(
        "  println(ss(list.filter([\"1\", \"zz\"], (s) => int.parse(s)! > 1)))",
        PARSE_ERR,
    );
}

#[test]
fn try_filter_map_ok_and_first_err() {
    expect_try(
        "  println(ss(list.filter_map([\"1\", \"2\"], (s) => if int.parse(s)! > 1 then some(s) else none)))",
        "ok[2]\n",
    );
    expect_try(
        "  println(ss(list.filter_map([\"zz\"], (s) => if int.parse(s)! > 1 then some(s) else none)))",
        PARSE_ERR,
    );
}

#[test]
fn try_flat_map_ok_and_first_err() {
    expect_try(
        "  println(si(list.flat_map([\"1\", \"2\"], (s) => [int.parse(s)!, 0])))",
        "ok[1,0,2,0]\n",
    );
    expect_try(
        "  println(si(list.flat_map([\"1\", \"zz\"], (s) => [int.parse(s)!, 0])))",
        PARSE_ERR,
    );
}

#[test]
fn try_find_hit_stops_before_a_later_error() {
    // The HIT ends the traversal, so the trailing "zz" is NEVER parsed: the
    // short-circuit is on the find, not only on the failure.
    expect_try(
        "  println(so(list.find([\"1\", \"2\", \"zz\"], (s) => int.parse(s)! == 2)))",
        "ok:2\n",
    );
    expect_try(
        "  println(so(list.find([\"1\"], (s) => int.parse(s)! == 9)))",
        "ok:<none>\n",
    );
    expect_try(
        "  println(so(list.find([\"zz\", \"1\"], (s) => int.parse(s)! == 1)))",
        PARSE_ERR,
    );
}

#[test]
fn try_fold_ok_and_first_err() {
    expect_try(
        "  println(sn(list.fold([\"1\", \"2\"], 0, (acc, s) => acc + int.parse(s)!)))",
        "ok:3\n",
    );
    expect_try(
        "  println(sn(list.fold([\"1\", \"zz\"], 0, (acc, s) => acc + int.parse(s)!)))",
        PARSE_ERR,
    );
}

#[test]
fn try_each_ok_and_first_err() {
    // `each` is the effect-only member, so the short-circuit is visible as the
    // MISSING "e3" line: the tail element is never reached.
    expect_try(
        "  println(su(list.each([\"1\", \"2\"], (s) => println(\"e\" + int.to_string(int.parse(s)!)))))",
        "e1\ne2\nok:unit\n",
    );
    expect_try(
        "  println(su(list.each([\"zz\", \"3\"], (s) => println(\"e\" + int.to_string(int.parse(s)!)))))",
        PARSE_ERR,
    );
}

// ── The effect-fn Result carrier (#1366) ─────────────────────────────────────
//
// An `effect fn f() -> T` has ABI return type `Result[T, String]`, and BOTH
// backends materialize that carrier. The interpreter used to hand the success
// value back BARE while modelling the failure channel as `Result(Err(..))`, so
// the subject of `match <effect call> { ok(v) => .., err(e) => .. }` was a
// plain scalar, no arm matched, and the run aborted with "non-exhaustive
// match" — a WRONG third vote against two agreeing backends on one of
// ADR-0008's sanctioned consumption spellings, which is worse than an honest
// skip. Found by the 3-way oracle while building #1341's fixture.

#[test]
fn matching_an_effect_call_sees_the_ok_carrier() {
    let (exit, out, err) = run(
        "effect fn pick(xs: List[Int]) -> Int = {\n\
         \x20 let r: Result[Int, String] = match list.get(xs, 0) {\n\
         \x20   some(a) => ok(a + 1),\n\
         \x20   none => err(\"no first\"),\n\
         \x20 }\n\
         \x20 r!\n\
         }\n\
         effect fn main() -> Unit = {\n\
         \x20 match pick([10]) {\n\
         \x20   ok(v) => println(\"sum \" + int.to_string(v)),\n\
         \x20   err(e) => println(\"fail \" + e),\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "sum 11\n"), "stderr: {err}");
}

#[test]
fn matching_an_effect_call_sees_the_err_carrier() {
    let (exit, out, err) = run(
        "effect fn pick(xs: List[Int]) -> Int = {\n\
         \x20 let r: Result[Int, String] = match list.get(xs, 0) {\n\
         \x20   some(a) => ok(a + 1),\n\
         \x20   none => err(\"no first\"),\n\
         \x20 }\n\
         \x20 r!\n\
         }\n\
         effect fn main() -> Unit = {\n\
         \x20 match pick([]) {\n\
         \x20   ok(v) => println(\"sum \" + int.to_string(v)),\n\
         \x20   err(e) => println(\"fail \" + e),\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "fail no first\n"), "stderr: {err}");
}

/// The carrier must NOT be applied twice. A declared-Result effect fn emits
/// `Result<T, E>` (spec §3: no double wrap), so `!` on its call still yields
/// the payload rather than a nested `Ok(Ok(..))`.
#[test]
fn a_declared_result_effect_fn_is_not_double_wrapped() {
    let (exit, out, err) = run(
        "effect fn twice(n: Int) -> Result[Int, String] =\n\
         \x20 if n < 0 then err(\"neg\") else ok(n * 2)\n\
         effect fn main() -> Unit = {\n\
         \x20 println(int.to_string(twice(21)!))\n\
         \x20 match twice(3) { ok(v) => println(\"ok \" + int.to_string(v)), err(e) => println(e) }\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "42\nok 6\n"), "stderr: {err}");
}
/// The NEIGHBOUR of the effect-fn gap, measured rather than assumed: a fallible
/// LAMBDA (ADR-0009 — its `!` falls into the lambda's own `Result[T, String]`
/// channel) already hands the carrier back, so `match` over its call was never
/// broken. `Closure` carries no return type, so the fix above could not have
/// covered this path even if it had needed covering — pinning it here records
/// that it does not.
#[test]
fn matching_a_fallible_lambda_call_already_sees_the_carrier() {
    let (exit, out, err) = run(
        "effect fn main() -> Unit = {\n\
         \x20 let g = (s: String) => int.parse(s)! * 2\n\
         \x20 match g(\"21\") {\n\
         \x20   ok(v) => println(\"ok \" + int.to_string(v)),\n\
         \x20   err(e) => println(\"err \" + e),\n\
         \x20 }\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "ok 42\n"), "stderr: {err}");
}

/// The carrier must not disturb the OTHER sanctioned ways to consume an effect
/// call's Result. Wrapping the success value changes what every consumption
/// site sees, so each is exercised rather than read: `!` propagation, `??`
/// fallback, `?` to Option, the value in an interpolation, and passing the
/// unwrapped payload as an argument.
#[test]
fn every_consumption_site_still_sees_the_payload() {
    let (exit, out, err) = run(
        "effect fn half(n: Int) -> Int = if n < 0 then err(\"neg\") else ok(n / 2)\n\
         fn twice(n: Int) -> Int = n * 2\n\
         effect fn main() -> Unit = {\n\
         \x20 println(int.to_string(half(10)!))\n\
         \x20 println(int.to_string(half(-1) ?? 99))\n\
         \x20 println(int.to_string(half(8) ?? 99))\n\
         \x20 println(\"interp \" + int.to_string(half(6)!))\n\
         \x20 println(int.to_string(twice(half(4)!)))\n\
         }\n",
    );
    assert_eq!(
        (exit, out.as_str()),
        (0, "5\n99\n4\ninterp 3\n4\n"),
        "stderr: {err}"
    );
}

/// An opaque newtype (`mod type` / `local type`) is erased by both backends:
/// its ctor call is the payload and its ctor pattern destructures the
/// payload. Pinned for the entry program's PLAIN newtype (bare identity,
/// `Token`) and its shadow of a stdlib-owned name (`self.Value`, #1835), the
/// two spellings the ctor call and pattern carry into the IR.
#[test]
fn opaque_newtype_ctor_is_identity_on_its_payload() {
    let (exit, out, err) = run(
        "mod type Value = String\n\
         local type Token = Int\n\
         fn un(v: Value) -> String = match v {\n\
         \x20 Value(s) => s\n\
         }\n\
         fn depth(t: Token) -> Int = match t {\n\
         \x20 Token(n) => n + 1\n\
         }\n\
         fn main() -> Unit = {\n\
         \x20 let mine = Value(\"x\")\n\
         \x20 let items: List[Value] = [Value(\"a\"), Value(\"b\")]\n\
         \x20 println(\"${un(mine)} ${mine == Value(\"x\")} ${mine == Value(\"y\")} ${list.len(items)} ${un(items[1])}\")\n\
         \x20 println(int.to_string(depth(Token(2))))\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "x true false 2 b\n3\n"), "stderr: {err}");
}

/// A Unit-returning in-place writer over a TEMPORARY receiver (#1849): the
/// arguments evaluate in order, the temporary is mutated and dropped, and a
/// temporary that aliases a live binding's `Rc` is mutated as a COW copy —
/// the binding is untouched. A variable receiver keeps its write-back.
#[test]
fn inplace_writer_on_a_temporary_receiver_mutates_and_drops_it() {
    let (exit, out, err) = run(
        "fn same(b: Bytes) -> Bytes = b\n\
         fn traced(v: Int) -> Int = {\n\
         \x20 println(\"arg ${v}\")\n\
         \x20 v\n\
         }\n\
         fn main() -> Unit = {\n\
         \x20 bytes.append_u8(bytes.new(2), 511)\n\
         \x20 bytes.append_i16_be(bytes.from_list([traced(1)]), traced(2))\n\
         \x20 let x = bytes.from_list([1, 2])\n\
         \x20 bytes.append_u8(same(x), 7)\n\
         \x20 bytes.fill(same(x), 9)\n\
         \x20 println(\"${bytes.to_list(x)}\")\n\
         \x20 var v = bytes.new(0)\n\
         \x20 bytes.append_u8(v, 511)\n\
         \x20 println(\"${bytes.to_list(v)}\")\n\
         }\n",
    );
    assert_eq!((exit, out.as_str()), (0, "arg 1\narg 2\n[1, 2]\n[255]\n"), "stderr: {err}");
}
