
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
        // var r = helper()..other()  — a Range bind lowers to ONE Opaque `Alloc`,
        // ELIDING its operand calls. `record_elided_calls` surfaces each as a bare
        // EFFECT MARKER `CallFn{dst:None, args:[], result:None}` so the caps fold
        // can see them, while the value content stays deferred. (The original
        // vehicle — a scalar list literal with call elements — now WALLS instead of
        // deferring (C-144: never a silent `[]`), so the Range shape carries the
        // marker contract.)
        let range = IrExprKind::Range {
            start: Box::new(named("helper", vec![])),
            end: Box::new(named("other", vec![])),
            inclusive: false,
        };
        let b = body(vec![bind(0, list_int(), ir_expr(range, list_int()))]);
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
                IrExprKind::Range {
                    start: Box::new(named(
                        "apply",
                        vec![ir_expr(IrExprKind::Var { id: VarId(2) }, fn_ty)],
                    )),
                    end: Box::new(ir_expr(IrExprKind::Var { id: VarId(3) }, Ty::Int)),
                    inclusive: false,
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
    fn map_insert_is_a_functional_rebind() {
        // var m = []; m[k] = v  — map insertion REBINDS through the self-host:
        // `m = map.set(m, k, v)` (value semantics — the same treatment the
        // `map.insert(m, k, v)` CALL form gets), NOT an in-place MakeUnique
        // (the old elide-the-write model was a silent no-op on the v1 leg).
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
        assert!(
            mir.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name.starts_with("map.set"))),
            "map insert rebinds through map.set: {:?}",
            mir.ops
        );
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

