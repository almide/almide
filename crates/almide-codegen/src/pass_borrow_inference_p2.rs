/// Check if a parameter variable needs ownership.
/// Conservative: marks as Owned if used in ANY ownership-requiring position.
fn check_needs_ownership(expr: &IrExpr, var: VarId, needs: &mut bool) {
    if *needs { return; }
    match &expr.kind {
        // ── Tail position: returned value needs ownership ──
        IrExprKind::Var { id } if *id == var => {
            // Bare var reference — context determines if ownership needed.
            // When used as a standalone expression (tail), it's returned → own.
            // But we handle tail detection at the Block level below.
        }

        IrExprKind::Block { stmts, expr: Some(tail) } => {
            for s in stmts { check_needs_ownership_stmt(s, var, needs); }
            if is_var(tail, var) { *needs = true; return; }
            check_needs_ownership(tail, var, needs);
        }
        IrExprKind::Block { stmts, expr: None } => {
            for s in stmts { check_needs_ownership_stmt(s, var, needs); }
        }

        // ── Concatenation consumes operands ──
        IrExprKind::BinOp { op: BinOp::ConcatStr | BinOp::ConcatList, left, right } => {
            if is_var(left, var) || is_var(right, var) { *needs = true; return; }
            check_needs_ownership(left, var, needs);
            check_needs_ownership(right, var, needs);
        }

        // ── Function call ──
        // For stdlib Module calls, consult arg_transforms to learn which args
        // are borrowed (BorrowRef / BorrowStr / BorrowMut) vs. consumed. Only
        // consumed args require ownership. This is what lets a hot loop like
        // `bytes.read_u32_le(data, pos)` pass `data` 50 000× without cloning.
        // For user-defined Named calls, consult the fixed-point SIGS snapshot
        // so a caller can transitively keep `data` borrowed when the callee
        // also borrows it.
        IrExprKind::Call { .. } => check_needs_ownership_call(expr, var, needs),

        // ── Collection construction consumes ──
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { if is_var(v, var) { *needs = true; return; } }
            for (_, v) in fields { check_needs_ownership(v, var, needs); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { if is_var(e, var) { *needs = true; return; } }
            for e in elements { check_needs_ownership(e, var, needs); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            if is_var(base, var) { *needs = true; return; }
            for (_, v) in fields { if is_var(v, var) { *needs = true; return; } }
            check_needs_ownership(base, var, needs);
            for (_, v) in fields { check_needs_ownership(v, var, needs); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { if is_var(k, var) || is_var(v, var) { *needs = true; return; } }
        }

        // ── Wrapping in Result/Option/Some ──
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } => {
            if is_var(expr, var) { *needs = true; return; }
            check_needs_ownership(expr, var, needs);
        }

        // ── Lambda capture: captured vars need ownership ──
        IrExprKind::Lambda { body, .. } => {
            if uses_var(body, var) { *needs = true; }
        }

        // ── String interpolation consumes ──
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p {
                    if is_var(expr, var) { *needs = true; return; }
                    check_needs_ownership(expr, var, needs);
                }
            }
        }

        // ── ForIn: iterable is consumed ──
        IrExprKind::ForIn { iterable, body, .. } => {
            if is_var(iterable, var) { *needs = true; return; }
            check_needs_ownership(iterable, var, needs);
            for s in body { check_needs_ownership_stmt(s, var, needs); }
        }

        // ── IterChain: source consumed if consume=true ──
        IrExprKind::IterChain { .. } => check_needs_ownership_iter_chain(expr, var, needs),

        // ── Safe reads (no ownership needed) ──
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            // Indexing borrows — safe
            check_needs_ownership(object, var, needs);
            check_needs_ownership(index, var, needs);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            check_needs_ownership(object, var, needs);
        }
        IrExprKind::BinOp { left, right, .. } => {
            // Non-concat binop: comparison, arithmetic — safe reads
            check_needs_ownership(left, var, needs);
            check_needs_ownership(right, var, needs);
        }

        // ── Control flow: recurse ──
        IrExprKind::If { cond, then, else_ } => {
            check_needs_ownership(cond, var, needs);
            if is_var(then, var) || is_var(else_, var) { *needs = true; return; }
            check_needs_ownership(then, var, needs);
            check_needs_ownership(else_, var, needs);
        }
        IrExprKind::Match { .. } => check_needs_ownership_match(expr, var, needs),
        IrExprKind::While { cond, body } => {
            check_needs_ownership(cond, var, needs);
            for s in body { check_needs_ownership_stmt(s, var, needs); }
        }

        // ── Wrappers: recurse ──
        IrExprKind::UnOp { operand, .. } => check_needs_ownership(operand, var, needs),
        IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            check_needs_ownership(expr, var, needs);
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            // Both the unwrapped value and the `??` fallback flow OUT as the result,
            // so a param used as either ESCAPES and needs ownership — else the
            // fallback arm renders as a borrowed `&str`/`&[T]` while the unwrapped
            // arm is owned, and the lowered match's arms mismatch (#414). Mirrors
            // the If/Match/Option-wrapping escaping-child handling above.
            if is_var(expr, var) || is_var(fallback, var) { *needs = true; return; }
            check_needs_ownership(expr, var, needs);
            check_needs_ownership(fallback, var, needs);
        }
        IrExprKind::OptionalChain { expr, .. } => check_needs_ownership(expr, var, needs),
        IrExprKind::Range { start, end, .. } => {
            check_needs_ownership(start, var, needs);
            check_needs_ownership(end, var, needs);
        }
        IrExprKind::Fan { exprs } => {
            for e in exprs { if is_var(e, var) { *needs = true; return; } }
            for e in exprs { check_needs_ownership(e, var, needs); }
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { check_needs_ownership(a, var, needs); }
        }
        // RuntimeCall: lowered form of `@intrinsic` / bundled Module call.
        // Its borrow signature lives in SIGS_SNAPSHOT keyed by the mangled
        // symbol. If the arg slot is Own, the call consumes that arg and
        // the enclosing var must also be owned.
        IrExprKind::RuntimeCall { .. } => check_needs_ownership_runtime_call(expr, var, needs),
        // Leaves and nodes that carry no borrowable child use of `var` in this
        // analysis. Explicit-preserve (not recurse-more): borrow inference is
        // sensitive to which refs are seen, so every un-handled variant is
        // listed with the original `=> {}` behaviour, total-by-construction.
        // `Var { id }` where `id != var` (the `id == var` case is the guarded
        // arm at the top — a bare self-reference whose ownership is decided by
        // its enclosing context, e.g. Block tail / arg position).
        IrExprKind::Var { .. }
        | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::FnRef { .. }
        | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::TailCall { .. } | IrExprKind::EmptyMap
        | IrExprKind::OptionNone | IrExprKind::RcWrap { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

// ── `check_needs_ownership` arm extraction (cog>100 decomposition) ──
//
// Each of these is a 1:1 text-move of one of the router's largest match
// arms (pattern 2: uniform match arms, mirrors the `lower_expr` /
// `infer_expr_inner` extraction shape). The router's `if *needs { return; }`
// entry guard stays exactly where it is (top of `check_needs_ownership`)
// and is NOT duplicated here — every recursive `check_needs_ownership(child,
// ..)` call, whether made from the router or from one of these helpers,
// re-enters the router and passes through that SAME guard, so the
// short-circuit-once-found semantics are identical to the inlined form.
// Every `*needs = true; return;` inside an arm now returns from the
// helper instead of from `check_needs_ownership` directly — since the
// router's match is its last statement with no trailing code, returning
// from the helper (which the router's arm then falls out of normally) is
// externally indistinguishable from the original early return.

/// `IrExprKind::Call` case of `check_needs_ownership`, extracted verbatim.
/// Bytes-only stdlib-aware bundled-Module branch of
/// `check_needs_ownership_call`. Only skip ownership for Bytes args in
/// stdlib Module calls — Lists/Strings keep the old conservative
/// behaviour to avoid lambda-typing regressions in filter/map. Returns
/// `true` if this branch fully handled the call (mirrors the original's
/// unconditional `return` once inside the `is_bundled_module` block),
/// `false` to fall through to the next check. Extracted from
/// `check_needs_ownership_call` (cog>30 decomposition, second round).
fn check_needs_ownership_call_bundled_module(target: &CallTarget, args: &[IrExpr], var: VarId, needs: &mut bool) -> bool {
    let CallTarget::Module { module, func, .. } = target else { return false; };
    if !almide_lang::stdlib_info::is_bundled_module(module.as_str()) { return false; }
    for (i, arg) in args.iter().enumerate() {
        let borrowed = bundled_borrow_at(module.as_str(), func.as_str(), i)
            && matches!(arg.ty, Ty::Bytes);
        if !borrowed && is_var(arg, var) {
            *needs = true;
            return true;
        }
    }
    for arg in args { check_needs_ownership(arg, var, needs); }
    true
}

/// Self-recursive Named call branch of `check_needs_ownership_call`:
/// treat optimistically. For tail-recursive parsers passing the same
/// `data` through, we don't want the first-pass pessimism to lock the
/// param to Own and prevent the fixed point from promoting it to Ref.
/// Same `bool` return convention as `check_needs_ownership_call_bundled_module`.
fn check_needs_ownership_call_self_recursive(target: &CallTarget, args: &[IrExpr], var: VarId, needs: &mut bool) -> bool {
    let CallTarget::Named { name } = target else { return false; };
    let is_self = CURRENT_FN.with(|c| c.borrow().as_deref() == Some(name.as_str()));
    if !is_self { return false; }
    for arg in args { check_needs_ownership(arg, var, needs); }
    true
}

/// User-defined Named call branch of `check_needs_ownership_call`: only
/// skip ownership when the arg is Bytes AND the callee borrows that slot.
/// Same `bool` return convention as the other branches.
fn check_needs_ownership_call_user_named(target: &CallTarget, args: &[IrExpr], var: VarId, needs: &mut bool) -> bool {
    let CallTarget::Named { name } = target else { return false; };
    let Some(borrows) = lookup_user_borrows(name.as_str()) else { return false; };
    for (i, arg) in args.iter().enumerate() {
        // The callee borrows slot `i` (Ref/RefSlice/RefStr)
        // → forwarding a heap-typed var into it does NOT
        // consume the var, so the outer param can stay
        // borrowed. Previously gated to `Ty::Bytes` only;
        // generalized to every heap type (records, lists,
        // strings) so the natural `vocab_id(t, ..)` /
        // `merge_rank(t, ..)` factoring no longer clones the
        // whole record per call (#647). Downstream rendering
        // is type-agnostic: walker/mod.rs:264 emits `&T`,
        // ref_params (walker/mod.rs:146) + the `&t`→`t`
        // collapse (walker/expressions.rs:847) already work.
        let borrowed = borrows.get(i).map_or(false, |b| !matches!(b, ParamBorrow::Own))
            && is_heap_type(&arg.ty);
        if !borrowed && is_var(arg, var) { *needs = true; return true; }
    }
    for arg in args { check_needs_ownership(arg, var, needs); }
    true
}

/// Non-stdlib fallback branch of `check_needs_ownership_call`: any arg use
/// needs ownership.
fn check_needs_ownership_call_fallback(target: &CallTarget, args: &[IrExpr], var: VarId, needs: &mut bool) {
    for arg in args {
        if is_var(arg, var) { *needs = true; return; }
    }
    if let CallTarget::Method { object, .. } = target {
        if is_var(object, var) { *needs = true; return; }
    }
    match target {
        CallTarget::Method { object, .. } => check_needs_ownership(object, var, needs),
        CallTarget::Computed { callee } => check_needs_ownership(callee, var, needs),
        _ => {}
    }
    for arg in args { check_needs_ownership(arg, var, needs); }
}

fn check_needs_ownership_call(expr: &IrExpr, var: VarId, needs: &mut bool) {
    let IrExprKind::Call { target, args, .. } = &expr.kind else { unreachable!() };
    if check_needs_ownership_call_bundled_module(target, args, var, needs) { return; }
    if check_needs_ownership_call_self_recursive(target, args, var, needs) { return; }
    if check_needs_ownership_call_user_named(target, args, var, needs) { return; }
    check_needs_ownership_call_fallback(target, args, var, needs);
}

/// `IrExprKind::IterChain` case of `check_needs_ownership`, extracted verbatim.
fn check_needs_ownership_iter_chain(expr: &IrExpr, var: VarId, needs: &mut bool) {
    let IrExprKind::IterChain { source, consume, steps, collector } = &expr.kind else { unreachable!() };
    if *consume && is_var(source, var) { *needs = true; return; }
    check_needs_ownership(source, var, needs);
    for step in steps {
        match step {
            IterStep::Map { lambda } | IterStep::Filter { lambda }
            | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                if uses_var(lambda, var) { *needs = true; return; }
            }
        }
    }
    match collector {
        IterCollector::Collect => {}
        IterCollector::Fold { init, lambda } => {
            if is_var(init, var) { *needs = true; return; }
            if uses_var(lambda, var) { *needs = true; return; }
        }
        IterCollector::Any { lambda } | IterCollector::All { lambda }
        | IterCollector::Find { lambda } | IterCollector::Count { lambda } => {
            if uses_var(lambda, var) { *needs = true; return; }
        }
    }
}

/// `IrExprKind::Match` case of `check_needs_ownership`, extracted verbatim.
fn check_needs_ownership_match(expr: &IrExpr, var: VarId, needs: &mut bool) {
    let IrExprKind::Match { subject, arms } = &expr.kind else { unreachable!() };
    // Match subject: destructuring a borrowed value changes bind types
    // → needs ownership to avoid &-pattern complications
    if is_var(subject, var) { *needs = true; return; }
    check_needs_ownership(subject, var, needs);
    for arm in arms {
        if let Some(g) = &arm.guard { check_needs_ownership(g, var, needs); }
        if is_var(&arm.body, var) { *needs = true; return; }
        check_needs_ownership(&arm.body, var, needs);
    }
}

/// `IrExprKind::RuntimeCall` case of `check_needs_ownership`, extracted
/// verbatim. Lowered form of `@intrinsic` / bundled Module call. Its
/// borrow signature lives in SIGS_SNAPSHOT keyed by the mangled symbol.
/// If the arg slot is Own, the call consumes that arg and the enclosing
/// var must also be owned.
fn check_needs_ownership_runtime_call(expr: &IrExpr, var: VarId, needs: &mut bool) {
    let IrExprKind::RuntimeCall { symbol, args } = &expr.kind else { unreachable!() };
    let borrows = SIGS_SNAPSHOT.with(|s| s.borrow().get(symbol.as_str()).cloned());
    if let Some(borrows) = borrows {
        for (i, arg) in args.iter().enumerate() {
            let is_own = matches!(borrows.get(i), Some(ParamBorrow::Own));
            if is_own && is_var(arg, var) { *needs = true; return; }
        }
    } else {
        // No sig: be conservative — any var arg needs ownership.
        for arg in args { if is_var(arg, var) { *needs = true; return; } }
    }
    for arg in args { check_needs_ownership(arg, var, needs); }
}

fn check_needs_ownership_stmt(stmt: &IrStmt, var: VarId, needs: &mut bool) {
    if *needs { return; }
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            check_needs_ownership(value, var, needs);
        }
        IrStmtKind::IndexAssign { index, value, .. } | IrStmtKind::MapInsert { key: index, value, .. } => {
            check_needs_ownership(index, var, needs);
            check_needs_ownership(value, var, needs);
        }
        IrStmtKind::Expr { expr } => check_needs_ownership(expr, var, needs),
        IrStmtKind::Guard { cond, else_ } => {
            check_needs_ownership(cond, var, needs);
            check_needs_ownership(else_, var, needs);
        }
        // Explicit-preserve: stmt kinds with no ownership-relevant child expr
        // (or only VarId operands). Same `=> {}` behaviour as before.
        IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. }
        | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => {}
    }
}

/// Companion to `check_needs_ownership`: true when the body passes
/// `var` into a callee slot that expects `RefMut`. Used to promote a
/// bundled body's own param from `Ref` to `RefMut` so the forwarded
/// `&mut {arg}` wraps a `&mut Vec<u8>` instead of a `&Vec<u8>`.
fn check_needs_refmut(expr: &IrExpr, var: VarId, needs: &mut bool) {
    if *needs { return; }
    match &expr.kind {
        IrExprKind::Var { .. } => {}
        IrExprKind::Block { stmts, expr } => {
            for s in stmts { check_needs_refmut_stmt(s, var, needs); }
            if let Some(tail) = expr { check_needs_refmut(tail, var, needs); }
        }
        IrExprKind::If { cond, then, else_ } => {
            check_needs_refmut(cond, var, needs);
            check_needs_refmut(then, var, needs);
            check_needs_refmut(else_, var, needs);
        }
        IrExprKind::Match { subject, arms } => {
            check_needs_refmut(subject, var, needs);
            for arm in arms {
                if let Some(g) = &arm.guard { check_needs_refmut(g, var, needs); }
                check_needs_refmut(&arm.body, var, needs);
            }
        }
        // `RuntimeCall` — lowered `@intrinsic` with a mangled symbol.
        // Consult SIGS_SNAPSHOT (which carries the @intrinsic seed
        // sigs, including implicit_mut promotions) to learn the callee
        // slot kinds.
        IrExprKind::RuntimeCall { symbol, args } => {
            if let Some(borrows) = SIGS_SNAPSHOT.with(|s| s.borrow().get(symbol.as_str()).cloned()) {
                for (i, arg) in args.iter().enumerate() {
                    if matches!(borrows.get(i), Some(ParamBorrow::RefMut))
                        && is_var(arg, var)
                    {
                        *needs = true;
                        return;
                    }
                }
            }
            for arg in args { check_needs_refmut(arg, var, needs); }
        }
        // Named / Module call — look up user-defined borrow signatures.
        IrExprKind::Call { target, args, .. } => {
            let callee_sig: Option<Vec<ParamBorrow>> = match target {
                CallTarget::Named { name } => lookup_user_borrows(name.as_str()),
                CallTarget::Module { module, func, .. } => {
                    let key = format!("{}::{}", module, func);
                    SIGS_SNAPSHOT.with(|s| s.borrow().get(&key).cloned())
                }
                _ => None,
            };
            if let Some(borrows) = callee_sig {
                for (i, arg) in args.iter().enumerate() {
                    if matches!(borrows.get(i), Some(ParamBorrow::RefMut))
                        && is_var(arg, var)
                    {
                        *needs = true;
                        return;
                    }
                }
            }
            if let CallTarget::Method { object, .. } = target {
                check_needs_refmut(object, var, needs);
            }
            for arg in args { check_needs_refmut(arg, var, needs); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            check_needs_refmut(iterable, var, needs);
            for s in body { check_needs_refmut_stmt(s, var, needs); }
        }
        IrExprKind::While { cond, body } => {
            check_needs_refmut(cond, var, needs);
            for s in body { check_needs_refmut_stmt(s, var, needs); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            check_needs_refmut(left, var, needs);
            check_needs_refmut(right, var, needs);
        }
        IrExprKind::UnOp { operand, .. } => check_needs_refmut(operand, var, needs),
        IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            check_needs_refmut(expr, var, needs);
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            check_needs_refmut(expr, var, needs);
            check_needs_refmut(fallback, var, needs);
        }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } => check_needs_refmut(expr, var, needs),
        IrExprKind::Lambda { body, .. } => check_needs_refmut(body, var, needs),
        // Explicit-preserve: nodes whose children cannot forward `var` into a
        // RefMut callee slot for the purposes of this analysis. Listed
        // explicitly with the original `=> {}` behaviour (total-by-construction).
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::FnRef { .. } | IrExprKind::Fan { .. }
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::TailCall { .. }
        | IrExprKind::List { .. } | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap
        | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }
        | IrExprKind::Tuple { .. } | IrExprKind::Range { .. }
        | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::OptionNone
        | IrExprKind::OptionalChain { .. } | IrExprKind::RcWrap { .. }
        | IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

fn check_needs_refmut_stmt(stmt: &IrStmt, var: VarId, needs: &mut bool) {
    if *needs { return; }
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            check_needs_refmut(value, var, needs);
        }
        IrStmtKind::IndexAssign { index, value, .. } | IrStmtKind::MapInsert { key: index, value, .. } => {
            check_needs_refmut(index, var, needs);
            check_needs_refmut(value, var, needs);
        }
        IrStmtKind::Expr { expr } => check_needs_refmut(expr, var, needs),
        IrStmtKind::Guard { cond, else_ } => {
            check_needs_refmut(cond, var, needs);
            check_needs_refmut(else_, var, needs);
        }
        // Explicit-preserve: stmt kinds with no RefMut-relevant child expr.
        // Same `=> {}` behaviour as before.
        IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. }
        | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => {}
    }
}

fn is_var(expr: &IrExpr, var: VarId) -> bool {
    matches!(&expr.kind, IrExprKind::Var { id } if *id == var)
}

fn uses_var(expr: &IrExpr, var: VarId) -> bool {
    match &expr.kind {
        IrExprKind::Var { id } => *id == var,
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| stmt_uses_var(s, var))
            || expr.as_ref().map_or(false, |e| uses_var(e, var))
        }
        IrExprKind::If { cond, then, else_ } => uses_var(cond, var) || uses_var(then, var) || uses_var(else_, var),
        IrExprKind::Call { args, target, .. } => {
            match target {
                CallTarget::Method { object, .. } => { if uses_var(object, var) { return true; } }
                CallTarget::Computed { callee } => { if uses_var(callee, var) { return true; } }
                _ => {}
            }
            args.iter().any(|a| uses_var(a, var))
        }
        IrExprKind::BinOp { left, right, .. } => uses_var(left, var) || uses_var(right, var),
        IrExprKind::UnOp { operand, .. } => uses_var(operand, var),
        IrExprKind::Lambda { body, .. } => uses_var(body, var),
        IrExprKind::Match { subject, arms } => {
            uses_var(subject, var) || arms.iter().any(|a| {
                a.guard.as_ref().map_or(false, |g| uses_var(g, var)) || uses_var(&a.body, var)
            })
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            uses_var(iterable, var) || body.iter().any(|s| stmt_uses_var(s, var))
        }
        IrExprKind::While { cond, body } => {
            uses_var(cond, var) || body.iter().any(|s| stmt_uses_var(s, var))
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => uses_var(object, var),
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            uses_var(object, var) || uses_var(index, var)
        }
        IrExprKind::StringInterp { parts } => parts.iter().any(|p| {
            matches!(p, IrStringPart::Expr { expr } if uses_var(expr, var))
        }),
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => uses_var(expr, var),
        IrExprKind::UnwrapOr { expr, fallback } => uses_var(expr, var) || uses_var(fallback, var),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => elements.iter().any(|e| uses_var(e, var)),
        IrExprKind::Record { fields, .. } => fields.iter().any(|(_, v)| uses_var(v, var)),
        IrExprKind::SpreadRecord { base, fields } => {
            uses_var(base, var) || fields.iter().any(|(_, v)| uses_var(v, var))
        }
        IrExprKind::IterChain { source, steps, collector, .. } => {
            uses_var(source, var)
            || steps.iter().any(|s| match s {
                IterStep::Map { lambda } | IterStep::Filter { lambda }
                | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => uses_var(lambda, var),
            })
            || match collector {
                IterCollector::Collect => false,
                IterCollector::Fold { init, lambda } => uses_var(init, var) || uses_var(lambda, var),
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => uses_var(lambda, var),
            }
        }
        IrExprKind::RustMacro { args, .. } => args.iter().any(|a| uses_var(a, var)),
        IrExprKind::RuntimeCall { args, .. } => args.iter().any(|a| uses_var(a, var)),
        IrExprKind::Range { start, end, .. } => uses_var(start, var) || uses_var(end, var),
        IrExprKind::MapLiteral { entries } => entries.iter().any(|(k, v)| uses_var(k, var) || uses_var(v, var)),
        _ => false,
    }
}

fn stmt_uses_var(stmt: &IrStmt, var: VarId) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => uses_var(value, var),
        IrStmtKind::IndexAssign { index, value, .. } | IrStmtKind::MapInsert { key: index, value, .. } => {
            uses_var(index, var) || uses_var(value, var)
        }
        IrStmtKind::Expr { expr } => uses_var(expr, var),
        IrStmtKind::Guard { cond, else_ } => uses_var(cond, var) || uses_var(else_, var),
        _ => false,
    }
}

