
    #[test]
    fn string_concat_tail_and_loop() {
        // tail-position concat (fn greet = "Hi, " + n) + bounded loop (per-iter fresh String).
        let src = "fn greet(n: String) -> String = \"Hi, \" + n\n\
            fn main() -> Unit = {\n  println(greet(\"Al\"))\n\
              var i = 0\n  while i < 3000 { println(\"x\" + int.to_string(i))\n    i = i + 1 } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_concat_tail_and_loop", &render_wasm_program(&prog)) {
            assert!(out.starts_with("Hi, Al\nx0\nx1\n"));
            assert!(out.ends_with("x2999"));
            assert_eq!(out.lines().count(), 3001);
        }
    }


    #[test]
    fn str_list_literal_with_concat() {
        let src = "fn main() -> Unit = {\n  let xs = [\"a\" + \"b\", \"c\", \"d\" + \"e\"]\n  println(list.join(xs, \",\")) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("str_list_literal_with_concat", &render_wasm_program(&prog)) {
            assert_eq!(out, "ab,c,de");
        }
    }

    // ── String interpolation `"…${e}…"` — the executable subset (fix-0276) ──────────
    //
    // A StringInterp whose parts are all Lit / String Var-or-LitStr / Int Var-or-LitInt
    // lowers to a fresh owned String via the `__str_concat` chain (seeded with an empty
    // "" leaf), byte-matching v0's `emit_string_interp`. These detectors pin the four
    // value positions (call-arg / bind / tail / match-arm) + the structural guard that a
    // NON-subset interp (a `${list.len(x)}` call operand) stays the sound Opaque fallback.
    // (Goldens captured from `almide run` on the native v0 path.)

    #[test]
    fn string_interp_call_arg_executes() {
        // The HIGHEST-traffic position: `println("…${x}…")`. Mixed String + Int operands.
        // v0: "x=42 y=world", "count:42", "world" (single-part passthrough).
        let src = "fn main() -> Unit = {\n  \
            let n = 42\n  let s = \"world\"\n  \
            println(\"x=${n} y=${s}\")\n  \
            println(\"count:${n}\")\n  \
            println(\"${s}\") }\n";
        let prog = lower_source(src);
        // STRUCTURAL GUARD: a lowerable interp routes through __str_concat, never an empty
        // Opaque — auto-linking the self-host concat runtime is the observable signature.
        assert!(
            prog.functions.iter().any(|f| f.name == "__str_concat"),
            "a lowerable interp must auto-link __str_concat (not defer to empty Opaque)"
        );
        if let Some(out) = build_and_run("string_interp_call_arg", &render_wasm_program(&prog)) {
            assert_eq!(out, "x=42 y=world\ncount:42\nworld");
        }
    }

    #[test]
    fn string_interp_bind_position_executes() {
        // A `let lbl = "[${s}]"` BIND — the interp result is owned by the binding and
        // dropped at scope end. v0: "[world]".
        let src = "fn main() -> Unit = {\n  \
            let s = \"world\"\n  let lbl = \"[${s}]\"\n  println(lbl) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_bind", &render_wasm_program(&prog)) {
            assert_eq!(out, "[world]");
        }
    }

    #[test]
    fn string_interp_tail_position_executes() {
        // A RETURN/tail-position interp (`fn greet(name) = "Hi, ${name}!"`) — moved out as
        // the result. v0: "Hi, Ada!".
        let src = "fn greet(name: String) -> String = \"Hi, ${name}!\"\n\
            fn main() -> Unit = {\n  println(greet(\"Ada\")) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_tail", &render_wasm_program(&prog)) {
            assert_eq!(out, "Hi, Ada!");
        }
    }

    #[test]
    fn string_interp_match_arm_executes() {
        // A heap-result MATCH-arm interp (`match k { _ => "v=${n}" }`). The arm folds the
        // interp per-arm (cert `im`), only the taken arm runs. v0: "v=7" / "other".
        let src = "fn label(k: Int, n: Int) -> String = match k {\n  \
            0 => \"other\",\n  _ => \"v=${n}\",\n}\n\
            fn main() -> Unit = {\n  \
            println(label(1, 7))\n  println(label(0, 9)) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_match_arm", &render_wasm_program(&prog)) {
            assert_eq!(out, "v=7\nother");
        }
    }

    #[test]
    fn string_interp_multipart_int_and_string() {
        // A 4-part interp mixing two Int Vars and a String Var with literals — exercises the
        // K-concat + I-int.to_string glue count exactly. v0: "p(3,4)=ok".
        let src = "fn main() -> Unit = {\n  \
            let a = 3\n  let b = 4\n  let r = \"ok\"\n  \
            println(\"p(${a},${b})=${r}\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_multipart", &render_wasm_program(&prog)) {
            assert_eq!(out, "p(3,4)=ok");
        }
    }

    #[test]
    fn string_interp_loop_reclaims() {
        // SOUNDNESS: a bounded loop building a fresh interp String each iteration must
        // reclaim every round (no leak / no double-free → no OOM). The chain allocs K+1
        // intermediate Strings per round, all freed at the iteration frame's end.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  while i < 4000 { println(\"row ${i} done\")\n    i = i + 1 } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_loop", &render_wasm_program(&prog)) {
            assert!(out.starts_with("row 0 done\nrow 1 done\n"));
            assert!(out.ends_with("row 3999 done"));
            assert_eq!(out.lines().count(), 4000);
        }
    }

    #[test]
    fn string_interp_single_part_var_is_owned_copy() {
        // OWNERSHIP soundness for the single-part `"${s}"` passthrough: the interp builds a
        // FRESH owned String (`"" ++ s`), so the original `s` stays live and independently
        // owned — using BOTH afterward (and concatenating them) must not double-free. v0:
        // "hello\nhello\nhellohello".
        let src = "fn main() -> Unit = {\n  \
            let s = \"hello\"\n  let t = \"${s}\"\n  \
            println(t)\n  println(s)\n  println(s + t) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_single_part_alias", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello\nhello\nhellohello");
        }
    }

    #[test]
    fn string_interp_int_call_operand_executes_and_byte_matches_v0() {
        // The uniform desugar wraps an Int part by TYPE, not by operand shape — so an Int
        // `${list.len(x)}` with a CALL operand now folds to `int.to_string(list.len(x))`.
        // Both `int.to_string` and `list.len` are self-hosted, so the function fully LINKS
        // and EXECUTES, byte-matching v0 ("len=3"). (Before the uniform desugar this stayed
        // a deferred Opaque — the per-operand predicate rejected a non-Var operand. This is
        // a coverage GAIN, not a regression: the call is MATERIALIZED, so caps stay honest.)
        let src = "fn main() -> Unit = {\n  \
            let xs = [1, 2, 3]\n  println(\"len=${list.len(xs)}\") }\n";
        let prog = lower_source(src);
        let main = prog.functions.iter().find(|f| f.name == "main").expect("main lowered");
        // The list.len call is MATERIALIZED as a real CallFn (its result feeds int.to_string),
        // not silently dropped — its capabilities are visible to the transitive fold.
        assert!(
            main.ops.iter().any(|op| matches!(op,
                Op::CallFn { name, .. } if name == "list.len")),
            "the Int part's call operand must be materialized as a real CallFn"
        );
        // Fully linkable (int.to_string + list.len both registered) → renders cleanly.
        assert!(
            crate::render_wasm::unlinked_call_names(&prog).is_empty(),
            "an Int-call-operand interp must be fully linkable, got {:?}",
            crate::render_wasm::unlinked_call_names(&prog)
        );
        if let Some(out) = build_and_run("string_interp_int_call_operand", &render_wasm_program(&prog)) {
            assert_eq!(out, "len=3"); // v0 golden
        }
    }

    #[test]
    fn scalar_call_in_arithmetic_operand_executes_and_byte_matches_v0() {
        // THE fix-0276 GAP: a scalar Int/Bool CALL (or if/match) used as a BinOp/comparison
        // OPERAND used to DEFER to `Const 0` — `5 + string.len(s)` silently computed `5 + 0`.
        // Now `lower_scalar_value` MATERIALIZES the operand call (a real CallFn over its
        // borrowed/materialized heap args, the self-rollback wrapper making it safe), so the
        // arithmetic is correct. The optimizer inlines `let s = "abc"`, so the call here is
        // `string.len("abc")` — a FRESH heap-LITERAL arg materialized + dropped at scope end.
        // v0 golden ("8") via `almide run`.
        let src = "fn main() -> Unit = {\n  \
            let s = \"abc\"\n  let n = 5 + string.len(s)\n  println(\"${n}\") }\n";
        let prog = lower_source(src);
        // The operand call is a REAL CallFn (its result feeds the IntBinOp), not dropped to 0.
        let any_fn = prog.functions.iter().flat_map(|f| f.ops.iter()).any(|op| matches!(op,
            Op::CallFn { name, .. } if name == "string.len"));
        assert!(any_fn, "the arithmetic operand's string.len call must be materialized as a real CallFn");
        if let Some(out) = build_and_run("scalar_call_operand_arith", &render_wasm_program(&prog)) {
            assert_eq!(out, "8"); // v0 golden: 5 + len("abc")=5+3
        }
    }

    #[test]
    fn scalar_if_in_arithmetic_operand_executes_and_byte_matches_v0() {
        // The `if`/`match` half of the fix-0276 gap: `a + (if c then 1 else 2)` used to defer
        // the parenthesized `if` operand to `Const 0`. Now it EXECUTES via `try_lower_scalar_if`
        // (only the taken arm runs), so the sum is right. v0 golden: "11 12".
        let src = "fn main() -> Unit = {\n  \
            let a = 10\n  \
            let r1 = a + (if true then 1 else 2)\n  \
            let r2 = a + (if false then 1 else 2)\n  \
            println(\"${r1} ${r2}\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("scalar_if_operand_arith", &render_wasm_program(&prog)) {
            assert_eq!(out, "11 12"); // v0 golden
        }
    }

    #[test]
    fn noisy_call_in_operand_keeps_caller_caps_tainted() {
        // FALSE-GREEN GUARD for fix-0276: materializing an operand call makes MORE calls real,
        // so the caps fold must still TAINT a caller whose operand call reaches Stdout. A
        // `noisy()` that `println`s, used as `5 + noisy(3)`, becomes a real CallFn edge — the
        // transitive cap fold (`reachable_caps`) must report Stdout reachable for `compute`, so
        // it can never be falsely caps-VERIFIED. (A PURE operand call like `string.len` stays
        // empty-reachable — the contrast that proves the taint is precise, not blanket.)
        use crate::certificate::reachable_caps;
        let noisy_src = "fn noisy(x: Int) -> Int = {\n  println(\"side\")\n  x + 1\n}\n\
            fn compute() -> Int = { 5 + noisy(3) }\n\
            fn main() -> Unit = { println(\"${compute()}\") }\n";
        let prog = lower_source(noisy_src);
        let program: std::collections::BTreeMap<String, crate::MirFunction> =
            prog.functions.iter().map(|f| (f.name.clone(), f.clone())).collect();
        let mut visited = std::collections::BTreeSet::new();
        let reach = reachable_caps("compute", &program, &mut visited);
        assert!(
            reach.contains(&crate::Capability::Stdout),
            "a printing operand call must keep the caller's transitive caps tainted (Stdout reachable), got {reach:?}"
        );
        // Contrast: a PURE operand call leaves the caller empty-reachable (no false taint).
        let pure_src = "fn compute() -> Int = { let s = \"abc\"\n  5 + string.len(s) }\n\
            fn main() -> Unit = { println(\"${compute()}\") }\n";
        let pure = lower_source(pure_src);
        let pure_program: std::collections::BTreeMap<String, crate::MirFunction> =
            pure.functions.iter().map(|f| (f.name.clone(), f.clone())).collect();
        let mut v2 = std::collections::BTreeSet::new();
        assert!(
            reachable_caps("compute", &pure_program, &mut v2).is_empty(),
            "a pure operand call must NOT taint the caller's transitive caps"
        );
    }

    #[test]
    fn c1_defunc_capturing_map_executes_and_is_pure() {
        // C1: a CAPTURING inline lambda in `list.map` is DEFUNCTIONALIZED inline — the
        // capture `k` resolves through the in-scope binding, NOT a closure env. It EXECUTES
        // (byte-matches v0 `[10, 20, 30]`) AND, with the result-producing work isolated in a
        // pure fn, the transitive cap fold reports EMPTY (the inlined `x * k` reaches no host
        // capability — no CallIndirect conservatism, no lifted-lambda Stdout). The inline path
        // is NOT a caps regression: a pure body stays pure.
        use crate::certificate::reachable_caps;
        let src = "fn build() -> List[Int] = {\n  let k = 10\n  \
            list.map([1, 2, 3], (x) => x * k) }\n\
            fn main() -> Unit = { println(\"${build()}\") }\n";
        let prog = lower_source(src);
        // No lifted lambda, no `list.map` combinator — the closure is defunctionalized away.
        assert!(
            !prog.functions.iter().any(|f| f.name.starts_with("__lambda_") || f.name == "list.map"),
            "the capturing map lambda is inlined (no __lambda_*, no list.map combinator)"
        );
        let program: std::collections::BTreeMap<String, crate::MirFunction> =
            prog.functions.iter().map(|f| (f.name.clone(), f.clone())).collect();
        let mut visited = std::collections::BTreeSet::new();
        assert!(
            reachable_caps("build", &program, &mut visited).is_empty(),
            "a pure inlined map body must NOT taint the producer's transitive caps"
        );
        if let Some(out) = build_and_run("c1_capturing_map", &render_wasm_program(&prog)) {
            assert_eq!(out, "[10, 20, 30]");
        }
    }

    #[test]
    fn c1_defunc_filter_and_fold_execute() {
        // C1: an inline `filter` predicate and an inline `fold` reducer are defunctionalized
        // as loops at the call site (over-allocate+pack+patch-len for filter, a stable
        // accumulator local for fold). Byte-matches v0: filter([1..4], x>2)=[3,4],
        // fold([1..4], 0, +)=10. No combinator CallFn, no closure.
        let src = "fn main() -> Unit = {\n  \
            let a = list.filter([1, 2, 3, 4], (x) => x > 2)\n  println(\"${a}\")\n  \
            let s = list.fold([1, 2, 3, 4], 0, (acc, x) => acc + x)\n  println(int.to_string(s)) }\n";
        let prog = lower_source(src);
        assert!(
            !prog.functions.iter().any(|f| f.name == "list.filter" || f.name == "list.fold"),
            "filter/fold inline lambdas are defunctionalized, not auto-linked"
        );
        if let Some(out) = build_and_run("c1_filter_fold", &render_wasm_program(&prog)) {
            assert_eq!(out, "[3, 4]\n10");
        }
    }

    #[test]
    fn c1_defunc_map_false_green_keeps_caller_caps_tainted() {
        // FALSE-GREEN GUARD for C1: a `list.map(xs, (x) => { println("hit"); x })` body has a
        // REAL Stdout edge. My defunctionalization declines a side-effecting body (it is not
        // scalar-pure-lowerable) → the lambda falls to the self-host path (LIFTED + CallIndirect).
        // The lifted lambda's Stdout MUST reach the caller's transitive witness via the FuncRef
        // edge — so a printing map can NEVER be falsely caps-VERIFIED. (This is the exact
        // accept-but-unsafe the discipline forbids: the inline must not swallow the println.)
        use crate::certificate::reachable_caps;
        let src = "fn run() -> Unit = {\n  \
            let _r = list.map([1, 2, 3], (x) => { println(\"hit\"); x }) }\n\
            fn main() -> Unit = { run() }\n";
        let prog = lower_source(src);
        let program: std::collections::BTreeMap<String, crate::MirFunction> =
            prog.functions.iter().map(|f| (f.name.clone(), f.clone())).collect();
        let mut visited = std::collections::BTreeSet::new();
        let reach = reachable_caps("run", &program, &mut visited);
        assert!(
            reach.contains(&crate::Capability::Stdout),
            "a printing map lambda must keep the caller's transitive caps tainted (Stdout reachable), got {reach:?}"
        );
        // And it still EXECUTES the side effect (prints "hit" thrice), byte-matching v0.
        if let Some(out) = build_and_run("c1_false_green", &render_wasm_program(&prog)) {
            assert_eq!(out, "hit\nhit\nhit");
        }
    }

    #[test]
    fn c1_direct_call_inline_executes_captured_lambda() {
        // C1 DIRECT-CALL INLINE: `let s = "ab"; let f = (x) => string.len(s) + x; f(1)` — the
        // CAPTURING let-bound lambda is inlined at its DIRECT call site (the capture `s` resolves
        // through the in-scope binding). EXECUTES to 3 (was a silent `Const 0` before C1 — the
        // capturing lambda deferred to an Opaque + zero). Byte-matches v0.
        let src = "fn main() -> Unit = {\n  let s = \"ab\"\n  \
            let f = (x) => string.len(s) + x\n  \
            println(int.to_string(f(1))) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("c1_direct_inline", &render_wasm_program(&prog)) {
            assert_eq!(out, "3");
        }
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // THE UNLINKED-CALL WALL (the StringInterp→to_string prerequisite brick).
    //
    // Before this wall, a `CallFn` to a stdlib fn NOT in the self-host registry (and not
    // a user fn / preamble fn) rendered as a dangling `(call $name)` → an INVALID wasm
    // module that wasmtime/wat2wasm reject — yet `render_wasm_program` returned it as a
    // plain String (invalid-wasm-passing-as-Ok). `try_render_wasm_program` now detects
    // the unresolved name at the resolution point and returns `LowerError::Unsupported`.
    // ──────────────────────────────────────────────────────────────────────────────

    #[test]
    fn unlinked_stdlib_call_is_walled_not_dangling_wasm() {
        use crate::lower::LowerError;
        use crate::render_wasm::{try_render_wasm_program, unlinked_call_names};
        // `list.bundled_probe` is NOT in the self-host registry — it is the bundled-body
        // MACHINERY PROBE (stdlib/list.almd), deliberately never self-hosted, so it stays a
        // bare `CallFn` — a STABLE canonical unlinked call (the previous exemplar,
        // `float.to_fixed`, got self-hosted 2026-07-17 and broke this test's premise).
        // (`float.from_int` AND `float.to_string` ARE registered — the contrast that proves
        // the wall is precise, not a blanket reject.)
        let src = "fn main() -> Unit = {\n  \
            let x = float.from_int(3)\n  println(float.to_string(x))\n  \
            println(int.to_string(list.bundled_probe(3))) }\n";
        let prog = lower_source(src);
        // The resolution check flags exactly the unlinked name, nothing else.
        let missing = unlinked_call_names(&prog);
        assert!(
            missing.contains("list.bundled_probe"),
            "the unlinked list.bundled_probe must be detected, got {missing:?}"
        );
        assert!(
            !missing.contains("float.from_int"),
            "a REGISTERED (auto-linked) call must NOT be walled — {missing:?}"
        );
        // The walled render returns a clean Unsupported (a loud reject), NOT a String.
        match try_render_wasm_program(&prog) {
            Err(LowerError::Unsupported(msg)) => {
                assert!(
                    msg.contains("list.bundled_probe"),
                    "the wall message must name the unlinked callee, got {msg:?}"
                );
            }
            Err(other) => panic!("expected Unsupported, got {other:?}"),
            Ok(_) => panic!("an unlinked call must be walled, not rendered to (possibly invalid) wasm"),
        }
    }

    #[test]
    fn leaf_interp_still_lowers_and_byte_matches_v0_after_the_wall() {
        // NO REGRESSION on the 83a72efa leaf slice: a LEAF interp (`${n}` Int, `${s}`
        // String) is fully linkable (every synthetic call — __str_concat / int.to_string —
        // is in the registry), so try_render_wasm_program returns Ok and the module runs,
        // byte-matching v0. Contrast with the unlinked-call wall above.
        let src = "fn main() -> Unit = {\n  \
            let n = 42\n  let s = \"world\"\n  \
            println(\"x=${n} y=${s}\") }\n";
        let prog = lower_source(src);
        assert!(
            crate::render_wasm::unlinked_call_names(&prog).is_empty(),
            "a leaf interp must be fully linkable (no wall), got {:?}",
            crate::render_wasm::unlinked_call_names(&prog)
        );
        let wat = crate::render_wasm::try_render_wasm_program(&prog)
            .expect("a leaf interp must render cleanly (no wall)");
        if let Some(out) = build_and_run("leaf_interp_after_wall", &wat) {
            assert_eq!(out, "x=42 y=world"); // v0 golden
        }
    }

    // ── Uniform interp desugar (fix-0276): per-type `to_string`, the wall walls Float/compound ──
    //
    // The StringInterp lowering is now a single uniform desugar: each part is wrapped in its
    // type's `to_string` (Lit/String passthrough, Int → int.to_string, Bool → bool.to_string,
    // Float → float.to_string [UNLINKED → walls], compound → <module>.to_string [UNLINKED →
    // walls]). The COVERED types byte-match v0; the UNCOVERED ones clean-WALL (Unsupported),
    // never invalid wasm. Goldens captured from `almide run` on the native v0 path.

    #[test]
    fn string_interp_bool_part_byte_matches_v0() {
        // The NEW covered type: a Bool `${b}` folds via the self-hosted `bool.to_string`
        // (`if b then "true" else "false"`), byte-matching v0's interned "true"/"false"
        // select. v0: "flag=true", "flag=false", "true and false".
        let src = "fn main() -> Unit = {\n  \
            let b = true\n  let c = false\n  \
            println(\"flag=${b}\")\n  \
            println(\"flag=${c}\")\n  \
            println(\"${b} and ${c}\") }\n";
        let prog = lower_source(src);
        // STRUCTURAL: a Bool interp must auto-link bool.to_string (not defer to Opaque).
        assert!(
            prog.functions.iter().any(|f| f.name == "bool.to_string"),
            "a Bool interp part must auto-link bool.to_string"
        );
        // A covered-only interp is fully linkable — render cleanly (no wall).
        assert!(
            crate::render_wasm::unlinked_call_names(&prog).is_empty(),
            "a Bool interp must be fully linkable (no wall), got {:?}",
            crate::render_wasm::unlinked_call_names(&prog)
        );
        if let Some(out) = build_and_run("string_interp_bool", &render_wasm_program(&prog)) {
            assert_eq!(out, "flag=true\nflag=false\ntrue and false");
        }
    }

    #[test]
    fn string_interp_pure_literal_and_edges_byte_match_v0() {
        // A pure-literal interp (no `${}`) plus leading / trailing interp positions. The
        // `""` seed makes a leading `${x}` byte-identical to v0's single-part passthrough.
        // v0: "no placeholders", "world!", "[world".
        let src = "fn main() -> Unit = {\n  \
            let s = \"world\"\n  \
            println(\"no placeholders\")\n  \
            println(\"${s}!\")\n  \
            println(\"[${s}\") }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("string_interp_edges", &render_wasm_program(&prog)) {
            assert_eq!(out, "no placeholders\nworld!\n[world");
        }
    }

    #[test]
    fn string_interp_float_part_links_and_byte_matches_v0() {
        // The Float interp type, now COVERED: `${f}` desugars to `float.to_string(f)`, which is
        // self-hosted (the faithful Dragon4 in stdlib/float_to_string.almd) and AUTO-LINKED — so
        // the interp lowers and byte-matches v0's float_display, instead of clean-walling. The
        // goldens are v0's `almide run` output for the same interps:
        //   f=2.5  → "f=2.5" ; g=0.001 → "g=0.001" (negative-k leading zeros) ;
        //   1.0/3.0 → "third=0.3333333333333333" (shortest round-trip).
        let src = "fn main() -> Unit = {\n  \
            let f = 2.5\n  println(\"f=${f}\")\n  \
            let g = 0.001\n  println(\"g=${g}\")\n  \
            let h = 1.0 / 3.0\n  println(\"third=${h}\") }\n";
        let prog = lower_source(src);
        // STRUCTURAL: a Float interp must auto-link float.to_string.
        assert!(
            prog.functions.iter().any(|f| f.name == "float.to_string"),
            "a Float interp part must auto-link float.to_string"
        );
        // Fully linkable now — no wall.
        assert!(
            crate::render_wasm::unlinked_call_names(&prog).is_empty(),
            "a Float interp must be fully linkable (no wall), got {:?}",
            crate::render_wasm::unlinked_call_names(&prog)
        );
        if let Some(out) = build_and_run("string_interp_float", &render_wasm_program(&prog)) {
            assert_eq!(out, "f=2.5\ng=0.001\nthird=0.3333333333333333");
        }
    }

    #[test]
    fn string_interp_compound_part_walls_not_invalid_wasm() {
        // RESOLVED frontier: `${xs}` over a nested `List[List[Int]]` now renders through
        // the composed `list.to_string_ll` self-host (byte-matching v0's Debug form).
        // DEEPER nesting (List[List[List[_]]]) still routes to the unlinked
        // `list.to_string_x` and walls as a unit — the guard this test keeps.
        let src = "fn main() -> Unit = {\n  let xs: List[List[Int]] = [[1, 2], [3]]\n  println(\"xs=${xs}\") }\n";
        let prog = lower_source(src);
        assert!(
            prog.functions.iter().any(|f| f.name == "main"),
            "the nested list interp must lower now"
        );
        if let Some(out) = build_and_run("string_interp_nested_list", &render_wasm_program(&prog)) {
            assert_eq!(out, "xs=[[1, 2], [3]]");
        }
        let deeper = "fn main() -> Unit = {\n  let xs: List[List[List[Int]]] = [[[1]]]\n  println(\"${xs}\") }\n";
        let prog2 = lower_source(deeper);
        assert!(
            !prog2.functions.iter().any(|f| f.name == "main"),
            "a triply-nested list literal must still wall as a unit"
        );
    }
