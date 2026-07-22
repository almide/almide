/// Pass 3: Constant Propagation — replace vars bound to literals with the literal.

use std::collections::HashMap;
use almide_ir::*;

pub(super) fn constant_propagate(program: &mut IrProgram) {
    for f in &mut program.functions {
        let constants = collect_constants(&f.body);
        if !constants.is_empty() {
            propagate_expr(&mut f.body, &constants);
        }
    }
    for tl in &mut program.top_lets {
        let constants = collect_constants(&tl.value);
        if !constants.is_empty() {
            propagate_expr(&mut tl.value, &constants);
        }
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            let constants = collect_constants(&f.body);
            if !constants.is_empty() {
                propagate_expr(&mut f.body, &constants);
            }
        }
        for tl in &mut m.top_lets {
            let constants = collect_constants(&tl.value);
            if !constants.is_empty() {
                propagate_expr(&mut tl.value, &constants);
            }
        }
    }
}

/// Collect `let x = <literal>` bindings where x is immutable.
fn collect_constants(expr: &IrExpr) -> HashMap<VarId, IrExpr> {
    let mut out = HashMap::new();
    collect_constants_inner(expr, &mut out);
    out
}

fn collect_constants_inner(expr: &IrExpr, out: &mut HashMap<VarId, IrExpr>) {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts {
                if let IrStmtKind::Bind { var, value, mutability, .. } = &s.kind {
                    if matches!(mutability, Mutability::Let) && is_propagatable(value) {
                        out.insert(*var, value.clone());
                    }
                }
            }
            if let Some(t) = tail { collect_constants_inner(t, out); }
        }
        _ => {}
    }
}

/// Literals and simple Var references are safe to propagate.
fn is_propagatable(expr: &IrExpr) -> bool {
    matches!(&expr.kind,
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit
    )
}

/// Replace Var references with their constant values.
fn propagate_expr(expr: &mut IrExpr, constants: &HashMap<VarId, IrExpr>) {
    // Check if this Var can be replaced
    if let IrExprKind::Var { id } = &expr.kind {
        if let Some(replacement) = constants.get(id) {
            *expr = replacement.clone();
            return;
        }
    }
    // Recurse into subexpressions
    match &mut expr.kind {
        IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. } => propagate_expr_binop(expr, constants),
        IrExprKind::Block { .. } | IrExprKind::If { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. } => propagate_expr_control(expr, constants),
        IrExprKind::Match { .. } => propagate_expr_match(expr, constants),
        IrExprKind::Call { .. } => propagate_expr_call(expr, constants),
        IrExprKind::List { .. } | IrExprKind::Tuple { .. } | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Range { .. } | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::MapLiteral { .. } | IrExprKind::StringInterp { .. } => propagate_expr_containers(expr, constants),
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. } | IrExprKind::OptionSome { .. }
        | IrExprKind::Try { .. } | IrExprKind::Await { .. } => propagate_expr_wrap(expr, constants),
        // Do NOT propagate into lambda bodies — closures capture by value,
        // and replacing captured vars with literals breaks use-count tracking
        // (the captured var's use_count drops to 0, DCE removes the binding,
        // but CallTarget::Named still references the closure by name).
        IrExprKind::Lambda { .. } => {},
        _ => {}
    }
}

/// BinOp / UnOp: propagate into operands.
fn propagate_expr_binop(expr: &mut IrExpr, constants: &HashMap<VarId, IrExpr>) {
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => {
            propagate_expr(left, constants);
            propagate_expr(right, constants);
        }
        IrExprKind::UnOp { operand, .. } => propagate_expr(operand, constants),
        _ => unreachable!(),
    }
}

/// Block / If / ForIn / While: propagate into control-flow subexpressions and bodies.
fn propagate_expr_control(expr: &mut IrExpr, constants: &HashMap<VarId, IrExpr>) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { propagate_stmt(s, constants); }
            if let Some(t) = tail { propagate_expr(t, constants); }
        }
        IrExprKind::If { cond, then, else_ } => {
            propagate_expr(cond, constants);
            propagate_expr(then, constants);
            propagate_expr(else_, constants);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            propagate_expr(iterable, constants);
            for s in body { propagate_stmt(s, constants); }
        }
        IrExprKind::While { cond, body } => {
            propagate_expr(cond, constants);
            for s in body { propagate_stmt(s, constants); }
        }
        _ => unreachable!(),
    }
}

/// Match: propagate into subject, guards, and arm bodies.
fn propagate_expr_match(expr: &mut IrExpr, constants: &HashMap<VarId, IrExpr>) {
    let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
    propagate_expr(subject, constants);
    for a in arms {
        if let Some(g) = &mut a.guard { propagate_expr(g, constants); }
        propagate_expr(&mut a.body, constants);
    }
}

/// Call: propagate into the receiver (if any) and arguments.
fn propagate_expr_call(expr: &mut IrExpr, constants: &HashMap<VarId, IrExpr>) {
    let IrExprKind::Call { target, args, .. } = &mut expr.kind else { unreachable!() };
    if let CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } = target {
        propagate_expr(object, constants);
    }
    for a in args { propagate_expr(a, constants); }
}

/// List/Tuple/Record/SpreadRecord/Range/IndexAccess/MapAccess/Member/TupleIndex/MapLiteral/StringInterp:
/// propagate into each child expression.
fn propagate_expr_containers(expr: &mut IrExpr, constants: &HashMap<VarId, IrExpr>) {
    match &mut expr.kind {
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { propagate_expr(e, constants); }
        }
        IrExprKind::Record { fields, .. } => { for (_, v) in fields { propagate_expr(v, constants); } }
        IrExprKind::SpreadRecord { base, fields } => {
            propagate_expr(base, constants);
            for (_, v) in fields { propagate_expr(v, constants); }
        }
        IrExprKind::Range { start, end, .. } => {
            propagate_expr(start, constants);
            propagate_expr(end, constants);
        }
        IrExprKind::IndexAccess { object, index } => {
            propagate_expr(object, constants);
            propagate_expr(index, constants);
        }
        IrExprKind::MapAccess { object, key } => {
            propagate_expr(object, constants);
            propagate_expr(key, constants);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            propagate_expr(object, constants);
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { propagate_expr(k, constants); propagate_expr(v, constants); }
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { propagate_expr(e, constants); }
            }
        }
        _ => unreachable!(),
    }
}

/// ResultOk/ResultErr/OptionSome/Try/Await: propagate into the wrapped expression.
fn propagate_expr_wrap(expr: &mut IrExpr, constants: &HashMap<VarId, IrExpr>) {
    let (IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Await { expr: e }) = &mut expr.kind else { unreachable!() };
    propagate_expr(e, constants);
}

fn propagate_stmt(stmt: &mut IrStmt, constants: &HashMap<VarId, IrExpr>) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => propagate_expr(value, constants),
        IrStmtKind::IndexAssign { index, value, .. } => {
            propagate_expr(index, constants);
            propagate_expr(value, constants);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            propagate_expr(key, constants);
            propagate_expr(value, constants);
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            propagate_expr(a, constants);
            propagate_expr(b, constants);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            propagate_expr(end, constants);
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            propagate_expr(len, constants);
        }
        IrStmtKind::Guard { cond, else_ } => {
            propagate_expr(cond, constants);
            propagate_expr(else_, constants);
        }
        IrStmtKind::Expr { expr } => propagate_expr(expr, constants),
        IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => {}
    }
}
