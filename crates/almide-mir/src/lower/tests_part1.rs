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
            // An UNBOUND Fn-typed Var (no creation site — VarId(2) was never bound)
            // has no closure block to pass and no crea­tion-site caps fold: it walls
            // with the unresolved-function-value reason (a RESOLVED fn-value var now
            // passes by handle — the first-class-fn-to-HOF opening).
            Err(LowerError::Unsupported(m)) => {
                assert!(m.contains("function-value"), "got: {m}")
            }
            other => panic!("expected a function-value wall, got {other:?}"),
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
