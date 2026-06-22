
    #[test]
    fn name_witness_total_for_wellformed_mirs() {
        // The 2nd property: every used value id is defined (no dangling MIR
        // reference). For well-formed MIRs the witness satisfies the proven
        // `check_names` (used ⊆ defined). Pinned over the random corpus.
        for seed in 0u64..500 {
            let f = gen_wellformed(seed);
            let w = name_witness(&f);
            for u in &w.used {
                assert!(
                    w.defined.contains(u),
                    "seed {seed}: used {u:?} is not defined (dangling)\nops: {:?}",
                    f.ops
                );
            }
        }
    }

    #[test]
    fn cap_witness_derives_used_from_runtime_calls() {
        // The 4th property: used capabilities come from the body's runtime calls
        // (PrintInt reaches Stdout); pure heap ops reach none. The witness checks
        // them against the declared bound.
        let print = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
        ]);
        assert_eq!(cap_witness(&print).used, vec![Capability::Stdout]);

        // A pure heap op (no host effect) leaves the used set empty.
        let pure = func(vec![
            Op::Alloc { dst: ValueId(0), repr: heap(), init: Init::Opaque },
            Op::MakeUnique { v: ValueId(0) },
            Op::Call {
                dst: Some(ValueId(0)),
                func: RtFn::ListPush,
                args: vec![CallArg::Handle(ValueId(0)), CallArg::Imm(1)],
            result: None },
            Op::Drop { v: ValueId(0) },
        ]);
        assert!(cap_witness(&pure).used.is_empty());
    }

    #[test]
    fn call_indirect_conservatively_taints_every_capability() {
        // THE CLOSURES SOUNDNESS CRUX: a CallIndirect invokes an unanalyzable closure that
        // may reach ANY capability, so the witness must conservatively mark every modeled
        // cap (Stdout) USED — a fn with a closure call is caps-verified ONLY if it DECLARES
        // it. A pure-looking fn that calls a secretly-Stdout closure can never pass
        // un-witnessed (accept-but-unsafe).
        let closure_caller = func(vec![
            Op::ConstInt { dst: ValueId(0), value: 0 }, // the closure value (a table index)
            Op::CallIndirect { dst: None, table_idx: ValueId(0), args: vec![], result: None },
        ]);
        let w = cap_witness(&closure_caller);
        assert_eq!(w.used, vec![Capability::Stdout], "a CallIndirect must witness Stdout used");
        // With no declared caps (the default), used ⊄ allowed → the proven `used ⊆ allowed`
        // checker REJECTS it as caps-verified — it stays honestly caps-unverified.
        assert!(
            !w.used.iter().all(|c| w.allowed.contains(c)),
            "a closure caller with no declared caps must NOT be silently caps-verified"
        );
    }

    #[test]
    fn call_indirect_through_a_pure_funcref_folds_no_caps() {
        use std::collections::{BTreeMap, BTreeSet};
        // PRECISE FOLD: main = `f = FuncRef("pure_lambda"); (f)()` where the lambda is PURE.
        // The table index resolves to a KNOWN lifted lambda, so cap_witness does NOT
        // conservatively taint Stdout — the lambda's real (empty) caps are folded instead,
        // keeping a non-printing closure caps-VERIFIED (no spurious taint, no corpus drop).
        let mut pure_lambda = func(vec![Op::ConstInt { dst: ValueId(0), value: 1 }]);
        pure_lambda.name = "pure_lambda".into();
        let mut main = func(vec![
            Op::FuncRef { dst: ValueId(0), name: "pure_lambda".into() },
            Op::CallIndirect { dst: None, table_idx: ValueId(0), args: vec![], result: None },
        ]);
        main.name = "main".into();
        assert!(
            cap_witness(&main).used.is_empty(),
            "a CallIndirect to a known pure lambda must not conservatively taint"
        );
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [pure_lambda, main.clone()] {
            program.insert(f.name.clone(), f);
        }
        let mut seen = BTreeSet::new();
        assert!(
            reachable_caps("main", &program, &mut seen).is_empty(),
            "a pure-lambda closure reaches no capability"
        );
    }

    #[test]
    fn call_indirect_through_a_printing_funcref_still_reaches_stdout() {
        use std::collections::{BTreeMap, BTreeSet};
        // ADVERSARIAL: the lambda SECRETLY prints. The known-FuncRef fold must STILL surface
        // its Stdout (no accept-but-unsafe) — the fold follows the same edge cap_witness
        // dropped the taint for, so a printing lambda's effect always reaches the caller.
        let mut printing_lambda = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call {
                dst: None,
                func: RtFn::PrintInt,
                args: vec![CallArg::Scalar(ValueId(0))],
                result: None,
            },
        ]);
        printing_lambda.name = "printing_lambda".into();
        let mut main = func(vec![
            Op::FuncRef { dst: ValueId(0), name: "printing_lambda".into() },
            Op::CallIndirect { dst: None, table_idx: ValueId(0), args: vec![], result: None },
        ]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [printing_lambda, main.clone()] {
            program.insert(f.name.clone(), f);
        }
        let mut seen = BTreeSet::new();
        assert!(
            reachable_caps("main", &program, &mut seen).contains(&Capability::Stdout),
            "a printing closure must still surface Stdout transitively (no accept-but-unsafe)"
        );
        assert_eq!(
            transitive_cap_witness_string(&main, &program),
            "|0",
            "rejected: declares no caps but reaches Stdout through the lambda"
        );
    }

    #[test]
    fn func_ref_alone_accounts_the_lambda_caps_without_a_call_indirect() {
        use std::collections::{BTreeMap, BTreeSet};
        // COVERAGE-FREE SOUNDNESS: a FuncRef to a printing lambda accounts its Stdout in the
        // creating function EVEN WITH NO CallIndirect — the closure might be invoked via a
        // deferred/operand call path or passed elsewhere, so accounting at CREATION means
        // incremental lambda-lifting cannot lose a closure's effect however the call lowers.
        let mut printing = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call {
                dst: None,
                func: RtFn::PrintInt,
                args: vec![CallArg::Scalar(ValueId(0))],
                result: None,
            },
        ]);
        printing.name = "printing".into();
        // main only CREATES the closure (FuncRef) — it does NOT CallIndirect it.
        let mut main = func(vec![Op::FuncRef { dst: ValueId(0), name: "printing".into() }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [printing, main.clone()] {
            program.insert(f.name.clone(), f);
        }
        let mut seen = BTreeSet::new();
        assert!(
            reachable_caps("main", &program, &mut seen).contains(&Capability::Stdout),
            "creating a closure to a printing lambda must account its Stdout even without a call"
        );
    }

    #[test]
    fn cap_witness_string_matches_the_coq_parser_format() {
        // declares Stdout, prints → `0|0`  (allowed ⊇ used → checker accepts).
        let mut declared = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
        ]);
        declared.declared_caps = vec![Capability::Stdout];
        assert_eq!(cap_witness_string(&declared), "0|0");

        // declares nothing, prints → `|0`  (used ⊄ allowed → checker rejects).
        let mut undeclared = declared.clone();
        undeclared.declared_caps = vec![];
        assert_eq!(cap_witness_string(&undeclared), "|0");
    }

    #[test]
    fn program_cap_graph_witness_emits_the_call_graph_for_check_prog_cert() {
        use std::collections::BTreeMap;
        let no = |_: &str| false;
        // helper prints (Stdout=0); main calls helper; both declare Stdout. Sorted by name:
        // 0=helper, 1=main, 2=UNIVERSE. helper `0|0|`, main `0||0` (callee 0), UNIVERSE sentinel.
        let mut helper = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))], result: None },
        ]);
        helper.name = "helper".into();
        helper.declared_caps = vec![Capability::Stdout];
        let mut main = func(vec![Op::CallFn { dst: None, name: "helper".into(), args: vec![], result: None }]);
        main.name = "main".into();
        main.declared_caps = vec![Capability::Stdout];
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [helper, main] { program.insert(f.name.clone(), f); }
        // check_prog_cert ACCEPTS: main reaches helper's {0} ⊆ main's declared {0}.
        assert_eq!(program_cap_graph_witness(&program, &no, &no), "0|0|;0||0;1000000|1000000|");

        // an UNKNOWN (cross-file) callee is routed to the UNIVERSE sentinel → the proven checker
        // REJECTS (main reaches the sentinel cap ∉ its declared bound). main `0||1`, UNIVERSE 1.
        let mut m2 = func(vec![Op::CallFn { dst: None, name: "ext".into(), args: vec![], result: None }]);
        m2.name = "main".into();
        m2.declared_caps = vec![Capability::Stdout];
        let mut prog2: BTreeMap<String, MirFunction> = BTreeMap::new();
        prog2.insert("main".into(), m2);
        assert_eq!(program_cap_graph_witness(&prog2, &no, &no), "0||1;1000000|1000000|");
    }

    #[test]
    fn reachable_caps_folds_transitively_and_survives_cycles() {
        use std::collections::{BTreeMap, BTreeSet};
        // main → beep (prints, reaches Stdout). main has NO direct effect.
        let mut beep = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
        ]);
        beep.name = "beep".into();
        let mut main = func(vec![Op::CallFn { dst: None, name: "beep".into(), args: vec![] , result: None }]);
        main.name = "main".into();
        // A cycle main→loop→main must not diverge.
        let mut looper = func(vec![Op::CallFn { dst: None, name: "main".into(), args: vec![] , result: None }]);
        looper.name = "loop".into();
        main.ops.push(Op::CallFn { dst: None, name: "loop".into(), args: vec![] , result: None });

        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [beep, main.clone(), looper] {
            program.insert(f.name.clone(), f);
        }
        let mut seen = BTreeSet::new();
        let reach = reachable_caps("main", &program, &mut seen);
        // main reaches Stdout ONLY transitively (via beep) — the per-call-site fold.
        assert!(reach.contains(&Capability::Stdout));
        // And the transitive witness rejects (declared empty, reachable Stdout).
        assert_eq!(transitive_cap_witness_string(&main, &program), "|0");
    }

    // ── borrow-by-default calling convention (heap params) ──
    // A heap param is BORROWED: the caller owns the reference. So a param emits
    // NO `i` (no synthetic +1), and a body that releases/returns it WITHOUT first
    // acquiring its own reference (a `Dup`) is correctly REJECTED. The cert and
    // `verify_ownership` must agree on every case below.

    fn param_fn(name: &str, ops: Vec<Op>, ret: Option<ValueId>) -> MirFunction {
        MirFunction {
            name: name.into(),
            params: vec![MirParam { value: ValueId(0), repr: heap() }],
            ops,
            ret,
            ..Default::default()
        }
    }

    #[test]
    fn borrow_only_param_has_no_ownership_event() {
        // fn(p) { borrow p } — the param is read, never owned: empty cert, accepts.
        let f = param_fn("borrow_only", vec![Op::Borrow { v: ValueId(0) }], None);
        assert_eq!(ownership_certificate(&f), "");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn returning_a_borrowed_param_directly_is_rejected() {
        // fn(p) -> p : returning the borrowed reference without acquiring our own
        // hands the caller a SECOND owner of its own reference = a double-free.
        // Cert is `m` at rc 0 (no preceding `i`) → the proven checker faults.
        let f = param_fn("return_param", vec![], Some(ValueId(0)));
        assert_eq!(ownership_certificate(&f), "m\n");
        assert!(verify_ownership(&f).is_err());
    }

    #[test]
    fn acquiring_then_returning_a_param_balances() {
        // fn(p) { let q = dup p; q } — the CORRECT way to return a param: acquire
        // our own reference (`a`) then move it out (`m`). Cert `am` balances.
        let f = param_fn(
            "acquire_return",
            vec![Op::Dup { dst: ValueId(1), src: ValueId(0) }],
            Some(ValueId(1)),
        );
        assert_eq!(ownership_certificate(&f), "am\n");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    #[test]
    fn releasing_a_borrowed_param_is_rejected() {
        // fn(p) { drop p } — releasing a reference we do not own. Cert `d` at
        // rc 0 → faulted; verify reports the double-free.
        let f = param_fn("drop_borrow", vec![Op::Drop { v: ValueId(0) }], None);
        assert_eq!(ownership_certificate(&f), "d\n");
        assert!(verify_ownership(&f).is_err());
    }

    #[test]
    fn passing_a_borrowed_param_to_a_call_is_accepted() {
        // fn(p) { g(p) } — borrowing the param into a call (no refcount change):
        // no cert event for the param, accepts.
        let f = param_fn(
            "forward",
            vec![Op::CallFn {
                dst: None,
                name: "g".into(),
                args: vec![CallArg::Handle(ValueId(0))],
                result: None,
            }],
            None,
        );
        assert_eq!(ownership_certificate(&f), "");
        assert_eq!(verify_ownership(&f), Ok(()));
    }

    // ── conservative transitive capability reachability (brick #49) ──

    #[test]
    fn transitive_capability_through_callfn_is_caught() {
        use std::collections::{BTreeMap, BTreeSet};
        // main → beep; beep prints (PrintInt → Stdout). main has NO direct cap but
        // reaches one transitively — the fold MUST flag it (the direct witness wouldn't).
        let mut beep = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))], result: None },
        ]);
        beep.name = "beep".into();
        let mut main = func(vec![Op::CallFn { dst: None, name: "beep".into(), args: vec![], result: None }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [beep, main] {
            program.insert(f.name.clone(), f);
        }
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        let mut v = BTreeSet::new();
        assert!(reaches_capability_or_unknown("main", &program, &none_free, &not_elided, &mut v));
    }

    #[test]
    fn an_elided_call_taints_the_function_and_its_callers() {
        use std::collections::{BTreeMap, BTreeSet};
        // main → helper; helper has NO direct cap and NO CallFn, but it ELIDED a
        // call (its source had a call the MIR dropped to Opaque) — so its caps are
        // incompletely captured. main must be tainted transitively.
        let mut helper = func(vec![Op::Const { dst: ValueId(0) }]); // looks pure, but elided
        helper.name = "helper".into();
        let mut main = func(vec![Op::CallFn { dst: None, name: "helper".into(), args: vec![], result: None }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [helper, main] {
            program.insert(f.name.clone(), f);
        }
        let none_free = |_: &str| false;
        let elided_helper = |n: &str| n == "helper";
        let mut v = BTreeSet::new();
        assert!(reaches_capability_or_unknown("main", &program, &none_free, &elided_helper, &mut v));
        // Without the elision, the same pure chain is effect-free.
        let not_elided = |_: &str| false;
        let mut v2 = BTreeSet::new();
        assert!(!reaches_capability_or_unknown("main", &program, &none_free, &not_elided, &mut v2));
    }

    #[test]
    fn unknown_callee_is_conservatively_tainted_unless_freed_by_policy() {
        use std::collections::{BTreeMap, BTreeSet};
        // f → helper, helper NOT in the program: tainted by default, free iff the policy says so.
        let mut f = func(vec![Op::CallFn { dst: None, name: "helper".into(), args: vec![], result: None }]);
        f.name = "f".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        program.insert("f".to_string(), f);
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        let mut v1 = BTreeSet::new();
        assert!(reaches_capability_or_unknown("f", &program, &none_free, &not_elided, &mut v1));
        let helper_free = |n: &str| n == "helper";
        let mut v2 = BTreeSet::new();
        assert!(!reaches_capability_or_unknown("f", &program, &helper_free, &not_elided, &mut v2));
    }

    #[test]
    fn pure_chain_and_cycle_are_effect_free() {
        use std::collections::{BTreeMap, BTreeSet};
        // a → b → a, both pure (no caps, no unknown callees): effect-free, terminates.
        let mut a = func(vec![Op::CallFn { dst: None, name: "b".into(), args: vec![], result: None }]);
        a.name = "a".into();
        let mut b = func(vec![Op::CallFn { dst: None, name: "a".into(), args: vec![], result: None }]);
        b.name = "b".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for x in [a, b] {
            program.insert(x.name.clone(), x);
        }
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        let mut v = BTreeSet::new();
        assert!(!reaches_capability_or_unknown("a", &program, &none_free, &not_elided, &mut v));
    }

    #[test]
    fn reachable_caps_returns_the_set_or_taints() {
        use std::collections::{BTreeMap, BTreeSet};
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        // main → beep (prints Stdout): reachable = {Stdout}, FULLY known.
        let mut beep = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))], result: None },
        ]);
        beep.name = "beep".into();
        let mut main = func(vec![Op::CallFn { dst: None, name: "beep".into(), args: vec![], result: None }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [beep, main] {
            program.insert(f.name.clone(), f);
        }
        let mut v = BTreeSet::new();
        assert_eq!(
            reachable_caps_or_tainted("main", &program, &none_free, &not_elided, &mut v),
            Some(vec![Capability::Stdout])
        );
        // An unknown callee → None (incomplete reachable set, cannot verify).
        let mut f = func(vec![Op::CallFn { dst: None, name: "x".into(), args: vec![], result: None }]);
        f.name = "f".into();
        let mut p2: BTreeMap<String, MirFunction> = BTreeMap::new();
        p2.insert("f".to_string(), f);
        let mut v2 = BTreeSet::new();
        assert_eq!(reachable_caps_or_tainted("f", &p2, &none_free, &not_elided, &mut v2), None);
        // An elided callee → None.
        let elided = |n: &str| n == "f";
        let mut v3 = BTreeSet::new();
        assert_eq!(reachable_caps_or_tainted("f", &p2, &none_free, &elided, &mut v3), None);
    }

    #[test]
    fn tainted_fold_follows_funcref_edges_to_a_lifted_lambda() {
        use std::collections::{BTreeMap, BTreeSet};
        // THE harness path (classify_corpus uses `reachable_caps_or_tainted`, NOT
        // `reachable_caps`). A main that holds a lifted lambda via `Op::FuncRef` must fold
        // that lambda's caps here too, or the corpus gate would falsely caps-VERIFY a main
        // whose lifted lambda secretly prints (accept-but-unsafe) the moment lambda-lifting
        // emits FuncRef into the corpus.
        let none_free = |_: &str| false;
        let not_elided = |_: &str| false;
        // ADVERSARIAL: the lifted lambda prints. main = `FuncRef(printing_lambda)` only —
        // no CallIndirect — so coverage-free folding (at creation) must still surface Stdout.
        let mut printing_lambda = func(vec![
            Op::Const { dst: ValueId(0) },
            Op::Call {
                dst: None,
                func: RtFn::PrintInt,
                args: vec![CallArg::Scalar(ValueId(0))],
                result: None,
            },
        ]);
        printing_lambda.name = "printing_lambda".into();
        let mut main = func(vec![Op::FuncRef { dst: ValueId(0), name: "printing_lambda".into() }]);
        main.name = "main".into();
        let mut program: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [printing_lambda, main] {
            program.insert(f.name.clone(), f);
        }
        let mut v = BTreeSet::new();
        assert_eq!(
            reachable_caps_or_tainted("main", &program, &none_free, &not_elided, &mut v),
            Some(vec![Capability::Stdout]),
            "a main holding a printing lifted lambda must reach Stdout in the harness fold"
        );
        // A PURE lifted lambda folds ∅ — no spurious taint (keeps a non-printing closure
        // caps-verified, so the corpus caps count does not drop).
        let mut pure_lambda = func(vec![Op::ConstInt { dst: ValueId(0), value: 1 }]);
        pure_lambda.name = "pure_lambda".into();
        let mut main2 = func(vec![Op::FuncRef { dst: ValueId(0), name: "pure_lambda".into() }]);
        main2.name = "main".into();
        let mut p2: BTreeMap<String, MirFunction> = BTreeMap::new();
        for f in [pure_lambda, main2] {
            p2.insert(f.name.clone(), f);
        }
        let mut v2 = BTreeSet::new();
        assert_eq!(
            reachable_caps_or_tainted("main", &p2, &none_free, &not_elided, &mut v2),
            Some(Vec::new()),
            "a main holding a pure lifted lambda reaches no capability"
        );
    }

    #[test]
    fn every_plus_one_event_is_backed_by_a_real_op() {
        // The NON-RECURRING soundness gate. Borrow-by-default holds iff EVERY `+1`
        // in the certificate is backed by a real runtime op: an `i` by an `Alloc`
        // or a heap-result call, an `a` by a `Dup`. A param can NEVER inject an
        // unbacked `+1` (the gate-blind use-after-free class). If a future brick
        // ever emits a param `i` again, this equality breaks and the gate fails.
        fn backed(f: &MirFunction) -> bool {
            let cert = ownership_certificate(f);
            let i = cert.chars().filter(|c| *c == 'i').count();
            let a = cert.chars().filter(|c| *c == 'a').count();
            let allocs = f.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
            let heap_results = f
                .ops
                .iter()
                .filter(|o| match o {
                    Op::Call { dst: Some(_), result: Some(r), .. }
                    | Op::CallFn { dst: Some(_), result: Some(r), .. } => r.is_heap(),
                    _ => false,
                })
                .count();
            let dups = f.ops.iter().filter(|o| matches!(o, Op::Dup { .. })).count();
            i == allocs + heap_results && a == dups
        }
        for seed in 0u64..500 {
            let f = gen_wellformed(seed);
            assert!(backed(&f), "seed {seed} has an unbacked +1\nops: {:?}", f.ops);
        }
        // Param-bearing functions: the borrowed param injects no `i`/`a`.
        assert!(backed(&param_fn("b", vec![Op::Borrow { v: ValueId(0) }], None)));
        assert!(backed(&param_fn(
            "ar",
            vec![Op::Dup { dst: ValueId(1), src: ValueId(0) }],
            Some(ValueId(1)),
        )));
    }
