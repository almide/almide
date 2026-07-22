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
    fn branch_arm_heap_reassign_ssa_merges_by_value() {
        // var z = []; if c then { z = [9] } else { }  — the arm reassigns pre-branch z.
        // HISTORY: this used to assert the reassign was DEFERRED (elided) — which pinned
        // `z`'s pre-branch handle (no UAF) but silently DROPPED the new value: a LIVE
        // wrong-value class (the lp5 probe: v0 `ok:42`, v1 `err:normal`, no wall —
        // B127/B128). `desugar_unit_if_heap_reassign` now SSA-ifies the shape into a
        // let-bound value-`if` (`let z' = if c then [9] else z`), so the conditional
        // value merges BY VALUE through the proven heap-result-`if` machinery: BOTH
        // allocs are real (the pre-branch `[]` and the arm's `[9]`), every object is
        // dropped exactly once on each path, and the ownership certificate verifies.
        let then = unit_block(vec![stmt(IrStmtKind::Assign {
            var: VarId(0),
            value: ir_expr(IrExprKind::List { elements: vec![] }, list_int()),
        })]);
        let b = body(vec![
            bind(0, list_int(), ir_expr(IrExprKind::List { elements: vec![] }, list_int())),
            stmt(IrStmtKind::Expr { expr: iff(then, unit_block(vec![]), Ty::Unit) }),
        ]);
        let mir = lower_body(&b, "main").expect("the arm reassign SSA-merges, not walled");
        let allocs = mir.ops.iter().filter(|o| matches!(o, Op::Alloc { .. })).count();
        assert_eq!(
            allocs, 2,
            "both the pre-branch alloc AND the arm's new value are real: {:?}",
            mir.ops
        );
        assert_eq!(verify_ownership(&mir), Ok(()), "every path drop-balanced — no UAF, no leak");
    }
