/// Bundles the five values threaded unchanged through every recursive call
/// in `extract_invariants_from_stmt` / `try_hoist_expr` and their arm
/// helpers, so each fn stays at or under the `max-params` limit (2 params:
/// the node being visited, plus `&mut HoistCtx`). `loop_defined` is
/// per-scope — a `ForIn`/`While` arm rebuilds a fresh `HoistCtx` with an
/// extended set (reborrowing `vt`/`hoisted`/`pure_fns`/`mm`) before
/// descending into the loop body.
struct HoistCtx<'a> {
    loop_defined: &'a HashSet<VarId>,
    vt: &'a mut VarTable,
    hoisted: &'a mut Vec<IrStmt>,
    pure_fns: &'a HashSet<Sym>,
    mm: &'a MutationMap,
}

/// Try to extract invariant sub-expressions from a statement's value.
/// If the value of a Bind or Expr statement is loop-invariant, hoist it.
fn extract_invariants_from_stmt(
    stmt: &mut IrStmt,
    loop_defined: &HashSet<VarId>,
    vt: &mut VarTable,
    hoisted: &mut Vec<IrStmt>,
    pure_fns: &HashSet<Sym>,
    mm: &MutationMap,
) {
    let mut ctx = HoistCtx { loop_defined, vt, hoisted, pure_fns, mm };
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } => {
            try_hoist_expr(value, &mut ctx);
        }
        IrStmtKind::Expr { expr } => {
            try_hoist_expr(expr, &mut ctx);
        }
        // Don't hoist the whole RHS of assignments — the assignment itself
        // is a side effect (mutates a var). Only recurse into sub-expressions
        // if the RHS is complex enough to have hoistable sub-parts.
        IrStmtKind::Assign { value, .. } => {
            // The assignment itself stays in the loop, but sub-expressions of
            // the RHS may be hoistable (e.g., total = total + square(n) → hoist square(n)).
            try_hoist_expr(value, &mut ctx);
        }
        IrStmtKind::Guard { cond, .. } => {
            try_hoist_expr(cond, &mut ctx);
            // Do NOT hoist guard else — it's a control flow value (break/return),
            // not a computed expression. Hoisting ok(()) out of a guard makes
            // the hoisted binding's type (Result<(),_>) incompatible with
            // non-effect function return type (()).
        }
        // Explicit-preserve: only Bind / Expr / Assign / Guard values are
        // examined for invariant sub-expressions. The remaining statement
        // kinds carry no hoistable RHS in this analysis. Listing each one
        // makes a new IrStmtKind a compile error, not a silent skip.
        IrStmtKind::BindDestructure { .. } | IrStmtKind::FieldAssign { .. }
        | IrStmtKind::IndexAssign { .. } | IrStmtKind::MapInsert { .. }
        | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. }
        | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. }
        | IrStmtKind::Comment { .. } => {}
    }
}

/// If `expr` is loop-invariant and non-trivial, replace it with a Var reference
/// and push the original expression as a hoisted `let` binding.
/// Also recurses into sub-expressions to find hoistable parts.
fn try_hoist_expr(expr: &mut IrExpr, ctx: &mut HoistCtx) {
    // Check if the whole expression is hoistable
    if is_hoistable(expr, ctx.loop_defined, ctx.pure_fns) {
        let ty = expr.ty.clone();
        // Suffix each __licm with the next VarId so multiple hoists from the
        // same loop (especially nested loops that emit several bindings at
        // the same scope) don't shadow each other. Rust shadowing with
        // differing types silently breaks later uses — tracked down while
        // fixing the extract_q1_0_tensor inner loop regression.
        let var_name = almide_base::intern::sym(&format!("__licm_{}", ctx.vt.len()));
        let var = ctx.vt.alloc(var_name, ty.clone(), Mutability::Let, None);
        let original = std::mem::replace(expr, IrExpr {
            kind: IrExprKind::Var { id: var },
            ty: ty.clone(),
            span: expr.span, def_id: None,
        });
        ctx.hoisted.push(IrStmt {
            kind: IrStmtKind::Bind {
                var,
                mutability: Mutability::Let,
                ty,
                value: original,
            },
            span: None,
        });
        return;
    }

    // Otherwise, recurse into sub-expressions to find hoistable parts
    match &mut expr.kind {
        IrExprKind::Call { target, args, .. } => try_hoist_call(target, args, ctx),
        IrExprKind::RuntimeCall { args, .. } => {
            for arg in args {
                try_hoist_expr(arg, ctx);
            }
        }
        IrExprKind::BinOp { left, right, .. } => {
            try_hoist_expr(left, ctx);
            try_hoist_expr(right, ctx);
        }
        IrExprKind::UnOp { operand, .. } => {
            try_hoist_expr(operand, ctx);
        }
        IrExprKind::If { cond, then, else_ } => {
            try_hoist_expr(cond, ctx);
            try_hoist_expr(then, ctx);
            try_hoist_expr(else_, ctx);
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements {
                try_hoist_expr(e, ctx);
            }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields {
                try_hoist_expr(v, ctx);
            }
        }
        IrExprKind::Member { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            try_hoist_expr(object, ctx);
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            try_hoist_expr(object, ctx);
            try_hoist_expr(index, ctx);
        }
        IrExprKind::StringInterp { parts } => try_hoist_string_interp(parts, ctx),
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } => {
            try_hoist_expr(e, ctx);
        }
        IrExprKind::Range { start, end, .. } => {
            try_hoist_expr(start, ctx);
            try_hoist_expr(end, ctx);
        }
        // Nested loops: descend into the body so an expression that is
        // invariant w.r.t. BOTH the outer and inner loops (e.g. a struct
        // field read from a function parameter) can be hoisted all the
        // way out to the outer pre-loop region. Without this, a 248 MB
        // `file.clone().raw` sitting inside a doubly-nested decode loop
        // is rebuilt on every inner iteration.
        //
        // `loop_defined` is extended with the nested loop's variables so
        // we never hoist an expression that genuinely depends on the
        // inner loop (e.g. `byte_idx = bits_start + i / 8`).
        IrExprKind::ForIn { var, var_tuple, iterable, body } =>
            try_hoist_for_in(*var, var_tuple, iterable, body, ctx),
        IrExprKind::While { cond, body } => try_hoist_while(cond, body, ctx),
        // Explicit-preserve: the whole-expression hoist check above already
        // decided these nodes are not worth recursing into for sub-part
        // hoisting (they are leaves, control flow with their own scoping, or
        // wrappers handled by the whole-expr path). Listing every remaining
        // variant turns a new IrExprKind into a compile error, not a silent
        // dropped subtree.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::Match { .. } | IrExprKind::Block { .. }
        | IrExprKind::Fan { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::TailCall { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::EmptyMap | IrExprKind::SpreadRecord { .. }
        | IrExprKind::TupleIndex { .. } | IrExprKind::Lambda { .. }
        | IrExprKind::OptionNone | IrExprKind::Try { .. }
        | IrExprKind::Unwrap { .. } | IrExprKind::UnwrapOr { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => {}
    }
}

/// `Call { target, args, .. }` arm of [`try_hoist_expr`].
fn try_hoist_call(target: &mut CallTarget, args: &mut [IrExpr], ctx: &mut HoistCtx) {
    match target {
        CallTarget::Method { object, .. } => try_hoist_expr(object, ctx),
        CallTarget::Computed { callee } => try_hoist_expr(callee, ctx),
        other @ (CallTarget::Named { .. } | CallTarget::Module { .. }) => { let _ = other; }
    }
    for arg in args {
        try_hoist_expr(arg, ctx);
    }
}

/// `StringInterp { parts }` arm of [`try_hoist_expr`].
fn try_hoist_string_interp(parts: &mut [IrStringPart], ctx: &mut HoistCtx) {
    for part in parts {
        if let IrStringPart::Expr { expr: e } = part {
            try_hoist_expr(e, ctx);
        }
    }
}

/// `ForIn { var, var_tuple, iterable, body }` arm of [`try_hoist_expr`].
/// Descends into the body so an expression that is invariant w.r.t. BOTH
/// the outer and inner loops (e.g. a struct field read from a function
/// parameter) can be hoisted all the way out to the outer pre-loop region.
/// `loop_defined` is extended with the nested loop's variables so we never
/// hoist an expression that genuinely depends on the inner loop.
fn try_hoist_for_in(var: VarId, var_tuple: &mut Option<Vec<VarId>>, iterable: &mut IrExpr, body: &mut [IrStmt], ctx: &mut HoistCtx) {
    try_hoist_expr(iterable, ctx);
    let mut nested_defined = ctx.loop_defined.clone();
    nested_defined.insert(var);
    if let Some(vars) = var_tuple {
        for v in vars { nested_defined.insert(*v); }
    }
    collect_defined_vars_stmts(body, &mut nested_defined, ctx.mm);
    for stmt in body.iter_mut() {
        extract_invariants_from_stmt(stmt, &nested_defined, ctx.vt, ctx.hoisted, ctx.pure_fns, ctx.mm);
    }
}

/// `While { cond, body }` arm of [`try_hoist_expr`]. See [`try_hoist_for_in`]
/// for why the loop body is descended into with an extended `loop_defined`.
fn try_hoist_while(cond: &mut IrExpr, body: &mut [IrStmt], ctx: &mut HoistCtx) {
    try_hoist_expr(cond, ctx);
    let mut nested_defined = ctx.loop_defined.clone();
    collect_defined_vars_stmts(body, &mut nested_defined, ctx.mm);
    for stmt in body.iter_mut() {
        extract_invariants_from_stmt(stmt, &nested_defined, ctx.vt, ctx.hoisted, ctx.pure_fns, ctx.mm);
    }
}

/// An expression is hoistable if:
/// 1. All referenced variables are defined OUTSIDE the loop (not in `loop_defined`)
/// 2. It contains no calls to effect functions (side effects)
/// 3. It is not trivially cheap (skip Var, Lit*, Unit)
/// 4. It contains no control flow (loops, continue, break, return)
fn is_hoistable(expr: &IrExpr, loop_defined: &HashSet<VarId>, pure_fns: &HashSet<Sym>) -> bool {
    if is_trivial(expr) {
        return false;
    }
    if !is_pure(expr, pure_fns) {
        return false;
    }
    if has_control_flow(expr) {
        return false;
    }
    if has_assignment(expr) {
        return false;
    }
    // Never hoist a HEAP-typed result out of a loop (#696). Hoisting an
    // allocation (`var merged: List[Int] = []` → `__licm = []` before the
    // loop, `merged = __licm` inside) makes every iteration ALIAS one shared
    // buffer, so per-iteration value semantics survive only because the
    // AliasCow guard then clones on every in-place mutation — the quadratic
    // blow-up that OOM'd the merge sort (each `list.push` re-copied the whole
    // container). The hoist saves one allocation per iteration; the
    // correctness structure it demands costs O(n²). Scalars keep hoisting.
    if super::pass_alias_cow::is_heap_aliasable(&expr.ty) {
        return false;
    }
    refs_are_outside_loop(expr, loop_defined)
}

/// Returns true if the expression contains variable assignments.
/// Assignments are side effects that must not be hoisted out of loops.
fn has_assignment(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Assign { .. } | IrStmtKind::FieldAssign { .. } | IrStmtKind::IndexAssign { .. }) || has_assignment_stmt(s))
                || tail.as_ref().map_or(false, |e| has_assignment(e))
        }
        IrExprKind::If { cond, then, else_ } => has_assignment(cond) || has_assignment(then) || has_assignment(else_),
        IrExprKind::Match { subject, arms } => has_assignment(subject) || arms.iter().any(|a| has_assignment(&a.body)),
        // Explicit-preserve: only Block/If/Match are scanned for nested
        // assignments here; an Assign reachable through any other node kind
        // would not be hoistable for other reasons (impurity / control flow).
        // Listing each variant makes a new IrExprKind a compile error.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. }
        | IrExprKind::Fan { .. } | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. } | IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::List { .. }
        | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap
        | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }
        | IrExprKind::Tuple { .. } | IrExprKind::Range { .. }
        | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::Lambda { .. } | IrExprKind::StringInterp { .. }
        | IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. } | IrExprKind::OptionNone
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
        | IrExprKind::UnwrapOr { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. } | IrExprKind::Await { .. }
        | IrExprKind::Clone { .. } | IrExprKind::Deref { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => false,
    }
}

fn has_assignment_stmt(stmt: &IrStmt) -> bool {
    match &stmt.kind {
        IrStmtKind::Assign { .. } | IrStmtKind::FieldAssign { .. } | IrStmtKind::IndexAssign { .. } => true,
        IrStmtKind::Expr { expr } => has_assignment(expr),
        IrStmtKind::Bind { value, .. } => has_assignment(value),
        // Explicit-preserve: remaining statement kinds carry no nested
        // expression that could hide a hoistable assignment here. Listing
        // each one makes a new IrStmtKind a compile error.
        IrStmtKind::BindDestructure { .. } | IrStmtKind::MapInsert { .. }
        | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. }
        | IrStmtKind::Guard { .. } | IrStmtKind::RcInc { .. }
        | IrStmtKind::RcDec { .. } | IrStmtKind::Comment { .. } => false,
    }
}

/// Returns true if the expression contains loops, continue, break, or return.
/// These must never be hoisted out of their enclosing scope.
fn has_control_flow(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::ForIn { .. } | IrExprKind::While { .. } => true,
        IrExprKind::Continue | IrExprKind::Break => true,
        IrExprKind::BinOp { left, right, .. } => {
            has_control_flow(left) || has_control_flow(right)
        }
        IrExprKind::UnOp { operand, .. } => has_control_flow(operand),
        IrExprKind::Call { target, args, .. } => {
            let target_cf = match target {
                CallTarget::Method { object, .. } => has_control_flow(object),
                CallTarget::Computed { callee } => has_control_flow(callee),
                CallTarget::Named { .. } | CallTarget::Module { .. } => false,
            };
            target_cf || args.iter().any(|a| has_control_flow(a))
        }
        IrExprKind::RuntimeCall { args, .. } => {
            args.iter().any(|a| has_control_flow(a))
        }
        IrExprKind::If { cond, then, else_ } => {
            has_control_flow(cond) || has_control_flow(then) || has_control_flow(else_)
        }
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| has_control_flow_stmt(s))
                || expr.as_ref().is_some_and(|e| has_control_flow(e))
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().any(|e| has_control_flow(e))
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::OptionalChain { expr: e, .. } => {
            has_control_flow(e)
        }
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            has_control_flow(e) || has_control_flow(f)
        }
        // Explicit-preserve: these node kinds are treated as control-flow-free
        // for hoisting (Match/Lambda are conservatively reported as false here
        // because their bodies have their own scoping). Preserving the prior
        // `false` for every remaining variant makes a new IrExprKind a compile
        // error, not a silent miss.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
        | IrExprKind::Match { .. } | IrExprKind::Fan { .. }
        | IrExprKind::TailCall { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::EmptyMap | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Range { .. }
        | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::Lambda { .. } | IrExprKind::StringInterp { .. }
        | IrExprKind::OptionNone | IrExprKind::Await { .. }
        | IrExprKind::Borrow { .. } | IrExprKind::BoxNew { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::RenderedCall { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Hole | IrExprKind::Todo { .. } => false,
    }
}

fn has_control_flow_stmt(stmt: &IrStmt) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. }
        | IrStmtKind::FieldAssign { value, .. } => has_control_flow(value),
        IrStmtKind::Expr { expr } => has_control_flow(expr),
        IrStmtKind::Guard { cond, else_ } => has_control_flow(cond) || has_control_flow(else_),
        // Explicit-preserve: remaining statement kinds carry no expression
        // whose control flow would block hoisting here. Listing each one makes
        // a new IrStmtKind a compile error.
        IrStmtKind::BindDestructure { .. } | IrStmtKind::IndexAssign { .. }
        | IrStmtKind::MapInsert { .. } | IrStmtKind::ListSwap { .. }
        | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
        | IrStmtKind::ListCopySlice { .. } | IrStmtKind::RcInc { .. }
        | IrStmtKind::RcDec { .. } | IrStmtKind::Comment { .. } => false,
    }
}

/// Returns true if the expression is trivially cheap or should not be hoisted.
/// Lambda is included because closures rely on call-site context for type
/// inference in Rust — hoisting them to a standalone `let` binding strips
/// that context and causes `rustc` type annotation errors.
/// Range is included because a hoisted range becomes a `Vec::collect()`
/// bound outside the loop and then `clone()`d per outer iteration when
/// the inner for-loop consumes it; rendering the range inline lets
/// `expressions.rs::render ForIn` emit the bare `start..end` form.
fn is_trivial(expr: &IrExpr) -> bool {
    matches!(
        &expr.kind,
        IrExprKind::Var { .. }
        | IrExprKind::LitInt { .. }
        | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. }
        | IrExprKind::Unit
        | IrExprKind::OptionNone
        | IrExprKind::FnRef { .. }
        | IrExprKind::Lambda { .. }
        | IrExprKind::Range { .. }
    )
}
