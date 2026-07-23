/// VarId collection and remapping for `specialize_function`'s alpha-renaming.
///
/// Split out of `specialization.rs` (which owns cloning + type substitution)
/// purely to keep file size in check — no behavior change.
use std::collections::HashMap;
use almide_ir::*;

// ── VarId collection ────────────────────────────────────────────

pub(super) fn collect_var_id(id: VarId, out: &mut Vec<VarId>) {
    if !out.contains(&id) { out.push(id); }
}

pub(super) fn collect_varids_in_expr(expr: &IrExpr, out: &mut Vec<VarId>) {
    match &expr.kind {
        IrExprKind::Var { id } => collect_var_id(*id, out),
        IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. } | IrExprKind::If { .. }
        | IrExprKind::While { .. } => collect_varids_in_control(expr, out),
        IrExprKind::Match { .. } => collect_varids_in_match(expr, out),
        IrExprKind::Block { .. } => collect_varids_in_block(expr, out),
        IrExprKind::Call { .. } => collect_varids_in_call(expr, out),
        IrExprKind::ForIn { .. } => collect_varids_in_for_in(expr, out),
        IrExprKind::List { .. } | IrExprKind::Tuple { .. } | IrExprKind::Fan { .. }
        | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::Range { .. } | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::OptionalChain { .. } | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::RustMacro { .. } => collect_varids_in_containers(expr, out),
        IrExprKind::Lambda { .. } | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. } => {
            collect_varids_in_closure(expr, out)
        }
        IrExprKind::IterChain { .. } => collect_varids_in_iter_chain(expr, out),
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. } | IrExprKind::Try { .. }
        | IrExprKind::Await { .. } | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::Unwrap { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::UnwrapOr { .. } => collect_varids_in_wrap(expr, out),
        _ => {} // literals, unit, break, continue, etc.
    }
}

/// BinOp/UnOp/If/While: collect from operands, condition, and bodies.
fn collect_varids_in_control(expr: &IrExpr, out: &mut Vec<VarId>) {
    match &expr.kind {
        IrExprKind::BinOp { left, right, .. } => { collect_varids_in_expr(left, out); collect_varids_in_expr(right, out); }
        IrExprKind::UnOp { operand, .. } => collect_varids_in_expr(operand, out),
        IrExprKind::If { cond, then, else_ } => {
            collect_varids_in_expr(cond, out);
            collect_varids_in_expr(then, out);
            collect_varids_in_expr(else_, out);
        }
        IrExprKind::While { cond, body } => {
            collect_varids_in_expr(cond, out);
            for s in body { collect_varids_in_stmt(s, out); }
        }
        _ => unreachable!(),
    }
}

/// Block: collect from statements and tail.
fn collect_varids_in_block(expr: &IrExpr, out: &mut Vec<VarId>) {
    let IrExprKind::Block { stmts, expr } = &expr.kind else { unreachable!() };
    for s in stmts { collect_varids_in_stmt(s, out); }
    if let Some(e) = expr { collect_varids_in_expr(e, out); }
}

/// Lambda/ClosureCreate/EnvLoad: collect params/captures/env var and the lambda body.
fn collect_varids_in_closure(expr: &IrExpr, out: &mut Vec<VarId>) {
    match &expr.kind {
        IrExprKind::Lambda { params, body, .. } => {
            for (id, _) in params { collect_var_id(*id, out); }
            collect_varids_in_expr(body, out);
        }
        IrExprKind::ClosureCreate { captures, .. } => {
            for (id, _) in captures { collect_var_id(*id, out); }
        }
        IrExprKind::EnvLoad { env_var, .. } => collect_var_id(*env_var, out),
        _ => unreachable!(),
    }
}

/// Call: collect from the receiver (if any) and arguments.
fn collect_varids_in_call(expr: &IrExpr, out: &mut Vec<VarId>) {
    let IrExprKind::Call { target, args, .. } = &expr.kind else { unreachable!() };
    match target {
        CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => collect_varids_in_expr(object, out),
        _ => {}
    }
    for a in args { collect_varids_in_expr(a, out); }
}

/// ForIn: collect the loop var(s), the iterable, and the body.
fn collect_varids_in_for_in(expr: &IrExpr, out: &mut Vec<VarId>) {
    let IrExprKind::ForIn { var, var_tuple, iterable, body } = &expr.kind else { unreachable!() };
    collect_var_id(*var, out);
    if let Some(tvs) = var_tuple { for tv in tvs { collect_var_id(*tv, out); } }
    collect_varids_in_expr(iterable, out);
    for s in body { collect_varids_in_stmt(s, out); }
}

/// List/Tuple/Fan/Record/SpreadRecord/MapLiteral/Range/Member/TupleIndex/OptionalChain/
/// IndexAccess/MapAccess/StringInterp/RustMacro: collect from each child expression.
fn collect_varids_in_containers(expr: &IrExpr, out: &mut Vec<VarId>) {
    match &expr.kind {
        IrExprKind::List { .. } | IrExprKind::Tuple { .. } | IrExprKind::Fan { .. }
        | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. } | IrExprKind::MapLiteral { .. } => {
            collect_varids_in_containers_literals(expr, out)
        }
        IrExprKind::Range { .. } | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::OptionalChain { .. } | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::RustMacro { .. } => {
            collect_varids_in_containers_access(expr, out)
        }
        _ => unreachable!(),
    }
}

/// List/Tuple/Fan/Record/SpreadRecord/MapLiteral: collect from each element/entry.
fn collect_varids_in_containers_literals(expr: &IrExpr, out: &mut Vec<VarId>) {
    match &expr.kind {
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
            for e in elements { collect_varids_in_expr(e, out); }
        }
        IrExprKind::Record { fields, .. } => { for (_, e) in fields { collect_varids_in_expr(e, out); } }
        IrExprKind::SpreadRecord { base, fields } => {
            collect_varids_in_expr(base, out);
            for (_, e) in fields { collect_varids_in_expr(e, out); }
        }
        IrExprKind::MapLiteral { entries } => { for (k, v) in entries { collect_varids_in_expr(k, out); collect_varids_in_expr(v, out); } }
        _ => unreachable!(),
    }
}

/// Range/Member/TupleIndex/OptionalChain/IndexAccess/MapAccess/StringInterp/RustMacro:
/// collect from each accessed sub-expression.
fn collect_varids_in_containers_access(expr: &IrExpr, out: &mut Vec<VarId>) {
    match &expr.kind {
        IrExprKind::Range { start, end, .. } => { collect_varids_in_expr(start, out); collect_varids_in_expr(end, out); }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => collect_varids_in_expr(object, out),
        IrExprKind::IndexAccess { object, index } => { collect_varids_in_expr(object, out); collect_varids_in_expr(index, out); }
        IrExprKind::MapAccess { object, key } => { collect_varids_in_expr(object, out); collect_varids_in_expr(key, out); }
        IrExprKind::StringInterp { parts } => {
            for p in parts { if let IrStringPart::Expr { expr } = p { collect_varids_in_expr(expr, out); } }
        }
        IrExprKind::RustMacro { args, .. } => { for a in args { collect_varids_in_expr(a, out); } }
        _ => unreachable!(),
    }
}

/// ResultOk/ResultErr/OptionSome/Try/Await/Clone/Deref/Borrow/BoxNew/RcWrap/ToVec/Unwrap/ToOption/UnwrapOr:
/// collect from the wrapped expression(s).
fn collect_varids_in_wrap(expr: &IrExpr, out: &mut Vec<VarId>) {
    match &expr.kind {
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } | IrExprKind::Clone { expr }
        | IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. }
        | IrExprKind::BoxNew { expr } | IrExprKind::RcWrap { expr, .. }
        | IrExprKind::ToVec { expr } | IrExprKind::Unwrap { expr }
        | IrExprKind::ToOption { expr } => collect_varids_in_expr(expr, out),
        IrExprKind::UnwrapOr { expr, fallback } => {
            collect_varids_in_expr(expr, out);
            collect_varids_in_expr(fallback, out);
        }
        _ => unreachable!(),
    }
}

fn collect_varids_in_match(expr: &IrExpr, out: &mut Vec<VarId>) {
    let IrExprKind::Match { subject, arms } = &expr.kind else { unreachable!() };
    collect_varids_in_expr(subject, out);
    for arm in arms {
        collect_varids_in_pattern(&arm.pattern, out);
        if let Some(g) = &arm.guard { collect_varids_in_expr(g, out); }
        collect_varids_in_expr(&arm.body, out);
    }
}

fn collect_varids_in_iter_chain(expr: &IrExpr, out: &mut Vec<VarId>) {
    let IrExprKind::IterChain { source, steps, collector, .. } = &expr.kind else { unreachable!() };
    collect_varids_in_expr(source, out);
    for step in steps {
        match step {
            IterStep::Map { lambda } | IterStep::Filter { lambda }
            | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => collect_varids_in_expr(lambda, out),
        }
    }
    match collector {
        IterCollector::Collect => {}
        IterCollector::Fold { init, lambda } => { collect_varids_in_expr(init, out); collect_varids_in_expr(lambda, out); }
        IterCollector::Any { lambda } | IterCollector::All { lambda }
        | IterCollector::Find { lambda } | IterCollector::Count { lambda } => collect_varids_in_expr(lambda, out),
    }
}

fn collect_varids_in_stmt(stmt: &IrStmt, out: &mut Vec<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { var, value, .. } => { collect_var_id(*var, out); collect_varids_in_expr(value, out); }
        IrStmtKind::BindDestructure { pattern, value } => { collect_varids_in_pattern(pattern, out); collect_varids_in_expr(value, out); }
        IrStmtKind::Assign { var, value } => { collect_var_id(*var, out); collect_varids_in_expr(value, out); }
        IrStmtKind::IndexAssign { target, index, value } => { collect_var_id(*target, out); collect_varids_in_expr(index, out); collect_varids_in_expr(value, out); }
        IrStmtKind::MapInsert { target, key, value } => { collect_var_id(*target, out); collect_varids_in_expr(key, out); collect_varids_in_expr(value, out); }
        IrStmtKind::FieldAssign { target, value, .. } => { collect_var_id(*target, out); collect_varids_in_expr(value, out); }
        IrStmtKind::ListSwap { target, a, b } => { collect_var_id(*target, out); collect_varids_in_expr(a, out); collect_varids_in_expr(b, out); }
        IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => { collect_var_id(*target, out); collect_varids_in_expr(end, out); }
        IrStmtKind::ListCopySlice { dst, src, len } => { collect_var_id(*dst, out); collect_var_id(*src, out); collect_varids_in_expr(len, out); }
        IrStmtKind::Expr { expr } => collect_varids_in_expr(expr, out),
        IrStmtKind::Guard { cond, else_ } => { collect_varids_in_expr(cond, out); collect_varids_in_expr(else_, out); }
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => { collect_var_id(*var, out); }
        IrStmtKind::Comment { .. } => {}
    }
}

fn collect_varids_in_pattern(pattern: &IrPattern, out: &mut Vec<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => collect_var_id(*var, out),
        IrPattern::Constructor { args, .. } => { for a in args { collect_varids_in_pattern(a, out); } }
        IrPattern::Tuple { elements } | IrPattern::List { elements } => { for e in elements { collect_varids_in_pattern(e, out); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => collect_varids_in_pattern(inner, out),
        IrPattern::RecordPattern { fields, .. } => { for f in fields { if let Some(p) = &f.pattern { collect_varids_in_pattern(p, out); } } }
        IrPattern::Literal { expr } => collect_varids_in_expr(expr, out),
        _ => {} // Wildcard, None
    }
}

// ── VarId remapping ─────────────────────────────────────────────

fn remap_id(id: VarId, remap: &HashMap<VarId, VarId>) -> VarId {
    remap.get(&id).copied().unwrap_or(id)
}

pub(super) fn remap_expr_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    match &mut expr.kind {
        IrExprKind::Var { id } => *id = remap_id(*id, remap),
        IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. } | IrExprKind::If { .. }
        | IrExprKind::While { .. } => remap_control_varids(expr, remap),
        IrExprKind::Match { .. } => remap_match_varids(expr, remap),
        IrExprKind::Block { .. } => remap_block_varids(expr, remap),
        IrExprKind::Call { .. } => remap_call_varids(expr, remap),
        IrExprKind::ForIn { .. } => remap_for_in_varids(expr, remap),
        IrExprKind::List { .. } | IrExprKind::Tuple { .. } | IrExprKind::Fan { .. }
        | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. } | IrExprKind::MapLiteral { .. } => {
            remap_container_literal_varids(expr, remap)
        }
        IrExprKind::Range { .. } | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::OptionalChain { .. } | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::RustMacro { .. } => {
            remap_container_access_varids(expr, remap)
        }
        IrExprKind::Lambda { .. } | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. } => {
            remap_closure_varids(expr, remap)
        }
        IrExprKind::IterChain { .. } => remap_iter_chain_varids(expr, remap),
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
        | IrExprKind::OptionSome { .. } | IrExprKind::Try { .. }
        | IrExprKind::Await { .. } | IrExprKind::Clone { .. }
        | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
        | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
        | IrExprKind::ToVec { .. } | IrExprKind::Unwrap { .. }
        | IrExprKind::ToOption { .. } | IrExprKind::UnwrapOr { .. } => remap_wrap_varids(expr, remap),
        _ => {} // literals, unit, break, continue, etc.
    }
}

/// BinOp/UnOp/If/While: remap operands, condition, and bodies.
fn remap_control_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => { remap_expr_varids(left, remap); remap_expr_varids(right, remap); }
        IrExprKind::UnOp { operand, .. } => remap_expr_varids(operand, remap),
        IrExprKind::If { cond, then, else_ } => {
            remap_expr_varids(cond, remap);
            remap_expr_varids(then, remap);
            remap_expr_varids(else_, remap);
        }
        IrExprKind::While { cond, body } => {
            remap_expr_varids(cond, remap);
            for s in body { remap_stmt_varids(s, remap); }
        }
        _ => unreachable!(),
    }
}

/// Block: remap statements and tail.
fn remap_block_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    let IrExprKind::Block { stmts, expr } = &mut expr.kind else { unreachable!() };
    for s in stmts { remap_stmt_varids(s, remap); }
    if let Some(e) = expr { remap_expr_varids(e, remap); }
}

/// Lambda/ClosureCreate/EnvLoad: remap params/captures/env var and the lambda body.
fn remap_closure_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    match &mut expr.kind {
        IrExprKind::Lambda { params, body, .. } => {
            for (id, _) in params { *id = remap_id(*id, remap); }
            remap_expr_varids(body, remap);
        }
        IrExprKind::ClosureCreate { captures, .. } => {
            for (id, _) in captures { *id = remap_id(*id, remap); }
        }
        IrExprKind::EnvLoad { env_var, .. } => *env_var = remap_id(*env_var, remap),
        _ => unreachable!(),
    }
}

/// Call: remap the receiver (if any) and arguments.
fn remap_call_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    let IrExprKind::Call { target, args, .. } = &mut expr.kind else { unreachable!() };
    match target {
        CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } => remap_expr_varids(object, remap),
        _ => {}
    }
    for a in args { remap_expr_varids(a, remap); }
}

/// ForIn: remap the loop var(s), the iterable, and the body.
fn remap_for_in_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    let IrExprKind::ForIn { var, var_tuple, iterable, body } = &mut expr.kind else { unreachable!() };
    *var = remap_id(*var, remap);
    if let Some(tvs) = var_tuple { for tv in tvs { *tv = remap_id(*tv, remap); } }
    remap_expr_varids(iterable, remap);
    for s in body { remap_stmt_varids(s, remap); }
}

/// List/Tuple/Fan/Record/SpreadRecord/MapLiteral: remap each element/entry.
fn remap_container_literal_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    match &mut expr.kind {
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } | IrExprKind::Fan { exprs: elements } => {
            for e in elements { remap_expr_varids(e, remap); }
        }
        IrExprKind::Record { fields, .. } => { for (_, e) in fields { remap_expr_varids(e, remap); } }
        IrExprKind::SpreadRecord { base, fields } => {
            remap_expr_varids(base, remap);
            for (_, e) in fields { remap_expr_varids(e, remap); }
        }
        IrExprKind::MapLiteral { entries } => { for (k, v) in entries { remap_expr_varids(k, remap); remap_expr_varids(v, remap); } }
        _ => unreachable!(),
    }
}

/// Range/Member/TupleIndex/OptionalChain/IndexAccess/MapAccess/StringInterp/RustMacro:
/// remap each accessed sub-expression.
fn remap_container_access_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    match &mut expr.kind {
        IrExprKind::Range { start, end, .. } => { remap_expr_varids(start, remap); remap_expr_varids(end, remap); }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => remap_expr_varids(object, remap),
        IrExprKind::IndexAccess { object, index } => { remap_expr_varids(object, remap); remap_expr_varids(index, remap); }
        IrExprKind::MapAccess { object, key } => { remap_expr_varids(object, remap); remap_expr_varids(key, remap); }
        IrExprKind::StringInterp { parts } => {
            for p in parts { if let IrStringPart::Expr { expr } = p { remap_expr_varids(expr, remap); } }
        }
        IrExprKind::RustMacro { args, .. } => { for a in args { remap_expr_varids(a, remap); } }
        _ => unreachable!(),
    }
}

/// ResultOk/ResultErr/OptionSome/Try/Await/Clone/Deref/Borrow/BoxNew/RcWrap/ToVec/Unwrap/ToOption/UnwrapOr:
/// remap the wrapped expression(s).
fn remap_wrap_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    match &mut expr.kind {
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } | IrExprKind::Clone { expr }
        | IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. }
        | IrExprKind::BoxNew { expr } | IrExprKind::RcWrap { expr, .. }
        | IrExprKind::ToVec { expr } | IrExprKind::Unwrap { expr }
        | IrExprKind::ToOption { expr } => remap_expr_varids(expr, remap),
        IrExprKind::UnwrapOr { expr, fallback } => {
            remap_expr_varids(expr, remap);
            remap_expr_varids(fallback, remap);
        }
        _ => unreachable!(),
    }
}

fn remap_match_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
    remap_expr_varids(subject, remap);
    for arm in arms {
        remap_pattern_varids(&mut arm.pattern, remap);
        if let Some(g) = &mut arm.guard { remap_expr_varids(g, remap); }
        remap_expr_varids(&mut arm.body, remap);
    }
}

fn remap_iter_chain_varids(expr: &mut IrExpr, remap: &HashMap<VarId, VarId>) {
    let IrExprKind::IterChain { source, steps, collector, .. } = &mut expr.kind else { unreachable!() };
    remap_expr_varids(source, remap);
    for step in steps {
        match step {
            IterStep::Map { lambda } | IterStep::Filter { lambda }
            | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => remap_expr_varids(lambda, remap),
        }
    }
    match collector {
        IterCollector::Collect => {}
        IterCollector::Fold { init, lambda } => { remap_expr_varids(init, remap); remap_expr_varids(lambda, remap); }
        IterCollector::Any { lambda } | IterCollector::All { lambda }
        | IterCollector::Find { lambda } | IterCollector::Count { lambda } => remap_expr_varids(lambda, remap),
    }
}

fn remap_stmt_varids(stmt: &mut IrStmt, remap: &HashMap<VarId, VarId>) {
    match &mut stmt.kind {
        IrStmtKind::Bind { var, value, .. } => { *var = remap_id(*var, remap); remap_expr_varids(value, remap); }
        IrStmtKind::BindDestructure { pattern, value } => { remap_pattern_varids(pattern, remap); remap_expr_varids(value, remap); }
        IrStmtKind::Assign { var, value } => { *var = remap_id(*var, remap); remap_expr_varids(value, remap); }
        IrStmtKind::IndexAssign { target, index, value } => { *target = remap_id(*target, remap); remap_expr_varids(index, remap); remap_expr_varids(value, remap); }
        IrStmtKind::MapInsert { target, key, value } => { *target = remap_id(*target, remap); remap_expr_varids(key, remap); remap_expr_varids(value, remap); }
        IrStmtKind::FieldAssign { target, value, .. } => { *target = remap_id(*target, remap); remap_expr_varids(value, remap); }
        IrStmtKind::ListSwap { target, a, b } => { *target = remap_id(*target, remap); remap_expr_varids(a, remap); remap_expr_varids(b, remap); }
        IrStmtKind::ListReverse { target, end } | IrStmtKind::ListRotateLeft { target, end } => { *target = remap_id(*target, remap); remap_expr_varids(end, remap); }
        IrStmtKind::ListCopySlice { dst, src, len } => { *dst = remap_id(*dst, remap); *src = remap_id(*src, remap); remap_expr_varids(len, remap); }
        IrStmtKind::Expr { expr } => remap_expr_varids(expr, remap),
        IrStmtKind::Guard { cond, else_ } => { remap_expr_varids(cond, remap); remap_expr_varids(else_, remap); }
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => { *var = remap_id(*var, remap); }
        IrStmtKind::Comment { .. } => {}
    }
}

fn remap_pattern_varids(pattern: &mut IrPattern, remap: &HashMap<VarId, VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => *var = remap_id(*var, remap),
        IrPattern::Constructor { args, .. } => { for a in args { remap_pattern_varids(a, remap); } }
        IrPattern::Tuple { elements } | IrPattern::List { elements } => { for e in elements { remap_pattern_varids(e, remap); } }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => remap_pattern_varids(inner, remap),
        IrPattern::RecordPattern { fields, .. } => { for f in fields { if let Some(p) = &mut f.pattern { remap_pattern_varids(p, remap); } } }
        _ => {} // Wildcard, None, Literal (literals don't bind VarIds)
    }
}
