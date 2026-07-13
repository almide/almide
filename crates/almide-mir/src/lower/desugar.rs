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
pub fn dump_desugared_ir(fn_name: &str, body: &IrExpr, layouts: &crate::lower::VariantLayouts) {
    if std::env::var("DBG_DESUGAR_FN").is_ok_and(|v| v == fn_name) {
        if std::env::var("DBG_DESUGAR_RAW").is_ok() {
            eprintln!("=== RAW {fn_name} ===\n{:#?}", desugar_all(body, fn_name == "main", layouts));
        } else {
            eprintln!(
                "=== DESUGARED {fn_name} ===\n{}",
                dump_ir(&desugar_all(body, fn_name == "main", layouts))
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
pub fn desugar_method_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{CallTarget, IrExpr, IrExprKind};
    use almide_lang::types::Ty;

    struct V {
        changed: bool,
    }
    impl IrMutVisitor for V {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            // Post-order: resolve the receiver / args (which may themselves be method calls)
            // BEFORE rewriting this node, so a chained `a.f().g()` resolves inside-out.
            walk_expr_mut(self, e);
            let (target, args) = match &mut e.kind {
                IrExprKind::Call { target, args, .. } => (target, args),
                IrExprKind::TailCall { target, args } => (target, args),
                _ => return,
            };
            let name = match &*target {
                CallTarget::Method { object, method } => {
                    if method.as_str().contains('.') {
                        Some(method.as_str().to_string())
                    } else if let Ty::Named(n, _) = &object.ty {
                        Some(format!("{}.{}", n.as_str(), method.as_str()))
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
) -> IrExpr {
    let mut cur = body.clone();
    loop {
        if let Some(r) = desugar_method_calls(&cur) {
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
        if let Some(r) = desugar_effect_unwrap(&cur, unit_main, layouts) {
            cur = r;
            continue;
        }
        if unit_main {
            if let Some(r) = desugar_unit_main_err_arms(&cur) {
                cur = r;
                continue;
            }
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
    let (i, var, ty, mutability, inner_stmts, inner_tail) =
        stmts.iter().enumerate().find_map(|(i, s)| match &s.kind {
            IrStmtKind::Bind { var, ty, value, mutability } => match &value.kind {
                IrExprKind::Block { stmts: inner, expr: Some(it) } => {
                    Some((i, *var, ty.clone(), *mutability, inner.clone(), (**it).clone()))
                }
                _ => None,
            },
            _ => None,
        })?;
    let mut new_stmts = stmts[..i].to_vec();
    new_stmts.extend(inner_stmts);
    new_stmts.push(IrStmt {
        kind: IrStmtKind::Bind { var, ty, value: inner_tail, mutability },
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
