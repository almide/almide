// ── tail of tests_part5_c.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

#[test]
fn nested_interp_min_int_and_computed_list_payloads() {
    // Two adversarial-fuzz regressions: (A) i64::MIN in a list interp rendered "-0" (negate overflow),
    // (B) some/ok of a COMPUTED list read none/ok([]). Both fixed.
    let src = "effect fn main() -> Unit = {\n\
        let mn: List[Int] = [0 - 9223372036854775807 - 1, 7] println(\"${mn}\")\n\
        let a: Option[List[Int]] = some(list.map([1, 2, 3], (n) => n * 2)) println(\"${a}\")\n\
        let b: Result[List[Int], String] = ok([1, 2] + [3]) println(\"${b}\")\n\
        let c: Option[List[Bool]] = some(list.map([1, 2], (n) => n > 1)) println(\"${c}\") }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("nested_edgecases", &render_wasm_program(&prog)) {
        assert_eq!(out, "[-9223372036854775808, 7]\nsome([2, 4, 6])\nok([1, 2, 3])\nsome([false, true])");
    }
}

#[test]
fn result_outer_nested_interp() {
    // Result-outer nested `${…}`: the ResultOk heap materializer admits a nested Option/Result ctor
    // Ok payload (construction) and the nested-payload bind seeds its read-shape (inner match).
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[Bool, String] = ok(true) let b: Result[List[Bool], String] = ok([true, false])\n\
        let c: Result[Option[Int], String] = ok(some(5)) let d: Result[Option[Int], String] = ok(none)\n\
        let e: Result[Result[Int, String], String] = ok(err(\"x\"))\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") println(\"${e}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_b"), "must auto-link result.to_string_b");
    if let Some(out) = build_and_run("result_nested", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok(true)\nok([true, false])\nok(some(5))\nok(none)\nok(err(\"x\"))");
    }
}

#[test]
fn option_outer_nested_interp() {
    // Option-outer nested `${…}`, incl a cap-as-tag inner Result[String,String] (the seed_variant_param
    // nested-payload fix) and a bool-list inner.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[Option[Bool]] = some(some(true)) let b: Option[Option[String]] = some(some(\"a\"))\n\
        let c: Option[Result[Int, String]] = some(ok(5)) let d: Option[Result[String, String]] = some(ok(\"q\"))\n\
        let e: Option[List[Bool]] = some([false, true])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") println(\"${e}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_rs"), "must auto-link option.to_string_rs");
    if let Some(out) = build_and_run("option_nested", &render_wasm_program(&prog)) {
        assert_eq!(out, "some(some(true))\nsome(some(\"a\"))\nsome(ok(5))\nsome(ok(\"q\"))\nsome([false, true])");
    }
}

#[test]
fn option_list_str_interp() {
    // `${Option[List[String]]}` → `some(["a", "b"])` / `none` — a HEAP-element inner list. The self-host
    // `option.to_string_ls` inlines the string quote+escape (\ " \n \r \t) since self-hosts can't call
    // each other. Escaping is exercised by the embedded quote/backslash.
    let src = "effect fn main() -> Unit = {\n\
        let a: Option[List[String]] = some([\"a\", \"b\"]) let b: Option[List[String]] = none let c: Option[List[String]] = some([])\n\
        let d: Option[List[String]] = some([\"q\\\"x\"])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_ls"), "must auto-link option.to_string_ls");
    if let Some(out) = build_and_run("option_list_str_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([\"a\", \"b\"])\nnone\nsome([])\nsome([\"q\\\"x\"])");
    }
}

#[test]
fn option_list_int_interp_and_construction() {
    // `${Option[List[Int]]}` → `some([1, 2, 3])` / `none` (nested compound). Two gaps close: the
    // OptionSome heap materializer now admits a scalar-list literal (incl the empty `some([])`), and
    // the self-host `option.to_string_li` renders it. A constructed Some list is also matchable.
    let src = "fn describe(o: Option[List[Int]]) -> String = match o { some(v) => int.to_string(list.len(v)), none => \"none\" }\n\
        effect fn main() -> Unit = {\n\
        let a: Option[List[Int]] = some([1, 2, 3]) let b: Option[List[Int]] = none let c: Option[List[Int]] = some([])\n\
        println(\"${a}\") println(\"${b}\") println(\"${c}\")\n\
        println(describe(a) + \",\" + describe(c)) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "option.to_string_li"), "Option[List[Int]] interp must auto-link option.to_string_li");
    if let Some(out) = build_and_run("option_list_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "some([1, 2, 3])\nnone\nsome([])\n3,0");
    }
}

#[test]
fn noncapturing_lambda_returned_as_funcref() {
    // A function RETURNING a non-capturing lambda / a bare fn reference — the trust-spine lifts it to
    // a table slot and returns the scalar funcref; the caller tracks the bound result so `f(args)`
    // dispatches through CallIndirect. A capturing closure still walls (a real env is a later brick).
    let src = "fn inc() -> (Int) -> Int = (x) => x + 1\n\
        fn tp(x: Int) -> Int = x * 2 + 3\n\
        fn getter() -> (Int) -> Int = tp\n\
        effect fn main() -> Unit = {\n\
        let f = inc() println(int.to_string(f(5))) println(int.to_string(f(41)))\n\
        let h = getter() println(int.to_string(h(6))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "inc"), "inc must lower");
    if let Some(out) = build_and_run("closure_return", &render_wasm_program(&prog)) {
        assert_eq!(out, "6\n42\n15");
    }
}

#[test]
fn fan_any_inlines_a_literal_thunk_list() {
    // `fan.any` over a LITERAL thunk list, deterministic on wasm: the FIRST Ok in list order,
    // else the fixed `fan.any: all candidates failed`. It inlines into a plain match chain,
    // avoiding an unrepresentable List[funcref] — the outer arms folded into each thunk level.
    //
    // This test also covered `fan.race`, which was REMOVED in 0.42.0 (E027 tombstone): under
    // the deterministic model it was exactly `thunks[0]()`, so its name promised a race the
    // language does not have. `fan.any` exercises the same literal-thunk-list lowering path.
    // Wave 1 respelling: the block form is the surface; it synthesizes the same
    // literal thunk list, so the inline path this test pins is unchanged.
    let src = "effect fn okn(n: Int) -> Result[Int, String] = ok(n * 3)\n\
        effect fn failing() -> Result[Int, String] = err(\"boom\")\n\
        effect fn main() -> Unit = {\n\
        match (fan.any { okn(10), okn(20) }) { ok(v) => println(\"r=\" + int.to_string(v)), err(e) => println(e) }\n\
        match (fan.any { failing(), okn(7) }) { ok(v) => println(\"any=\" + int.to_string(v)), err(e) => println(e) }\n\
        match (fan.any { failing(), failing() }) { ok(v) => println(\"ok\"), err(e) => println(\"af=\" + e) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("fan_any_literal", &render_wasm_program(&prog)) {
        assert_eq!(out, "r=30\nany=21\naf=fan.any: all candidates failed");
    }
}

#[test]
fn fan_map_int_lowers_to_self_host_traverse() {
    // `fan.map` over List[Int] with an (Int) -> Result[Int, String] callback — the compiler intrinsic
    // routed to the self-host `fan_map` (a fallible traverse invoking the lifted callback via
    // CallIndirect), collecting ok values in list order and short-circuiting on the first err. The
    // result is matched / auto-`!`-unwrapped.
    let src = "effect fn dbl(x: Int) -> Result[Int, String] = ok(x * 2)\n\
        effect fn checked(x: Int) -> Result[Int, String] = if x < 0 then err(\"neg\") else ok(x)\n\
        effect fn main() -> Unit = {\n\
        let doubled = fan.map([1, 2, 3], (x) => dbl(x))\n\
        println(int.to_string(doubled[0]) + \",\" + int.to_string(doubled[2]))\n\
        match fan.map([1, -2, 3], (x) => checked(x)) { ok(ys) => println(\"ok\"), err(e) => println(\"short:\" + e) } }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "fan.map"), "fan.map must auto-link the fan_map self-host");
    if let Some(out) = build_and_run("fan_map_int", &render_wasm_program(&prog)) {
        assert_eq!(out, "2,6\nshort:neg");
    }
}

#[test]
fn higher_order_result_traverse_matches_call_indirect() {
    // A fallible list traverse over a funcref callback — `match f(x) { ok => .., err => .. }` where
    // `f` is invoked via CallIndirect. The trust-spine seeds the CallIndirect result's read-shape and
    // hoists a computed-call match subject, so the traverse (short-circuit on first err) lowers. The
    // sequential `fan.map` semantics on wasm.
    let src = "fn go(xs: List[Int], f: (Int) -> Result[Int, String], i: Int, acc: List[Int]) -> Result[List[Int], String] =\n\
        if i >= list.len(xs) then ok(acc)\n\
        else match f(list.get(xs, i) ?? 0) { ok(y) => go(xs, f, i + 1, acc + [y]), err(e) => err(e) }\n\
        fn traverse(xs: List[Int], f: (Int) -> Result[Int, String]) -> Result[List[Int], String] = go(xs, f, 0, [])\n\
        fn show(r: Result[List[Int], String]) -> String = match r { ok(ys) => \"ok:\" + int.to_string(list.sum(ys)), err(e) => \"err:\" + e }\n\
        effect fn main() -> Unit = {\n\
        println(show(traverse([1, 2, 3, 4], (x) => ok(x * 2))))\n\
        println(show(traverse([1, -2, 3], (x) => if x > 0 then ok(x) else err(\"neg\")))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "traverse"), "traverse must lower");
    if let Some(out) = build_and_run("ho_traverse", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok:20\nerr:neg");
    }
}

#[test]
fn higher_order_heap_return_via_call_indirect() {
    // `fn apply(g, x) = g(x)` returning a heap value (Result / String) through a known funcref used to
    // wall a tail heap-result computed call; it now executes via `Op::CallIndirect` and moves the
    // owned result out. Opens higher-order functions returning heap values (the fan.map foundation).
    let src = "fn apply_r(f: (Int) -> Result[Int, String], x: Int) -> Result[Int, String] = f(x)\n\
        fn apply_s(f: (Int) -> String, x: Int) -> String = f(x)\n\
        fn show(r: Result[Int, String]) -> String = match r { ok(v) => \"ok:\" + int.to_string(v), err(e) => \"err:\" + e }\n\
        effect fn main() -> Unit = {\n\
        println(show(apply_r((y) => ok(y * 2), 5)))\n\
        println(show(apply_r((y) => if y > 0 then ok(y) else err(\"neg\"), -3)))\n\
        println(apply_s((y) => \"v\" + int.to_string(y), 7)) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "apply_r"), "apply_r must lower");
    if let Some(out) = build_and_run("higher_order_heap", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok:10\nerr:neg\nv7");
    }
}

#[test]
fn result_ok_err_concat_payload_materializes() {
    // `ok("n" + int.to_string(x))` / `err("bad " + …)` — a computed (ConcatStr) String payload the
    // trust-spine used to wall (only literal/Var/call payloads were handled). It now materializes the
    // concat and moves it into the Result, dropping the borrowed operand temps.
    let src = "fn classify(x: Int) -> Result[String, String] =\n\
        if x > 0 then ok(\"pos \" + int.to_string(x)) else err(\"neg \" + int.to_string(x))\n\
        fn show(r: Result[String, String]) -> String = match r { ok(s) => \"OK:\" + s, err(e) => \"ERR:\" + e }\n\
        effect fn main() -> Unit = { println(show(classify(7))) println(show(classify(-3))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "classify"), "classify must lower");
    if let Some(out) = build_and_run("result_concat", &render_wasm_program(&prog)) {
        assert_eq!(out, "OK:pos 7\nERR:neg -3");
    }
}

#[test]
fn result_interp_self_hosts_per_element_pair() {
    // `${Result[Int, String]}` / `${Result[String, String]}` render v0's `ok(<T>)` / `err(<E>)` via a
    // per-(T,E) self-host (`result.to_string` / `result.to_string_ss`); a String payload is quoted +
    // escaped. Any other pairing stays an unlinked clean wall.
    let src = "effect fn main() -> Unit = {\n\
        let a: Result[Int, String] = ok(42) let b: Result[Int, String] = err(\"bad\")\n\
        println(\"${a}\") println(\"${b}\") println(\"r=${a}!\")\n\
        let c: Result[String, String] = ok(\"hi\") let d: Result[String, String] = err(\"x\")\n\
        println(\"${c}\") println(\"${d}\") }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string"), "Result[Int,String] interp must auto-link result.to_string");
    assert!(prog.functions.iter().any(|f| f.name == "result.to_string_ss"), "Result[String,String] interp must auto-link result.to_string_ss");
    if let Some(out) = build_and_run("result_interp", &render_wasm_program(&prog)) {
        assert_eq!(out, "ok(42)\nerr(\"bad\")\nr=ok(42)!\nok(\"hi\")\nerr(\"x\")");
    }
}

#[test]
fn tuple_multifield_and_single_ctor_matches_lower() {
    // A TUPLE-subject match (`desugar_tuple_match`), a MULTI-FIELD variant match (regrouped into a
    // tuple payload sub-match), and a SINGLE-CTOR newtype match (routed through an IfThen merge with
    // an unreachable empty-heap else — no double-move). Every arm's literal/column must select the
    // exact result, byte-identical to v0.
    let src = "type Ev = KV(String, Int) | Tag(String)\n\
        type Rec = Pair(Int, String)\n\
        type Boxed = B(Int)\n\
        fn tup(t: (String, Int)) -> String = match t { (\"a\", 1) => \"A1\", (\"a\", _) => \"AX\", (_, 0) => \"X0\", (_, _) => \"XX\" }\n\
        fn ev(e: Ev) -> String = match e { KV(\"count\", n) => \"C\" + int.to_string(n), KV(_, n) => \"K\" + int.to_string(n), Tag(_) => \"T\" }\n\
        fn rec(r: Rec) -> String = match r { Pair(1, \"one\") => \"1ONE\", Pair(1, _) => \"1X\", Pair(_, _) => \"XX\" }\n\
        fn unbox(b: Boxed) -> String = match b { B(n) => \"b\" + int.to_string(n) }\n\
        effect fn main() -> Unit = {\n\
        println(tup((\"a\", 1))) println(tup((\"z\", 0))) println(tup((\"z\", 5)))\n\
        println(ev(KV(\"count\", 3))) println(ev(KV(\"x\", 7))) println(ev(Tag(\"t\")))\n\
        println(rec(Pair(1, \"one\"))) println(rec(Pair(1, \"z\"))) println(rec(Pair(2, \"z\")))\n\
        println(unbox(B(42))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "unbox"), "single-ctor unbox must lower");
    if let Some(out) = build_and_run("tuple_multifield_match", &render_wasm_program(&prog)) {
        assert_eq!(out, "A1\nX0\nXX\nC3\nK7\nT\n1ONE\n1X\nXX\nb42");
    }
}

#[test]
fn guarded_option_result_match_regroups_and_lowers() {
    // A heap-result `match` over an Option / Result subject whose arms carry GUARDS + LITERAL
    // payloads regroups into constructor dispatch + a scalar payload sub-match (`some(n) if g` →
    // `some($p) => match $p { n if g => .. }`), so the guarded-variant case reduces to the proven
    // variant-tag dispatch + scalar guard/literal chain. A guard/literal MUST select the exact arm.
    let src = "type Tok = Word(String) | Num(Int)\n\
        fn olabel(x: Option[Int]) -> String = match x { some(n) if n > 100 => \"big\", some(n) if n > 0 => \"pos\", some(0) => \"zero\", some(_) => \"neg\", none => \"none\" }\n\
        fn rlabel(r: Result[Int, String]) -> String = match r { ok(v) if v > 0 => \"ok+\", ok(0) => \"ok0\", ok(_) => \"ok-\", err(e) if string.len(e) > 5 => \"eL\", err(_) => \"eS\" }\n\
        fn tclass(t: Tok) -> String = match t { Word(\"hi\") => \"HI\", Word(_) => \"W\", Num(7) => \"SEVEN\", Num(_) => \"N\" }\n\
        effect fn main() -> Unit = {\n\
        println(olabel(some(200))) println(olabel(some(5))) println(olabel(some(0))) println(olabel(none))\n\
        println(rlabel(ok(7))) println(rlabel(ok(0))) println(rlabel(err(\"longmsg\"))) println(rlabel(err(\"no\")))\n\
        println(tclass(Word(\"hi\"))) println(tclass(Word(\"z\"))) println(tclass(Num(7))) println(tclass(Num(3))) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "olabel"), "olabel must lower");
    assert!(prog.functions.iter().any(|f| f.name == "tclass"), "tclass must lower");
    if let Some(out) = build_and_run("guarded_variant_match", &render_wasm_program(&prog)) {
        assert_eq!(out, "big\npos\nzero\nnone\nok+\nok0\neL\neS\nHI\nW\nSEVEN\nN");
    }
}

#[test]
fn variant_ctor_in_result_ok_materializes() {
    // `ok(<user-variant ctor>)` in Result-Ok position (the derived variant-decode `ok(Pair(..))`
    // shape) MATERIALIZES the tagged variant block (the SAME block `let p = Pair(..)` builds, with its
    // recursive `$__drop_<V>` drop) and wraps it — NOT a dangling `CallFn "Pair"`. Covers a tuple
    // variant (heap + scalar fields), a scalar variant, a unit variant, and the Err arm; the consumer
    // reads the Ok payload as a real variant. Byte-identical to v0 `--target wasm`.
    let src = "type Shape = | Pair(Int, String) | Solo(Int) | Plain\n\
        fn build(t: Int, n: Int, s: String) -> Result[Shape, String] =\n\
        if t == 0 then ok(Pair(n, s)) else if t == 1 then ok(Solo(n)) else if t == 2 then ok(Plain) else err(\"bad\")\n\
        effect fn main() -> Unit = {\n\
        match build(0, 7, \"x\") { ok(v) => match v { Pair(n, s) => println(int.to_string(n) + s), Solo(n) => println(\"solo\"), Plain => println(\"plain\") }, err(e) => println(e) }\n\
        match build(2, 0, \"\") { ok(v) => match v { Pair(n, s) => println(\"p\"), Solo(n) => println(\"solo\"), Plain => println(\"plain\") }, err(e) => println(e) }\n\
        match build(9, 0, \"\") { ok(v) => match v { Pair(n, s) => println(\"p\"), Solo(n) => println(\"solo\"), Plain => println(\"plain\") }, err(e) => println(e) } }\n";
    let prog = lower_source(src);
    if let Some(out) = build_and_run("variant_result_ctor", &render_wasm_program(&prog)) {
        assert_eq!(out, "7x\nplain\nbad");
    }
}

#[test]
fn derived_codec_option_fields_decode() {
    // B-2 completion: Codec `Option[T]` fields. The self-hosted `__decode_option_T` builds a
    // `Result[Option[T], String]` (ok(some(x)) / ok(none) / err(e)) — a STRING leaf freed by the
    // recursive `$__drop_opt_str` (`resrec:opt_str`), a SCALAR leaf flat — byte-identical to v0.
    // Encode → decode → re-encode roundtrip: present (Some) survives, absent + explicit-null →
    // None, and a None field OMITS its key on re-encode (C-209: the encode-omission law).
    let src = "type Rec: Codec = { name: String, nick: Option[String], age: Option[Int] }\n\
        effect fn main() -> Unit = {\n\
        let r1 = Rec { name: \"A\", nick: some(\"nn\"), age: some(30) }\n\
        let v1 = r1.encode()\n\
        println(json.stringify(v1))\n\
        match Rec.decode(v1) { ok(r) => println(json.stringify(r.encode())), err(e) => println(\"err:\" + e) }\n\
        let pv = json.parse(\"{\\\"name\\\":\\\"B\\\",\\\"age\\\":null}\")\n\
        match pv { ok(pj) => match Rec.decode(pj) { ok(r) => println(json.stringify(r.encode())), err(e) => println(\"err:\" + e) }, err(pe) => println(\"parse:\" + pe) } }\n";
    let prog = lower_source(&format!("import json\n{src}"));
    assert!(prog.functions.iter().any(|f| f.name == "__decode_option_string"), "string option decode helper must link");
    assert!(prog.functions.iter().any(|f| f.name == "__decode_option_int"), "int option decode helper must link");
    if let Some(out) = build_and_run("codec_option_field", &render_wasm_program(&prog)) {
        assert_eq!(
            out,
            "{\"name\":\"A\",\"nick\":\"nn\",\"age\":30}\n\
             {\"name\":\"A\",\"nick\":\"nn\",\"age\":30}\n\
             {\"name\":\"B\"}"
        );
    }

}

#[test]
fn heap_and_fn_captures_execute_and_free_via_drop_closure() {
    // CLOSURE ENV FULL MODE: a String capture (co-owned, read back borrowed), a
    // List[Int] capture, and Fn captures (compose — the block captures two other
    // closure blocks, freed by $__drop_closure's SELF-RECURSION). Every closure
    // drop routes through the self-describing $__drop_closure — slot 0 (the
    // fnidx) is never treated as a pointer: a corrupted free would trap here,
    // so a clean byte-matched run IS the slot-0/mask/recursion pin.
    let src = "fn greeter(name: String) -> (String) -> String = (x) => name + \", \" + x\n\
        fn adder(n: Int) -> (Int) -> Int = (x) => x + n\n\
        fn compose(f: (Int) -> Int, g: (Int) -> Int) -> (Int) -> Int = (x) => g(f(x))\n\
        effect fn main() -> Unit = {\n\
        let hi = greeter(\"Hello\")\n\
        println(hi(\"world\"))\n\
        let ns = [10, 20, 30]\n\
        let picker = (i: Int) => list.get(ns, i) ?? 0\n\
        println(int.to_string(picker(1)))\n\
        let h = compose(adder(3), adder(100))\n\
        println(int.to_string(h(5))) }\n";
    let prog = lower_source(src);
    let wat = render_wasm_program(&prog);
    assert!(
        wat.contains("$__drop_closure"),
        "closure drops must route through the uniform recursive $__drop_closure"
    );
    if let Some(out) = build_and_run("heap_fn_captures", &wat) {
        assert_eq!(out, "Hello, world\n20\n108");
    }
}

#[test]
fn while_continue_break_executes_on_wasmtime() {
    // #1277: `continue` in a scalar while body lowers as the GUARDED REST
    // (`if (1-c) then { rest }` — no branch op), and the conditional `break`
    // inside that rest emits its `LoopBreakUnless` INSIDE the `IfThen` region
    // (named `$brk` labels branch out of a nested wasm `if`). Byte-matches v0:
    // s = 1+2+4+5+6 = 18 (i=3 skipped by continue), i = 7 (break before s += 7).
    let src = "effect fn main() -> Unit = {\n\
        var i = 0\n\
        var s = 0\n\
        while i < 10 {\n\
          i = i + 1\n\
          if i == 3 then continue\n\
          if i > 6 then break\n\
          s = s + i\n\
        }\n\
        println(int.to_string(s))\n\
        println(int.to_string(i)) }\n";
    let prog = lower_source(src);
    assert!(
        prog.functions.iter().any(|f| f.name == "main"),
        "the scalar-loop machinery must admit the #1277 repro (continue = guarded rest)"
    );
    if let Some(out) = build_and_run("while_continue_break", &render_wasm_program(&prog)) {
        assert_eq!(out, "18\n7");
    }
}

#[test]
fn for_range_continue_still_steps_on_wasmtime() {
    // #1277: `continue` in a for-range body must still run the implicit index
    // STEP (the step is emitted after the body; the guarded-rest form falls
    // through to it — a br-to-head would skip it and loop forever).
    // acc = 0+1+3+4 = 8 (j=2 skipped).
    let src = "effect fn main() -> Unit = {\n\
        var acc = 0\n\
        for j in 0..<5 {\n\
          if j == 2 then continue\n\
          acc = acc + j\n\
        }\n\
        println(int.to_string(acc)) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "main"));
    if let Some(out) = build_and_run("for_range_continue", &render_wasm_program(&prog)) {
        assert_eq!(out, "8");
    }
}

#[test]
fn mid_body_break_is_immediate_on_wasmtime() {
    // Pins the `desugar_loop_break` positional tightening (#1277 measurement):
    // a NON-trailing `if c then break` must break IMMEDIATELY (v0/interp
    // semantics — `last` stays 3), not defer past the rest of the iteration
    // (the old flag rewrite printed 4). The shape now routes to the scalar
    // machinery's `LoopBreakUnless` at the statement position.
    let src = "effect fn main() -> Unit = {\n\
        var last = 0\n\
        for k in 0..<10 {\n\
          if k > 3 then break\n\
          last = k\n\
        }\n\
        println(int.to_string(last)) }\n";
    let prog = lower_source(src);
    assert!(prog.functions.iter().any(|f| f.name == "main"));
    if let Some(out) = build_and_run("mid_body_break_immediate", &render_wasm_program(&prog)) {
        assert_eq!(out, "3");
    }
}

#[test]
fn continue_outside_recognized_positions_still_walls() {
    // #1277 scope pin: a `continue` with statements BEFORE it inside the arm is
    // NOT in the guarded-rest subset — the scalar attempt declines and the
    // model-one-iteration fallback keeps the LOUD wall (no wrong value). The
    // walled `main` is absent from the lowered program.
    let src = "effect fn main() -> Unit = {\n\
        var i = 0\n\
        var s = 0\n\
        while i < 5 {\n\
          i = i + 1\n\
          if i == 2 then {\n\
            s = s + 100\n\
            continue\n\
          }\n\
          s = s + i\n\
        }\n\
        println(int.to_string(s)) }\n";
    let prog = lower_source(src);
    assert!(
        !prog.functions.iter().any(|f| f.name == "main"),
        "a continue outside the recognized statement positions must keep the honest wall"
    );
}

#[test]
fn heap_option_none_reserves_the_payload_slot() {
    // #1526 (atzurody): the variant-match/`??` machinery pre-binds the @12
    // payload slot BEFORE the tag test, so a header-only 12-byte `none` block
    // read 4 bytes past itself — an OOB trap whenever that block sat at the
    // linear-memory frontier (trap decided by argv[0] length in the teastia
    // repro). Every heap-Option `none` producer must reserve the payload slot:
    // no rendered module may allocate a cap-0 list block for an Option, and
    // the self-host `list.get_str` none arms must carry a nonzero cap.
    let src = "fn main() -> Unit = {\n  \
        let cs = string.chars(\"ab\")\n  \
        let c = list.get(cs, 5) ?? \"\"\n  \
        println(\"<\" + c + \">\")\n}\n";
    let prog = lower_source(src);
    let wat = render_wasm_program(&prog);
    assert!(
        !wat.contains("(call $list_new (i32.const 0) (i32.const 0))"),
        "a cap-0 Option block reappeared — the @12 pre-read goes OOB at the frontier"
    );
    // The self-host list.get_str's two none arms: len 0 with cap RESERVED
    // (Init::OptNone renders $list_new(0, 1 + PUSH_HEADROOM)).
    let get_str = wat
        .split("(func $list.get_str ")
        .nth(1)
        .map(|s| &s[..s.find("\n  (func ").unwrap_or(s.len())])
        .expect("list.get_str rendered");
    assert!(
        !get_str.contains("(call $alloc (i32.add (i32.const 12) (i32.mul"),
        "list.get_str's none went back to the header-only DynListStr alloc"
    );
    assert!(get_str.contains("(call $list_new (i32.const 0) (i32.const 9))"));
    if let Some(out) = build_and_run("heap_option_none_reserves_the_payload_slot", &render_wasm_program(&prog)) {
        assert_eq!(out, "<>");
    }
}
