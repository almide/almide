
    #[test]
    fn match_arm_heap_payload_binding_aliases_the_subject() {
        use almide_lang::intern::sym;
        // var opt = make(); match opt { Some(x) => use(x), None => () } — an UNTRACKED
        // subject with a CALL-bearing arm. This used to LINEARIZE (both arms run, the
        // heap payload aliasing the subject container-grain): caps/ownership-sound but
        // an OUTPUT miscompile — `use(x)` ran even on the None path (the porta
        // read_message `method=` garbage class, 2026-07-03). The linearization is now
        // gated to effect-free arms, so this shape WALLS cleanly instead.
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
        let err = lower_body(&b, "main").expect_err("call-bearing arm over an untracked subject walls");
        assert!(
            matches!(&err, LowerError::Unsupported(m) if m.contains("linearization")),
            "walls with the linearization guard, not a silent both-arms run: {err:?}"
        );
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
    fn error_operators_with_no_live_heap_local_lower() {
        // var x = []!  and  var y = []  ?? []  with NO prior live heap local — an error
        // operator yields a FRESH Opaque, the operand deferred. `!`/`?` early-return is
        // safe to defer here: with no live heap handle there is no Drop the wasm Err path
        // could skip = no leak. UnwrapOr (`??`) never early-returns regardless.
        let unwrap = ir_expr(
            IrExprKind::Unwrap {
                expr: Box::new(ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            },
            list_int(),
        );
        let unwrap_or = ir_expr(
            IrExprKind::UnwrapOr {
                expr: Box::new(ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
                fallback: Box::new(ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            },
            list_int(),
        );
        // `var x = []!` bound to a let/var — the deferred lowering was memory-safe (balanced)
        // but its VALUE was a deferred Const/Opaque = a SILENT MISCOMPILE. The faithful
        // early-return lowering has since LANDED: the monadic `!` desugar rewrites
        // `let x = e!; rest` into the err-propagating match continuation, so this now
        // either lowers balanced or walls inside the match machinery — never the old
        // silently-wrong bind. Accept both honest outcomes for this synthetic (not
        // even well-typed) `[]!`; the real shape is pinned by `effect_assign_unwrap`
        // in the parity baseline. `var y = [] ?? []` (UnwrapOr) still lowers.
        let b = body(vec![bind(0, list_int(), unwrap)]);
        if let Ok(mir) = lower_body(&b, "main") {
            assert_eq!(verify_ownership(&mir), Ok(()), "if it lowers it must be balanced");
        }
        let b2 = body(vec![bind(1, list_int(), unwrap_or)]);
        let mir = lower_body(&b2, "main").expect("UnwrapOr still lowers");
        assert_eq!(verify_ownership(&mir), Ok(()), "fresh results balanced by scope-end drops");
    }

    #[test]
    fn unwrap_over_a_live_heap_local_lowers() {
        // var opt = []; var x = opt!  — `opt` is a LIVE owned heap local when the `!`
        // early-returns. This used to WALL (the v0 wasm Err path leaked `opt`); the v0
        // codegen now frees live heap locals before the Err `return_`
        // (emit_wasm::emit_early_return_decs), so the deferred-continue cert is faithful
        // on both targets and this LOWERS, balanced.
        let unwrap = ir_expr(
            IrExprKind::Unwrap {
                expr: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
            },
            list_int(),
        );
        // The faithful early-return lowering LANDED (the monadic `!` desugar): `var x =
        // opt!` rewrites into the err-propagating match continuation, so this either
        // lowers balanced or walls inside the match machinery — never the old
        // silently-wrong deferred bind. (The well-typed end-to-end shape is pinned by
        // `effect_assign_unwrap` in the parity baseline.)
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(1, list_int(), unwrap),
        ]);
        if let Ok(mir) = lower_body(&b, "main") {
            assert_eq!(verify_ownership(&mir), Ok(()), "if it lowers it must be balanced");
        }
    }

    #[test]
    fn heap_method_call_bound_to_a_var_is_walled_not_an_empty_opaque() {
        use almide_lang::intern::sym;
        // var x = obj.method()  — a Method callee on a NON-Named receiver now RESOLVES to
        // free-fn UFCS (`method(obj)`): the checker guarantees a surviving Method names a
        // real free fn, so the desugar emits an ordinary Named CallFn with the receiver
        // prepended (a genuinely-missing fn is caught by the render's unlinked wall, never
        // an empty Opaque). The OLD contract (Method = unresolvable = wall) is superseded.
        let mcall = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
                    method: sym("method"),
                },
                args: vec![],
                type_args: vec![],
            },
            list_int(),
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(1, list_int(), mcall),
        ]);
        match lower_body(&b, "main") {
            Ok(mir) => {
                assert!(
                    mir.ops.iter().any(|o| matches!(o,
                        Op::CallFn { name, args, .. } if name == "method" && args.len() == 1)),
                    "the UFCS resolution emits CallFn method(obj): {:?}",
                    mir.ops
                );
                assert_eq!(verify_ownership(&mir), Ok(()));
            }
            other => panic!("expected the resolved UFCS call to lower, got: {other:?}"),
        }
    }

    #[test]
    fn method_call_on_named_receiver_resolves_to_the_qualified_free_fn() {
        use almide_lang::intern::sym;
        use almide_lang::types::Ty;
        // B-1: `p.encode()` with `p: Person` (a Ty::Named receiver) resolves to the derived
        // free fn `Person.encode(p)` — the receiver becomes the first argument. Mirrors the v0
        // emitter's Method catch-all (emit_wasm/calls_p2.rs). A NON-Named receiver (the test
        // above) stays an unresolved Method and walls.
        let person = Ty::Named(sym("Person"), vec![]);
        let mcall = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Method {
                    object: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, person.clone())),
                    method: sym("encode"),
                },
                args: vec![],
                type_args: vec![],
            },
            Ty::Named(sym("Value"), vec![]),
        );
        let rewritten =
            crate::lower::desugar_method_calls(&mcall, &Default::default()).expect("a Named-receiver method resolves");
        match &rewritten.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                assert_eq!(name.as_str(), "Person.encode", "qualified as TypeName.method");
                assert_eq!(args.len(), 1, "receiver prepended as the first argument");
                assert!(
                    matches!(&args[0].kind, IrExprKind::Var { id } if *id == VarId(0)),
                    "the receiver is the first arg",
                );
            }
            other => panic!("expected a resolved Named call, got: {other:?}"),
        }
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

        // A NESTED-container HEAP extraction (the immediate container is itself an
        // extraction, not a tracked var) has no `src` to Dup. The OLD path fell back to a
        // deferred `Alloc{Opaque}` — an EMPTY heap value borrowed into `g` = a silent
        // miscompile. It now REJECTS explicitly (the extraction's `Err` propagates).
        let nested = ir_expr(
            IrExprKind::Member { object: Box::new(idx(c(), list_int())), field: sym("x") },
            list_int(),
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
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("expected an explicit nested-extraction reject, got: {other:?}"),
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

    #[test]
    fn guard_else_desugars_to_a_conditional() {
        // `guard cond else E; …` is a conditional early return; the Phase-A desugar
        // rewrites it to `if cond then { rest } else E` so the proven `if`/tail machinery
        // runs the `!cond` path (validated("") must return err, not the deferred always-ok
        // miscompile). Assert the desugar FIRES and leaves NO Guard statement behind.
        use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
        let b = body(vec![
            stmt(IrStmtKind::Guard {
                cond: ir_expr(IrExprKind::LitBool { value: true }, Ty::Bool),
                else_: ir_expr(
                    IrExprKind::ResultErr {
                        expr: Box::new(ir_expr(
                            IrExprKind::LitStr { value: "empty".into() },
                            Ty::String,
                        )),
                    },
                    Ty::Applied(TypeConstructorId::Result, vec![Ty::String, Ty::String]),
                ),
            }),
        ]);
        let desugared = crate::lower::desugar_guard(&b).expect("guard desugar fires");
        struct GuardHunter(bool);
        impl IrVisitor for GuardHunter {
            fn visit_stmt(&mut self, s: &almide_ir::IrStmt) {
                if matches!(s.kind, IrStmtKind::Guard { .. }) {
                    self.0 = true;
                }
                walk_stmt(self, s);
            }
            fn visit_expr(&mut self, e: &IrExpr) {
                walk_expr(self, e);
            }
        }
        let mut h = GuardHunter(false);
        h.visit_expr(&desugared);
        assert!(!h.0, "no Guard statement must remain after the desugar");
    }

    #[test]
    fn heap_option_record_unwrap_or_walls_not_miscompiles() {
        // `let t = list.get(xs, i) ?? { … }` over a `List[record]` (Option[record] `??`) has NO
        // faithful lowering — the Value-shaped option.value_unwrap_or corrupted the record field
        // block (both arms printed garbage/empty vs v0; the mir>ir gate flagged it on porta
        // parse_manifest). It must WALL, not fall to the empty-record Opaque. Modeled minimally:
        // an UnwrapOr whose operand is an Option[record]-typed var and fallback a record literal.
        let rec_ty = Ty::Record {
            fields: vec![(almide_lang::intern::sym("name"), Ty::String)],
        };
        let opt_rec = Ty::Applied(TypeConstructorId::Option, vec![rec_ty.clone()]);
        let b = body(vec![
            bind(0, opt_rec.clone(), ir_expr(IrExprKind::OptionNone, opt_rec.clone())),
            bind(
                1,
                rec_ty.clone(),
                ir_expr(
                    IrExprKind::UnwrapOr {
                        expr: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, opt_rec)),
                        fallback: Box::new(ir_expr(
                            IrExprKind::Record {
                                name: None,
                                fields: vec![(
                                    almide_lang::intern::sym("name"),
                                    ir_expr(IrExprKind::LitStr { value: "d".into() }, Ty::String),
                                )],
                            },
                            rec_ty,
                        )),
                    },
                    Ty::Record {
                        fields: vec![(almide_lang::intern::sym("name"), Ty::String)],
                    },
                ),
            ),
        ]);
        match lower_body(&b, "f") {
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("expected an Option[record] ?? wall, got {other:?}"),
        }
    }
