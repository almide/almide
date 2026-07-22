
/// Flatten a scopeless `Block { stmts: [], expr: e }` to `e`, EVERYWHERE it appears (a match-arm body,
/// an `if` branch, a nested block tail). An empty-statement block binds nothing, so it opens no drop
/// scope — it is observationally `e`, but the trust-spine's arm/branch lowering keys on the concrete
/// tail kind (a bare `Match`/`Ok` lowers; a `Block` wrapping it takes a different path that can wall).
/// The desugared derived variant decode (`let _e0 = as_int(..)?; …; ok(Ctor(..))`) leaves one such
/// wrapper per field-bind after `desugar_let_unwrap` rewrites each `?` bind to a match — this collapses
/// them so the nested monadic matches lower like the hand-written form. Run in BOTH the lowering and the
/// `count_ir_calls` gate; an empty block has no calls, so `mir == ir` is unaffected.
pub fn desugar_flatten_empty_block(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Block { stmts, expr: Some(inner) } = &e.kind {
                if stmts.is_empty() {
                    let inner = (**inner).clone();
                    *e = inner;
                    self.changed = true;
                }
            }
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

/// Group the per-constructor arms of an `Option`/`Result` `match` whose `some`/`ok`/`err` arms carry
/// GUARDS or LITERAL payloads into a payload SUB-MATCH — `match x { some(n) if g => A, some(0) => B,
/// some(_) => C, none => D }` becomes `match x { some($p) => match $p { n if g => A, 0 => B, _ => C },
/// none => D }`. The trust-spine lowers the OUTER (variant-tag dispatch, scalar payload bind) and the
/// INNER (scalar guard/literal chain via `build_match_chain`) separately — each proven — but NOT the
/// guarded-VARIANT combination directly (`try_lower_variant_value_match` gates out guards; the
/// heap-result path walls). Regrouping is sound because a variant's constructors are DISJOINT: a
/// `none` arm can never intercept a `some` value, so collecting all `some` arms in order preserves
/// arm order + fall-through byte-for-byte. Runs in BOTH the lowering and the `count_ir_calls` gate.
/// Hoist LITERAL record/tuple STRING-INTERPOLATION parts (`"${(1, \"x\", true)}"`,
/// `"${P{x: 1}}"`) to temp bindings at the enclosing STATEMENT level, so each part
/// becomes a materialized `Var` the EXPAND-fold display can read (a literal part is
/// never a tracked block — `aggregate_part_expandable` requires a Var — so it fell
/// to the unlinked `compound.to_string` wall). `println("${(1, 2)}")` becomes
/// `{ let $t = (1, 2); println("${$t}") }` — the binds are PREPENDED to the
/// statement (a Block in call-arg position would itself wall), and a literal
/// construction is effect-free so the hoist preserves evaluation order. A part the
/// display still cannot expand keeps the same wall it had; the bind rides the
/// ordinary materialized-aggregate ownership (`i` + scope-end `d`).
pub fn desugar_interp_literal_aggregate_hoist(
    body: &IrExpr,
    next_var: &mut u32,
) -> Option<IrExpr> {
    use almide_ir::{IrStmt, IrStmtKind, IrStringPart, Mutability, VarId};

    // Rewrite every literal-aggregate interp part INSIDE `e` to a fresh Var,
    // collecting the hoisted binds (in evaluation order).
    fn rewrite_expr(e: &mut IrExpr, next: &mut u32, binds: &mut Vec<IrStmt>, changed: &mut bool) {
        // Do NOT descend into nested Blocks — their own statement lists are the
        // hoist points for their contents (handled by rewrite_block below).
        if matches!(e.kind, IrExprKind::Block { .. }) {
            return;
        }
        if let IrExprKind::StringInterp { parts } = &mut e.kind {
            for p in parts.iter_mut() {
                let IrStringPart::Expr { expr } = p else { continue };
                if !matches!(expr.kind, IrExprKind::Record { .. } | IrExprKind::Tuple { .. }) {
                    continue;
                }
                let tmp = VarId(*next);
                *next += 1;
                binds.push(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: tmp,
                        mutability: Mutability::Let,
                        ty: expr.ty.clone(),
                        value: expr.clone(),
                    },
                    span: expr.span.clone(),
                });
                *expr = IrExpr {
                    kind: IrExprKind::Var { id: tmp },
                    ty: expr.ty.clone(),
                    span: expr.span.clone(),
                    def_id: None,
                };
                *changed = true;
            }
        }
        // Recurse into children manually (skipping Block, handled above).
        use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
        struct Kids<'a> {
            next: &'a mut u32,
            binds: &'a mut Vec<IrStmt>,
            changed: &'a mut bool,
        }
        impl IrMutVisitor for Kids<'_> {
            fn visit_expr_mut(&mut self, c: &mut IrExpr) {
                rewrite_expr(c, self.next, self.binds, self.changed);
            }
        }
        let mut k = Kids { next, binds, changed };
        walk_expr_mut(&mut k, e);
    }

    fn rewrite_block(e: &mut IrExpr, next: &mut u32, changed: &mut bool) {
        // First recurse structurally so INNER blocks hoist into themselves.
        use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
        struct B<'a> {
            next: &'a mut u32,
            changed: &'a mut bool,
        }
        impl IrMutVisitor for B<'_> {
            fn visit_expr_mut(&mut self, c: &mut IrExpr) {
                if matches!(c.kind, IrExprKind::Block { .. }) {
                    rewrite_block(c, self.next, self.changed);
                } else {
                    walk_expr_mut(self, c);
                }
            }
        }
        let IrExprKind::Block { stmts, expr } = &mut e.kind else { return };
        let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
        for mut st in stmts.drain(..) {
            let mut binds = Vec::new();
            match &mut st.kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                    rewrite_expr(value, next, &mut binds, changed);
                }
                IrStmtKind::Expr { expr } => {
                    rewrite_expr(expr, next, &mut binds, changed);
                }
                _ => {}
            }
            // Nested blocks inside this statement's exprs hoist into themselves.
            {
                let mut b = B { next, changed };
                match &mut st.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => {
                        b.visit_expr_mut(value)
                    }
                    IrStmtKind::Expr { expr } => b.visit_expr_mut(expr),
                    _ => {}
                }
            }
            out.extend(binds);
            out.push(st);
        }
        *stmts = out;
        if let Some(tail) = expr {
            let mut binds = Vec::new();
            rewrite_expr(tail, next, &mut binds, changed);
            let mut b = B { next, changed };
            b.visit_expr_mut(tail);
            stmts.extend(binds);
        }
    }

    let mut out = body.clone();
    let mut changed = false;
    if matches!(out.kind, IrExprKind::Block { .. }) {
        rewrite_block(&mut out, next_var, &mut changed);
    } else {
        // A non-block body (`fn f() = "${(1, 2)}"`): hoist into a wrapping Block
        // (allowed in tail position).
        let mut binds = Vec::new();
        let mut tail = out.clone();
        rewrite_expr(&mut tail, next_var, &mut binds, &mut changed);
        if changed {
            out = IrExpr {
                kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(tail.clone())) },
                ty: tail.ty.clone(),
                span: tail.span.clone(),
                def_id: tail.def_id,
            };
        }
    }
    if changed { Some(out) } else { None }
}


/// Rewrite `r?` (`ToOption`) over a `Result[Int, String]` operand into the SELF-HOST bridge
/// call `result.to_option(r)` — a REAL IR Call node, so every position (bind / call-arg /
/// tail) lowers through the proven Module-call machinery and the caps `mir == ir` count sees
/// the call on BOTH sides by construction (desugar-before-both). `result.to_option` is pure
/// (prim reads + an Option ctor), registered, and `is_self_host_option_module_fn`-seeded, so
/// a later `match`/`??` over the bound result reads a real materialized Option. ToOption was
/// previously fully deferred (the strict-value wall) — a pure widening.
pub fn desugar_to_option_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, CallTarget, IrMutVisitor};
    use almide_lang::intern::sym;
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::ToOption { expr } = &e.kind else { return };
            // `Option[T]?` is the IDENTITY (the `?` matrix's "Option → identity" row): `?`
            // is the to-Option CONVERSION (not `!`-propagation), so an already-Option
            // operand converts to itself — replace the node by its operand, in any
            // position. Count-invariant (ToOption is not a counted call; the operand's
            // calls appear exactly once either way).
            if matches!(&expr.ty, Ty::Applied(TypeConstructorId::Option, _)) && expr.ty == e.ty {
                let inner = (**expr).clone();
                *e = inner;
                self.changed = true;
                return;
            }
            let admits = matches!(&expr.ty,
                Ty::Applied(TypeConstructorId::Result, a)
                    if a.len() == 2 && matches!(a[0], Ty::Int) && matches!(a[1], Ty::String))
                && matches!(&e.ty,
                    Ty::Applied(TypeConstructorId::Option, oa)
                        if oa.len() == 1 && matches!(oa[0], Ty::Int));
            if !admits {
                return;
            }
            e.kind = IrExprKind::Call {
                target: CallTarget::Module {
                    module: sym("result"),
                    func: sym("to_option"),
                    def_id: None,
                },
                args: vec![(**expr).clone()],
                type_args: Vec::new(),
            };
            self.changed = true;
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}


/// Rewrite an OFF-SIGNATURE `testing.assert_some` / `testing.assert_ok` call to the
/// unlinkable `_x` name so it WALLS at render instead of misreading a block: the self-host
/// sigs are `Option[String]` (len-as-tag) and `Result[String, String]` (cap-as-tag@16) —
/// a different instantiation has a DIFFERENT tag layout, and the linked reader would
/// silently pass/fail wrongly. Count-invariant (the call node is unchanged, only renamed).
pub fn desugar_offtype_testing_asserts(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, CallTarget, IrMutVisitor};
    use almide_lang::intern::sym;
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } =
                &mut e.kind
            else {
                return;
            };
            if module.as_str() != "testing" {
                return;
            }
            let ok_sig = match func.as_str() {
                "assert_some" => matches!(args.first().map(|a| &a.ty),
                    Some(Ty::Applied(TypeConstructorId::Option, a))
                        if a.len() == 1 && matches!(a[0], Ty::String)),
                "assert_ok" => matches!(args.first().map(|a| &a.ty),
                    Some(Ty::Applied(TypeConstructorId::Result, a))
                        if a.len() == 2 && matches!(a[0], Ty::String) && matches!(a[1], Ty::String)),
                _ => return,
            };
            if !ok_sig {
                *func = sym(&format!("{}_x", func.as_str()));
                self.changed = true;
            }
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    v.changed.then_some(out)
}

/// Desugar a NON-EMPTY map literal `["k": v, …]` into `map.from_list([(k, v), …])` — the trust-spine
/// materializes a map literal as a DEFERRED-Opaque (empty) block, so a subsequent `map.len` / `map.get`
/// / `map.keys` would SILENTLY read the empty block (v0=2, v1=0 — a miscompile). `map.from_list`
/// builds the REAL map from a `List[(K, V)]` (byte-verified), so routing the literal through it both
/// fixes the miscompile AND opens map-literal usage. v0 is untouched (this is a v1-lowering rewrite).
/// The EMPTY literal `[:]` is already materialized correctly, so it is left alone.
pub fn desugar_map_literal(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::MapLiteral { entries } = &e.kind else { return };
            if entries.is_empty() {
                return;
            }
            let (k_ty, v_ty) = match &e.ty {
                Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2 => (a[0].clone(), a[1].clone()),
                _ => return,
            };
            let tuple_ty = Ty::Tuple(vec![k_ty, v_ty]);
            let elements: Vec<IrExpr> = entries
                .iter()
                .map(|(k, v)| IrExpr {
                    kind: IrExprKind::Tuple {
                        elements: vec![k.clone(), v.clone()],
                    },
                    ty: tuple_ty.clone(),
                    span: e.span.clone(),
                    def_id: None,
                })
                .collect();
            let list_expr = IrExpr {
                kind: IrExprKind::List { elements },
                ty: Ty::Applied(TypeConstructorId::List, vec![tuple_ty]),
                span: e.span.clone(),
                def_id: None,
            };
            e.kind = IrExprKind::Call {
                target: almide_ir::CallTarget::Module {
                    module: almide_lang::intern::sym("map"),
                    func: almide_lang::intern::sym("from_list"),
                    def_id: None,
                },
                args: vec![list_expr],
                type_args: vec![],
            };
            self.changed = true;
        }
    }
    let mut v = V { changed: false };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

/// The kind of a call's resolved target — used to make a walled `Call`'s reason
/// precise (the histogram then names which call SHAPE to admit next: a free
/// `Named` call vs a stdlib `Module` dispatch vs an unresolved `Method` vs a
/// `Computed` callee), so the coverage roadmap is evidence-based, not guessed.
pub(crate) fn call_target_kind(t: &CallTarget) -> &'static str {
    match t {
        CallTarget::Named { .. } => "Named",
        CallTarget::Module { .. } => "Module",
        CallTarget::Method { .. } => "Method",
        CallTarget::Computed { .. } => "Computed",
    }
}

pub(crate) fn kind_name(k: &IrExprKind) -> &'static str {
    // Named precisely so the corpus-wall `<other>` buckets break down into the
    // exact expression forms still to admit (an evidence-based roadmap, the same
    // discipline as `call_target_kind`). Unnamed kinds remain `<other>`.
    match k {
        IrExprKind::LitInt { .. } => "LitInt",
        IrExprKind::LitFloat { .. } => "LitFloat",
        IrExprKind::LitStr { .. } => "LitStr",
        IrExprKind::LitBool { .. } => "LitBool",
        IrExprKind::Unit => "Unit",
        IrExprKind::Var { .. } => "Var",
        IrExprKind::List { .. } => "List",
        IrExprKind::Record { .. } => "Record",
        IrExprKind::Tuple { .. } => "Tuple",
        IrExprKind::Block { .. } => "Block",
        IrExprKind::Call { .. } => "Call",
        IrExprKind::RuntimeCall { .. } => "RuntimeCall",
        IrExprKind::BinOp { .. } => "BinOp",
        IrExprKind::UnOp { .. } => "UnOp",
        IrExprKind::If { .. } => "If",
        IrExprKind::Match { .. } => "Match",
        IrExprKind::Member { .. } => "Member",
        IrExprKind::TupleIndex { .. } => "TupleIndex",
        IrExprKind::IndexAccess { .. } => "IndexAccess",
        IrExprKind::MapAccess { .. } => "MapAccess",
        IrExprKind::Range { .. } => "Range",
        IrExprKind::MapLiteral { .. } => "MapLiteral",
        IrExprKind::EmptyMap => "EmptyMap",
        IrExprKind::StringInterp { .. } => "StringInterp",
        IrExprKind::Lambda { .. } => "Lambda",
        IrExprKind::ClosureCreate { .. } => "ClosureCreate",
        IrExprKind::FnRef { .. } => "FnRef",
        IrExprKind::ResultOk { .. } => "ResultOk",
        IrExprKind::ResultErr { .. } => "ResultErr",
        IrExprKind::OptionSome { .. } => "OptionSome",
        IrExprKind::OptionNone => "OptionNone",
        IrExprKind::Try { .. } => "Try",
        IrExprKind::Unwrap { .. } => "Unwrap",
        IrExprKind::UnwrapOr { .. } => "UnwrapOr",
        IrExprKind::ForIn { .. } => "ForIn",
        IrExprKind::While { .. } => "While",
        IrExprKind::Fan { .. } => "Fan",
        IrExprKind::Break => "Break",
        IrExprKind::Continue => "Continue",
        IrExprKind::TailCall { .. } => "TailCall",
        IrExprKind::IterChain { .. } => "IterChain",
        IrExprKind::Await { .. } => "Await",
        IrExprKind::Clone { .. } => "Clone",
        IrExprKind::Deref { .. } => "Deref",
        IrExprKind::Borrow { .. } => "Borrow",
        IrExprKind::ToVec { .. } => "ToVec",
        IrExprKind::BoxNew { .. } => "BoxNew",
        IrExprKind::SpreadRecord { .. } => "SpreadRecord",
        _ => "<other>",
    }
}

/// BETA-REDUCE a DIRECT lambda application (`(λ(p) => body)(arg)` — the pipe-into-
/// lambda projection `fold(...) |> ((pair) => pair.0)`, argmax): rewrite to
/// `{ let p = arg; body }` so the ordinary bind + scalar-field machinery lowers it
/// (the Computed-callee call is otherwise unanalyzable → deferred/walled). Each arg
/// is bound ONCE (no duplication; call-count only DECREASES, so the caps gate's
/// `mir ≤ ir` is preserved). Bottom-up over the whole body; `None` = no change.
pub fn desugar_beta_reduce(body: &IrExpr) -> Option<IrExpr> {
    fn rewrite(e: IrExpr, changed: &mut bool) -> IrExpr {
        let e = e.map_children(&mut |c| rewrite(c, changed));
        if let IrExprKind::Call { target: CallTarget::Computed { callee }, args, .. } = &e.kind {
            if let IrExprKind::Lambda { params, body, .. } = &callee.kind {
                if params.len() == args.len() {
                    *changed = true;
                    let stmts: Vec<almide_ir::IrStmt> = params
                        .iter()
                        .zip(args.iter())
                        .map(|((var, ty), arg)| almide_ir::IrStmt {
                            kind: almide_ir::IrStmtKind::Bind {
                                var: *var,
                                mutability: almide_ir::Mutability::Let,
                                ty: ty.clone(),
                                value: arg.clone(),
                            },
                            span: None,
                        })
                        .collect();
                    return IrExpr {
                        kind: IrExprKind::Block { stmts, expr: Some(body.clone()) },
                        ty: e.ty.clone(),
                        span: e.span.clone(),
                        def_id: e.def_id,
                    };
                }
            }
        }
        e
    }
    let mut changed = false;
    let out = rewrite(body.clone(), &mut changed);
    changed.then_some(out)
}

/// Desugar `opt ?? fallback` over an `Option[<all-scalar tuple>]` (`list.get(xs, k) ??
/// (0.0, 0.0)` — the fft element pick) into `match opt { some($p) => $p, none => fallback }`,
/// which the proven variant-value-match machinery lowers (Option-tuple payload borrow @12,
/// subject dropped after the arms). Without this the UnwrapOr path treats the tuple payload
/// as a SCALAR (an i32 handle in an i64 slot — invalid wasm the engine rejects). Bottom-up;
/// `None` = no change.
pub fn desugar_tuple_unwrap_or(body: &IrExpr) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    fn is_scalar_tuple(ty: &Ty) -> bool {
        matches!(ty, Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(|t| !is_heap_ty(t)))
    }
    fn rewrite(e: IrExpr, changed: &mut bool, next: &mut u32) -> IrExpr {
        let e = e.map_children(&mut |c| rewrite(c, changed, next));
        // Both surface forms: the `??` operator (UnwrapOr) AND the explicit
        // `option.unwrap_or(opt, fb)` module call (the pipe form).
        let parts: Option<(&IrExpr, &IrExpr)> = match &e.kind {
            IrExprKind::UnwrapOr { expr, fallback } => Some((expr, fallback)),
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if module.as_str() == "option" && func.as_str() == "unwrap_or"
                    && args.len() == 2 =>
            {
                Some((&args[0], &args[1]))
            }
            _ => None,
        };
        if let Some((expr, fallback)) = parts {
            let is_opt_tuple = matches!(&expr.ty,
                Ty::Applied(TypeConstructorId::Option, a)
                    if a.len() == 1 && is_scalar_tuple(&a[0]));
            if is_opt_tuple {
                *changed = true;
                let p = almide_ir::VarId(*next);
                *next += 1;
                let arms = vec![
                    almide_ir::IrMatchArm {
                        pattern: almide_ir::IrPattern::Some {
                            inner: Box::new(almide_ir::IrPattern::Bind { var: p, ty: e.ty.clone() }),
                        },
                        guard: None,
                        body: IrExpr {
                            kind: IrExprKind::Var { id: p },
                            ty: e.ty.clone(),
                            span: e.span.clone(),
                            def_id: e.def_id,
                        },
                    },
                    almide_ir::IrMatchArm {
                        pattern: almide_ir::IrPattern::None,
                        guard: None,
                        body: fallback.clone(),
                    },
                ];
                return IrExpr {
                    kind: IrExprKind::Match { subject: Box::new(expr.clone()), arms },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                };
            }
        }
        e
    }
    let mut changed = false;
    let mut next = crate::lower::max_var_id(body) + 1;
    let out = rewrite(body.clone(), &mut changed, &mut next);
    changed.then_some(out)
}

/// `option.unwrap_or(option.map(list.find(xs, λ1), λ2), 32)` — the C-127 PIPED
/// generic chain. The lambda lift is statement-position-sensitive: the SAME chain
/// written as source `let`s lowers, while the nested-call form walls its HOF
/// links. ANF the nested heap-result HOF links into `let` bindings at the
/// fn/Block TAIL, so the lowering sees the proven decomposed form. Trigger: a
/// tail Module call carrying a heap-result Module-call argument that
/// (transitively) takes a Lambda. Call COUNT is preserved (desugar-before-both:
/// the counting gate sees the same calls, merely let-bound).
pub(crate) fn desugar_hof_chain_anf(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarId};
    fn call_carries_lambda(e: &IrExpr) -> bool {
        let IrExprKind::Call { args, .. } = &e.kind else { return false };
        args.iter()
            .any(|a| matches!(a.kind, IrExprKind::Lambda { .. }) || call_carries_lambda(a))
    }
    fn needs_anf_arg(a: &IrExpr) -> bool {
        crate::lower::is_heap_ty(&a.ty)
            && matches!(&a.kind, IrExprKind::Call { target: CallTarget::Module { .. }, .. })
            && call_carries_lambda(a)
    }
    fn anf_arg(a: &IrExpr, next: &mut u32, binds: &mut Vec<IrStmt>) -> IrExpr {
        let IrExprKind::Call { target, args, type_args } = &a.kind else {
            return a.clone();
        };
        let new_args: Vec<IrExpr> = args
            .iter()
            .map(|ia| if needs_anf_arg(ia) { anf_arg(ia, next, binds) } else { ia.clone() })
            .collect();
        let rebuilt = IrExpr {
            kind: IrExprKind::Call {
                target: target.clone(),
                args: new_args,
                type_args: type_args.clone(),
            },
            ty: a.ty.clone(),
            span: a.span.clone(),
            def_id: a.def_id,
        };
        let tmp = VarId(*next);
        *next += 1;
        binds.push(IrStmt {
            kind: IrStmtKind::Bind {
                var: tmp,
                mutability: Mutability::Let,
                ty: a.ty.clone(),
                value: rebuilt,
            },
            span: a.span.clone(),
        });
        IrExpr {
            kind: IrExprKind::Var { id: tmp },
            ty: a.ty.clone(),
            span: a.span.clone(),
            def_id: None,
        }
    }
    fn rewrite_tail(e: &IrExpr, next: &mut u32, changed: &mut bool) -> IrExpr {
        match &e.kind {
            IrExprKind::Block { stmts, expr } => {
                let new_tail =
                    expr.as_deref().map(|t| Box::new(rewrite_tail(t, next, changed)));
                IrExpr {
                    kind: IrExprKind::Block { stmts: stmts.clone(), expr: new_tail },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                }
            }
            IrExprKind::Call { target: CallTarget::Module { .. }, args, .. }
                if args.iter().any(needs_anf_arg) =>
            {
                *changed = true;
                let IrExprKind::Call { target, args, type_args } = &e.kind else {
                    unreachable!()
                };
                let mut binds = Vec::new();
                let new_args: Vec<IrExpr> = args
                    .iter()
                    .map(|a| {
                        if needs_anf_arg(a) {
                            anf_arg(a, next, &mut binds)
                        } else {
                            a.clone()
                        }
                    })
                    .collect();
                let call = IrExpr {
                    kind: IrExprKind::Call {
                        target: target.clone(),
                        args: new_args,
                        type_args: type_args.clone(),
                    },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                };
                IrExpr {
                    kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(call)) },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                }
            }
            _ => e.clone(),
        }
    }
    // C-127 TYPE AUTHORITY: `unwrap_or(o: Option[T], d: T)` type-checks with ONE
    // T, so the chain payload and the default are the SAME type — but an
    // under-constrained generic chain leaves the payload UNRESOLVED (`Option[B]`),
    // which is judged heap and routes the whole chain (and its lambda) down the
    // heap leg — invalid for the scalar values that actually flow. The default's
    // type is authoritative (the C-127 contract): DEEP-substitute the unresolved
    // payload spelling with the default's concrete type across the rewritten
    // tail. A resolved chain has payload == default, so the substitution is the
    // identity there.
    fn subst_ty(t: &Ty, from: &Ty, to: &Ty) -> Ty {
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        if t == from {
            return to.clone();
        }
        match t {
            Ty::Applied(c, args) => Ty::Applied(
                match c {
                    TCI::List => TCI::List,
                    other => other.clone(),
                },
                args.iter().map(|a| subst_ty(a, from, to)).collect(),
            ),
            Ty::Tuple(ts) => Ty::Tuple(ts.iter().map(|a| subst_ty(a, from, to)).collect()),
            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|a| subst_ty(a, from, to)).collect(),
                ret: Box::new(subst_ty(ret, from, to)),
            },
            _ => t.clone(),
        }
    }
    fn subst_expr(e: &mut IrExpr, from: &Ty, to: &Ty) {
        use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
        struct S<'a> {
            from: &'a Ty,
            to: &'a Ty,
        }
        impl IrMutVisitor for S<'_> {
            fn visit_expr_mut(&mut self, e: &mut IrExpr) {
                e.ty = subst_ty(&e.ty, self.from, self.to);
                if let IrExprKind::Lambda { params, .. } = &mut e.kind {
                    for (_, t) in params.iter_mut() {
                        *t = subst_ty(t, self.from, self.to);
                    }
                }
                walk_expr_mut(self, e);
            }
            fn visit_stmt_mut(&mut self, s: &mut almide_ir::IrStmt) {
                if let IrStmtKind::Bind { ty, .. } = &mut s.kind {
                    *ty = subst_ty(ty, self.from, self.to);
                }
                walk_stmt_mut(self, s);
            }
        }
        S { from, to }.visit_expr_mut(e);
    }
    // Pure guard, no recursion — named so the recursive `tail_unwrap_payload`
    // match arm below reads as one condition instead of three inlined clauses.
    fn is_unwrap_or_call(module: almide_lang::intern::Sym, func: almide_lang::intern::Sym, arg_count: usize) -> bool {
        func.as_str() == "unwrap_or" && matches!(module.as_str(), "option" | "result") && arg_count == 2
    }
    // Pure extraction, no recursion — the Option/Result payload ty an `unwrap_or`'s
    // first arg carries, or None outside those two shapes.
    fn unwrap_or_payload_ty(arg_ty: &Ty) -> Option<Ty> {
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        match arg_ty {
            Ty::Applied(TCI::Option, a) if a.len() == 1 => Some(a[0].clone()),
            Ty::Applied(TCI::Result, a) if a.len() == 2 => Some(a[0].clone()),
            _ => None,
        }
    }
    // Find the (unresolved payload, authoritative default) pair at the Block tail.
    fn tail_unwrap_payload(e: &IrExpr) -> Option<(Ty, Ty)> {
        match &e.kind {
            IrExprKind::Block { expr: Some(t), .. } => tail_unwrap_payload(t),
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if is_unwrap_or_call(*module, *func, args.len()) =>
            {
                let d_ty = args[1].ty.clone();
                let payload = unwrap_or_payload_ty(&args[0].ty)?;
                (payload != d_ty).then_some((payload, d_ty))
            }
            _ => None,
        }
    }
    let mut changed = false;
    let mut next = crate::lower::max_var_id(body) + 1;
    let mut out = rewrite_tail(body, &mut next, &mut changed);
    if changed {
        // Substitute across the WHOLE rewritten body — the chain links now live in
        // the hoisted binds, not inside the tail call's own subtree.
        if let Some((p, d_ty)) = tail_unwrap_payload(&out) {
            subst_expr(&mut out, &p, &d_ty);
        }
    }
    changed.then_some(out)
}
