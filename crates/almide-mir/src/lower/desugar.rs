/// GUARD-ELSE → conditional (Phase A of the v1→v0 parity plan). `guard cond else E; rest`
/// is a CONDITIONAL EARLY EXIT: when `!cond`, `E` becomes the result (a function early
/// return, or a loop `continue`/`break`). v1 has no early-return op, so DEFERRING it
/// (always-continue) silently miscompiled the `!cond` path (`guard len>0 else err();
/// ok(x)` returned ok for the empty input). This PURE IR rewrite restructures the block
/// so the SAME control flow runs through the proven `if` machinery:
///
///   `{ pre…; guard cond else E; rest…; tail }`
///     → `{ pre…; if cond then { rest…; tail } else E' }`
///
/// where `E'` is `E` verbatim EXCEPT a `continue` becomes `()`: `guard cond else continue;
/// rest` means "skip the rest of THIS iteration when `!cond`", which is exactly `if cond
/// then { rest } else ()` — eliminating the `continue` node so the scalar-loop path (which
/// declines a body with `continue`/`break`) accepts it. A `break` stays verbatim (it needs
/// real loop-exit — the residual guard-break shapes wall until break support lands). A
/// function-body guard's `E` (an `err(…)`/value) makes the `if` the block TAIL → the proven
/// heap/scalar-result-`if` handles the early-return value.
///
/// CALL-COUNT-INVARIANT: `cond`, `E`, and each `rest` statement appear EXACTLY ONCE before
/// and after (no duplication — `rest` nests in ONE arm), so `mir == ir` holds and the caps
/// gate stays exact without re-running. Recurses into every block (function body, loop
/// bodies, `if`/`match` arms) and handles CHAINED guards (a second guard in `rest` is
/// rewritten by the recursive call on the constructed then-block).
pub fn desugar_guard(body: &IrExpr) -> Option<IrExpr> {
    let mut changed = false;
    let rewritten = desugar_guard_rec(body.clone(), &mut changed);
    changed.then_some(rewritten)
}

/// Apply the FULL pre-lowering desugar fixpoint — the EXACT sequence (and order)
/// `lower_body_into` runs before it lowers a body: guard → beta-reduce → tuple-unwrap-or →
/// effect-unwrap → heap-branches, restarting from the top after each rewrite. The MIR the
/// lowering emits reflects this fully-desugared tree, so any consumer that must count calls /
/// interps 1:1 against the MIR (the caps `mir == ir` gate, the interp-coverage count in
/// classify_corpus) MUST read the SAME tree — otherwise a tail-duplicating rewrite (a
/// let-bound `match` pushing its continuation into each arm) duplicates a call in the MIR that
/// the under-desugared count never sees (the `option.unwrap_or((tuple)); f(r.0)` mir>ir breach).
/// This is the single "desugar-before-both" source of truth; callers use it instead of
/// hand-picking a subset of the desugars.
/// COMPACT IR pretty-printer for desugar debugging (env-gated via `DBG_DESUGAR_FN`). Shows the
/// tree structure — `Block`, `Match`/`If` with per-arm patterns, `Call` targets, `Try`/`Unwrap`,
/// ctors, `Var`/`Lit` — concise enough to `diff` two desugared bodies (e.g. a derived-Codec
/// `decode` vs the proven separate-bind form) and pinpoint where they diverge. NOT used at
/// runtime; a pure diagnostic reachable through `dump_desugared_ir`.
pub fn dump_ir(e: &IrExpr) -> String {
    fn go(e: &IrExpr, ind: usize, out: &mut String) {
        use almide_ir::{IrExprKind as K, IrStmtKind as S};
        let pad = "  ".repeat(ind);
        match &e.kind {
            K::Block { stmts, expr } => {
                out.push_str(&format!("{pad}Block\n"));
                for s in stmts {
                    match &s.kind {
                        S::Bind { var, value, .. } => {
                            out.push_str(&format!("{pad}  let v{}=\n", var.0));
                            go(value, ind + 2, out);
                        }
                        S::Expr { expr } => {
                            out.push_str(&format!("{pad}  expr\n"));
                            go(expr, ind + 2, out);
                        }
                        other => out.push_str(&format!("{pad}  stmt({other:?})\n")),
                    }
                }
                if let Some(t) = expr {
                    out.push_str(&format!("{pad}  tail\n"));
                    go(t, ind + 2, out);
                }
            }
            K::Match { subject, arms } => {
                out.push_str(&format!("{pad}Match\n{pad}  subj\n"));
                go(subject, ind + 2, out);
                for a in arms {
                    out.push_str(&format!("{pad}  arm {:?} =>\n", a.pattern));
                    go(&a.body, ind + 2, out);
                }
            }
            K::If { cond, then, else_ } => {
                out.push_str(&format!("{pad}If\n{pad}  cond\n"));
                go(cond, ind + 2, out);
                out.push_str(&format!("{pad}  then\n"));
                go(then, ind + 2, out);
                out.push_str(&format!("{pad}  else\n"));
                go(else_, ind + 2, out);
            }
            K::Call { target, args, .. } => {
                out.push_str(&format!("{pad}Call({})\n", crate::lower::call_target_kind(target)));
                for a in args {
                    go(a, ind + 1, out);
                }
            }
            K::Try { expr } => {
                out.push_str(&format!("{pad}Try\n"));
                go(expr, ind + 1, out);
            }
            K::Unwrap { expr } => {
                out.push_str(&format!("{pad}Unwrap\n"));
                go(expr, ind + 1, out);
            }
            K::ResultOk { expr } => {
                out.push_str(&format!("{pad}Ok\n"));
                go(expr, ind + 1, out);
            }
            K::ResultErr { expr } => {
                out.push_str(&format!("{pad}Err\n"));
                go(expr, ind + 1, out);
            }
            K::Record { name, fields } => {
                out.push_str(&format!("{pad}Record({:?}, {} fields)\n", name, fields.len()));
                for f in fields {
                    go(&f.1, ind + 1, out);
                }
            }
            K::Var { id } => out.push_str(&format!("{pad}v{}\n", id.0)),
            K::UnwrapOr { expr, fallback } => {
                out.push_str(&format!("{pad}UnwrapOr\n{pad}  val\n"));
                go(expr, ind + 2, out);
                out.push_str(&format!("{pad}  else\n"));
                go(fallback, ind + 2, out);
            }
            K::IndexAccess { object, index } => {
                out.push_str(&format!("{pad}Index\n"));
                go(object, ind + 1, out);
                go(index, ind + 1, out);
            }
            K::Member { object, field } => {
                out.push_str(&format!("{pad}Member(.{})\n", field.as_str()));
                go(object, ind + 1, out);
            }
            other => out.push_str(&format!("{pad}{}\n", crate::lower::kind_name(other))),
        }
    }
    let mut s = String::new();
    go(e, 0, &mut s);
    s
}

/// Env-gated desugar dump: when `DBG_DESUGAR_FN == fn_name`, print the fully-desugared body so the
/// derived-Codec `decode` chain can be diffed against the proven separate-bind form. No-op otherwise.
pub fn dump_desugared_ir(
    fn_name: &str,
    body: &IrExpr,
    layouts: &crate::lower::VariantLayouts,
    record_layouts: &crate::lower::RecordLayouts,
) {
    if std::env::var("DBG_DESUGAR_FN").is_ok_and(|v| v == fn_name) {
        if std::env::var("DBG_DESUGAR_RAW").is_ok() {
            eprintln!("=== RAW {fn_name} ===\n{:#?}", desugar_all(body, fn_name == "main", layouts, record_layouts, &[]));
        } else {
            eprintln!(
                "=== DESUGARED {fn_name} ===\n{}",
                dump_ir(&desugar_all(body, fn_name == "main", layouts, record_layouts, &[]))
            );
        }
    }
}

/// Resolve a UFCS / derived-method `Call`/`TailCall` whose target is `CallTarget::Method
/// { object, method }` to the concrete free function it names — the SAME resolution the v0
/// emitter does at emit time (`emit_wasm/calls_p2.rs`'s `Method` catch-all): a `Ty::Named(T)`
/// receiver → the derived/user fn `T.method` (`p.encode()` → `Person.encode(p)`), an
/// already-qualified `method` (contains '.') → that name verbatim. The receiver becomes the
/// FIRST argument, matching v0 (`emit_expr(object)` then the args). An unresolvable Method
/// (a non-`Named` receiver with a bare method — e.g. a stdlib UFCS the frontend left as Method)
/// is LEFT in place, so it still walls honestly rather than resolving to a bogus name. Run in
/// BOTH the real lowering (`lower_body_into`) and the `count_ir_calls` gate (`desugar_all`) so
/// `mir == ir` holds by construction. Returns `None` when no Method was rewritten.
pub fn desugar_method_calls(
    body: &IrExpr,
    record_layouts: &crate::lower::RecordLayouts,
) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{CallTarget, IrExpr, IrExprKind};
    use almide_lang::types::Ty;

    struct V<'a> {
        changed: bool,
        record_layouts: &'a crate::lower::RecordLayouts,
    }
    impl IrMutVisitor for V<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            // Post-order: resolve the receiver / args (which may themselves be method calls)
            // BEFORE rewriting this node, so a chained `a.f().g()` resolves inside-out.
            walk_expr_mut(self, e);
            let (target, args) = match &mut e.kind {
                IrExprKind::Call { target, args, .. } => (target, args),
                IrExprKind::TailCall { target, args } => (target, args),
                _ => return,
            };
            // A NAMED-record receiver whose "method" is a declared FN FIELD
            // (`h.run("hello")` over `type Handler = { run: (String) -> String, … }`):
            // the Type.method Named resolution below would fabricate an UNDEFINED
            // `Handler.run` fn — resolve the FIELD ty from the record registry and take
            // the field-call rewrite instead (the same Computed(Member) the structural
            // receiver gets).
            let named_fn_field: Option<Ty> = match &*target {
                CallTarget::Method { object, method } if !method.as_str().contains('.') => {
                    match &object.ty {
                        Ty::Named(n, _) => crate::lower::canonical_record_key(
                            self.record_layouts,
                            n.as_str(),
                        )
                        .and_then(|k| self.record_layouts.get(k))
                        .and_then(|(names, fields)| {
                            let _ = names;
                            fields
                                .iter()
                                .find(|(fname, _)| fname == method)
                                .map(|(_, t)| t.clone())
                        })
                        .filter(|t| matches!(t, Ty::Fn { .. })),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(field_ty) = named_fn_field {
                if let CallTarget::Method { object, method } = &*target {
                    let callee = IrExpr {
                        kind: IrExprKind::Member { object: object.clone(), field: *method },
                        ty: field_ty,
                        span: None,
                        def_id: None,
                    };
                    *target = CallTarget::Computed { callee: Box::new(callee) };
                    self.changed = true;
                    return;
                }
            }
            let name = match &*target {
                CallTarget::Method { object, method } => {
                    if method.as_str().contains('.') {
                        // A pre-dotted method (`Pigment.decode` via `varlib.Pigment.decode`)
                        // resolves through the derived-method owner map too (#790 codec
                        // bridge) — a uniquely-owned module type's codec method links the
                        // module-mangled derived fn instead of an unlinked bare name.
                        Some(crate::lower::resolve_derived_method_owner(
                            method.as_str().to_string(),
                        ))
                    } else if let Ty::Named(n, _) = &object.ty {
                        Some(crate::lower::resolve_derived_method_owner(format!(
                            "{}.{}",
                            n.as_str(),
                            method.as_str()
                        )))
                    } else if !matches!(&object.ty, Ty::Record { .. } | Ty::OpenRecord { .. }) {
                        // A non-Named, non-record receiver (`3.double()`,
                        // `"hello".exclaim()`): the checker already resolved stdlib
                        // UFCS to Module calls, so a SURVIVING Method here is plain
                        // free-fn UFCS — `x.f(a)` = `f(x, a)`. (A record receiver may
                        // be a FN-FIELD call — left for the Computed-callee brick.)
                        Some(method.as_str().to_string())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            // A STRUCTURAL-record receiver (`h.run("hello")` where `run: (String) ->
            // String` is a FN FIELD): the "method" is the field's closure — rewrite to
            // a Computed call through the Member read (`(h.run)("hello")`), which the
            // funcref/closure-call machinery executes. (A Named receiver keeps the
            // Type.method resolution above; count-invariant either way — one call.)
            let field_call = matches!(&*target,
                CallTarget::Method { object, .. }
                    if matches!(&object.ty, Ty::Record { .. } | Ty::OpenRecord { .. }));
            if field_call {
                if let CallTarget::Method { object, method } = &*target {
                    let field_ty = match &object.ty {
                        Ty::Record { fields } | Ty::OpenRecord { fields } => fields
                            .iter()
                            .find(|(n, _)| *n == *method)
                            .map(|(_, t)| t.clone()),
                        _ => None,
                    };
                    let Some(field_ty) = field_ty else { return };
                    let callee = IrExpr {
                        kind: IrExprKind::Member {
                            object: object.clone(),
                            field: *method,
                        },
                        ty: field_ty,
                        span: None,
                        def_id: None,
                    };
                    *target = CallTarget::Computed { callee: Box::new(callee) };
                    self.changed = true;
                    return;
                }
            }
            if let Some(name) = name {
                if let CallTarget::Method { object, .. } = target {
                    let obj = (**object).clone();
                    let mut new_args = Vec::with_capacity(args.len() + 1);
                    new_args.push(obj);
                    new_args.append(args);
                    *args = new_args;
                }
                *target = CallTarget::Named {
                    name: almide_lang::intern::sym(&name),
                };
                self.changed = true;
            }
        }
    }

    let mut v = V { changed: false, record_layouts };
    let mut out = body.clone();
    v.visit_expr_mut(&mut out);
    if v.changed {
        Some(out)
    } else {
        None
    }
}

/// `list.sort_by(xs, (x) => key)` → `list.sort_by_keys(xs, list.map(xs, (x) => key))` —
/// v0's `sort_by_cached_key` semantics made structural (C-055: the key fn runs ONCE PER
/// ELEMENT, n calls, on BOTH targets). The map leg then takes the DEFUNC inline path, so
/// a side-effectful key (`(x) => { calls = calls + 1; x }` — sort_by_call_count) mutates
/// its capture DIRECTLY and correctly; the sort itself is the closure-free
/// `list.sort_by_keys` self-host. Gated to a Var/literal source (no double evaluation),
/// an INLINE single-param lambda, and Int elements/keys (the self-host's domain).
pub fn desugar_sort_by_cached_keys(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    use almide_lang::types::constructor::TypeConstructorId;
    use almide_lang::types::Ty;
    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let hit = matches!(&e.kind,
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                if module.as_str() == "list" && func.as_str() == "sort_by" && args.len() == 2
                    && matches!(&args[0].kind, IrExprKind::Var { .. } | IrExprKind::List { .. })
                    && matches!(&args[0].ty, Ty::Applied(TypeConstructorId::List, a)
                        if a.len() == 1 && matches!(a[0], Ty::Int))
                    && matches!(&args[1].kind, IrExprKind::Lambda { params, .. } if params.len() == 1)
                    // The KEY type must be Int too — the synthesized keys list is typed
                    // `xs.ty` (List[Int]) and sort_by_keys compares raw i64 slots, so a
                    // String key linked the scalar list.map (indirect-call type mismatch
                    // trap, fuzz seed-20260718 index 866) and a Float key would sort by
                    // i64 BIT patterns (wrong order for negatives). Non-Int keys fall
                    // through to the mod_p4 sort_by route: Float → sort_by_float,
                    // String → the honest `sort_by_str_key_x` wall.
                    && matches!(&args[1].ty, Ty::Fn { ret, .. } if **ret == Ty::Int));
            if !hit {
                return;
            }
            let IrExprKind::Call { args, .. } = &e.kind else { unreachable!() };
            let xs = args[0].clone();
            let lam = args[1].clone();
            let keys = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module { module: sym("list"), func: sym("map"), def_id: None },
                    args: vec![xs.clone(), lam],
                    type_args: vec![],
                },
                ty: xs.ty.clone(),
                span: e.span.clone(),
                def_id: None,
            };
            e.kind = IrExprKind::Call {
                target: CallTarget::Module {
                    module: sym("list"),
                    func: sym("sort_by_keys"),
                    def_id: None,
                },
                args: vec![xs, keys],
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

pub fn desugar_all(
    body: &IrExpr,
    unit_main: bool,
    layouts: &crate::lower::VariantLayouts,
    record_layouts: &crate::lower::RecordLayouts,
    params: &[almide_ir::IrParam],
) -> IrExpr {
    let mut cur = body.clone();
    loop {
        if let Some(r) = desugar_method_calls(&cur, record_layouts) {
            cur = r;
            continue;
        }
        // assert/assert_eq/assert_ne → the controlled-halt `if`/die shape — the SAME
        // rewrite `lower_function_all_impl` applies before lowering. Without it here the
        // COUNTED tree kept the bare `assert_eq(a, b)` Call while the lowering emitted the
        // desugared eq's synthetic calls → a false `mir > ir` caps breach on every test fn
        // whose assert condition now lowers (desugar-before-both must mean BOTH).
        if let Some(r) = desugar_assert_calls(&cur) {
            cur = r;
            continue;
        }
        // `m[k]` → `map.get(m, k)` — same desugar-before-both contract as the assert
        // rewrite above (the counted Call node matches the lowering's one CallFn).
        if let Some(r) = desugar_map_access_calls(&cur) {
            cur = r;
            continue;
        }
        // `buf[i]` over Bytes → `bytes.index(buf, i)` — same contract.
        if let Some(r) = desugar_bytes_index_calls(&cur) {
            cur = r;
            continue;
        }
        // Matrix BinOps → matrix.mul/add/sub — same contract.
        if let Some(r) = desugar_matrix_binops(&cur) {
            cur = r;
            continue;
        }
        // `buf[i] = v` over Bytes → `bytes.set_at(buf, i, v)` — same contract
        // (the rewrite adds ONE counted Module call matching the lowering's CallFn).
        if let Some(r) = desugar_bytes_index_assign(&cur, params) {
            cur = r;
            continue;
        }
        // `xs[a..b]` slice RuntimeCall → `list.slice(xs, a, b)` — same contract
        // (an elided RuntimeCall becomes ONE counted pure Module call, both sides).
        if let Some(r) = desugar_list_slice_calls(&cur) {
            cur = r;
            continue;
        }
        // `p?.f` → the some/none match — same contract (adds no calls).
        if let Some(r) = desugar_optional_chain(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_guard(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_beta_reduce(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_tuple_unwrap_or(&cur) {
            cur = r;
            continue;
        }
        // `ret_is_result=false`: this debug-dump-only path has no per-fn ABI fact available;
        // the bare-tail-Option-`!` rewrite it skips is call-count-invariant, so the dump stays
        // representative for the count-diff use this function serves.
        if let Some(r) = desugar_effect_unwrap(&cur, unit_main, false, layouts) {
            cur = r;
            continue;
        }
        if unit_main {
            if let Some(r) = desugar_unit_main_err_arms(&cur) {
                cur = r;
                continue;
            }
        }
        if let Some(r) = desugar_sort_by_cached_keys(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_to_option_calls(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_offtype_testing_asserts(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_heap_branches(&cur, layouts) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_scalar_tuple_literal_match(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_scalar_guard_match(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_tuple_variant_match(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_tuple_variant_match_deep(&cur, layouts) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_tuple_empty_list_match(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_fan_block(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_record_destructure_match(&cur) {
            cur = r;
            continue;
        }
        if let Some(r) = desugar_list_pattern_match(&cur) {
            cur = r;
            continue;
        }
        break;
    }
    cur
}

/// Rewrite the FIRST `guard` in a loop-body statement list into an `if` statement:
/// `[pre…, Guard{cond,else}, rest…]` → `[pre…, Expr(if cond then { rest… } else E')]`
/// (`continue` → `()`, `break`/value verbatim). The `if`'s then-block is recursively
/// desugared so a chained guard in `rest` is handled. No guard → the list is returned
/// unchanged (recursion into each statement's sub-exprs already happened via the caller's
/// `map_children`, so only the list-level restructuring remains here).
fn rewrite_guard_stmt_list(
    body: Vec<almide_ir::IrStmt>,
    changed: &mut bool,
) -> Vec<almide_ir::IrStmt> {
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind};
    let Some(i) = body
        .iter()
        .position(|s| matches!(s.kind, IrStmtKind::Guard { .. }))
    else {
        return body;
    };
    // ONLY `continue` (→ `()`) and `break` (verbatim) are sound inside a loop body.
    // A VALUE else (`guard v != t else ok(mid)` — a function-level EARLY RETURN from
    // inside the loop) has no loop-exit channel here: rewriting it to
    // `if cond then { rest } else ok(mid)` in the Unit loop body silently DROPPED the
    // return and looped forever (binary_search hung at v == target). Leave the Guard
    // un-rewritten — the loop lowering declines a Guard body and walls honestly.
    if let IrStmtKind::Guard { else_, .. } = &body[i].kind {
        if !matches!(else_.kind, IrExprKind::Continue | IrExprKind::Break) {
            return body;
        }
    }
    *changed = true;
    let mut pre = body;
    let rest = pre.split_off(i + 1);
    let guard = pre.pop().expect("guard at index i");
    let (cond, else_, gspan) = match guard.kind {
        IrStmtKind::Guard { cond, else_ } => (cond, else_, guard.span),
        _ => unreachable!("position() found a Guard"),
    };
    // The then-body is `{ rest… }` (Unit); recurse so a further guard in it is rewritten.
    let then_block = desugar_guard_rec(
        IrExpr {
            kind: IrExprKind::Block { stmts: rest, expr: None },
            ty: Ty::Unit,
            span: gspan,
            def_id: None,
        },
        changed,
    );
    let else_branch = match &else_.kind {
        IrExprKind::Continue => IrExpr {
            kind: IrExprKind::Unit,
            ty: Ty::Unit,
            span: else_.span,
            def_id: else_.def_id,
        },
        _ => else_,
    };
    let if_expr = IrExpr {
        kind: IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(then_block),
            else_: Box::new(else_branch),
        },
        ty: Ty::Unit,
        span: gspan,
        def_id: None,
    };
    pre.push(IrStmt { kind: IrStmtKind::Expr { expr: if_expr }, span: gspan });
    pre
}

fn desugar_guard_rec(e: IrExpr, changed: &mut bool) -> IrExpr {
    use almide_ir::{IrExprKind, IrStmtKind};
    // Bottom-up: rewrite every child expression first (arm bodies, bind values — all
    // reachable through `map_children`), so a guard nested inside a sub-expression is
    // handled before this node's own restructuring.
    let e = e.map_children(&mut |c| desugar_guard_rec(c, changed));
    // A LOOP BODY is a `Vec<IrStmt>` (NOT a `Block` expr — `map_children` maps each
    // statement's sub-exprs but never restructures the list), so a `guard … else continue`
    // inside `for`/`while` needs its OWN statement-list rewrite: `[pre…, Guard, rest…]` →
    // `[pre…, Expr(if cond then { rest… } else E')]`. Same continue→() / break-verbatim rule.
    if let IrExprKind::ForIn { var, var_tuple, iterable, body } = e.kind {
        let new_body = rewrite_guard_stmt_list(body, changed);
        return IrExpr {
            kind: IrExprKind::ForIn { var, var_tuple, iterable, body: new_body },
            ty: e.ty,
            span: e.span,
            def_id: e.def_id,
        };
    }
    if let IrExprKind::While { cond, body } = e.kind {
        let new_body = rewrite_guard_stmt_list(body, changed);
        return IrExpr {
            kind: IrExprKind::While { cond, body: new_body },
            ty: e.ty,
            span: e.span,
            def_id: e.def_id,
        };
    }
    let IrExprKind::Block { stmts, expr } = &e.kind else {
        return e;
    };
    let Some(i) = stmts
        .iter()
        .position(|s| matches!(s.kind, IrStmtKind::Guard { .. }))
    else {
        return e;
    };
    *changed = true;
    let ty = e.ty.clone();
    let span = e.span.clone();
    let def_id = e.def_id;
    let mut pre = stmts.clone();
    let rest = pre.split_off(i + 1); // rest = stmts[i+1..]
    let guard = pre.pop().expect("guard at index i"); // remove the guard; pre = stmts[0..i]
    let (cond, else_) = match guard.kind {
        IrStmtKind::Guard { cond, else_ } => (cond, else_),
        _ => unreachable!("position() found a Guard"),
    };
    // The THEN branch (`cond` true) is the continuation `{ rest…; tail }`, same result
    // type as this block. Recurse so a FURTHER guard inside `rest` is rewritten too.
    let then_block = desugar_guard_rec(
        IrExpr {
            kind: IrExprKind::Block { stmts: rest, expr: expr.clone() },
            ty: ty.clone(),
            span,
            def_id,
        },
        changed,
    );
    // The ELSE branch (`cond` false) is `E`, except `continue` → `()` (see doc).
    let else_branch = match &else_.kind {
        IrExprKind::Continue => IrExpr {
            kind: IrExprKind::Unit,
            ty: Ty::Unit,
            span: else_.span,
            def_id: else_.def_id,
        },
        _ => else_,
    };
    let if_expr = IrExpr {
        kind: IrExprKind::If {
            cond: Box::new(cond),
            then: Box::new(then_block),
            else_: Box::new(else_branch),
        },
        ty: ty.clone(),
        span,
        def_id,
    };
    IrExpr {
        kind: IrExprKind::Block { stmts: pre, expr: Some(Box::new(if_expr)) },
        ty,
        span,
        def_id,
    }
}

/// `let v = { s…; tail }` — a let bound to a BLOCK — is FLATTENED to `s…; let v = tail`, hoisting the
/// inner statements into the enclosing block. REQUIRED so the inline-accumulator desugar sees the real
/// accumulator value: the let-bound-`if` pre-desugar turns `let new_acc = if c then { let b1=…; acc+
/// [..,b1] } else …` into a `let new_acc = { let b1=…; acc+[..,b1] }` arm, whose value is a Block (not a
/// ConcatList), so the inline skips it. Flattening yields `let b1=…; let new_acc = acc+[..,b1]` — now
/// the inline + the TCO admit it. VarIds are unique (no shadow), so hoisting the inner lets is sound.
/// base64 decode_chunks's nested byte-extraction; toml accumulators.
pub fn desugar_flatten_let_block(body: &IrExpr) -> Option<IrExpr> {
    let IrExprKind::Block { stmts, expr: tail } = &body.kind else {
        return None;
    };
    // `let x = { inner…; t }` AND `let (a, b) = { inner…; t }` (the fan sequential
    // rewrite's destructure-of-block) both splice the inner statements before a
    // rebuilt bind of the inner tail — a block value binds nothing extra, so the
    // lifetime extension is the same conservative one the Bind arm always took.
    enum Target {
        Bind { var: VarId, ty: Ty, mutability: almide_ir::Mutability },
        Destructure { pattern: almide_ir::IrPattern },
    }
    let (i, target, inner_stmts, inner_tail) =
        stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
            IrStmtKind::Bind { var, ty, value, mutability } => match &value.kind {
                IrExprKind::Block { stmts: inner, expr: Some(it) } => Some((
                    i,
                    Target::Bind { var: *var, ty: ty.clone(), mutability: *mutability },
                    inner.clone(),
                    (**it).clone(),
                )),
                _ => None,
            },
            IrStmtKind::BindDestructure { pattern, value } => match &value.kind {
                IrExprKind::Block { stmts: inner, expr: Some(it) } => Some((
                    i,
                    Target::Destructure { pattern: pattern.clone() },
                    inner.clone(),
                    (**it).clone(),
                )),
                _ => None,
            },
            _ => None,
        })?;
    let mut new_stmts = stmts[..i].to_vec();
    new_stmts.extend(inner_stmts);
    new_stmts.push(IrStmt {
        kind: match target {
            Target::Bind { var, ty, mutability } => {
                IrStmtKind::Bind { var, ty, value: inner_tail, mutability }
            }
            Target::Destructure { pattern } => {
                IrStmtKind::BindDestructure { pattern, value: inner_tail }
            }
        },
        span: None,
    });
    new_stmts.extend_from_slice(&stmts[i + 1..]);
    Some(IrExpr {
        kind: IrExprKind::Block { stmts: new_stmts, expr: tail.clone() },
        ty: body.ty.clone(),
        span: body.span.clone(),
        def_id: body.def_id,
    })
}
