    /// CONTROL-FLOW EXECUTION + print_int: a recursive itoa (`put_int`) over a SCALAR
    /// `if` — lowered to IfThen/Else/EndIf so ONLY THE TAKEN ARM runs (Div/Mod +
    /// recursion + prim) — prints an integer's decimal digits, byte-matching v0's
    /// `println(int.to_string(12345))`. Proves the `if` EXECUTES (not the old
    /// linearize-both-arms-and-defer), the keystone for control flow + numbers.
    #[test]
    fn scalar_if_executes_print_int() {
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    \
            pos + 1 } else { let p = put_int(n / 10, pos)\n    \
            prim.store8(p, 48 + (n % 10))\n    \
            p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  \
            prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  \
            let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn main() -> Unit = write_int(12345)\n";
        let prog = lower_source(src);
        let put = prog.functions.iter().find(|f| f.name == "put_int").expect("put_int lowered");
        // The `if` is EXECUTABLE control flow (IfThen marker), not the deferred Const.
        assert!(
            put.ops.iter().any(|op| matches!(op, Op::IfThen { .. })),
            "put_int's if must lower to IfThen (executable), got {:?}",
            put.ops
        );
        if let Some(out) = build_and_run("print_int", &render_wasm_program(&prog)) {
            assert_eq!(out, "12345");
        }
    }

    /// FIZZBUZZ — the canonical real program — runs through v1 and byte-matches v0.
    /// Exercises EVERYTHING composed: a chained Unit `if … else if … else …` that
    /// executes ONLY THE TAKEN branch (not all arms), `%`, `==`, comparison, recursion
    /// (write_int → put_int), Div/Mod, the prim floor, println, and self-hosted
    /// print_int. `fizzbuzz(6)` is "Fizz" — proving the nested else-if executes (the
    /// old linearization would print "Fizz\nBuzz\n6").
    #[test]
    fn fizzbuzz_matches_v0() {
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn fizzbuzz(n: Int) -> Unit =\n  \
            if n % 15 == 0 then println(\"FizzBuzz\")\n  \
            else if n % 3 == 0 then println(\"Fizz\")\n  \
            else if n % 5 == 0 then println(\"Buzz\")\n  \
            else write_int(n)\n\
            fn main() -> Unit = fizzbuzz(6)\n";
        let prog = lower_source(src);
        // Only the taken branch runs: fizzbuzz(6) prints exactly "Fizz" (6 % 3 == 0).
        if let Some(out) = build_and_run("fizzbuzz", &render_wasm_program(&prog)) {
            assert_eq!(out, "Fizz");
        }
    }

    /// `b == false` / `b1 != b2` EXECUTE — a `Bool` is an i64 0/1, so the SAME
    /// `IntOp::Eq`/`Ne` render the Int comparison uses is bit-exact. Before the gate
    /// admitted `Ty::Bool` operands, the condition deferred to a `Const 0` and BOTH
    /// arms ran (v0 mismatch). Here `cmp(false)` prints exactly "is-false" and
    /// `differ(true,false)` prints "differ" — proving only the taken arm runs.
    #[test]
    fn bool_equality_executes_the_taken_arm() {
        let src = "fn cmp(b: Bool) -> Unit =\n  \
            if b == false then println(\"is-false\") else println(\"is-true\")\n\
            fn differ(x: Bool, y: Bool) -> Unit =\n  \
            if x != y then println(\"differ\") else println(\"same\")\n\
            fn main() -> Unit = {\n  \
            cmp(false)\n  cmp(true)\n  differ(true, false)\n  differ(true, true) }\n";
        let prog = lower_source(src);
        // The condition must lower to a real `IntBinOp{Eq/Ne}`, not a deferred Const.
        let cmp = prog.functions.iter().find(|f| f.name == "cmp").expect("lowered fn \"cmp\" not found");
        assert!(
            cmp.ops.iter().any(|op| matches!(op, Op::IntBinOp { op: IntOp::Eq, .. })),
            "Bool `==` must lower to IntBinOp Eq, got {:?}",
            cmp.ops
        );
        let differ = prog.functions.iter().find(|f| f.name == "differ").expect("lowered fn \"differ\" not found");
        assert!(
            differ.ops.iter().any(|op| matches!(op, Op::IntBinOp { op: IntOp::Ne, .. })),
            "Bool `!=` must lower to IntBinOp Ne, got {:?}",
            differ.ops
        );
        if let Some(out) = build_and_run("bool_eq", &render_wasm_program(&prog)) {
            assert_eq!(out, "is-false\nis-true\ndiffer\nsame");
        }
    }

    /// VALUE-POSITION UnOp + logical And/Or EXECUTE (audit bucket 1). Before this fix
    /// `lower_scalar_value_inner` had NO `UnOp` arm and no `And`/`Or` case, so `-a` /
    /// `not x` / `a and b` / `a or b` in a value position fell through to the caller's
    /// `Const 0` materialization (read 0), and as an `if` CONDITION the un-lowered
    /// predicate made `try_lower_scalar_if`/`try_lower_unit_if` run BOTH arms. The
    /// operands here are FUNCTION PARAMETERS (not literals) so the frontend's constant
    /// `fold.rs` cannot evaporate the `UnOp`/`And`/`Or` before lowering sees it — the
    /// new arms are genuinely exercised. `classify(5, 0)`:
    ///   - `neg(n)` = `0 - n` over a param → int negation
    ///   - `not (n < 0)` → boolean not in a cond (only the taken arm runs)
    ///   - `n > 0 and m > 0` (m=0) → false → else arm
    ///   - `n > 0 or m > 0` → true → then arm
    /// Each result feeds `int.to_string` (auto-linked, non-negative), printed. Output
    /// byte-matches v0 (`almide run`).
    #[test]
    fn value_position_unop_and_logical_and_or_execute() {
        let src = "fn ineg(n: Int) -> Int = 0 - n\n\
            fn bnot(n: Int) -> Int = if not (n < 0) then 1 else 0\n\
            fn band(n: Int, m: Int) -> Int = if n > 0 and m > 0 then 1 else 0\n\
            fn bor(n: Int, m: Int) -> Int = if n > 0 or m > 0 then 7 else 9\n\
            fn main() -> Unit = {\n  \
            println(int.to_string(ineg(0 - 5)))\n  \
            println(int.to_string(bnot(5)))\n  \
            println(int.to_string(band(5, 0)))\n  \
            println(int.to_string(bor(5, 0))) }\n";
        let prog = lower_source(src);
        // `not (n < 0)` (a param-driven Not) must lower to a real `IntBinOp{Sub}` (1 - b),
        // not a deferred Const — proving the new UnOp arm is wired and reached.
        let bnot = prog.functions.iter().find(|f| f.name == "bnot").expect("lowered fn \"bnot\" not found");
        assert!(
            bnot.ops.iter().any(|op| matches!(op, Op::IntBinOp { op: IntOp::Sub, .. })),
            "`not` must lower to IntBinOp Sub (1 - b), got {:?}",
            bnot.ops
        );
        // `and` / `or` over params in a cond now SHORT-CIRCUIT (v1-spine hole E2): native + interp
        // evaluate the RHS lazily, so `a and b` → `if a then b else false` and `a or b` → `if a then
        // true else b` — lowered to IfThen/Else/EndIf control flow (the RHS ops emitted INSIDE the
        // taken branch), NOT the prior EAGER `IntOp::And`/`Or` over both operands (which traps on a
        // side-effecting/trapping RHS). Assert the short-circuit markers are present and NO eager
        // And/Or IntBinOp remains.
        let band = prog.functions.iter().find(|f| f.name == "band").expect("lowered fn \"band\" not found");
        let bor = prog.functions.iter().find(|f| f.name == "bor").expect("lowered fn \"bor\" not found");
        assert!(
            band.ops.iter().any(|op| matches!(op, Op::IfThen { .. }))
                && !band.ops.iter().any(|op| matches!(op, Op::IntBinOp { op: IntOp::And, .. })),
            "`and` must SHORT-CIRCUIT to IfThen control flow (no eager IntOp And), got {:?}",
            band.ops
        );
        assert!(
            bor.ops.iter().any(|op| matches!(op, Op::IfThen { .. }))
                && !bor.ops.iter().any(|op| matches!(op, Op::IntBinOp { op: IntOp::Or, .. })),
            "`or` must SHORT-CIRCUIT to IfThen control flow (no eager IntOp Or), got {:?}",
            bor.ops
        );
        if let Some(out) = build_and_run("value_unop_and_or", &render_wasm_program(&prog)) {
            // 5 (ineg(-5) = 5), 1 (not(5<0)=not false=true→then), 0 (5>0 and 0>0=false→else),
            // 7 (5>0 or 0>0=true→then).
            assert_eq!(out, "5\n1\n0\n7");
        }
    }

    /// Float negation in a value position EXECUTES via the `f64.neg` prim (the
    /// i64-uniform value holds the f64 bits; the prim reinterprets around the negate).
    /// The operand is a PARAM (`fneg(x) = -x`) so the constant fold cannot collapse it
    /// before lowering — the new `UnOp::NegFloat` arm is exercised. `fneg(2.5)` = -2.5,
    /// byte-matching v0's `float.to_string(-x)`.
    #[test]
    fn value_position_float_neg_executes() {
        let src = "fn fneg(x: Float) -> Float = -x\n\
            fn main() -> Unit = println(float.to_string(fneg(2.5)))\n";
        let prog = lower_source(src);
        // `-x` over a param must reach the f64.neg prim (FloatUn Neg), not a deferred Const.
        let fneg = prog.functions.iter().find(|f| f.name == "fneg").expect("lowered fn \"fneg\" not found");
        assert!(
            fneg.ops.iter().any(
                |op| matches!(op, Op::Prim { kind: PrimKind::FloatUn(FUnOp::Neg), .. })
            ),
            "float `-x` must lower to the FloatUn Neg prim, got {:?}",
            fneg.ops
        );
        if let Some(out) = build_and_run("value_float_neg", &render_wasm_program(&prog)) {
            assert_eq!(out, "-2.5");
        }
    }

    #[test]
    fn heap_result_if_returns_the_taken_arm_string() {
        // `if c then "yes" else "no"` RETURNS a String — only the taken arm allocates,
        // returned rc=1 to the caller (per-arm Alloc+Consume balance). label(true)="yes",
        // label(false)="no", byte-matching v0.
        let src = "fn label(c: Bool) -> String = if c then \"yes\" else \"no\"\n\
            fn main() -> Unit = {\n  \
            println(label(true))\n  println(label(false)) }\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "label").expect("lowered fn \"label\" not found");
        // It must EXECUTE (IfThen marker), not defer to a single Opaque Alloc.
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::IfThen { .. })),
            "heap-result if must lower to IfThen (executable), got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("heap_result_if", &render_wasm_program(&prog)) {
            assert_eq!(out, "yes\nno");
        }
    }

    #[test]
    fn string_allocating_loop_reuses_freed_blocks() {
        // A loop that allocates a String literal every iteration must run in BOUNDED
        // memory — each iteration's string is freed (rc_dec) and the free-list REUSES it.
        // 5000 iterations × a 20-byte block would overrun the single 64 KiB page (~2900
        // allocs) if freed String blocks were not reclaimed; before the list-compatible
        // String sizing fix that OOM-trapped. Completing all 5000 lines proves reuse.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 5000 {\n    println(\"x\")\n    i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("str_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 5000, "every iteration must print (no OOM)");
            assert!(out.lines().all(|l| l == "x"));
        }
    }

    #[test]
    fn heap_result_match_returns_the_matched_arm_string() {
        // A String-returning `match` over Int literals desugars to a NESTED heap-result
        // `if` and RUNS only the matched arm (each arm Alloc+Consume = "im"). name(0)=zero,
        // name(1)=one, name(7)=other, byte-matching v0.
        let src = "fn name(n: Int) -> String = match n {\n  \
            0 => \"zero\",\n  1 => \"one\",\n  _ => \"other\",\n  }\n\
            fn main() -> Unit = {\n  \
            println(name(0))\n  println(name(1))\n  println(name(7)) }\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "name").expect("lowered fn \"name\" not found");
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::IfThen { .. })),
            "heap-result match must lower to nested IfThen (executable), got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("heap_result_match", &render_wasm_program(&prog)) {
            assert_eq!(out, "zero\none\nother");
        }
    }

    #[test]
    fn heap_result_if_with_a_call_arm_executes() {
        // `if c then shout() else "no"` — a CALL arm returns a fresh owned String
        // (CallFn-with-heap-result = cert i), moved out by the arm's Consume (cert m), the
        // same "im" balance as a literal arm. pick(true)=shout()="HEY", pick(false)="no".
        let src = "fn shout() -> String = \"HEY\"\n\
            fn pick(c: Bool) -> String = if c then shout() else \"no\"\n\
            fn main() -> Unit = {\n  \
            println(pick(true))\n  println(pick(false)) }\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "pick").expect("lowered fn \"pick\" not found");
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::IfThen { .. }))
                && f.ops.iter().any(|op| matches!(op, Op::CallFn { .. })),
            "call-arm heap-result if must lower to IfThen + CallFn, got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("heap_if_call_arm", &render_wasm_program(&prog)) {
            assert_eq!(out, "HEY\nno");
        }
    }

    #[test]
    fn heap_result_if_with_var_arms_executes() {
        // `if c then a else b` over borrowed String PARAMS (TAIL position) — each Var arm
        // ACQUIRES a fresh owned reference (`Op::Dup` = the per-arm "am" balance) and moves
        // it out as the function result; the caller's params are untouched (no double-
        // free). pick(true,…)="yes". The cert is `am·am` (Dup acquire + Consume move-out
        // per arm) — verified ACCEPT by the proven ownership checker.
        let src = "fn pick(c: Bool, a: String, b: String) -> String = if c then a else b\n\
            fn main() -> Unit = {\n  \
            println(pick(true, \"yes\", \"no\"))\n  println(pick(false, \"yes\", \"no\")) }\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "pick").expect("lowered fn \"pick\" not found");
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::IfThen { .. }))
                && f.ops.iter().any(|op| matches!(op, Op::Dup { .. })),
            "var-arm heap-result if must lower to IfThen + Dup, got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("heap_if_var_arm", &render_wasm_program(&prog)) {
            assert_eq!(out, "yes\nno");
        }
    }

    #[test]
    fn heap_result_if_with_bool_call_cond_executes() {
        // `if string.contains(s, "x") then "y" else "n"` (TAIL position) — a Bool-
        // returning PURE call WITH HEAP ARGS as the condition. It is materialized to a
        // scalar BEFORE the IfThen, its transient heap arg temps freed within the cond
        // frame (cert `id` for the call's owned arg temps, balanced). tag("box")="y".
        let src = "fn tag(s: String) -> String = if string.contains(s, \"x\") then \"y\" else \"n\"\n\
            fn main() -> Unit = { println(tag(\"box\")) }\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "tag").expect("lowered fn \"tag\" not found");
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::IfThen { .. })),
            "bool-call-cond heap-result if must lower to IfThen, got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("heap_if_bool_call_cond", &render_wasm_program(&prog)) {
            assert_eq!(out, "y");
        }
    }

    #[test]
    fn let_bound_heap_result_if_executes_via_tail_duplication() {
        // A let-bound (NON-TAIL) heap-result `if` is RESTRUCTURED by the tail-duplication
        // desugar: `let label = if x > 20 then "big" else "small"; println(label)` becomes
        // `if x > 20 then { let label = "big"; println(label) } else { let label = "small";
        // println(label) }`, so each arm independently allocates `label`, uses it, and drops
        // it at the arm's frame end (the per-arm `i…d` balance the proven checker already
        // accepts for a Unit `if` — NO certificate change, no merged-dst). Only one arm runs,
        // so duplicating `println(label)` is semantically identical to v0. `main` now LOWERS
        // (no longer walled) and EXECUTES — x=15, 15 > 20 is false, so it prints "small",
        // byte-matching v0. This is the root-fix that recovers the walled corpus functions.
        let src = "fn main() -> Unit = {\n  \
            let x = 15\n  \
            let label = if x > 20 then \"big\" else \"small\"\n  \
            println(label) }\n";
        let prog = lower_source(src);
        let main = prog
            .functions
            .iter()
            .find(|f| f.name == "main")
            .expect("a let-bound heap-result if now LOWERS via tail-duplication (not walled)");
        assert!(
            main.ops.iter().any(|op| matches!(op, Op::IfThen { .. })),
            "the desugared let-bound if executes via an IfThen marker, got {:?}",
            main.ops
        );
        if let Some(out) = build_and_run("let_bound_heap_if", &render_wasm_program(&prog)) {
            assert_eq!(out, "small", "the taken (else) arm prints its bound String");
        }
    }

    #[test]
    fn variant_match_over_a_materialized_option_executes() {
        // `match opt { Some(x) => …, None => … }` over a LOCALLY-bound, MATERIALIZED
        // Option RUNS only the matched arm — Option is the 0-or-1-element-list layout
        // (`Some(x)` = len=1, `data[0]`=x; `None` = len=0), so the match reads `len` as
        // the tag and extracts `data[0]` as the payload. show(Some(42))="42",
        // show(None)="none", byte-matching v0's native Option match. The subject is a
        // TRACKED materialized Option (the `materialized_options` gate); a non-tracked
        // Option would keep the sound linearized both-arms fallback.
        let src = "fn main() -> Unit = {\n  \
            let a = Some(42)\n  \
            match a {\n    \
            Some(x) => println(int.to_string(x)),\n    \
            None => println(\"none\"),\n  }\n  \
            let b: Option[Int] = None\n  \
            match b {\n    \
            Some(x) => println(int.to_string(x)),\n    \
            None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        let main = prog.functions.iter().find(|f| f.name == "main").expect("lowered fn \"main\" not found");
        // The match EXECUTES (IfThen marker over the tag read), not linearized-both-arms.
        assert!(
            main.ops.iter().any(|op| matches!(op, Op::IfThen { .. })),
            "variant match must lower to IfThen (executable), got {:?}",
            main.ops
        );
        // `Some(42)` materializes a payload-carrying Alloc (the 1-element list).
        assert!(
            main.ops.iter().any(|op| matches!(op, Op::Alloc { init: Init::OptSome { .. }, .. })),
            "Some(42) must materialize Init::OptSome, got {:?}",
            main.ops
        );
        if let Some(out) = build_and_run("variant_match", &render_wasm_program(&prog)) {
            assert_eq!(out, "42\nnone");
        }
    }

    #[test]
    fn option_allocating_loop_matches_bounded() {
        // ADVERSARIAL: a loop that MATERIALIZES a `Some(i)` Option block every iteration
        // and matches it must run in BOUNDED memory — each iteration's Option is freed
        // (rc_dec) at the iteration frame's end and the free-list REUSES it. 3000
        // iterations × an Option block would overrun the single 64 KiB page (~2900 allocs)
        // if the materialized Option leaked. Completing all 3000 lines proves the per-arm/
        // per-iteration ownership balance holds for the new `Init::OptSome` Alloc.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 3000 {\n    \
            let o = Some(i)\n    \
            match o {\n      \
            Some(x) => println(int.to_string(x)),\n      \
            None => println(\"none\"),\n    }\n    \
            i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("option_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 3000, "every iteration must print (no OOM/leak)");
            assert_eq!(out.lines().next(), Some("0"));
            assert_eq!(out.lines().last(), Some("2999"));
        }
    }

    #[test]
    fn self_hosted_list_get_returns_some_or_none() {
        // `list.get(xs, i)` SELF-HOSTED over the prim floor returns `Some(element)` in
        // bounds / `None` out of bounds, matched via the materialized-Option variant match
        // (list.get's result is tracked, so the `match` executes only the taken arm).
        // get(1)=Some(20)→"20", get(5)=None→"none", byte-matching v0's list.get + match.
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30]\n  \
            match list.get(xs, 1) {\n    \
            Some(x) => println(int.to_string(x)),\n    \
            None => println(\"none\"),\n  }\n  \
            match list.get(xs, 5) {\n    \
            Some(x) => println(int.to_string(x)),\n    \
            None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        // list.get is self-hosted (auto-linked), not an external runtime call.
        assert!(
            prog.functions.iter().any(|f| f.name == "list.get"),
            "list.get must be auto-linked as a self-host fn"
        );
        if let Some(out) = build_and_run("list_get", &render_wasm_program(&prog)) {
            assert_eq!(out, "20\nnone");
        }
    }

    #[test]
    fn self_hosted_list_get_via_bound_var_executes() {
        // The BOUND-VAR form `let o = list.get(xs, i); match o { … }` also executes — the
        // self-host Option result bound to `o` is tracked at the bind. get(0)=Some(10)→"10".
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30]\n  \
            let o = list.get(xs, 0)\n  \
            match o {\n    \
            Some(x) => println(int.to_string(x)),\n    \
            None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("list_get_var", &render_wasm_program(&prog)) {
            assert_eq!(out, "10");
        }
    }

    #[test]
    fn self_hosted_list_get_loop_is_bounded() {
        // ADVERSARIAL: calling list.get + matching its Option every iteration must run in
        // BOUNDED memory — each call's fresh Option (returned through the heap-result-if
        // move-out, materialized into the match subject's owned temp) is freed at the
        // iteration's end and reused. 2000 iterations would overrun the 64 KiB page if the
        // per-call Option leaked. Completing all 2000 proves the call + match ownership
        // balance (no leak/double-free through the self-host call path).
        let src = "fn main() -> Unit = {\n  \
            let xs = [7, 8, 9]\n  \
            var i = 0\n  \
            while i < 2000 {\n    \
            match list.get(xs, 1) {\n      \
            Some(x) => println(int.to_string(x)),\n      \
            None => println(\"none\"),\n    }\n    \
            i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("list_get_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 2000, "every iteration prints (no OOM/leak)");
            assert!(out.lines().all(|l| l == "8"), "list.get(xs,1) is always Some(8)");
        }
    }

    #[test]
    fn self_hosted_list_first_and_last_return_some_or_none() {
        // list.first / list.last self-hosted over the SAME Option machinery (reusing the
        // 0-or-1-element-list helpers): Some(end element) for a non-empty list, None for
        // empty. first([10,20,30])=Some(10), last=Some(30), first([])=None — byte-matching
        // v0. Proves the materialized-Option layout generalizes beyond list.get.
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30]\n  \
            match list.first(xs) {\n    \
            Some(x) => println(int.to_string(x)),\n    None => println(\"none\"),\n  }\n  \
            match list.last(xs) {\n    \
            Some(x) => println(int.to_string(x)),\n    None => println(\"none\"),\n  }\n  \
            let ys: List[Int] = []\n  \
            match list.first(ys) {\n    \
            Some(x) => println(int.to_string(x)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.first"), "list.first linked");
        assert!(prog.functions.iter().any(|f| f.name == "list.last"), "list.last linked");
        if let Some(out) = build_and_run("list_first_last", &render_wasm_program(&prog)) {
            assert_eq!(out, "10\n30\nnone");
        }
    }

    #[test]
    fn self_hosted_list_contains_and_index_of() {
        // list.contains / list.index_of self-hosted (linear element search by == over the
        // i64 slots): contains([10,20,30],20)=true / 99=false; index_of([10,20,30],30)=
        // Some(2) / 99=None (a materialized Option, the match executes). byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [10, 20, 30]\n  \
            let a = list.contains(xs, 20)\n  \
            let b = list.contains(xs, 99)\n  \
            if a then println(\"T\") else println(\"F\")\n  \
            if b then println(\"T\") else println(\"F\")\n  \
            match list.index_of(xs, 30) {\n    \
            Some(i) => println(int.to_string(i)),\n    None => println(\"none\"),\n  }\n  \
            match list.index_of(xs, 99) {\n    \
            Some(i) => println(int.to_string(i)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.contains"), "linked");
        assert!(prog.functions.iter().any(|f| f.name == "list.index_of"), "linked");
        if let Some(out) = build_and_run("list_search", &render_wasm_program(&prog)) {
            assert_eq!(out, "T\nF\n2\nnone");
        }
    }

    #[test]
    fn self_hosted_list_product_max_min() {
        // list.product / list.max / list.min self-hosted (i64 left folds): product([2,3,4])
        // =24, product([])=1; max([3,1,4,1,5])=Some(5), min=Some(1), max([])=None (the empty
        // list yields None via the materialized Option). byte-matching v0.
        let src = "fn main() -> Unit = {\n  \
            let xs = [3, 1, 4, 1, 5]\n  \
            let zs = [2, 3, 4]\n  \
            let ys: List[Int] = []\n  \
            let p = list.product(zs)\n  \
            let q = list.product(ys)\n  \
            println(int.to_string(p))\n  \
            println(int.to_string(q))\n  \
            match list.max(xs) {\n    \
            Some(m) => println(int.to_string(m)),\n    None => println(\"none\"),\n  }\n  \
            match list.min(xs) {\n    \
            Some(m) => println(int.to_string(m)),\n    None => println(\"none\"),\n  }\n  \
            match list.max(ys) {\n    \
            Some(m) => println(int.to_string(m)),\n    None => println(\"none\"),\n  } }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "list.product"), "linked");
        assert!(prog.functions.iter().any(|f| f.name == "list.max"), "linked");
        if let Some(out) = build_and_run("list_fold", &render_wasm_program(&prog)) {
            assert_eq!(out, "24\n1\n5\n1\nnone");
        }
    }

    #[test]
    fn self_hosted_int_scalar_ops() {
        // int.abs/min/max/clamp self-hosted (scalar i64 arithmetic): abs(-7)=7, min(3,8)=3,
        // max(3,8)=8, clamp(5,0,10)=5, clamp(-2,0,10)=0, clamp(99,0,10)=10. byte-match v0.
        let src = "fn main() -> Unit = {\n  \
            let a = int.abs(0 - 7)\n  \
            let b = int.min(3, 8)\n  \
            let c = int.max(3, 8)\n  \
            let d = int.clamp(5, 0, 10)\n  \
            let e = int.clamp(0 - 2, 0, 10)\n  \
            let f = int.clamp(99, 0, 10)\n  \
            println(int.to_string(a))\n  \
            println(int.to_string(b))\n  \
            println(int.to_string(c))\n  \
            println(int.to_string(d))\n  \
            println(int.to_string(e))\n  \
            println(int.to_string(f)) }\n";
        let prog = lower_source(src);
        assert!(prog.functions.iter().any(|f| f.name == "int.clamp"), "linked");
        if let Some(out) = build_and_run("int_scalar", &render_wasm_program(&prog)) {
            assert_eq!(out, "7\n3\n8\n5\n0\n10");
        }
    }

