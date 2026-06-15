// render_wasm test suite — part 1 of 3 (renderer unit tests + shared helpers).
// Textually included by render_wasm/tests.rs (one module: helpers/tests share scope).

    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }

    /// Same program as the Rust-side test: `fn add(a,b)=a+b` + a `main` calling
    /// it. Both targets must print the same `5` — the dual-renderer thesis for
    /// USER FUNCTIONS (the mechanism that lets the runtime be self-hosted).
    fn add_program() -> MirProgram {
        let scalar = Repr::Scalar { width: ScalarWidth::Double };
        let add = MirFunction {
            name: "add".into(),
            params: vec![
                MirParam { value: ValueId(0), repr: scalar },
                MirParam { value: ValueId(1), repr: scalar },
            ],
            ops: vec![Op::IntBinOp {
                dst: ValueId(2),
                op: IntOp::Add,
                a: ValueId(0),
                b: ValueId(1),
            }],
            ret: Some(ValueId(2)),
            ..Default::default()
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::CallFn {
                    dst: Some(ValueId(0)),
                    name: "add".into(),
                    args: vec![CallArg::Imm(2), CallArg::Imm(3)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
            ],
            ret: None,
            ..Default::default()
        };
        MirProgram { functions: vec![add, main] }
    }

    #[test]
    fn function_call_lowers_and_runs_on_wasm() {
        let prog = add_program();
        if let Some(out) = build_and_run("fncall", &render_wasm_program(&prog)) {
            assert_eq!(out, "5");
        }
    }

    /// A HEAP-returning user call (`fn mk() -> String = "hi"`): the result is a Ptr
    /// (an i32 `$alloc` handle), so the caller's local must be typed i32 — NOT the
    /// scalar i64 default. Regression for `value_reprs_wasm` reading the call op's
    /// `result` repr; typing it i64 made `local.set` reject `$mk`'s i32 handle. Also
    /// pins the `Init::Str` un-defer: the literal's bytes are materialized.
    fn heap_call_program() -> MirProgram {
        let mk = MirFunction {
            name: "mk".into(),
            params: vec![],
            ops: vec![Op::Alloc { dst: ValueId(0), repr: heap(), init: Init::Str("hi".into()) }],
            ret: Some(ValueId(0)),
            ..Default::default()
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::CallFn {
                    dst: Some(ValueId(0)),
                    name: "mk".into(),
                    args: vec![],
                    result: Some(heap()),
                },
                // The returned object (rc 1) is released here — ownership-balanced.
                Op::Drop { v: ValueId(0) },
            ],
            ret: None,
            ..Default::default()
        };
        MirProgram { functions: vec![mk, main] }
    }

    #[test]
    fn heap_returning_call_types_result_as_i32_handle() {
        let prog = heap_call_program();
        let wat = render_wasm_program(&prog);
        // The fix: the call-result local is an i32 handle, never the scalar i64.
        assert!(wat.contains("(local $v0 i32)"), "call-result local must be i32:\n{wat}");
        assert!(!wat.contains("(local $v0 i64)"), "call-result local must NOT be i64:\n{wat}");
        // The Init::Str un-defer materialized the literal's bytes: 'h'=104, 'i'=105.
        assert!(
            wat.contains("(i32.const 104)") && wat.contains("(i32.const 105)"),
            "Init::Str bytes not materialized:\n{wat}"
        );
        // End-to-end: validates, runs clean (exit 0, no output) where wasmtime exists.
        if let Some(out) = build_and_run("heapcall", &wat) {
            assert_eq!(out, "");
        }
    }

    /// Lower real `.almd` SOURCE to a `MirProgram` through the existing frontend
    /// feeder (the same cut point as `examples/render_program.rs`) — for end-to-end
    /// tests over REAL lowering rather than hand-built MIR. Dev-only deps.
    fn lower_source(src: &str) -> MirProgram {
        use almide_frontend::check::Checker;
        use almide_frontend::lower::lower_program;
        use almide_frontend::{canonicalize, ir_link};
        use almide_lang::lexer::Lexer;
        use almide_lang::parser::Parser;
        use almide_optimize::{mono, optimize};
        let tokens = Lexer::tokenize(src);
        let mut prog = Parser::new(tokens).parse().expect("parse");
        let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
        let mut checker = Checker::from_env(canon.env);
        let _ = checker.infer_program(&mut prog);
        let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
        optimize::optimize_program(&mut ir);
        mono::monomorphize(&mut ir);
        ir_link::ir_link(&mut ir);
        let mut globals: std::collections::HashMap<almide_ir::VarId, almide_lang::types::Ty> =
            std::collections::HashMap::new();
        for tl in &ir.top_lets {
            globals.insert(tl.var, tl.ty.clone());
        }
        for m in &ir.modules {
            for tl in &m.top_lets {
                globals.insert(tl.var, tl.ty.clone());
            }
        }
        let mut functions: Vec<MirFunction> =
            ir.functions.iter().filter_map(|f| crate::lower::lower_function(f, &globals).ok()).collect();
        // Auto-link the self-hosted stdlib runtime: for each registry entry CALLED but not
        // defined, lower its Almide source and rename the impl fn to the call name (so
        // `(call $module.func)` resolves AND the caps gate reads it as a known-pure stdlib
        // `module.func` — no caps-coverage regression). Conditional, so non-using programs
        // are not bloated by builders + helpers.
        for (source, entries) in self_host_runtime() {
            let any_called = entries.iter().any(|(_, call)| calls_fn(&functions, call));
            let any_defined =
                entries.iter().any(|(_, call)| functions.iter().any(|f| &f.name == call));
            if any_called && !any_defined {
                let mut rt = lower_source(source);
                for f in rt.functions.iter_mut() {
                    if let Some((_, call)) = entries.iter().find(|(impl_fn, _)| &f.name == impl_fn) {
                        f.name = call.to_string();
                    }
                }
                functions.extend(rt.functions);
            }
        }
        // Auto-link the self-hosted print_str runtime (the v1 linker step) so a plain
        // `println(…)` program — which lowers to a PrintStr → `(call $print_str)` —
        // resolves, matching how render_program links it. Skip if already defined.
        if !functions.iter().any(|f| f.name == "print_str") {
            let rt = lower_source(include_str!("../../../../stdlib/print_str.almd"));
            functions.extend(rt.functions);
        }
        // A recursively-linked impl re-runs this auto-link, so it can carry its own copy of
        // print_str (or, later, a shared helper). Keep the FIRST definition of each name —
        // they are identical (same source), so dedup is a no-op on behavior, not a merge.
        let mut seen = std::collections::HashSet::new();
        functions.retain(|f| seen.insert(f.name.clone()));
        MirProgram { functions }
    }

    /// Does any function CALL `name` (a `CallFn` to it)? Drives conditional auto-linking.
    fn calls_fn(functions: &[MirFunction], name: &str) -> bool {
        functions.iter().any(|f| {
            f.ops.iter().any(|op| matches!(op, Op::CallFn { name: n, .. } if n == name))
        })
    }

    /// A scalar-result user call (`let _r = add(2, 3)`) lowered from REAL source is an
    /// EXECUTABLE `CallFn` — immediate args + a bound scalar result — not the pre-
    /// execution `Const` + empty elided marker. Regression for `try_lower_scalar_call`
    /// (the scalar-call execution slice). The swap is adversarially-verified SOUND: the
    /// real CallFn replaces the marker 1:1 (same callee NAME → caps fold unchanged) and
    /// a scalar result registers no ownership object.
    #[test]
    fn scalar_user_call_lowers_to_executable_callfn() {
        let prog = lower_source(
            "fn add(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = { let _r = add(2, 3) }\n",
        );
        let main = prog.functions.iter().find(|f| f.name == "main").expect("main lowered");
        // A real CallFn to `add`: a bound result repr + both literal args as immediates.
        let (args, result) = main
            .ops
            .iter()
            .find_map(|op| match op {
                Op::CallFn { dst: Some(_), name, args, result } if name == "add" => {
                    Some((args.clone(), *result))
                }
                _ => None,
            })
            .expect("a real CallFn to add with a bound dst");
        assert!(result.is_some(), "the scalar call result repr must be set");
        assert!(
            args.iter().any(|a| matches!(a, CallArg::Imm(2)))
                && args.iter().any(|a| matches!(a, CallArg::Imm(3))),
            "both literal args must be immediates, got {args:?}"
        );
        // ...and NOT also an empty elided marker for add (the call is real, not elided).
        assert!(
            !main.ops.iter().any(|op| matches!(op,
                Op::CallFn { dst: None, name, args, .. } if name == "add" && args.is_empty())),
            "add must not also appear as an empty elided caps marker"
        );
        // End-to-end: renders to a valid module that runs cleanly (where wasmtime is present).
        if let Some(out) = build_and_run("scalar_user_call", &render_wasm_program(&prog)) {
            assert_eq!(out, "");
        }
    }

    /// An `Int` literal in value position materializes its REAL value (`Op::ConstInt`
    /// → `(local.set $dst (i64.const v))`), not the deferred-`Const` zero — the
    /// scalar-value foundation that lets a self-hosted runtime fn compute real
    /// addresses/lengths. Regression: lowering emits ConstInt, render emits the const.
    #[test]
    fn int_literal_materializes_its_value() {
        let prog = lower_source(
            "fn answer() -> Int = 42\nfn main() -> Unit = { let _a = answer() }\n",
        );
        let answer = prog.functions.iter().find(|f| f.name == "answer").expect("answer lowered");
        assert!(
            answer.ops.iter().any(|op| matches!(op, Op::ConstInt { value: 42, .. })),
            "answer must materialize 42 via ConstInt, got {:?}",
            answer.ops
        );
        let wat = render_wasm_program(&prog);
        assert!(wat.contains("(i64.const 42)"), "render must emit the constant:\n{wat}");
        if let Some(out) = build_and_run("int_literal", &wat) {
            assert_eq!(out, "");
        }
    }

    /// Scalar `Int` arithmetic COMPUTES (`fn add(a, b) = a + b` → `IntBinOp{Add}`,
    /// rendered `i64.add` over the param locals) — not the deferred-Const zero. With
    /// the literal-materialization above, this is the foundation a self-hosted runtime
    /// fn needs to compute real addresses (`s + LIST_HEADER`). The add_program test
    /// already proves `i64.add` returns the right value end-to-end.
    #[test]
    fn scalar_arithmetic_computes_via_intbinop() {
        let prog = lower_source(
            "fn add(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = { let _r = add(2, 3) }\n",
        );
        let add = prog.functions.iter().find(|f| f.name == "add").expect("add lowered");
        assert!(
            add.ops.iter().any(|op| matches!(op, Op::IntBinOp { op: IntOp::Add, .. })),
            "add must compute a+b via IntBinOp, got {:?}",
            add.ops
        );
        let wat = render_wasm_program(&prog);
        assert!(wat.contains("(i64.add"), "render must emit i64.add:\n{wat}");
        if let Some(out) = build_and_run("scalar_arith", &wat) {
            assert_eq!(out, "");
        }
    }

    /// THE PRIM-FLOOR PROOF (sub-slice 1): a hand-built `print_str` MirFunction —
    /// written ENTIRELY over the prim floor (handle / load / store / fd_write) + the
    /// scalar-value foundation (ConstInt / IntBinOp) — reads a heap String's bytes and
    /// writes them + a newline to stdout via a 2-element iovec `fd_write`. main allocs
    /// "hi" and calls it. This proves the prim ops render to valid wasm and ACTUALLY
    /// PRINT, with NO new preamble runtime func (the discipline) — the whole mechanism
    /// for self-hosted print, validated in isolation before the frontend `prim` module.
    #[test]
    fn prim_floor_print_str_prints() {
        // print_str(s: String): writes the string's bytes + "\n" to stdout.
        let print_str = MirFunction {
            name: "print_str".into(),
            params: vec![MirParam { value: ValueId(0), repr: heap() }],
            ops: vec![
                // h = prim.handle(s)  — the block's i64 address
                Op::Prim { kind: PrimKind::Handle, dst: Some(ValueId(1)), args: vec![ValueId(0)] },
                // len = prim.load32(h + 4)   (LIST_LEN_OFFSET)
                Op::ConstInt { dst: ValueId(2), value: 4 },
                Op::IntBinOp { dst: ValueId(3), op: IntOp::Add, a: ValueId(1), b: ValueId(2) },
                Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(ValueId(4)), args: vec![ValueId(3)] },
                // data = h + 12   (LIST_HEADER)
                Op::ConstInt { dst: ValueId(5), value: 12 },
                Op::IntBinOp { dst: ValueId(6), op: IntOp::Add, a: ValueId(1), b: ValueId(5) },
                // iovec[0] = { ptr=data @ 8, len @ 12 }
                Op::ConstInt { dst: ValueId(7), value: 8 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(7), ValueId(6)] },
                Op::ConstInt { dst: ValueId(8), value: 12 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(8), ValueId(4)] },
                // "\n" (10) at scratch 512; iovec[1] = { ptr=512 @ 16, len=1 @ 20 }
                Op::ConstInt { dst: ValueId(9), value: 512 },
                Op::ConstInt { dst: ValueId(10), value: 10 },
                Op::Prim { kind: PrimKind::Store { width: 1 }, dst: None, args: vec![ValueId(9), ValueId(10)] },
                Op::ConstInt { dst: ValueId(11), value: 16 },
                Op::ConstInt { dst: ValueId(12), value: 512 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(11), ValueId(12)] },
                Op::ConstInt { dst: ValueId(13), value: 20 },
                Op::ConstInt { dst: ValueId(14), value: 1 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(13), ValueId(14)] },
                // fd_write(stdout=1, iovec@8, count=2, nwritten@0)
                Op::ConstInt { dst: ValueId(15), value: 1 },
                Op::ConstInt { dst: ValueId(16), value: 8 },
                Op::ConstInt { dst: ValueId(17), value: 2 },
                Op::ConstInt { dst: ValueId(18), value: 0 },
                Op::Prim {
                    kind: PrimKind::FdWrite,
                    dst: Some(ValueId(19)),
                    args: vec![ValueId(15), ValueId(16), ValueId(17), ValueId(18)],
                },
            ],
            ret: None,
            declared_caps: vec![crate::Capability::Stdout],
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::Alloc { dst: ValueId(0), repr: heap(), init: Init::Str("hi".into()) },
                Op::CallFn {
                    dst: None,
                    name: "print_str".into(),
                    args: vec![CallArg::Handle(ValueId(0))],
                    result: None,
                },
                Op::Drop { v: ValueId(0) },
            ],
            ret: None,
            ..Default::default()
        };
        let prog = MirProgram { functions: vec![print_str, main] };
        // The prim ops render to valid wasm and print "hi\n" (trimmed to "hi").
        if let Some(out) = build_and_run("prim_print_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "hi");
        }
    }

    /// THE OBSERVABILITY KEYSTONE (sub-slice 2+3): `println("hello")` from SOURCE runs
    /// through v1's SELF-HOSTED print_str — written in ALMIDE over the `prim` floor
    /// (prim.handle/load32/store*/fd_write → Op::Prim, mapped from the bundled `prim`
    /// module), compiled through v1's own lower→MIR→render pipeline — and prints
    /// "hello", byte-matching v0's native println. NO hand-written WAT growth (the
    /// discipline). The self-host runtime vision, realized for print.
    #[test]
    fn selfhosted_print_str_from_source_prints() {
        let src = "fn print_str(s: String) -> Unit = {\n  \
            let h = prim.handle(s)\n  \
            let len = prim.load32(h + 4)\n  \
            let data = h + 12\n  \
            prim.store32(8, data)\n  \
            prim.store32(12, len)\n  \
            prim.store8(512, 10)\n  \
            prim.store32(16, 512)\n  \
            prim.store32(20, 1)\n  \
            let _w = prim.fd_write(1, 8, 2, 0)\n\
            }\n\
            fn main() -> Unit = println(\"hello\")\n";
        let prog = lower_source(src);
        // print_str lowered to real prim-floor ops (not the deferred Const).
        let ps = prog.functions.iter().find(|f| f.name == "print_str").expect("print_str lowered");
        assert!(
            ps.ops.iter().any(|op| matches!(op, Op::Prim { kind: PrimKind::FdWrite, .. })),
            "print_str must reach Op::Prim FdWrite from source, got {:?}",
            ps.ops
        );
        // End-to-end: it prints "hello" (matching v0's native println).
        if let Some(out) = build_and_run("selfhost_println", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello");
        }
    }

    /// SEAMLESS v1=v0: a PLAIN `println` program — byte-identical to what runs on v0,
    /// with NO print_str defined — works on v1 because the self-hosted print_str
    /// runtime is AUTO-LINKED (the v1 linker step). Two printlns include the newline
    /// between them (print_str writes string + "\n" via two single-iovec fd_writes).
    #[test]
    fn plain_println_auto_links_and_prints() {
        let prog = lower_source(
            "fn main() -> Unit = {\n  println(\"line one\")\n  println(\"line two\")\n}\n",
        );
        // print_str was auto-linked (the source did not define it).
        assert!(
            prog.functions.iter().any(|f| f.name == "print_str"),
            "the self-hosted print_str must be auto-linked"
        );
        // build_and_run trims the trailing newline; the MIDDLE newline must remain.
        if let Some(out) = build_and_run("plain_println", &render_wasm_program(&prog)) {
            assert_eq!(out, "line one\nline two");
        }
    }

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

    #[test]
    fn heap_result_if_returns_the_taken_arm_string() {
        // `if c then "yes" else "no"` RETURNS a String — only the taken arm allocates,
        // returned rc=1 to the caller (per-arm Alloc+Consume balance). label(true)="yes",
        // label(false)="no", byte-matching v0.
        let src = "fn label(c: Bool) -> String = if c then \"yes\" else \"no\"\n\
            fn main() -> Unit = {\n  \
            println(label(true))\n  println(label(false)) }\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "label").unwrap();
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
        let f = prog.functions.iter().find(|f| f.name == "name").unwrap();
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
        let f = prog.functions.iter().find(|f| f.name == "pick").unwrap();
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
        let main = prog.functions.iter().find(|f| f.name == "main").unwrap();
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

