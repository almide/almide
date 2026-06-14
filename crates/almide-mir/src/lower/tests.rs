//! Unit tests for Core-IR → MIR lowering (extracted from lower.rs).
#![allow(clippy::all)]

    use super::*;
    use crate::{verify_ownership, ViolationKind};
    use almide_lang::types::constructor::TypeConstructorId;

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
        assert_eq!(mir.ops, vec![Op::Const { dst: ValueId(0) }]);
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
        let mir = lower_body(&b, "main").expect("higher-order pure combinator with a lambda lowers");
        // The closure body's call `f` is captured as a marker; the HOF result is a
        // fresh owned list (CallFn `list.map`).
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { dst: None, name, .. } if name == "f")),
            "closure body call captured as a marker: {:?}",
            mir.ops
        );
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { dst: Some(_), name, .. } if name == "list.map")),
            "the HOF result is a fresh owned value",
        );
        assert_eq!(verify_ownership(&mir), Ok(()));
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
        let mir = lower_body(&b, "main").expect("effect-position combinator lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { dst: None, name, .. } if name == "f")),
            "closure body call captured as a marker: {:?}",
            mir.ops
        );
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "list.each")),
            "the Unit-result combinator is emitted",
        );
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn effectful_module_call_is_walled() {
        // var x = fs.read_text(p)  → walled (fs is effectful; its capability cannot
        // yet be charged into the witness, so admitting it would be accept-but-unsafe).
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
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(m)) => assert!(m.contains("effectful/impure"), "got: {m}"),
            other => panic!("expected an effectful wall, got {other:?}"),
        }
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
    fn loop_with_break_is_walled() {
        // while c { break }  — the early-exit path would skip the per-iteration frame's
        // drops (a leak). Must WALL.
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

    #[test]
    fn loop_body_heap_reassign_is_walled() {
        // var acc = []; while c { acc = [] }  — a HEAP reassign of a pre-loop var is an
        // iteration-dependent value_of rebind (→ UAF). Must WALL. (A scalar reassign
        // would be admitted — see `while_with_scalar_counter_reassign_lowers`.)
        let reassign = stmt(IrStmtKind::Assign {
            var: VarId(0),
            value: ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
        });
        let w = ir_expr(
            IrExprKind::While { cond: Box::new(bool_var()), body: vec![reassign] },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: w }),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("reassigns a heap"), "got: {r}"),
            other => panic!("expected a heap-reassign wall, got {other:?}"),
        }
    }

    #[test]
    fn unit_if_with_effect_arms_linearizes_balanced() {
        use almide_lang::intern::sym;
        // if c then println("a") else println("b")  — each arm is a Unit effect call;
        // its string arg is materialized into an arm-local temp and dropped by the
        // per-arm frame. BOTH printlns lower (caps union); ownership balanced.
        let prn = |s: &str| {
            ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym("println") },
                    args: vec![ir_expr(IrExprKind::LitStr { value: s.into() }, Ty::String)],
                    type_args: vec![],
                },
                Ty::Unit,
            )
        };
        let b = body(vec![stmt(IrStmtKind::Expr { expr: iff(prn("a"), prn("b"), Ty::Unit) })]);
        let mir = lower_body(&b, "main").expect("unit if lowers");
        let prints = mir.ops.iter().filter(|o| matches!(o, Op::Call { .. })).count();
        assert_eq!(prints, 2, "both arms' println are lowered (caps union, not Const-skipped)");
        assert_eq!(verify_ownership(&mir), Ok(()));
        let allocs = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(allocs, drops, "every arm-local alloc has its per-arm drop (balanced)");
    }

    #[test]
    fn if_arm_local_alloc_is_dropped_within_the_arm() {
        // if c then { var w = [1,2,3] } else { }  — w is an arm-local heap value,
        // dropped by the per-arm frame (vacuous on the else path). Cert balanced.
        let then = unit_block(vec![bind(
            0,
            list_int(),
            ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
        )]);
        let b = body(vec![stmt(IrStmtKind::Expr {
            expr: iff(then, unit_block(vec![]), Ty::Unit),
        })]);
        let mir = lower_body(&b, "main").expect("lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Alloc { .. })), "arm-local alloc");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Drop { .. })), "arm-local drop");
        assert_eq!(verify_ownership(&mir), Ok(()), "arm balanced by construction");
    }

    #[test]
    fn scalar_if_tail_linearizes_arms_and_const_merges() {
        // fn f() = if c then 1 else 2  — arms lowered (for caps), result is ONE Const.
        let b = ir_expr(
            IrExprKind::Block {
                stmts: vec![],
                expr: Some(Box::new(iff(
                    ir_expr(IrExprKind::LitInt { value: 1 }, Ty::Int),
                    ir_expr(IrExprKind::LitInt { value: 2 }, Ty::Int),
                    Ty::Int,
                ))),
            },
            Ty::Int,
        );
        let mir = lower_body(&b, "f").expect("scalar if tail lowers");
        assert!(matches!(mir.ops.last(), Some(Op::Const { .. })), "merged scalar result is a Const");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn heap_result_if_yields_one_fresh_opaque_merged_slot() {
        use almide_lang::intern::sym;
        // fn f() = if c then make() else [9]  — a HEAP-result branch. Arms are
        // linearized (each per-arm balanced; the make() call's caps captured, its
        // value deferred), and the result is ONE fresh `Alloc{Opaque}` MOVED OUT (the
        // merged slot) — never per-arm phi-merged. Balanced + moved out by the cert.
        let then = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("make") },
                args: vec![],
                type_args: vec![],
            },
            list_int(),
        );
        let els = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let b = ir_expr(
            IrExprKind::Block { stmts: vec![], expr: Some(Box::new(iff(then, els, list_int()))) },
            list_int(),
        );
        let mir = lower_body(&b, "f").expect("heap if tail lowers");
        // The make() call's caps are captured as an effect marker (deferred value).
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { dst: None, name, .. } if name == "make")),
            "the arm call's caps are captured: {:?}",
            mir.ops
        );
        // The merged result is the LAST op: a fresh Opaque Alloc, MOVED OUT (returned,
        // so NOT dropped at scope end).
        assert!(matches!(mir.ops.last(), Some(Op::Alloc { init: Init::Opaque, .. })));
        assert!(mir.ret.is_some(), "the fresh merged result is the return value");
        assert!(!mir.ops.iter().any(|o| matches!(o, Op::Drop { .. })), "moved out, not dropped");
        assert_eq!(verify_ownership(&mir), Ok(()), "fresh result + balanced arms");
    }

    #[test]
    fn heap_result_if_bind_drops_the_merged_slot_at_scope_end() {
        // var x = if c then [1] else [2]  (Unit body) — the merged fresh Opaque is
        // BOUND and dropped at scope end (cert i + d, balanced).
        let then = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let els = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let b = body(vec![bind(0, list_int(), iff(then, els, list_int()))]);
        let mir = lower_body(&b, "main").expect("heap if bind lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Alloc { init: Init::Opaque, .. })));
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Drop { .. })), "bound slot dropped at scope end");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn branch_arm_reassigning_a_variable_is_walled() {
        // var z = []; if c then { z = [9] } else { }  — the arm reassigns pre-branch z
        // (a path-dependent value_of rebind → UAF the flat fold can't see). Must WALL.
        let then = unit_block(vec![stmt(IrStmtKind::Assign {
            var: VarId(0),
            value: ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
        })]);
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: iff(then, unit_block(vec![]), Ty::Unit) }),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("reassigns"), "got: {r}"),
            other => panic!("expected a reassign wall, got {other:?}"),
        }
    }

    #[test]
    fn match_arm_heap_payload_binding_aliases_the_subject() {
        use almide_lang::intern::sym;
        // var opt = make(); match opt { Some(x) => use(x), None => () }  — the heap
        // payload `x` aliases the WHOLE subject (Op::Dup, container-grain), dropped at
        // arm end; `None` binds nothing. Balanced.
        let arm_some = almide_ir::IrMatchArm {
            pattern: IrPattern::Some {
                inner: Box::new(IrPattern::Bind { var: VarId(1), ty: Ty::String }),
            },
            guard: None,
            body: ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym("use") },
                    args: vec![ir_expr(IrExprKind::Var { id: VarId(1) }, Ty::String)],
                    type_args: vec![],
                },
                Ty::Unit,
            ),
        };
        let arm_none = almide_ir::IrMatchArm {
            pattern: IrPattern::None,
            guard: None,
            body: ir_expr(IrExprKind::Unit, Ty::Unit),
        };
        let m = ir_expr(
            IrExprKind::Match {
                subject: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
                arms: vec![arm_some, arm_none],
            },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: m }),
        ]);
        let mir = lower_body(&b, "main").expect("payload-binding match lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::Dup { src: ValueId(0), .. })),
            "the heap payload aliases the subject (container-grain): {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()), "the payload Dup is balanced by an arm-end drop");
    }

    #[test]
    fn match_record_shorthand_pattern_is_walled() {
        // match r { { name } => () }  — a record shorthand field has no bound VarId to
        // thread, so it stays walled (totality, no silent miscompile).
        let arm = almide_ir::IrMatchArm {
            pattern: IrPattern::RecordPattern {
                name: "R".into(),
                fields: vec![almide_ir::IrFieldPattern { name: "name".into(), pattern: None }],
                rest: false,
            },
            guard: None,
            body: ir_expr(IrExprKind::Unit, Ty::Unit),
        };
        let m = ir_expr(
            IrExprKind::Match {
                subject: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
                arms: vec![arm],
            },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: m }),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("shorthand"), "got: {r}"),
            other => panic!("expected a record-shorthand wall, got {other:?}"),
        }
    }

    #[test]
    fn constructor_destructure_binds_container_grain() {
        // var r = make(); let Foo(a) = r  — the heap field `a` aliases the whole
        // subject `r` (Op::Dup, container-grain), dropped at scope end. Balanced.
        let pattern = IrPattern::Constructor {
            name: "Foo".into(),
            args: vec![IrPattern::Bind { var: VarId(1), ty: Ty::String }],
        };
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::BindDestructure {
                pattern,
                value: ir_expr(IrExprKind::Var { id: VarId(0) }, list_int()),
            }),
        ]);
        let mir = lower_body(&b, "main").expect("constructor destructure lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::Dup { src: ValueId(0), .. })),
            "the heap field aliases the subject container: {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn error_operators_yield_a_fresh_value() {
        // var opt = []; var x = opt!  and  fn g() = opt ?? []  — an error operator
        // yields a FRESH value (Opaque heap here), the operand deferred and the
        // early-return of `!`/`?` deferred (always-continue is balanced). Both lower.
        let unwrap = ir_expr(
            IrExprKind::Unwrap {
                expr: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
            },
            list_int(),
        );
        let unwrap_or = ir_expr(
            IrExprKind::UnwrapOr {
                expr: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
                fallback: Box::new(ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            },
            list_int(),
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(1, list_int(), unwrap),
            bind(2, list_int(), unwrap_or),
        ]);
        let mir = lower_body(&b, "main").expect("error operators lower");
        let opaque = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { init: Init::Opaque, .. })).count();
        assert_eq!(opaque, 2, "each of the two error operators is a fresh Opaque: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()), "fresh results balanced by scope-end drops");
    }

    #[test]
    fn match_on_a_fresh_heap_subject_is_materialized_and_dropped() {
        use almide_lang::intern::sym;
        // match make() { _ => () }  — the fresh heap subject is MATERIALIZED into an
        // owned temp (CallFn `make`) dropped at scope end (never leaked); the arms
        // inspect it. Balanced.
        let subject = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("make") },
                args: vec![],
                type_args: vec![],
            },
            list_int(),
        );
        let arm = almide_ir::IrMatchArm {
            pattern: IrPattern::Wildcard,
            guard: None,
            body: ir_expr(IrExprKind::Unit, Ty::Unit),
        };
        let m = ir_expr(
            IrExprKind::Match { subject: Box::new(subject), arms: vec![arm] },
            Ty::Unit,
        );
        let b = body(vec![stmt(IrStmtKind::Expr { expr: m })]);
        let mir = lower_body(&b, "main").expect("fresh heap subject is materialized");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { dst: Some(_), name, .. } if name == "make")),
            "the subject is materialized into an owned temp: {:?}",
            mir.ops
        );
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Drop { .. })), "the subject temp is dropped");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn option_result_constructor_lowers_like_a_literal() {
        // var x = Some("hi")  — a heap Option variant is materialized via `Alloc`
        // (value semantics: the payload is copied, the shell owned + dropped),
        // exactly like a container literal. `list_int()` stands in as a heap type;
        // the lowering keys on the expression KIND + `is_heap_ty`, not the payload.
        let some = ir_expr(
            IrExprKind::OptionSome {
                expr: Box::new(ir_expr(IrExprKind::LitStr { value: "hi".into() }, Ty::String)),
            },
            list_int(),
        );
        let b = body(vec![bind(0, list_int(), some)]);
        let mir = lower_body(&b, "main").expect("Option constructor lowers");
        assert!(
            matches!(mir.ops[0], Op::Alloc { .. }),
            "the constructor is materialized via Alloc: {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn binop_value_materializes_scalar_const_and_heap_alloc() {
        use almide_ir::BinOp;
        use almide_lang::intern::sym;
        let binop = |op, ty, l: IrExpr, r: IrExpr| {
            ir_expr(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) }, ty)
        };
        let v = |id| ir_expr(IrExprKind::Var { id: VarId(id) }, Ty::Int);
        // f(a + b)  — a scalar BinOp argument is a fresh `Const` (no ownership).
        let scalar = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("f") },
                args: vec![binop(BinOp::AddInt, Ty::Int, v(0), v(1))],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let mir = lower_body(&body(vec![stmt(IrStmtKind::Expr { expr: scalar })]), "main")
            .expect("scalar BinOp arg lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Const { .. })), "scalar BinOp is Const: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()));

        // var s = a ++ b  — a heap (string-concat) BinOp is a fresh `Alloc`, dropped.
        let sv = |id| ir_expr(IrExprKind::Var { id: VarId(id) }, Ty::String);
        let concat = binop(BinOp::ConcatStr, Ty::String, sv(0), sv(1));
        let mir2 = lower_body(&body(vec![bind(2, Ty::String, concat)]), "main")
            .expect("heap concat bind lowers");
        assert!(matches!(mir2.ops[0], Op::Alloc { .. }), "heap BinOp is Alloc: {:?}", mir2.ops);
        assert_eq!(verify_ownership(&mir2), Ok(()));
    }

    #[test]
    fn scalar_extraction_is_const_heap_extraction_aliases_container() {
        use almide_lang::intern::sym;
        let idx = |obj: IrExpr, ty: Ty| {
            ir_expr(
                IrExprKind::IndexAccess {
                    object: Box::new(obj),
                    index: Box::new(ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int)),
                },
                ty,
            )
        };
        let c = || ir_expr(IrExprKind::Var { id: VarId(0) }, list_int());

        // fn f() = xs[i]  with a SCALAR element type → a `Const` copy (no ownership).
        let scalar = idx(c(), Ty::Int);
        let mir = lower_body(
            &ir_expr(IrExprKind::Block { stmts: vec![], expr: Some(Box::new(scalar)) }, Ty::Int),
            "main",
        )
        .expect("scalar extraction lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::Const { .. })), "scalar extraction is Const: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()));

        // var xs = [..]; f(xs[0])  with a HEAP element → ALIAS the container (Op::Dup),
        // borrowed into the call and dropped at scope end (cert `a` + `d`).
        let heap_call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("f") },
                args: vec![idx(c(), Ty::String)],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: heap_call }),
        ]);
        let mir2 = lower_body(&b, "main").expect("heap extraction aliases the container");
        assert!(mir2.ops.iter().any(|o| matches!(o, Op::Dup { .. })), "heap extraction is a container Dup: {:?}", mir2.ops);
        assert_eq!(verify_ownership(&mir2), Ok(()));

        // A NESTED-container extraction (the immediate container is itself an
        // extraction, not a tracked var) stays walled — there is no `src` to Dup.
        let nested = ir_expr(
            IrExprKind::Member { object: Box::new(idx(c(), list_int())), field: sym("x") },
            Ty::String,
        );
        let nested_call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("g") },
                args: vec![nested],
                type_args: vec![],
            },
            Ty::Unit,
        );
        let b2 = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: nested_call }),
        ]);
        match lower_body(&b2, "main") {
            Err(LowerError::Unsupported(m)) => assert!(m.contains("not a tracked heap var"), "got: {m}"),
            other => panic!("expected a nested-container wall, got {other:?}"),
        }
    }

    #[test]
    fn reassignment_rebinds_and_old_rides_to_scope_end() {
        use almide_lang::intern::sym;
        // var x = [..]; x = [..]  — old + new both allocated, both dropped (the old
        // rides to scope-end, dropped exactly once; never a double-free).
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Assign {
                var: VarId(0),
                value: ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
            }),
        ]);
        let mir = lower_body(&b, "main").expect("reassignment lowers");
        let allocs = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(allocs, 2, "old + new both allocated: {:?}", mir.ops);
        assert_eq!(drops, 2, "old + new both dropped: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()));

        // var x = [..]; x = f(x)  — reading the old x in the new value borrows the
        // still-live old handle (the read lowers before the rebind), NOT a UAF.
        let b2 = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Assign {
                var: VarId(0),
                value: ir_expr(
                    IrExprKind::Call {
                        target: CallTarget::Named { name: sym("f") },
                        args: vec![ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())],
                        type_args: vec![],
                    },
                    list_int(),
                ),
            }),
        ]);
        let mir2 = lower_body(&b2, "main").expect("reassign reading old x lowers");
        assert_eq!(verify_ownership(&mir2), Ok(()), "no UAF reading old x: {:?}", mir2.ops);
    }

    #[test]
    fn tuple_destructure_aliases_components() {
        let heap_binds = || {
            IrPattern::Tuple {
                elements: vec![
                    IrPattern::Bind { var: VarId(2), ty: list_int() },
                    IrPattern::Bind { var: VarId(3), ty: list_int() },
                ],
            }
        };
        // var x; var y; let (a, b) = (x, y)  — component-wise: a aliases x, b aliases y.
        let tup_lit = ir_expr(
            IrExprKind::Tuple {
                elements: vec![
                    ir_expr(IrExprKind::Var { id: VarId(0) }, list_int()),
                    ir_expr(IrExprKind::Var { id: VarId(1) }, list_int()),
                ],
            },
            list_int(),
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(1, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::BindDestructure { pattern: heap_binds(), value: tup_lit }),
        ]);
        let mir = lower_body(&b, "main").expect("tuple-literal destructure lowers");
        assert_eq!(
            mir.ops.iter().filter(|o| matches!(o, Op::Dup { .. })).count(),
            2,
            "a aliases x, b aliases y: {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()));

        // var t; let (a, b) = t  — each heap component aliases the container t.
        let b2 = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::BindDestructure {
                pattern: heap_binds(),
                value: ir_expr(IrExprKind::Var { id: VarId(0) }, list_int()),
            }),
        ]);
        let mir2 = lower_body(&b2, "main").expect("container-var destructure lowers");
        assert_eq!(
            mir2.ops.iter().filter(|o| matches!(o, Op::Dup { .. })).count(),
            2,
            "both components alias t: {:?}",
            mir2.ops
        );
        assert_eq!(verify_ownership(&mir2), Ok(()));
    }
