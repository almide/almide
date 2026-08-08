/// Is a call's destination known to be side-effect free? Method/Computed
/// dispatch can hide effects, so both are conservatively impure.
fn call_target_is_pure(target: &CallTarget, pure_fns: &HashSet<Sym>) -> bool {
    match target {
        CallTarget::Module { module, func, .. } => {
            let key = almide_base::intern::sym(&format!("{}.{}", module, func));
            pure_fns.contains(&key)
        }
        CallTarget::Named { name } => pure_fns.contains(name),
        CallTarget::Method { .. } | CallTarget::Computed { .. } => false,
    }
}

/// Returns true if the expression is pure (no function calls, no I/O, no mutation).
/// Only pure expressions can be hoisted out of loops.
/// Conservative: any function call makes the expression impure.
/// Grouped by CHILD SHAPE: apart from calls (which consult the pure-fn set)
/// and the conservatively-impure tail, a node is pure exactly when all of its
/// children are.
fn is_pure(expr: &IrExpr, pure_fns: &HashSet<Sym>) -> bool {
    let pure = |e: &IrExpr| is_pure(e, pure_fns);
    match &expr.kind {
        // ── No children: always pure ──
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Var { .. } | IrExprKind::FnRef { .. } | IrExprKind::Hole
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::EmptyMap => true,

        // ── Function calls: pure if the target is known-pure and so are the args ──
        IrExprKind::Call { target, args, .. } => {
            call_target_is_pure(target, pure_fns) && args.iter().all(pure)
        }
        IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. } => false,

        // ── One child ──
        IrExprKind::UnOp { operand: e, .. }
        | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Clone { expr: e }
        | IrExprKind::Deref { expr: e } | IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::BoxNew { expr: e } | IrExprKind::ToVec { expr: e } => pure(e),

        // ── Two children ──
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::UnwrapOr { expr: a, fallback: b }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::MapAccess { object: a, key: b }
        | IrExprKind::Range { start: a, end: b, .. } => pure(a) && pure(b),

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs } => {
            xs.iter().all(pure)
        }

        // ── Name-tagged children ──
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| pure(v)),
        IrExprKind::SpreadRecord { base, fields } => {
            pure(base) && fields.iter().all(|(_, v)| pure(v))
        }

        // ── Shapes with their own traversal ──
        IrExprKind::MapLiteral { entries } => {
            entries.iter().all(|(k, v)| pure(k) && pure(v))
        }
        IrExprKind::StringInterp { parts } => parts.iter().all(|p| match p {
            IrStringPart::Expr { expr } => pure(expr),
            IrStringPart::Lit { .. } => true,
        }),

        // Everything else: conservatively impure. Listed explicitly so a new
        // IrExprKind is a compile error here, not a silently-impure default.
        IrExprKind::If { .. } | IrExprKind::Match { .. }
        | IrExprKind::Block { .. } | IrExprKind::Fan { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. }
        | IrExprKind::TailCall { .. } | IrExprKind::RuntimeCall { .. }
        | IrExprKind::Lambda { .. } | IrExprKind::Try { .. }
        | IrExprKind::Unwrap { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::Todo { .. } => false,
    }
}

/// Returns true if all variable references in the expression are outside the loop
/// (i.e., none of them are in `loop_defined`).
/// Grouped by CHILD SHAPE: apart from `Var` (the actual test) and `Lambda`
/// (whose params shadow the loop's), a node's refs are outside the loop exactly
/// when all of its children's are.
fn refs_are_outside_loop(expr: &IrExpr, loop_defined: &HashSet<VarId>) -> bool {
    let outside = |e: &IrExpr| refs_are_outside_loop(e, loop_defined);
    match &expr.kind {
        IrExprKind::Var { id } => !loop_defined.contains(id),

        // ── One child ──
        IrExprKind::UnOp { operand: e, .. }
        | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::OptionalChain { expr: e, .. }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::Borrow { expr: e, .. } | IrExprKind::BoxNew { expr: e }
        | IrExprKind::ToVec { expr: e } => outside(e),

        // ── Two children ──
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::UnwrapOr { expr: a, fallback: b }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::MapAccess { object: a, key: b }
        | IrExprKind::Range { start: a, end: b, .. } => outside(a) && outside(b),

        // ── Three children ──
        IrExprKind::If { cond, then, else_ } => {
            outside(cond) && outside(then) && outside(else_)
        }

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs } => {
            xs.iter().all(outside)
        }

        // ── Name-tagged children ──
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| outside(v)),
        IrExprKind::SpreadRecord { base, fields } => {
            outside(base) && fields.iter().all(|(_, v)| outside(v))
        }

        // Shapes with their own traversal — see [`refs_are_outside_loop_nested`].
        IrExprKind::Call { .. } | IrExprKind::Block { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::Match { .. }
        | IrExprKind::Lambda { .. } => refs_are_outside_loop_nested(expr, loop_defined),

        // Leaf nodes and nodes whose inner refs aren't tracked here: treated as
        // "all refs outside loop" (true). Listed explicitly so a new IrExprKind
        // is a compile error, not a silent always-true default.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::FnRef { .. } | IrExprKind::Fan { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. }
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::EmptyMap
        | IrExprKind::OptionNone
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::Hole
        | IrExprKind::Todo { .. } => true,
    }
}

/// The [`refs_are_outside_loop`] shapes that do not fall out of a plain child
/// walk: a call's target and args, a block's statements and tail, a map/interp
/// literal's parts, a match's subject/guards/bodies, and a lambda (whose params
/// are LOCAL, so they are NOT loop-defined for its body — remove them before
/// checking the body's free variables).
fn refs_are_outside_loop_nested(expr: &IrExpr, loop_defined: &HashSet<VarId>) -> bool {
    let outside = |e: &IrExpr| refs_are_outside_loop(e, loop_defined);
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            let target_ok = match target {
                CallTarget::Method { object, .. } => outside(object),
                CallTarget::Computed { callee } => outside(callee),
                CallTarget::Named { .. } | CallTarget::Module { .. } => true,
            };
            target_ok && args.iter().all(outside)
        }
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().all(|s| refs_are_outside_loop_stmt(s, loop_defined))
                && expr.as_ref().is_none_or(|e| outside(e))
        }
        IrExprKind::MapLiteral { entries } => {
            entries.iter().all(|(k, v)| outside(k) && outside(v))
        }
        IrExprKind::StringInterp { parts } => parts.iter().all(|p| match p {
            IrStringPart::Expr { expr } => outside(expr),
            IrStringPart::Lit { .. } => true,
        }),
        IrExprKind::Match { subject, arms } => {
            outside(subject)
                && arms.iter().all(|a| {
                    a.guard.as_ref().is_none_or(|g| outside(g)) && outside(&a.body)
                })
        }
        // Lambda params are local, so they are NOT loop-defined for the body:
        // remove them before checking the body's free variables.
        IrExprKind::Lambda { body, params, .. } => {
            let mut extended = loop_defined.clone();
            for (v, _) in params {
                extended.remove(v);
            }
            refs_are_outside_loop(body, &extended)
        }
        _ => unreachable!("dispatched by refs_are_outside_loop's own arm list"),
    }
}

fn refs_are_outside_loop_stmt(stmt: &IrStmt, loop_defined: &HashSet<VarId>) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            refs_are_outside_loop(value, loop_defined)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            refs_are_outside_loop(index, loop_defined) && refs_are_outside_loop(value, loop_defined)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            refs_are_outside_loop(key, loop_defined) && refs_are_outside_loop(value, loop_defined)
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            refs_are_outside_loop(a, loop_defined) && refs_are_outside_loop(b, loop_defined)
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            refs_are_outside_loop(end, loop_defined)
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            refs_are_outside_loop(len, loop_defined)
        }
        IrStmtKind::Guard { cond, else_ } => {
            refs_are_outside_loop(cond, loop_defined) && refs_are_outside_loop(else_, loop_defined)
        }
        IrStmtKind::Expr { expr } => refs_are_outside_loop(expr, loop_defined),
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => !loop_defined.contains(var),
        IrStmtKind::Comment { .. } => true,
    }
}

/// Check if an `@inline_rust` template contains `&mut` (indicating mutation).
fn has_mut_in_inline_rust(attrs: &[almide_lang::ast::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.name.as_str() == "inline_rust"
            && a.args.first().map_or(false, |arg| {
                matches!(&arg.value, almide_lang::ast::AttrValue::String { value } if value.contains("&mut "))
            })
    })
}

// ── User function purity analysis (fixpoint) ──────────────────

/// Analyze all user functions and return the set of names that are pure.
/// A function is pure if its body contains no impure operations.
/// Uses fixpoint iteration: mark impure functions, propagate, repeat until stable.
fn analyze_pure_functions(program: &IrProgram) -> HashSet<Sym> {

    // Collect all function names
    let mut all_fns: HashSet<Sym> = HashSet::new();
    let mut fn_bodies: Vec<(Sym, &IrExpr)> = Vec::new();
    for func in &program.functions {
        all_fns.insert(func.name);
        fn_bodies.push((func.name, &func.body));
    }
    for module in &program.modules {
        for func in &module.functions {
            all_fns.insert(func.name);
            fn_bodies.push((func.name, &func.body));
        }
    }

    // Start: assume all functions are pure
    let mut pure_set = all_fns.clone();

    // Fixpoint: remove functions whose body is impure, repeat until stable
    loop {
        let mut changed = false;
        for &(name, body) in &fn_bodies {
            if !pure_set.contains(&name) { continue; }
            if !expr_is_pure_with(body, &pure_set) {
                pure_set.remove(&name);
                changed = true;
            }
        }
        if !changed { break; }
    }

    pure_set
}

/// Check if an expression is pure given a current set of known-pure user functions.
/// Similar to `is_pure` but works on immutable IR (no VarTable needed).
/// Grouped by CHILD SHAPE like [`is_pure`]. This variant covers a WIDER
/// impure tail: without a VarTable it cannot reason about the loop and
/// propagation nodes, so those stay conservatively impure here.
fn expr_is_pure_with(expr: &IrExpr, pure_fns: &HashSet<Sym>) -> bool {
    let pure = |e: &IrExpr| expr_is_pure_with(e, pure_fns);
    match &expr.kind {
        // ── No children: always pure ──
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Var { .. } | IrExprKind::FnRef { .. } | IrExprKind::Hole
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::EmptyMap => true,

        // ── Function calls ──
        IrExprKind::Call { target, args, .. } => {
            call_target_is_pure(target, pure_fns) && args.iter().all(pure)
        }
        IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. } => false,

        // ── One child ──
        IrExprKind::UnOp { operand: e, .. } | IrExprKind::Lambda { body: e, .. }
        | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Clone { expr: e }
        | IrExprKind::Deref { expr: e } | IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::BoxNew { expr: e } | IrExprKind::ToVec { expr: e } => pure(e),

        // ── Two children ──
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::UnwrapOr { expr: a, fallback: b }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::Range { start: a, end: b, .. } => pure(a) && pure(b),

        // ── Three children ──
        IrExprKind::If { cond, then, else_ } => pure(cond) && pure(then) && pure(else_),

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs } => {
            xs.iter().all(pure)
        }

        // ── Name-tagged children ──
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| pure(v)),

        // ── Shapes with their own traversal ──
        IrExprKind::Match { subject, arms } => {
            pure(subject) && arms.iter().all(|a| pure(&a.body))
        }
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().all(|s| stmt_is_pure_with(s, pure_fns))
                && expr.as_ref().is_none_or(|e| pure(e))
        }
        IrExprKind::StringInterp { parts } => parts.iter().all(|p| match p {
            IrStringPart::Expr { expr } => pure(expr),
            IrStringPart::Lit { .. } => true,
        }),

        // ForIn, While, Fan, Await, etc. — conservatively impure. Listed
        // explicitly so a new IrExprKind is a compile error here, not a
        // silently-impure default.
        IrExprKind::Fan { .. } | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. } | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. }
        | IrExprKind::RcWrap { .. }
        | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
        | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
        | IrExprKind::Todo { .. } => false,
    }
}

fn stmt_is_pure_with(stmt: &IrStmt, pure_fns: &HashSet<Sym>) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
            expr_is_pure_with(value, pure_fns)
        }
        // Assignments are mutations → impure
        IrStmtKind::Assign { .. } | IrStmtKind::IndexAssign { .. }
        | IrStmtKind::FieldAssign { .. } | IrStmtKind::MapInsert { .. }
        | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
        | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => false,
        IrStmtKind::Expr { expr } => expr_is_pure_with(expr, pure_fns),
        IrStmtKind::Guard { cond, else_ } => {
            expr_is_pure_with(cond, pure_fns) && expr_is_pure_with(else_, pure_fns)
        }
        IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => false,
        IrStmtKind::Comment { .. } => true,
    }
}
