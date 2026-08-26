
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
            // Admitted payloads (#1075 widening — the `value.as_*(v)?` /
            // `value.field(v, k)?` survivors of the json.as_*/json.get
            // aliases produce `Result[T, String]?` for every T below): the
            // i64-slot scalars ride the len-tag base impl bit-preservingly,
            // and ANY heap-Ok instantiation routes to the payload-type-
            // INDEPENDENT `_h` twin at the call-name layer
            // (`result_call_name`, C-149 — Ok shares the handle into
            // some(), Err is never read). E stays pinned to String, the
            // error type of every stdlib Result surface. The emitted name
            // is always `result.to_option` — the tracked seed
            // (`is_self_host_option_module_fn`) and the twin routing key on
            // it, so a later `match`/`??` over the bound result reads a
            // real materialized Option.
            let admits = match (&expr.ty, &e.ty) {
                (
                    Ty::Applied(TypeConstructorId::Result, a),
                    Ty::Applied(TypeConstructorId::Option, oa),
                ) if a.len() == 2
                    && matches!(a[1], Ty::String)
                    && oa.len() == 1
                    && a[0] == oa[0] =>
                {
                    matches!(a[0], Ty::Int | Ty::Bool | Ty::Float)
                        || crate::lower::is_heap_ty(&a[0])
                }
                _ => false,
            };
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
            // The payload-class matrix (#1233, the family rule): these asserts
            // read ONLY the variant TAG (stdlib/testing_assert.almd — no
            // payload access), so admissibility is a LAYOUT question, not a
            // payload-type question.
            //   Option twins: the 0-or-1-element-list layout's len@4 is the
            //     tag for EVERY payload class — any Option is admissible.
            //   Result twins: the LAYOUT follows the Ok side (the C-229
            //     lesson) — a HEAP-Ok Result is cap-as-tag @16 (the plain
            //     twin's read), a SCALAR-Ok Result is len-as-tag @4 (the
            //     `_sc` twin's read). Both err classes ride along unread.
            // Anything outside those cells (a shape with no layout, e.g. an
            // unresolved generic) still routes `_x` and walls at render.
            enum Route {
                Plain,
                Scalar,
                Wall,
            }
            let route = match func.as_str() {
                "assert_some" | "assert_none" => match args.first().map(|a| &a.ty) {
                    Some(Ty::Applied(TypeConstructorId::Option, a)) if a.len() == 1 => {
                        Route::Plain
                    }
                    _ => Route::Wall,
                },
                "assert_ok" | "assert_err" => match args.first().map(|a| &a.ty) {
                    Some(Ty::Applied(TypeConstructorId::Result, a)) if a.len() == 2 => {
                        if crate::lower::is_heap_ty(&a[0]) {
                            Route::Plain
                        } else {
                            Route::Scalar
                        }
                    }
                    _ => Route::Wall,
                },
                _ => return,
            };
            match route {
                Route::Plain => {}
                Route::Scalar => {
                    *func = sym(&format!("{}_sc", func.as_str()));
                    self.changed = true;
                }
                Route::Wall => {
                    *func = sym(&format!("{}_x", func.as_str()));
                    self.changed = true;
                }
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
    //
    // Three tables, split by role only to keep each arm list readable: leaves and
    // containers, then the operators and control flow, then the wrappers.
    kind_name_leaf(k)
        .or_else(|| kind_name_control(k))
        .or_else(|| kind_name_wrapper(k))
        .unwrap_or("<other>")
}

/// Literals, variables and the container constructors/accessors.
fn kind_name_leaf(k: &IrExprKind) -> Option<&'static str> {
    kind_name_literal(k).or_else(|| kind_name_container(k))
}

/// The literal/variable half of the leaf table — split from
/// `kind_name_leaf` for the complexity budget.
fn kind_name_literal(k: &IrExprKind) -> Option<&'static str> {
    let name = match k {
        IrExprKind::LitInt { .. } => "LitInt",
        IrExprKind::LitFloat { .. } => "LitFloat",
        IrExprKind::LitStr { .. } => "LitStr",
        IrExprKind::LitBool { .. } => "LitBool",
        IrExprKind::Unit => "Unit",
        IrExprKind::Var { .. } => "Var",
        IrExprKind::FnRef { .. } => "FnRef",
        _ => return None,
    };
    Some(name)
}

/// The container constructor/accessor half of the leaf table — split from
/// `kind_name_leaf` for the complexity budget.
fn kind_name_container(k: &IrExprKind) -> Option<&'static str> {
    let name = match k {
        IrExprKind::List { .. } => "List",
        IrExprKind::Record { .. } => "Record",
        IrExprKind::SpreadRecord { .. } => "SpreadRecord",
        IrExprKind::Tuple { .. } => "Tuple",
        IrExprKind::MapLiteral { .. } => "MapLiteral",
        IrExprKind::EmptyMap => "EmptyMap",
        IrExprKind::Range { .. } => "Range",
        IrExprKind::StringInterp { .. } => "StringInterp",
        IrExprKind::Member { .. } => "Member",
        IrExprKind::TupleIndex { .. } => "TupleIndex",
        IrExprKind::IndexAccess { .. } => "IndexAccess",
        IrExprKind::MapAccess { .. } => "MapAccess",
        _ => return None,
    };
    Some(name)
}

/// Operators, calls, control flow and the loop forms.
fn kind_name_control(k: &IrExprKind) -> Option<&'static str> {
    let name = match k {
        IrExprKind::BinOp { .. } => "BinOp",
        IrExprKind::UnOp { .. } => "UnOp",
        IrExprKind::If { .. } => "If",
        IrExprKind::Match { .. } => "Match",
        IrExprKind::Block { .. } => "Block",
        IrExprKind::Call { .. } => "Call",
        IrExprKind::TailCall { .. } => "TailCall",
        IrExprKind::RuntimeCall { .. } => "RuntimeCall",
        IrExprKind::Lambda { .. } => "Lambda",
        IrExprKind::ClosureCreate { .. } => "ClosureCreate",
        IrExprKind::ForIn { .. } => "ForIn",
        IrExprKind::While { .. } => "While",
        IrExprKind::Fan { .. } => "Fan",
        IrExprKind::Break => "Break",
        IrExprKind::Continue => "Continue",
        IrExprKind::IterChain { .. } => "IterChain",
        _ => return None,
    };
    Some(name)
}

/// The Result/Option constructors and the ownership wrappers.
fn kind_name_wrapper(k: &IrExprKind) -> Option<&'static str> {
    let name = match k {
        IrExprKind::ResultOk { .. } => "ResultOk",
        IrExprKind::ResultErr { .. } => "ResultErr",
        IrExprKind::OptionSome { .. } => "OptionSome",
        IrExprKind::OptionNone => "OptionNone",
        IrExprKind::Try { .. } => "Try",
        IrExprKind::Unwrap { .. } => "Unwrap",
        IrExprKind::UnwrapOr { .. } => "UnwrapOr",
        IrExprKind::Clone { .. } => "Clone",
        IrExprKind::Deref { .. } => "Deref",
        IrExprKind::Borrow { .. } => "Borrow",
        IrExprKind::ToVec { .. } => "ToVec",
        IrExprKind::BoxNew { .. } => "BoxNew",
        _ => return None,
    };
    Some(name)
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
                let bind = almide_ir::IrPattern::Bind { var: p, ty: e.ty.clone() };
                let payload = IrExpr {
                    kind: IrExprKind::Var { id: p },
                    ty: e.ty.clone(),
                    span: e.span.clone(),
                    def_id: e.def_id,
                };
                // Result and Option carry the SAME shape under different
                // constructors — Option's polarity is len-as-tag-opposite, so
                // the pattern pair must follow the subject's own type.
                let (hit_pat, miss_pat) = if expr.ty.is_result() {
                    (
                        almide_ir::IrPattern::Ok { inner: Box::new(bind) },
                        almide_ir::IrPattern::Err { inner: Box::new(almide_ir::IrPattern::Wildcard) },
                    )
                } else {
                    (
                        almide_ir::IrPattern::Some { inner: Box::new(bind) },
                        almide_ir::IrPattern::None,
                    )
                };
                let arms = vec![
                    almide_ir::IrMatchArm { pattern: hit_pat, guard: None, body: payload },
                    almide_ir::IrMatchArm {
                        pattern: miss_pat,
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
    let mut next = crate::lower::desugar_var_seed();
    let out = rewrite(body.clone(), &mut changed, &mut next);
    changed.then_some(out)
}

/// `carrier ?? f(..)!` over a HEAP (handle-carrying) payload — the `??` whose FALLBACK is an
/// INLINE propagating unwrap (#1375). The fallback arm lands in the heap arm lowering as a bare
/// `Unwrap`, whose `e! ≡ e` pass-through is the identity ONLY in a Result-typed arm; here the arm
/// is PAYLOAD-typed, so the pass-through yielded the fallback's whole `Result` BLOCK where the
/// merge expects the handle — `json.parse("hi") ?? json.parse("[1,2]")!` printed `true` (the
/// ok-discriminant read through the Json tag slot) instead of `[1,2]`.
///
/// Rewrite to the exact identity, which keeps the fallback CONDITIONAL (hoisting `f(..)!` into a
/// preceding `let` would evaluate — and propagate — it on the ok path too):
///
///   `a ?? f(..)!`  ≡  `(match a { ok($p) => ok($p), err(_) => f(..) })!`
///   `o ?? f(..)!`  ≡  `(match o { some($p) => ok($p), none => f(..) })!`
///
/// Both sides evaluate `a`/`o` once and `f(..)` only on the miss path; the `!` moved OUT of the
/// arm now sits in the proven `let x = e!` / call-arg-hoist position, and the match itself is the
/// Result-typed shape the arm pass-through is actually valid for. CALL-COUNT-INVARIANT (the two
/// operand calls appear exactly once before and after), so `mir == ir` holds without re-counting.
/// SCALAR payloads keep their own proven `??` route untouched — they never reach the heap arm.
/// The carrier's HIT/MISS pattern pair for a payload bound to `p`, or `None` when the
/// operand is neither an `Option[<payload>]` nor a `Result[<payload>, _]`. Shared by the
/// `?? f(..)!` rewrite below and the route-zoo replacement experiment
/// (`desugar_unwrap_or_to_match`).
fn carrier_patterns(
    carrier: &Ty,
    payload: &Ty,
    p: almide_ir::VarId,
) -> Option<(almide_ir::IrPattern, almide_ir::IrPattern)> {
    use almide_ir::IrPattern;
    use almide_lang::types::constructor::TypeConstructorId;
    let bind = IrPattern::Bind { var: p, ty: payload.clone() };
    match carrier {
        Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && a[0] == *payload => Some((
            IrPattern::Ok { inner: Box::new(bind) },
            IrPattern::Err { inner: Box::new(IrPattern::Wildcard) },
        )),
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 && a[0] == *payload => {
            Some((IrPattern::Some { inner: Box::new(bind) }, IrPattern::None))
        }
        _ => None,
    }
}

pub fn desugar_unwrap_or_unwrap_fallback(body: &IrExpr) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    fn rewrite(e: IrExpr, changed: &mut bool, next: &mut u32) -> IrExpr {
        let e = e.map_children(&mut |c| rewrite(c, changed, next));
        let IrExprKind::UnwrapOr { expr, fallback } = &e.kind else { return e };
        // `Try` is the frontend auto-`?`; both spellings propagate the same way.
        let (IrExprKind::Unwrap { expr: inner } | IrExprKind::Try { expr: inner }) = &fallback.kind
        else {
            return e;
        };
        // Formerly HEAP payloads only ("the scalar `??` never reaches the
        // miscompiling arm") — no longer true since the #1418 match-first
        // inversion: a SCALAR `a ?? f(..)!` whose match form reaches the
        // value-position machinery hits the same propagation-arm miscompile
        // class (#1421, invalid wasm). Rewriting BOTH payload classes to the
        // `(match …)!` statement-propagation form routes every propagating
        // fallback through the proven `desugar_let_unwrap` rails instead.
        // The fallback's `!` must yield EXACTLY the `??`'s own payload type, so `ok($p)` in the
        // hit arm and `f(..)` in the miss arm are the same `Result[payload, _]`.
        if !matches!(&inner.ty,
            Ty::Applied(TypeConstructorId::Result, a) if a.len() == 2 && a[0] == e.ty)
        {
            return e;
        }
        let p = almide_ir::VarId(*next);
        let Some((hit, miss)) = carrier_patterns(&expr.ty, &e.ty, p) else { return e };
        *next += 1;
        *changed = true;
        let payload = IrExpr {
            kind: IrExprKind::Var { id: p },
            ty: e.ty.clone(),
            span: e.span.clone(),
            def_id: None,
        };
        // The hit arm RE-WRAPS the payload at the fallback's Result type — the carrier's own
        // err type never has to join with the fallback's.
        let rewrapped = IrExpr {
            kind: IrExprKind::ResultOk { expr: Box::new(payload) },
            ty: inner.ty.clone(),
            span: e.span.clone(),
            def_id: None,
        };
        let merged = IrExpr {
            kind: IrExprKind::Match {
                subject: expr.clone(),
                arms: vec![
                    almide_ir::IrMatchArm { pattern: hit, guard: None, body: rewrapped },
                    almide_ir::IrMatchArm {
                        pattern: miss,
                        guard: None,
                        body: (**inner).clone(),
                    },
                ],
            },
            ty: inner.ty.clone(),
            span: e.span.clone(),
            def_id: e.def_id,
        };
        IrExpr {
            kind: IrExprKind::Unwrap { expr: Box::new(merged) },
            ty: e.ty.clone(),
            span: e.span.clone(),
            def_id: e.def_id,
        }
    }
    let mut changed = false;
    let mut next = crate::lower::desugar_var_seed();
    let out = rewrite(body.clone(), &mut changed, &mut next);
    changed.then_some(out)
}


include!("desugar_b_tail.rs");


/// Rewrite a `result.map` / `result.map_err` / `result.flat_map` call whose type
/// instantiation has NO linked typed twin — [`result_call_name`] answers the deliberately
/// unlinked `_x` — into the equivalent `match` (#1492's combinator leg):
///
///   map:      `match r { ok(v) => ok(f(v)),  err(e) => err(e) }`
///   map_err:  `match r { ok(v) => ok(v),     err(e) => err(f(e)) }`
///   flat_map: `match r { ok(v) => f(v),      err(e) => err(e) }`
///
/// This is the reference-compiler architecture for combinators (rustc lowers `?` to ONE
/// generic match; Swift's `??` is a library function; Roc desugars `??` to literally this
/// match in canonicalization): the combinator is not a special payload-class route, it is
/// the match the user could have written — and the heap-payload match now lowers. The `f`
/// argument is used exactly ONCE (no duplication); a non-Var subject is ANF-lifted into a
/// `let`, the optional-chain desugar's discipline. A twin-linked instantiation is left
/// untouched (its proven typed path stays byte-identical), and a match shape the lowering
/// still cannot express walls exactly as the `_x` name did — never worse, usually lowered.
pub fn desugar_result_combinator_to_match(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, CallTarget, IrMatchArm, IrMutVisitor, IrPattern};
    use almide_lang::types::constructor::TypeConstructorId;
    struct S {
        changed: bool,
        next_var: u32,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } =
                &e.kind
            else {
                return;
            };
            if module.as_str() != "result" || args.len() != 2 {
                return;
            }
            let fname = func.as_str().to_string();
            if !matches!(fname.as_str(), "map" | "map_err" | "flat_map") {
                return;
            }
            let Ty::Applied(TypeConstructorId::Result, ra) = &args[0].ty else { return };
            if ra.len() != 2 {
                return;
            }
            // Only when the router would emit the unlinked `_x`. Asking the SAME router the
            // lowering uses keeps the twin-availability logic in exactly one place — the
            // desugar can never disagree with the dispatch about which cells are linked.
            let arg_tys: Vec<Ty> = args.iter().map(|a| a.ty.clone()).collect();
            match result_call_name(&fname, &arg_tys, &e.ty) {
                Some(n) if n.ends_with("_x") => {}
                _ => return,
            }
            let Ty::Applied(TypeConstructorId::Result, oa) = &e.ty else { return };
            if oa.len() != 2 {
                return;
            }
            let (a_ty, e_ty) = (ra[0].clone(), ra[1].clone());
            let (b_ty, e2_ty) = (oa[0].clone(), oa[1].clone());
            let out_ty = e.ty.clone();
            let span = e.span.clone();
            let mk = |kind: IrExprKind, ty: Ty| IrExpr {
                kind,
                ty,
                span: span.clone(),
                def_id: None,
            };
            let r_expr = args[0].clone();
            let f_expr = args[1].clone();
            let v = VarId(self.next_var);
            self.next_var += 1;
            let w = VarId(self.next_var);
            self.next_var += 1;
            let call_f = |payload: IrExpr, ret: Ty| {
                mk(
                    IrExprKind::Call {
                        target: CallTarget::Computed { callee: Box::new(f_expr.clone()) },
                        args: vec![payload],
                        type_args: Vec::new(),
                    },
                    ret,
                )
            };
            let v_read = mk(IrExprKind::Var { id: v }, a_ty.clone());
            let w_read = mk(IrExprKind::Var { id: w }, e_ty.clone());
            let (ok_body, err_body) = match fname.as_str() {
                "map" => (
                    mk(
                        IrExprKind::ResultOk {
                            expr: Box::new(call_f(v_read, b_ty.clone())),
                        },
                        out_ty.clone(),
                    ),
                    mk(IrExprKind::ResultErr { expr: Box::new(w_read) }, out_ty.clone()),
                ),
                "map_err" => (
                    mk(IrExprKind::ResultOk { expr: Box::new(v_read) }, out_ty.clone()),
                    mk(
                        IrExprKind::ResultErr {
                            expr: Box::new(call_f(w_read, e2_ty.clone())),
                        },
                        out_ty.clone(),
                    ),
                ),
                // flat_map: the ok arm IS the whole result of f.
                _ => (
                    call_f(v_read, out_ty.clone()),
                    mk(IrExprKind::ResultErr { expr: Box::new(w_read) }, out_ty.clone()),
                ),
            };
            let arms = vec![
                IrMatchArm {
                    pattern: IrPattern::Ok {
                        inner: Box::new(IrPattern::Bind { var: v, ty: a_ty.clone() }),
                    },
                    guard: None,
                    body: ok_body,
                },
                IrMatchArm {
                    pattern: IrPattern::Err {
                        inner: Box::new(IrPattern::Bind { var: w, ty: e_ty.clone() }),
                    },
                    guard: None,
                    body: err_body,
                },
            ];
            // ANF-lift a non-Var subject so the match branches on a tracked bind — the
            // optional-chain desugar's exact discipline.
            let (stmts, subject) = if matches!(&r_expr.kind, IrExprKind::Var { .. }) {
                (Vec::new(), Box::new(r_expr))
            } else {
                let s_var = VarId(self.next_var);
                self.next_var += 1;
                let subj_ty = r_expr.ty.clone();
                let bind = IrStmt {
                    kind: IrStmtKind::Bind {
                        var: s_var,
                        mutability: almide_ir::Mutability::Let,
                        ty: subj_ty.clone(),
                        value: r_expr,
                    },
                    span: span.clone(),
                };
                (vec![bind], Box::new(mk(IrExprKind::Var { id: s_var }, subj_ty)))
            };
            let match_expr = mk(IrExprKind::Match { subject, arms }, out_ty.clone());
            *e = if stmts.is_empty() {
                match_expr
            } else {
                mk(IrExprKind::Block { stmts, expr: Some(Box::new(match_expr)) }, out_ty)
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false, next_var: crate::lower::desugar_var_seed() };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}
