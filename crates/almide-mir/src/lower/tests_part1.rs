// Core-IR → MIR lowering tests — part 1 of 2. Included by lower/tests.rs (one module).

    fn ir_expr(kind: IrExprKind, ty: Ty) -> IrExpr {
        IrExpr { kind, ty, span: None, def_id: None }
    }
    fn stmt(kind: IrStmtKind) -> IrStmt {
        IrStmt { kind, span: None }
    }
    fn list_int() -> Ty {
        // Any Applied/heap type works for the ownership logic; List[Int] is the
        // value-semantics shape under test.
        Ty::Applied(TypeConstructorId::List, vec![Ty::Int])
    }
    fn bind(var: u32, ty: Ty, value: IrExpr) -> IrStmt {
        stmt(IrStmtKind::Bind {
            var: VarId(var),
            mutability: almide_ir::Mutability::Var,
            ty,
            value,
        })
    }
    /// Build a Unit-returning body block (avoids constructing a full IrFunction).
    fn body(stmts: Vec<IrStmt>) -> IrExpr {
        ir_expr(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
    }

    #[test]
    fn alias_then_cow_lowers_to_balanced_mir() {
        // var a = [1,2,3]; var b = a; a[0] = 9
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(1, list_int(), ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
            stmt(IrStmtKind::IndexAssign {
                target: VarId(0),
                index: ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int),
                value: ir_expr(IrExprKind::LitInt { value: 9 }, Ty::Int),
            }),
        ]);
        let mir = lower_body(&b, "main").expect("lowers");

        // Expect: Alloc(a=V0), Dup(b=V1 from V0), MakeUnique(a=V0), Drop, Drop.
        assert!(matches!(mir.ops[0], Op::Alloc { dst: ValueId(0), .. }));
        assert!(matches!(mir.ops[1], Op::Dup { dst: ValueId(1), src: ValueId(0) }));
        assert!(matches!(mir.ops[2], Op::MakeUnique { v: ValueId(0) }));
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(drops, 2, "two handles (a, b) → two scope-end drops");

        // The single ownership decision must be balanced by construction.
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn heap_return_is_a_balanced_move_out() {
        // fn build() -> List[Int] = { var a = [1,2,3]; a }
        // The tail `a` is a heap move-out: Alloc(+1), returned/consumed(−1), and
        // NOT dropped at scope end. Ownership witness `id` → balanced.
        let tail = ir_expr(IrExprKind::Var { id: VarId(0) }, list_int());
        let b = ir_expr(
            IrExprKind::Block {
                stmts: vec![bind(
                    0,
                    list_int(),
                    ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
                )],
                expr: Some(Box::new(tail)),
            },
            list_int(),
        );
        let mir = lower_body(&b, "build").expect("lowers");
        assert!(matches!(mir.ops[0], Op::Alloc { dst: ValueId(0), .. }));
        // moved out, so NO scope-end Drop of the returned handle.
        assert!(!mir.ops.iter().any(|o| matches!(o, Op::Drop { .. })));
        assert_eq!(mir.ret, Some(ValueId(0)));
        // The move-out balances the Alloc — the verifier accepts.
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn scalar_bind_needs_no_ownership() {
        // let n = 5
        let b = body(vec![bind(
            0,
            Ty::Int,
            ir_expr(IrExprKind::LitInt { value: 5 }, Ty::Int),
        )]);
        let mir = lower_body(&b, "main").expect("lowers");
        // An int literal materializes its real value (ConstInt) — still no ownership.
        assert_eq!(mir.ops, vec![Op::ConstInt { dst: ValueId(0), value: 5 }]);
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn fresh_heap_bind_allocs_and_drops() {
        // var s = "hi"
        let b = body(vec![bind(
            0,
            Ty::String,
            ir_expr(IrExprKind::LitStr { value: "hi".into() }, Ty::String),
        )]);
        let mir = lower_body(&b, "main").expect("lowers");
        assert!(matches!(mir.ops[0], Op::Alloc { .. }));
        assert!(matches!(mir.ops[1], Op::Drop { .. }));
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn out_of_subset_is_an_explicit_error_not_silent() {
        // A bare expression statement is outside this brick → explicit Unsupported.
        let b = body(vec![stmt(IrStmtKind::Expr {
            expr: ir_expr(IrExprKind::LitInt { value: 1 }, Ty::Int),
        })]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_is_rejected_at_repr() {
        assert!(matches!(repr_of(&Ty::Unknown), Err(LowerError::Unsupported(_))));
    }

    #[test]
    fn use_after_free_caught_if_decision_were_wrong() {
        // Sanity that the verifier guards the lowering: a hand-broken MIR with a
        // missing alias Dup would leave the alias' Drop unbalanced.
        let broken = MirFunction {
            name: "broken".into(),
            ops: vec![
                Op::Alloc { dst: ValueId(0), repr: Repr::Ptr { layout: PLACEHOLDER_LAYOUT }, init: Init::Opaque },
                Op::Drop { v: ValueId(0) },
                Op::Drop { v: ValueId(0) }, // second drop with no Dup → double free
            ],
            ..Default::default()
        };
        let errs = verify_ownership(&broken).unwrap_err();
        assert!(errs.iter().any(|e| e.kind == ViolationKind::DoubleFree));
    }

    // ── stdlib Module-call lowering (brick #47) ──

    fn module_call(module: &str, func: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
        use almide_lang::intern::sym;
        ir_expr(
            IrExprKind::Call {
                target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None },
                args,
                type_args: vec![],
            },
            ty,
        )
    }

    #[test]
    fn is_higher_order_detects_function_typed_args() {
        let fn_ty = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) };
        let plain = ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String);
        let closure = ir_expr(IrExprKind::Var { id: VarId(1) }, fn_ty);
        assert!(!is_higher_order(std::slice::from_ref(&plain)));
        assert!(is_higher_order(&[plain, closure]));
    }

    #[test]
    fn pure_first_order_module_call_lowers() {
        // var s = "x"; var t = string.trim(s)  — first-order + pure → admitted.
        let b = body(vec![
            bind(0, Ty::String, ir_expr(IrExprKind::LitStr { value: "x".into() }, Ty::String)),
            bind(
                1,
                Ty::String,
                module_call(
                    "string",
                    "trim",
                    vec![ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String)],
                    Ty::String,
                ),
            ),
        ]);
        let mir = lower_body(&b, "main").expect("pure first-order Module call lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "string.trim")),
            "expected an Op::CallFn named string.trim, got {:?}",
            mir.ops
        );
        // A fresh owned heap result, balanced by a scope-end drop.
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn higher_order_module_call_with_opaque_fn_value_is_walled() {
        // var ys = list.map(xs, f)  with f : (Int) -> Int an OPAQUE function value —
        // its capabilities are unanalyzable here, so it is walled (admitting it would
        // be accept-but-unsafe). (An analyzable Lambda closure IS admitted — see
        // `higher_order_module_call_with_lambda_captures_closure_caps`.)
        let fn_ty = Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) };
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(
                1,
                list_int(),
                module_call(
                    "list",
                    "map",
                    vec![
                        ir_expr(IrExprKind::Var { id: VarId(0) }, list_int()),
                        ir_expr(IrExprKind::Var { id: VarId(2) }, fn_ty),
                    ],
                    list_int(),
                ),
            ),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(m)) => {
                assert!(m.contains("opaque function-value"), "got: {m}")
            }
            other => panic!("expected an opaque-function-value wall, got {other:?}"),
        }
    }

    #[test]
    fn higher_order_module_call_with_lambda_captures_closure_caps() {
        use almide_lang::intern::sym;
        // var ys = list.map(xs, (x) => f(x))  — a higher-order PURE combinator with an
        // analyzable Lambda closure is ADMITTED: the closure body's call `f` is
        // captured as an effect marker (so its caps reach the witness), the closure is
        // deferred (no env materialized), and the result is a fresh owned list.
        let lambda = ir_expr(
            IrExprKind::Lambda {
                params: vec![(VarId(3), Ty::Int)],
                body: Box::new(ir_expr(
                    IrExprKind::Call {
                        target: CallTarget::Named { name: sym("f") },
                        args: vec![ir_expr(IrExprKind::Var { id: VarId(3) }, Ty::Int)],
                        type_args: vec![],
                    },
                    Ty::Int,
                )),
                lambda_id: None,
            },
            Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) },
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(
                1,
                list_int(),
                module_call(
                    "list",
                    "map",
                    vec![ir_expr(IrExprKind::Var { id: VarId(0) }, list_int()), lambda],
                    list_int(),
                ),
            ),
        ]);
        let all =
            lower_body_all(&b, "main").expect("higher-order pure combinator with a lambda lowers");
        let main = &all[0];
        // C1 DEFUNCTIONALIZATION: the INLINE lambda `(x) => f(x)` is now SPECIALIZED as a
        // loop INSIDE main — the body call `f` is a DIRECT `CallFn` in main (NOT in a lifted
        // `__lambda_*`, NOT behind a `CallIndirect`). This is strictly MORE sound for caps:
        // the fold sees `f`'s call edge DIRECTLY in main (no FuncRef-edge indirection, no
        // CallIndirect conservatism), so a printing/effectful `f` taints main honestly.
        assert!(
            main.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "f")),
            "the inlined closure body call `f` is a direct CallFn in main: {:?}",
            main.ops
        );
        // No lifted lambda and no FuncRef — the closure was defunctionalized away.
        assert!(
            !all[1..].iter().any(|fnc| fnc.name.starts_with("__lambda_")),
            "no __lambda_* aux is emitted (the lambda is inlined): {all:?}"
        );
        assert!(
            !main.ops.iter().any(|o| matches!(o, Op::FuncRef { .. } | Op::CallIndirect { .. })),
            "no FuncRef / CallIndirect — the closure is inlined, not lifted: {:?}",
            main.ops
        );
        // No `list.map` CallFn — the combinator itself is inlined as a loop (LoopStart),
        // and the result is a fresh OWNED `DynList` (the Alloc), dropped at scope end.
        assert!(
            main.ops.iter().any(|o| matches!(o, Op::LoopStart)),
            "the map is a real loop: {:?}",
            main.ops
        );
        assert!(
            !main.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "list.map")),
            "no `list.map` combinator call — it is inlined: {:?}",
            main.ops
        );
        for fnc in &all {
            assert_eq!(verify_ownership(fnc), Ok(()));
        }
    }

    #[test]
    fn effect_position_pure_combinator_captures_closure_caps() {
        use almide_lang::intern::sym;
        // list.each(xs, (x) => f(x))  as a STATEMENT — the side effect is the
        // closure's: `f` is captured as a marker, the Unit-result HOF carries no
        // ownership. (An effectful Module effect call still walls via the purity gate.)
        let lambda = ir_expr(
            IrExprKind::Lambda {
                params: vec![(VarId(3), Ty::Int)],
                body: Box::new(ir_expr(
                    IrExprKind::Call {
                        target: CallTarget::Named { name: sym("f") },
                        args: vec![ir_expr(IrExprKind::Var { id: VarId(3) }, Ty::Int)],
                        type_args: vec![],
                    },
                    Ty::Unit,
                )),
                lambda_id: None,
            },
            Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Unit) },
        );
        let each = module_call(
            "list",
            "each",
            vec![ir_expr(IrExprKind::Var { id: VarId(0) }, list_int()), lambda],
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: each }),
        ]);
        let all = lower_body_all(&b, "main").expect("effect-position combinator lowers");
        let main = &all[0];
        // The non-capturing closure LIFTS: its `f` call lives in the lifted lambda (caps
        // still reach the witness via the FuncRef edge), and main passes the slot to
        // `list.each` and binds it via FuncRef.
        assert!(
            all[1..]
                .iter()
                .any(|fnc| fnc.name.starts_with("__lambda_")
                    && fnc.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "f"))),
            "the closure body call `f` lives in the lifted lambda: {all:?}"
        );
        assert!(
            main.ops.iter().any(|o| matches!(o, Op::FuncRef { .. })),
            "main binds the lifted lambda via FuncRef: {:?}",
            main.ops
        );
        assert!(
            main.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "list.each")),
            "the Unit-result combinator is emitted",
        );
        for fnc in &all {
            assert_eq!(verify_ownership(fnc), Ok(()));
        }
    }

    #[test]
    fn effectful_module_call_is_walled() {
        // var x = fs.stat(p)  → walled. `fs` is effectful; `fs.stat` is NOT one of the
        // ADMITTED self-hosted effectful calls (random.int / env.args / fs.read_text /
        // fs.read_bytes — those charge a real capability into the transitive witness via
        // their prim floor), so its capability cannot be charged here and admitting it
        // would be accept-but-unsafe. (`fs.read_bytes` is deliberately admitted now, like
        // read_text — see `effectful_read_text_is_admitted`.)
        // A HEAP result so the value-call purity gate is the path that walls it.
        let b = body(vec![
            bind(0, Ty::String, ir_expr(IrExprKind::LitStr { value: "p".into() }, Ty::String)),
            bind(
                1,
                Ty::list(Ty::Int),
                module_call(
                    "http",
                    "serve",
                    vec![ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String)],
                    Ty::list(Ty::Int),
                ),
            ),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(m)) => assert!(m.contains("effectful/impure"), "got: {m}"),
            other => panic!("expected an effectful wall, got {other:?}"),
        }
    }

    #[test]
    fn effectful_read_text_is_admitted() {
        // var c = fs.read_text(p)  → ADMITTED (no longer walled). `fs.read_text` is a
        // self-hosted effectful call whose prim floor (`prim.read_text_file`) is linked into
        // the program, so its FsRead capability IS charged into the transitive cap_witness —
        // the `used ⊆ declared` checker then verifies an `effect fn` caller. It lowers to a
        // real `fs.read_text` CallFn returning a fresh OWNED `Result[String, String]` (the
        // cap-as-tag DynListStr, dropped by the scope-end DropListStr — no leak).
        let b = body(vec![
            bind(0, Ty::String, ir_expr(IrExprKind::LitStr { value: "p".into() }, Ty::String)),
            bind(
                1,
                Ty::String,
                module_call(
                    "fs",
                    "read_text",
                    vec![ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String)],
                    Ty::String,
                ),
            ),
        ]);
        let mir = lower_body(&b, "main").expect("fs.read_text is an admitted effectful call");
        assert!(
            mir.ops.iter().any(|op| matches!(op, crate::Op::CallFn { name, .. } if name == "fs.read_text")),
            "fs.read_text lowers to a real CallFn: {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()), "the owned Result is balanced by DropListStr");
    }

    #[test]
    fn nested_call_arg_materializes_into_owned_temp() {
        use almide_lang::intern::sym;
        // var x = outer(inner())  — inner()'s heap result is materialized into an
        // owned temp, borrowed into outer, and dropped at scope end; outer's result
        // is bound and dropped. Two CallFns emitted, in evaluation order.
        let named = |n: &str, args: Vec<IrExpr>| {
            ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym(n) },
                    args,
                    type_args: vec![],
                },
                list_int(),
            )
        };
        let b = body(vec![bind(0, list_int(), named("outer", vec![named("inner", vec![])]))]);
        let mir = lower_body(&b, "main").expect("nested call arg lowers");
        let callfns: Vec<&str> = mir
            .ops
            .iter()
            .filter_map(|o| match o {
                Op::CallFn { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(callfns, vec!["inner", "outer"], "inner materialized before outer");
        // The materialized temp + the outer result are both balanced (each `i`
        // matched by a scope-end `d`).
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn literal_call_arg_materializes_and_drops() {
        use almide_lang::intern::sym;
        // f("hello")  — the string literal argument is materialized via `Alloc`,
        // borrowed into the call, and dropped at scope end (cert `i` + `d`).
        let call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("f") },
                args: vec![ir_expr(IrExprKind::LitStr { value: "hello".into() }, Ty::String)],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: call })]);
        let mir = lower_body(&b, "main").expect("literal call arg lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::Alloc { .. })),
            "the literal is materialized via Alloc: {:?}",
            mir.ops
        );
        assert!(mir.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "f")));
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn elided_calls_in_an_opaque_value_emit_cert_neutral_effect_markers() {
        use almide_lang::intern::sym;
        let named = |n: &str, args: Vec<IrExpr>| {
            ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym(n) },
                    args,
                    type_args: vec![],
                },
                list_int(),
            )
        };
        // var xs = [helper(), other()]  — the list literal lowers to ONE Opaque
        // `Alloc`, ELIDING its element calls. `record_elided_calls` surfaces each as
        // a bare EFFECT MARKER `CallFn{dst:None, args:[], result:None}` so the caps
        // fold can see them, while the value content stays deferred.
        let elements = vec![named("helper", vec![]), named("other", vec![])];
        let b = body(vec![bind(0, list_int(), ir_expr(IrExprKind::List { elements }, list_int()))]);
        let mir = lower_body(&b, "main").expect("lowers");

        let markers: Vec<&str> = mir
            .ops
            .iter()
            .filter_map(|o| match o {
                Op::CallFn { dst: None, name, args, result: None } if args.is_empty() => {
                    Some(name.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(markers, vec!["helper", "other"], "one marker per elided call");

        // CERT-NEUTRAL: ownership is just the list Alloc (+1) and its scope-end Drop
        // (−1) — a marker injects no `+1`/drop. NAMES-NEUTRAL: a dst-less, arg-less
        // marker references nothing, so it cannot dangle.
        assert_eq!(verify_ownership(&mir), Ok(()));
        let cert = crate::certificate::ownership_certificate(&mir);
        assert_eq!(cert.matches('i').count(), 1, "only the list Alloc is a +1, not the markers");
        let nw = crate::certificate::name_witness(&mir);
        assert!(nw.used.iter().all(|u| nw.defined.contains(u)), "no dangling MIR reference");

        // A HIGHER-ORDER call is SKIPPED (unmodelled closure caps): no marker, so the
        // `ir_calls > mir_calls` gate keeps such a function honestly tainted.
        let fn_ty = Ty::Fn { params: vec![], ret: Box::new(Ty::Int) };
        let ho = body(vec![bind(
            1,
            list_int(),
            ir_expr(
                IrExprKind::List {
                    elements: vec![named(
                        "apply",
                        vec![ir_expr(IrExprKind::Var { id: VarId(2) }, fn_ty)],
                    )],
                },
                list_int(),
            ),
        )]);
        let mir2 = lower_body(&ho, "main").expect("lowers");
        assert!(
            !mir2.ops.iter().any(|o| matches!(o, Op::CallFn { dst: None, .. })),
            "a higher-order call is not recorded as a marker"
        );
    }

    fn bool_var() -> IrExpr {
        ir_expr(IrExprKind::Var { id: VarId(5) }, Ty::Bool)
    }
    fn unit_block(stmts: Vec<IrStmt>) -> IrExpr {
        ir_expr(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
    }
    fn iff(then: IrExpr, els: IrExpr, ty: Ty) -> IrExpr {
        ir_expr(
            IrExprKind::If { cond: Box::new(bool_var()), then: Box::new(then), else_: Box::new(els) },
            ty,
        )
    }

    #[test]
    fn for_in_heap_element_aliases_container_per_iteration() {
        use almide_lang::intern::sym;
        // var xs = []; for s in xs { println(s) }  — the heap loop var `s` aliases the
        // whole container `xs` (Op::Dup) for the iteration and is dropped at iteration
        // end; the println borrows it. Per-iteration frame balanced.
        let prn_s = stmt(IrStmtKind::Expr {
            expr: ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym("println") },
                    args: vec![ir_expr(IrExprKind::Var { id: VarId(1) }, Ty::String)],
                    type_args: vec![],
                },
                Ty::Unit,
            ),
        });
        let forin = ir_expr(
            IrExprKind::ForIn {
                var: VarId(1),
                var_tuple: None,
                iterable: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
                body: vec![prn_s],
            },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: forin }),
        ]);
        let mir = lower_body(&b, "main").expect("for-in lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::Dup { src: ValueId(0), .. })),
            "loop var aliases the container: {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()), "per-iteration frame balanced");
    }

    #[test]
    fn computed_effect_call_is_deferred_and_tainted() {
        // var g = (x) => …; (g)()  — a Computed effect call (closure-VALUE callee) in
        // statement position. Deferred like a Computed value call: no nameable CallFn is
        // emitted (the call is elided), so it lowers (memory-safe, no ownership op for a
        // Unit result) and the ir_calls>mir_calls gate taints it caps-unverified.
        let g_ref = ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::Unit);
        let computed = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(g_ref) },
                args: vec![],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: computed })]);
        let mir = lower_body(&b, "main").expect("a Computed effect call is deferred, not walled");
        // No nameable CallFn for the computed callee (it is elided → caps taint).
        assert!(
            !mir.ops.iter().any(|o| matches!(o, Op::CallFn { .. } | Op::Call { .. })),
            "the computed call is elided (no marker): {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn heap_bind_from_block_lowers() {
        // var x = { var a = [1]; a }  — a heap BLOCK value: lower the block's stmts, then
        // bind x to the heap tail (here the block-local `a`, aliased via Dup). Balanced.
        let blk = ir_expr(
            IrExprKind::Block {
                stmts: vec![bind(1, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int()))],
                expr: Some(Box::new(ir_expr(IrExprKind::Var { id: VarId(1) }, list_int()))),
            },
            list_int(),
        );
        let b = body(vec![bind(0, list_int(), blk)]);
        let mir = lower_body(&b, "main").expect("heap bind from a block lowers");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn map_insert_is_a_place_mutation() {
        // var m = []; m[k] = v  — map insertion is in-place: MakeUnique (copy-on-write).
        let mi = stmt(IrStmtKind::MapInsert {
            target: VarId(0),
            key: ir_expr(IrExprKind::LitStr { value: "b".into() }, Ty::String),
            value: ir_expr(IrExprKind::LitInt { value: 2 }, Ty::Int),
        });
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            mi,
        ]);
        let mir = lower_body(&b, "main").expect("map insert lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::MakeUnique { .. })), "map insert is MakeUnique: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn nested_tuple_destructure_recurses() {
        // let (a, (b, c)) = (1, (2, "x"))  — the nested sub-pattern (b, c) binds against
        // the nested tuple literal (2, "x") component-wise (recursion). String leaf = heap.
        let inner_pat = IrPattern::Tuple {
            elements: vec![
                IrPattern::Bind { var: VarId(1), ty: Ty::Int },
                IrPattern::Bind { var: VarId(2), ty: Ty::String },
            ],
        };
        let pat = IrPattern::Tuple {
            elements: vec![IrPattern::Bind { var: VarId(0), ty: Ty::Int }, inner_pat],
        };
        let inner_val = ir_expr(
            IrExprKind::Tuple {
                elements: vec![
                    ir_expr(IrExprKind::LitInt { value: 2 }, Ty::Int),
                    ir_expr(IrExprKind::LitStr { value: "x".into() }, Ty::String),
                ],
            },
            Ty::Unit,
        );
        let val = ir_expr(
            IrExprKind::Tuple {
                elements: vec![ir_expr(IrExprKind::LitInt { value: 1 }, Ty::Int), inner_val],
            },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::BindDestructure { pattern: pat, value: val })]);
        let mir = lower_body(&b, "main").expect("nested tuple destructure lowers");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn scalar_global_reference_is_a_const() {
        use almide_lang::intern::sym;
        // A function references a top-level SCALAR `let` global (Int) it never binds
        // locally. value_or_global admits it from the DECLARED global set as a Copy
        // `Const` — a real value, not a deferred heap object.
        let mut globals = HashMap::new();
        globals.insert(VarId(7), Ty::Int);
        let call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("f") },
                args: vec![ir_expr(IrExprKind::Var { id: VarId(7) }, Ty::Int)],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: call })]);
        let mir = lower_body_with_globals(&b, "main", globals).expect("scalar global ref admitted");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::Const { .. })),
            "scalar global is a Const: {:?}",
            mir.ops
        );
    }

    #[test]
    fn heap_global_reference_is_walled_not_an_empty_opaque() {
        use almide_lang::intern::sym;
        // A reference to a HEAP module-level global (List) USED TO bind a fresh owned
        // `Alloc{Opaque}` — an EMPTY heap value. Observing it (here: passing it to `f`)
        // emitted empty bytes = a silent miscompile. value_or_global now REJECTS a heap
        // global reference explicitly so the function walls cleanly.
        let mut globals = HashMap::new();
        globals.insert(VarId(8), list_int());
        let call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("f") },
                args: vec![ir_expr(IrExprKind::Var { id: VarId(8) }, list_int())],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: call })]);
        match lower_body_with_globals(&b, "main", globals) {
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("expected an explicit heap-global reject, got: {other:?}"),
        }
    }

    #[test]
    fn unbound_non_global_var_still_walls() {
        use almide_lang::intern::sym;
        // The DISCIPLINE: a value_of miss that is NOT in the declared global set is a
        // genuine lowering gap and must still WALL — never silently absorbed as a "global".
        let call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("f") },
                args: vec![ir_expr(IrExprKind::Var { id: VarId(99) }, Ty::Int)],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: call })]);
        match lower_body_with_globals(&b, "main", HashMap::new()) {
            Err(LowerError::Unsupported(m)) => assert!(m.contains("unbound var"), "got: {m}"),
            other => panic!("expected an unbound-var wall (a real gap), got {other:?}"),
        }
    }

    #[test]
    fn while_with_scalar_counter_reassign_lowers() {
        // var i = 0; while c { i = 5 }  — a SCALAR reassign is a Copy `Const` (no
        // handle), admitted; the body has no heap, so the loop lowers balanced.
        let inc = stmt(IrStmtKind::Assign {
            var: VarId(0),
            value: ir_expr(IrExprKind::LitInt { value: 5 }, Ty::Int),
        });
        let w = ir_expr(
            IrExprKind::While { cond: Box::new(bool_var()), body: vec![inc] },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, Ty::Int, ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int)),
            stmt(IrStmtKind::Expr { expr: w }),
        ]);
        let mir = lower_body(&b, "main").expect("while with scalar reassign lowers");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn scalar_frame_break_walls() {
        // while c { break }  — the model-one-iteration `while` fallback runs the body once
        // with no early-exit branch, so the `break` is silently dropped: a `while i<100 { if
        // i==7 then break; i=i+1 }` would print `1` (one iteration), v0 prints `7`. The frame
        // holds no heap handle (so it is leak-safe), but the SELECTION is still wrong — WALL it
        // (the discipline fix), since a faithful break needs the real-loop markers.
        let w = ir_expr(
            IrExprKind::While {
                cond: Box::new(bool_var()),
                body: vec![stmt(IrStmtKind::Expr { expr: ir_expr(IrExprKind::Break, Ty::Unit) })],
            },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: w })]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("break/continue"), "got: {r}"),
            other => panic!("expected a break/continue wall, got {other:?}"),
        }
    }

    // ── ADT brick 1: build_variant_layouts — the variant value-model registry ──────────

    /// `type Shape = Circle(Float) | Rect { w: Float, h: Float } | Dot` exercises all three
    /// constructor shapes (tuple / record / unit) in one decl. The registry must record:
    /// tags in declaration order; tuple fields synthesised as `_0`, `_1`, … and record
    /// fields by declared name (both matching v0's emit_wasm registration); a `slot_count`
    /// padded to `1 (tag) + widest arity` so every constructor shares one block size; and a
    /// ctor-name → type reverse index for `lookup_ctor`.
    #[test]
    fn build_variant_layouts_registers_tags_fields_and_slot_count() {
        use almide_lang::intern::sym;
        let f = |name: &str, ty: Ty| IrFieldDecl {
            name: sym(name),
            ty,
            default: None,
            alias: None,
            attrs: vec![],
        };
        let decl = IrTypeDecl {
            name: "Shape".into(),
            kind: IrTypeDeclKind::Variant {
                cases: vec![
                    IrVariantDecl {
                        name: "Circle".into(),
                        kind: IrVariantKind::Tuple { fields: vec![Ty::Float] },
                    },
                    IrVariantDecl {
                        name: "Rect".into(),
                        kind: IrVariantKind::Record {
                            fields: vec![f("w", Ty::Float), f("h", Ty::Float)],
                        },
                    },
                    IrVariantDecl { name: "Dot".into(), kind: IrVariantKind::Unit },
                ],
                is_generic: false,
                boxed_args: HashSet::new(),
                boxed_record_fields: HashSet::new(),
            },
            deriving: None,
            generics: None,
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
        };

        let vl = build_variant_layouts(&[decl]);
        let layout = vl.by_type.get("Shape").expect("Shape registered");

        // slot 0 = tag, slots 1.. = the widest constructor (Rect, arity 2) → 1 + 2 = 3.
        assert_eq!(layout.slot_count, 3);
        assert!(layout.generics.is_empty());
        assert_eq!(layout.cases.len(), 3);

        // Tuple ctor: tag 0, positional field named `_0`.
        assert_eq!(layout.cases[0].ctor.as_str(), "Circle");
        assert_eq!(layout.cases[0].tag, 0);
        assert_eq!(layout.cases[0].fields.len(), 1);
        assert_eq!(layout.cases[0].fields[0].0.as_str(), "_0");
        assert_eq!(layout.cases[0].fields[0].1, Ty::Float);

        // Record ctor: tag 1, declared field names preserved in order.
        assert_eq!(layout.cases[1].ctor.as_str(), "Rect");
        assert_eq!(layout.cases[1].tag, 1);
        let rect_names: Vec<&str> =
            layout.cases[1].fields.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(rect_names, ["w", "h"]);

        // Unit ctor: tag 2, no fields.
        assert_eq!(layout.cases[2].ctor.as_str(), "Dot");
        assert_eq!(layout.cases[2].tag, 2);
        assert!(layout.cases[2].fields.is_empty());

        // Reverse index: every ctor resolves to its owning type.
        for ctor in ["Circle", "Rect", "Dot"] {
            assert_eq!(vl.ctor_to_type.get(ctor).map(String::as_str), Some("Shape"));
        }

        // lookup_ctor ties the reverse index to the case by name.
        let (ty_name, _layout, case) = vl.lookup_ctor("Rect").expect("Rect resolves");
        assert_eq!(ty_name, "Shape");
        assert_eq!(case.tag, 1);
        assert!(vl.lookup_ctor("Nope").is_none());
    }

