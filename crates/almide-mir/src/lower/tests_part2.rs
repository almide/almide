// Core-IR → MIR lowering tests — part 2 of 2. Included by lower/tests.rs (one module).
    #[test]
    fn heap_frame_break_is_walled() {
        // var xs = []; for s in xs { if c then { break } else { } }  — the loop variable
        // `s` is a String (heap element → Op::Dup, a per-iteration FRAME handle). A real
        // break skips its Drop, and the v0 wasm backend frees AFTER the break target = a
        // LEAK. This shape (control_flow_test.almd:329) must KEEP WALLING.
        let brk = unit_block(vec![stmt(IrStmtKind::Expr { expr: ir_expr(IrExprKind::Break, Ty::Unit) })]);
        let body_if = stmt(IrStmtKind::Expr { expr: iff(brk, unit_block(vec![]), Ty::Unit) });
        let forin = ir_expr(
            IrExprKind::ForIn {
                var: VarId(1),
                var_tuple: None,
                iterable: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
                body: vec![body_if],
            },
            Ty::Unit,
        );
        // Bind VarId(1) to String somewhere so find_var_ty sees a heap element.
        let use_s = stmt(IrStmtKind::Expr {
            expr: ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: almide_lang::intern::sym("use") },
                    args: vec![ir_expr(IrExprKind::Var { id: VarId(1) }, Ty::String)],
                    type_args: vec![],
                },
                Ty::Unit,
            ),
        });
        let forin = match forin.kind {
            IrExprKind::ForIn { var, var_tuple, iterable, mut body } => {
                body.push(use_s);
                ir_expr(IrExprKind::ForIn { var, var_tuple, iterable, body }, Ty::Unit)
            }
            _ => unreachable!(),
        };
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: forin }),
        ]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("heap frame"), "got: {r}"),
            other => panic!("expected a heap-frame break wall, got {other:?}"),
        }
    }

    #[test]
    fn scalar_loop_with_heap_accumulator_and_break_lowers() {
        // var acc = []; for i in 0..n { if c then { break } else { } ; acc = acc + [i] }
        // The loop variable `i` is scalar (Const, NOT a frame handle) and the heap
        // accumulator is DEFERRED via in_frame (also not a frame handle), so the frame is
        // empty of heap handles → the break is admitted (no leak). The common pattern.
        let brk = unit_block(vec![stmt(IrStmtKind::Expr { expr: ir_expr(IrExprKind::Break, Ty::Unit) })]);
        let body_if = stmt(IrStmtKind::Expr { expr: iff(brk, unit_block(vec![]), Ty::Unit) });
        let acc_plus = stmt(IrStmtKind::Assign {
            var: VarId(0),
            value: ir_expr(
                IrExprKind::BinOp {
                    op: almide_ir::BinOp::ConcatList,
                    left: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
                    right: Box::new(ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
                },
                list_int(),
            ),
        });
        let forin = ir_expr(
            IrExprKind::ForIn {
                var: VarId(1),
                var_tuple: None,
                iterable: Box::new(ir_expr(
                    IrExprKind::Range {
                        start: Box::new(ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int)),
                        end: Box::new(ir_expr(IrExprKind::LitInt { value: 5 }, Ty::Int)),
                        inclusive: false,
                    },
                    Ty::Int,
                )),
                body: vec![body_if, acc_plus],
            },
            Ty::Unit,
        );
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: forin }),
        ]);
        let mir = lower_body(&b, "main").expect("scalar loop + heap accumulator + break lowers");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn loop_body_heap_accumulator_is_deferred_and_safe() {
        // var acc = []; while c { acc = [] }  — a HEAP reassign of a pre-loop var is the
        // loop ACCUMULATOR. It is DEFERRED (not rebound): `acc` keeps its still-live
        // pre-loop handle across iterations, so no iteration drops a handle a later
        // iteration reads. The reassign emits NO ownership op (acc's `value_of` is
        // unchanged) → exactly one Alloc (the pre-loop `[]`) and one Drop (scope end).
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
        let mir = lower_body(&b, "main").expect("the accumulator is deferred, not walled");
        let allocs = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(allocs, 1, "only the pre-loop alloc — the reassign is deferred: {:?}", mir.ops);
        assert_eq!(drops, 1, "acc dropped exactly once at scope end: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()), "no UAF — acc stays pinned to its live handle");
    }

    #[test]
    fn accumulator_read_after_the_loop_is_memory_safe() {
        use almide_lang::intern::sym;
        // The ADVERSARIAL case the deferral exists for:
        //   var acc = []; for x in xs { acc = acc + [x] }; use(acc)
        // A naive rebind would point `value_of[acc]` at a handle the per-iteration frame
        // DROPS, then `use(acc)` after the loop dereferences it → UAF. The deferral pins
        // `acc` to its pre-loop handle (still live at the post-loop read), so this is
        // memory-safe by construction. `acc` must still be a tracked var the call borrows.
        let plus = ir_expr(
            IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatList,
                left: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())),
                right: Box::new(ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            },
            list_int(),
        );
        let acc_plus = stmt(IrStmtKind::Assign { var: VarId(0), value: plus });
        let forin = ir_expr(
            IrExprKind::ForIn {
                var: VarId(1),
                var_tuple: None,
                iterable: Box::new(ir_expr(IrExprKind::Var { id: VarId(2) }, list_int())),
                body: vec![acc_plus],
            },
            Ty::Unit,
        );
        let use_acc = stmt(IrStmtKind::Expr {
            expr: ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym("use") },
                    args: vec![ir_expr(IrExprKind::Var { id: VarId(0) }, list_int())],
                    type_args: vec![],
                },
                Ty::Unit,
            ),
        });
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(2, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: forin }),
            use_acc,
        ]);
        let mir = lower_body(&b, "main").expect("accumulator + post-loop read lowers");
        assert_eq!(verify_ownership(&mir), Ok(()), "post-loop read borrows a still-live handle — no UAF");
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
    fn heap_result_if_outside_executable_subset_is_walled_not_an_empty_opaque() {
        use almide_lang::intern::sym;
        // fn f() = if c then make() else []  — a HEAP-result branch OUTSIDE the executable
        // `try_lower_heap_result_if` subset (an empty-list arm has no faithful encoding).
        // The OLD path linearized the arms and MOVED OUT one fresh `Alloc{Opaque}` — an
        // EMPTY merged result the caller observes = a silent miscompile. It now REJECTS
        // explicitly so the function walls cleanly.
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
        match lower_body(&b, "f") {
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("expected an explicit heap-result-if reject, got: {other:?}"),
        }
    }

    #[test]
    fn heap_result_if_bound_to_a_var_is_walled_not_an_empty_opaque() {
        // var x = if c then [] else []  (Unit body) — a let-bound heap-result `if`. The OLD
        // path bound `x` to a fresh `Alloc{Opaque}` (an EMPTY list); any later read of `x`
        // observes empty bytes = a silent miscompile, AND the merged slot has no sound
        // scope-end drop in the flat certificate. It now REJECTS explicitly.
        let then = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let els = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let b = body(vec![bind(0, list_int(), iff(then, els, list_int()))]);
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("expected an explicit let-bound heap-result-if reject, got: {other:?}"),
        }
    }

    #[test]
    fn branch_arm_heap_reassign_is_deferred_and_safe() {
        // var z = []; if c then { z = [9] } else { }  — the arm reassigns pre-branch z.
        // A naive rebind would point `value_of[z]` at an arm-local handle the per-arm
        // teardown drops, so a post-branch read would UAF. The reassign is DEFERRED: `z`
        // keeps its still-live pre-branch handle. Exactly one Alloc (`[]`), one Drop.
        let then = unit_block(vec![stmt(IrStmtKind::Assign {
            var: VarId(0),
            value: ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
        })]);
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: iff(then, unit_block(vec![]), Ty::Unit) }),
        ]);
        let mir = lower_body(&b, "main").expect("the arm reassign is deferred, not walled");
        let allocs = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(allocs, 1, "only the pre-branch alloc — the arm reassign is deferred: {:?}", mir.ops);
        assert_eq!(drops, 1, "z dropped exactly once at scope end: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()), "no path-dependent UAF — z stays pinned");
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
        // `var x = []!` is the FIRST heap-affecting stmt (live_heap_handles empty), then
        // `var y = [] ?? []`. UnwrapOr is not an early return, so x being live is fine.
        let b = body(vec![bind(0, list_int(), unwrap), bind(1, list_int(), unwrap_or)]);
        let mir = lower_body(&b, "main").expect("error operators with no live heap local lower");
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
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            bind(1, list_int(), unwrap),
        ]);
        let mir = lower_body(&b, "main").expect("unwrap over a live heap local now lowers (v0 leak fixed)");
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn heap_method_call_bound_to_a_var_is_walled_not_an_empty_opaque() {
        use almide_lang::intern::sym;
        // var x = obj.method()  — an unresolvable Method callee returning a HEAP value. The
        // OLD path bound `x` to a deferred `Alloc{Opaque}` (an EMPTY list); any later read
        // of `x` observes empty bytes = a silent miscompile. It now REJECTS explicitly.
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
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("expected an explicit heap method-call reject, got: {other:?}"),
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
