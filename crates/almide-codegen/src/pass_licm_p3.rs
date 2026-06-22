/// Returns true if the expression is pure (no function calls, no I/O, no mutation).
/// Only pure expressions can be hoisted out of loops.
/// Conservative: any function call makes the expression impure.
fn is_pure(expr: &IrExpr, pure_fns: &HashSet<Sym>) -> bool {
    match &expr.kind {
        // Leaf nodes: always pure
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Var { .. } | IrExprKind::FnRef { .. } | IrExprKind::Hole
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::EmptyMap => true,

        // Function calls: pure if target is known-pure and all args are pure.
        IrExprKind::Call { target, args, .. } => {
            let call_pure = match target {
                CallTarget::Module { module, func, .. } => {
                    let key = almide_base::intern::sym(&format!("{}.{}", module, func));
                    pure_fns.contains(&key)
                }
                CallTarget::Named { name } => pure_fns.contains(name),
                // Method/Computed dispatch can hide effects → conservatively impure.
                CallTarget::Method { .. } | CallTarget::Computed { .. } => false,
            };
            call_pure && args.iter().all(|a| is_pure(a, pure_fns))
        }
        IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. } => false,

        // Operators: pure if operands are pure
        IrExprKind::BinOp { left, right, .. } => is_pure(left, pure_fns) && is_pure(right, pure_fns),
        IrExprKind::UnOp { operand, .. } => is_pure(operand, pure_fns),

        // Collection constructors: pure if elements are pure
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter().all(|e| is_pure(e, pure_fns)),
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| is_pure(v, pure_fns)),
        IrExprKind::SpreadRecord { base, fields } => is_pure(base, pure_fns) && fields.iter().all(|(_, v)| is_pure(v, pure_fns)),
        IrExprKind::MapLiteral { entries } => entries.iter().all(|(k, v)| is_pure(k, pure_fns) && is_pure(v, pure_fns)),
        IrExprKind::Range { start, end, .. } => is_pure(start, pure_fns) && is_pure(end, pure_fns),

        // Access: pure if sub-exprs are pure
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => is_pure(object, pure_fns),
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            is_pure(object, pure_fns) && is_pure(index, pure_fns)
        }

        // Wrappers: pure if inner is pure
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr } | IrExprKind::Clone { expr }
        | IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. }
        | IrExprKind::BoxNew { expr } | IrExprKind::ToVec { expr } => is_pure(expr, pure_fns),
        IrExprKind::UnwrapOr { expr, fallback } => is_pure(expr, pure_fns) && is_pure(fallback, pure_fns),

        // String interpolation: pure if all parts are pure
        IrExprKind::StringInterp { parts } => {
            parts.iter().all(|p| match p {
                IrStringPart::Expr { expr } => is_pure(expr, pure_fns),
                lit @ IrStringPart::Lit { .. } => { let _ = lit; true }
            })
        }

        // Everything else: conservatively impure. Listed explicitly so a new
        // IrExprKind is a compile error here, not a silently-impure default.
        IrExprKind::If { .. } | IrExprKind::Match { .. }
        | IrExprKind::Block { .. } | IrExprKind::Fan { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. }
        | IrExprKind::TailCall { .. } | IrExprKind::RuntimeCall { .. }
        | IrExprKind::Lambda { .. } | IrExprKind::Try { .. }
        | IrExprKind::Unwrap { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. } | IrExprKind::Await { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::Todo { .. } => false,
    }
}

/// Returns true if all variable references in the expression are outside the loop
/// (i.e., none of them are in `loop_defined`).
fn refs_are_outside_loop(expr: &IrExpr, loop_defined: &HashSet<VarId>) -> bool {
    match &expr.kind {
        IrExprKind::Var { id } => !loop_defined.contains(id),
        IrExprKind::Call { target, args, .. } => {
            let target_ok = match target {
                CallTarget::Method { object, .. } => refs_are_outside_loop(object, loop_defined),
                CallTarget::Computed { callee } => refs_are_outside_loop(callee, loop_defined),
                CallTarget::Named { .. } | CallTarget::Module { .. } => true,
            };
            target_ok && args.iter().all(|a| refs_are_outside_loop(a, loop_defined))
        }
        IrExprKind::BinOp { left, right, .. } => {
            refs_are_outside_loop(left, loop_defined) && refs_are_outside_loop(right, loop_defined)
        }
        IrExprKind::UnOp { operand, .. } => refs_are_outside_loop(operand, loop_defined),
        IrExprKind::If { cond, then, else_ } => {
            refs_are_outside_loop(cond, loop_defined)
                && refs_are_outside_loop(then, loop_defined)
                && refs_are_outside_loop(else_, loop_defined)
        }
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().all(|s| refs_are_outside_loop_stmt(s, loop_defined))
                && expr.as_ref().map_or(true, |e| refs_are_outside_loop(e, loop_defined))
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().all(|e| refs_are_outside_loop(e, loop_defined))
        }
        IrExprKind::Record { fields, .. } => {
            fields.iter().all(|(_, v)| refs_are_outside_loop(v, loop_defined))
        }
        IrExprKind::SpreadRecord { base, fields } => {
            refs_are_outside_loop(base, loop_defined)
                && fields.iter().all(|(_, v)| refs_are_outside_loop(v, loop_defined))
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            refs_are_outside_loop(object, loop_defined)
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            refs_are_outside_loop(object, loop_defined)
                && refs_are_outside_loop(index, loop_defined)
        }
        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } => {
            refs_are_outside_loop(expr, loop_defined)
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            refs_are_outside_loop(expr, loop_defined)
                && refs_are_outside_loop(fallback, loop_defined)
        }
        IrExprKind::StringInterp { parts } => {
            parts.iter().all(|p| match p {
                IrStringPart::Expr { expr } => refs_are_outside_loop(expr, loop_defined),
                lit @ IrStringPart::Lit { .. } => { let _ = lit; true }
            })
        }
        IrExprKind::MapLiteral { entries } => {
            entries.iter().all(|(k, v)| {
                refs_are_outside_loop(k, loop_defined) && refs_are_outside_loop(v, loop_defined)
            })
        }
        IrExprKind::Range { start, end, .. } => {
            refs_are_outside_loop(start, loop_defined)
                && refs_are_outside_loop(end, loop_defined)
        }
        IrExprKind::Lambda { body, params, .. } => {
            // Lambda params are local — don't count them as loop-defined.
            // But the lambda body's free variables still matter.
            // For simplicity, consider the whole lambda as not depending on loop vars
            // if its free variables don't reference loop-defined vars.
            // We need to exclude params from the check.
            let mut extended = loop_defined.clone();
            for (v, _) in params { extended.remove(v); }
            refs_are_outside_loop(body, &extended)
        }
        IrExprKind::Match { subject, arms } => {
            refs_are_outside_loop(subject, loop_defined)
                && arms.iter().all(|a| {
                    a.guard.as_ref().map_or(true, |g| refs_are_outside_loop(g, loop_defined))
                        && refs_are_outside_loop(&a.body, loop_defined)
                })
        }
        // Leaf nodes and nodes whose inner refs aren't tracked here: treated as
        // "all refs outside loop" (true). Listed explicitly so a new IrExprKind
        // is a compile error, not a silent always-true default.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::FnRef { .. } | IrExprKind::Fan { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. }
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::EmptyMap
        | IrExprKind::OptionNone | IrExprKind::Await { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::Hole
        | IrExprKind::Todo { .. } => true,
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
fn expr_is_pure_with(expr: &IrExpr, pure_fns: &HashSet<Sym>) -> bool {
    match &expr.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Var { .. } | IrExprKind::FnRef { .. } | IrExprKind::Hole
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::EmptyMap => true,

        IrExprKind::Call { target, args, .. } => {
            let call_pure = match target {
                CallTarget::Module { module, func, .. } => {
                    let key = almide_base::intern::sym(&format!("{}.{}", module, func));
                    pure_fns.contains(&key)
                }
                CallTarget::Named { name } => pure_fns.contains(name),
                // Method/Computed dispatch can hide effects → conservatively impure.
                CallTarget::Method { .. } | CallTarget::Computed { .. } => false,
            };
            call_pure && args.iter().all(|a| expr_is_pure_with(a, pure_fns))
        }
        IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. } => false,

        IrExprKind::BinOp { left, right, .. } => expr_is_pure_with(left, pure_fns) && expr_is_pure_with(right, pure_fns),
        IrExprKind::UnOp { operand, .. } => expr_is_pure_with(operand, pure_fns),
        IrExprKind::If { cond, then, else_ } => {
            expr_is_pure_with(cond, pure_fns) && expr_is_pure_with(then, pure_fns) && expr_is_pure_with(else_, pure_fns)
        }
        IrExprKind::Match { subject, arms } => {
            expr_is_pure_with(subject, pure_fns) && arms.iter().all(|a| expr_is_pure_with(&a.body, pure_fns))
        }
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().all(|s| stmt_is_pure_with(s, pure_fns))
                && expr.as_ref().map_or(true, |e| expr_is_pure_with(e, pure_fns))
        }
        IrExprKind::Lambda { body, .. } => expr_is_pure_with(body, pure_fns),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            elements.iter().all(|e| expr_is_pure_with(e, pure_fns))
        }
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| expr_is_pure_with(v, pure_fns)),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => expr_is_pure_with(object, pure_fns),
        IrExprKind::IndexAccess { object, index } => {
            expr_is_pure_with(object, pure_fns) && expr_is_pure_with(index, pure_fns)
        }
        IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Clone { expr: e }
        | IrExprKind::Deref { expr: e } | IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::BoxNew { expr: e } | IrExprKind::ToVec { expr: e } => expr_is_pure_with(e, pure_fns),
        IrExprKind::UnwrapOr { expr: e, fallback: f } => {
            expr_is_pure_with(e, pure_fns) && expr_is_pure_with(f, pure_fns)
        }
        IrExprKind::Range { start, end, .. } => expr_is_pure_with(start, pure_fns) && expr_is_pure_with(end, pure_fns),
        IrExprKind::StringInterp { parts } => {
            parts.iter().all(|p| match p {
                IrStringPart::Expr { expr } => expr_is_pure_with(expr, pure_fns),
                lit @ IrStringPart::Lit { .. } => { let _ = lit; true }
            })
        }
        // ForIn, While, Fan, Await, etc. — conservatively impure. Listed
        // explicitly so a new IrExprKind is a compile error here, not a
        // silently-impure default.
        IrExprKind::Fan { .. } | IrExprKind::ForIn { .. }
        | IrExprKind::While { .. } | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. }
        | IrExprKind::Await { .. } | IrExprKind::RcWrap { .. }
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
