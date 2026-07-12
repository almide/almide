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
            Err(LowerError::Unsupported(r)) => assert!(r.contains("break/continue"), "got: {r}"),
            other => panic!("expected a break/continue wall, got {other:?}"),
        }
    }

    #[test]
    fn scalar_loop_with_break_walls() {
        // var acc = []; for i in 0..n { if c then { break } else { } ; acc = acc + [i] }
        // The model-one-iteration fallback runs the body straight-line ONCE with no
        // early-exit branch, so the `break` is silently dropped — the loop would run to the
        // end instead of stopping. WALL it (the discipline fix: never silently miscompile an
        // early exit; faithful break needs the real-loop markers, which don't yet cover it).
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
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(r)) => assert!(r.contains("break/continue"), "got: {r}"),
            other => panic!("expected a break/continue wall, got {other:?}"),
        }
    }

    #[test]
    fn while_body_heap_accumulator_walls() {
        // var acc = []; while c { acc = [] }  — a HEAP reassign of a pre-loop var is the
        // loop ACCUMULATOR. The model-one-iteration `while` fallback DEFERS it (no rebind),
        // which is memory-safe BUT drops the accumulation: the loop would print the initial
        // `acc`, not the accumulated one (`var acc="S"; while … { acc=acc+"x" }` → v0 `Sxxx`,
        // the fallback `S`). WALL it (the discipline fix) — a faithful heap accumulator needs
        // a real loop with cross-back-edge heap merge, not yet in this brick.
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
            Err(LowerError::Unsupported(r)) => {
                assert!(r.contains("heap-accumulator"), "got: {r}")
            }
            other => panic!("expected a heap-accumulator wall, got {other:?}"),
        }
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
    fn unit_if_with_effect_arms_walls_instead_of_linearizing() {
        use almide_lang::intern::sym;
        // if c then println("a") else println("b") — when the real-branch paths decline
        // the condition, the linearized render RUNS BOTH arms (the rc4 double-print:
        // `println(if e == err("a") then "eq" else "ne")` printed eq AND ne,
        // 2026-07-12). A call-bearing arm therefore WALLS instead of linearizing —
        // a clean Unsupported, never wrong output. (The former contract asserted the
        // both-arms caps-union lowering; rc4 proved that observably wrong.)
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
        match lower_body(&b, "main") {
            Err(LowerError::Unsupported(m)) => {
                assert!(m.contains("call-bearing arm"), "got: {m}")
            }
            other => panic!("expected the call-bearing linearization wall, got {other:?}"),
        }
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
    fn heap_result_if_bound_to_a_var_is_memory_safe_after_desugar() {
        // var x = if c then [] else []  (Unit body, x UNUSED) — a let-bound heap-result `if`.
        // The tail-duplication desugar restructures this to `if c then { let x = [] } else
        // { let x = [] }` as the Unit tail. The cond is an unbound `Var(5)` here, so the
        // executable Unit-`if` machinery cannot build an `IfThen` and falls back to the SOUND
        // both-arms LINEARIZATION (each arm allocs its own empty list, binds the unused x,
        // drops it at the arm frame end). The result is OWNERSHIP-BALANCED — one Alloc + one
        // Drop per arm, no silent-empty merged-dst, no double-free, no leak. This pins the
        // discipline: even outside the executable subset the desugared form is memory-safe
        // (NEVER wrong bytes), and `x` being unused means the empty Opaque arms are
        // observationally identical to v0. (The FAITHFUL, USED case — a resolvable cond +
        // string arms read by the continuation — LOWERS+executes via real `IfThen` markers;
        // see `let_bound_heap_result_if_executes_via_tail_duplication`.)
        let then = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let els = ir_expr(IrExprKind::List { elements: vec![] }, list_int());
        let b = body(vec![bind(0, list_int(), iff(then, els, list_int()))]);
        let mir = lower_body(&b, "main").expect("the desugared form lowers memory-safely");
        let allocs = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
        let drops = mir.ops.iter().filter(|o| matches!(o, Op::Drop { .. })).count();
        assert_eq!(allocs, drops, "every per-arm alloc is dropped within its arm: {:?}", mir.ops);
        assert_eq!(verify_ownership(&mir), Ok(()), "no double-free, no leak");
    }

    // A faithful Bool cond (lowers to a scalar 0/1 via `lower_scalar_value`), so the
    // tail-duplication desugar can fire.
    fn faithful_cond() -> IrExpr {
        ir_expr(IrExprKind::LitBool { value: true }, Ty::Bool)
    }
    fn iff_faithful(then: IrExpr, els: IrExpr, ty: Ty) -> IrExpr {
        ir_expr(
            IrExprKind::If {
                cond: Box::new(faithful_cond()),
                then: Box::new(then),
                else_: Box::new(els),
            },
            ty,
        )
    }
    fn lit_str(s: &str) -> IrExpr {
        ir_expr(IrExprKind::LitStr { value: s.into() }, Ty::String)
    }

    #[test]
    fn let_bound_heap_result_if_lowers_via_tail_duplication_and_is_balanced() {
        // let s = if c then "A" else "B"; println(s)  — the canonical shape. The desugar
        // pushes `println(s)` into each arm: `if c then { let s = "A"; println(s) } else
        // { let s = "B"; println(s) }`. Each arm allocs its own String, uses it, drops it at
        // the arm frame end — the per-arm `i…d` balance the checker already accepts. The body
        // now LOWERS (not walled) and is ownership-BALANCED, with executable `IfThen` markers.
        let then = lit_str("A");
        let els = lit_str("B");
        let prn = stmt(IrStmtKind::Expr {
            expr: ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: almide_lang::intern::sym("println") },
                    args: vec![ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String)],
                    type_args: vec![],
                },
                Ty::Unit,
            ),
        });
        let b = body(vec![bind(0, Ty::String, iff_faithful(then, els, Ty::String)), prn]);
        let mir = lower_body(&b, "main").expect("the faithful let-bound heap-result if lowers");
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::IfThen { .. })),
            "executes via an IfThen marker (only the taken arm runs): {:?}",
            mir.ops
        );
        // Two Allocs (one per arm — each arm's "A"/"B"), each dropped within its arm.
        assert_eq!(
            mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count(),
            2,
            "one String alloc per arm (no merged-dst): {:?}",
            mir.ops
        );
        assert_eq!(
            verify_ownership(&mir),
            Ok(()),
            "each arm independently allocs + drops its own s — no double-free, no leak"
        );
    }

    #[test]
    fn let_bound_heap_result_if_with_a_continuation_use_lowers() {
        // let s = if c then "A" else "B"; let t = s + "!"; println(t)  — the continuation
        // ITSELF builds a heap value from s. Both `let t = …` and `println(t)` are pushed into
        // each arm, so each arm's `s`, `t` are alloc'd + dropped within the arm. Balanced.
        let then = lit_str("A");
        let els = lit_str("B");
        let concat = ir_expr(
            IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                left: Box::new(ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String)),
                right: Box::new(lit_str("!")),
            },
            Ty::String,
        );
        let prn = stmt(IrStmtKind::Expr {
            expr: ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: almide_lang::intern::sym("println") },
                    args: vec![ir_expr(IrExprKind::Var { id: VarId(1) }, Ty::String)],
                    type_args: vec![],
                },
                Ty::Unit,
            ),
        });
        let b = body(vec![
            bind(0, Ty::String, iff_faithful(then, els, Ty::String)),
            bind(1, Ty::String, concat),
            prn,
        ]);
        let mir = lower_body(&b, "main").expect("the continuation-using let-bound if lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::IfThen { .. })));
        assert_eq!(
            verify_ownership(&mir),
            Ok(()),
            "the duplicated continuation (let t = s + \"!\") is per-arm balanced"
        );
    }

    #[test]
    fn let_bound_heap_result_if_scalar_continuation_returns_value() {
        // fn f() -> Int = { let s = if c then "A" else "BB"; string.len(s) }  — a SCALAR-
        // returning body whose continuation reads `s`. The desugar pushes `string.len(s)` into
        // each arm; the scalar `if` machinery moves out the per-arm scalar result. The Strings
        // alloc + drop within each arm; the returned Int is a value (no ownership). Balanced.
        let then = lit_str("A");
        let els = lit_str("BB");
        let len_call = ir_expr(
            IrExprKind::Call {
                target: CallTarget::Module {
                    module: almide_lang::intern::sym("string"),
                    func: almide_lang::intern::sym("len"),
                    def_id: None,
                },
                args: vec![ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String)],
                type_args: vec![],
            },
            Ty::Int,
        );
        let b = ir_expr(
            IrExprKind::Block {
                stmts: vec![bind(0, Ty::String, iff_faithful(then, els, Ty::String))],
                expr: Some(Box::new(len_call)),
            },
            Ty::Int,
        );
        let mir = lower_body(&b, "f").expect("the scalar-continuation let-bound if lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::IfThen { .. })));
        assert_eq!(verify_ownership(&mir), Ok(()), "each arm's String drops within the arm");
    }

    #[test]
    fn let_bound_heap_result_match_lowers_via_tail_duplication() {
        // let s = match n { 0 => "zero", _ => "other" }; println(s)  — the match analog. The
        // match desugars to a nested literal-pattern `if` chain, and the continuation is pushed
        // into each leaf arm. Lowers + balanced.
        let arm0 = almide_ir::IrMatchArm {
            pattern: IrPattern::Literal { expr: ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int) },
            guard: None,
            body: lit_str("zero"),
        };
        let arm_default = almide_ir::IrMatchArm {
            pattern: IrPattern::Wildcard,
            guard: None,
            body: lit_str("other"),
        };
        let subject = ir_expr(IrExprKind::LitInt { value: 0 }, Ty::Int);
        let m = ir_expr(
            IrExprKind::Match { subject: Box::new(subject), arms: vec![arm0, arm_default] },
            Ty::String,
        );
        let prn = stmt(IrStmtKind::Expr {
            expr: ir_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: almide_lang::intern::sym("println") },
                    args: vec![ir_expr(IrExprKind::Var { id: VarId(0) }, Ty::String)],
                    type_args: vec![],
                },
                Ty::Unit,
            ),
        });
        let b = body(vec![bind(0, Ty::String, m), prn]);
        let mir = lower_body(&b, "main").expect("the let-bound heap-result match lowers");
        assert!(mir.ops.iter().any(|o| matches!(o, Op::IfThen { .. })));
        assert_eq!(verify_ownership(&mir), Ok(()));
    }

    #[test]
    fn nested_let_bound_branch_continuation_bounded_duplication() {
        // let s = if c then "A" else "B"; let t = if c then "C" else "D"; println(s)
        // — the continuation has a SECOND heap let-bound if. With the BOUNDED-duplication relaxation
        // (≤ 2 remaining branch binds → ≤ 2^3 leaf arms), the fixpoint now resolves BOTH ifs into flat
        // leaves `let s=…; let t=…; println(s)`, each with its own balanced s+t drops — so it LOWERS
        // soundly. (block_scalar's `let joined = if…; (value.str(if…), end)` is this 2-if shape.)
        let println_s = |v: u32| {
            stmt(IrStmtKind::Expr {
                expr: ir_expr(
                    IrExprKind::Call {
                        target: CallTarget::Named { name: almide_lang::intern::sym("println") },
                        args: vec![ir_expr(IrExprKind::Var { id: VarId(v) }, Ty::String)],
                        type_args: vec![],
                    },
                    Ty::Unit,
                ),
            })
        };
        let mk = || iff_faithful(lit_str("A"), lit_str("B"), Ty::String);
        let b = body(vec![bind(0, Ty::String, mk()), bind(1, Ty::String, mk()), println_s(0)]);
        let mir = lower_body(&b, "main").expect("the bounded 2-if continuation now lowers");
        assert_eq!(
            verify_ownership(&mir),
            Ok(()),
            "each leaf's s+t allocs are dropped exactly once: {:?}",
            mir.ops
        );

        // FOUR ifs (3 in the continuation after the first) EXCEED the bound → still WALLS: the
        // exponential-blow-up guard, GENERALIZED: the per-count bound (≤ 2) was retired
        // when the desugar generalized — a 4-chain lowers 16 balanced copies (real
        // chains are 2–4 deep). What remains is the NODE-COUNT cap: a 20-chain
        // (≈2^20 copies) is discarded and the bind WALLS — an honest refusal
        // instead of a compile-time hang.
        let b4 = body(vec![
            bind(0, Ty::String, mk()),
            bind(1, Ty::String, mk()),
            bind(2, Ty::String, mk()),
            bind(3, Ty::String, mk()),
            println_s(0),
        ]);
        let mir4 = lower_body(&b4, "main").expect("a 4-chain duplicates boundedly and lowers");
        assert_eq!(verify_ownership(&mir4), Ok(()), "all 16 leaves balanced");
        let b20 = body(
            (0..20)
                .map(|i| bind(i, Ty::String, mk()))
                .chain([println_s(0)])
                .collect::<Vec<_>>(),
        );
        match lower_body(&b20, "main") {
            Err(LowerError::Unsupported(_)) => {}
            other => panic!("a 20-chain must hit the node cap and wall, got: {other:?}"),
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
            crate::lower::desugar_method_calls(&mcall).expect("a Named-receiver method resolves");
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
