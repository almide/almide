/// Borrow inference: Lobster-style automatic escape analysis on typed IR.
/// Determines which function parameters can be passed by reference (&str, &[T])
/// instead of by value (String, Vec<T>), eliminating unnecessary clones.

use std::collections::{HashMap, HashSet};
use almide::ir::*;
use almide::types::Ty;

/// Ownership classification for a function parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamOwnership {
    /// Parameter can be borrowed: emit &str / &[T]
    Borrow,
    /// Parameter must be owned: emit String / Vec<T>
    Owned,
}

/// Borrow analysis results for all functions in the program.
pub struct BorrowInfo {
    /// fn_name → vec of ParamOwnership (one per param, in order)
    pub fn_params: HashMap<String, Vec<ParamOwnership>>,
}

impl BorrowInfo {
    pub fn new() -> Self {
        BorrowInfo { fn_params: HashMap::new() }
    }

    /// Get ownership for a specific function parameter.
    /// Returns Owned if function not found (conservative).
    pub fn param_ownership(&self, fn_name: &str, param_idx: usize) -> ParamOwnership {
        self.fn_params.get(fn_name)
            .and_then(|params| params.get(param_idx).copied())
            .unwrap_or(ParamOwnership::Owned)
    }
}

/// Check if a Ty represents a heap-allocated type that benefits from borrowing.
fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::List(_) | Ty::Map(_, _))
}

/// Analyze all functions in an IrProgram and its module IrPrograms.
/// Uses fixpoint iteration: starts with all params as Borrow, then refines
/// using inter-procedure analysis until stable.
pub fn analyze_program(
    ir: &IrProgram,
    module_irs: &HashMap<String, IrProgram>,
) -> BorrowInfo {
    let mut fn_decls: Vec<(String, Vec<(VarId, bool)>, &IrExpr, &VarTable)> = Vec::new();

    for f in &ir.functions {
        if f.name == "main" { continue; }
        let params: Vec<(VarId, bool)> = f.params.iter()
            .map(|(vid, ty)| (*vid, is_heap_type(ty)))
            .collect();
        fn_decls.push((f.name.clone(), params, &f.body, &ir.var_table));
    }

    for (mod_name, mod_ir) in module_irs {
        for f in &mod_ir.functions {
            let qualified = format!("{}.{}", mod_name, f.name);
            let params: Vec<(VarId, bool)> = f.params.iter()
                .map(|(vid, ty)| (*vid, is_heap_type(ty)))
                .collect();
            fn_decls.push((qualified, params, &f.body, &mod_ir.var_table));
        }
    }

    // Initial pass: analyze without inter-procedure info
    let mut info = BorrowInfo::new();
    for (name, params, body, _vt) in &fn_decls {
        let heap_vars: HashSet<VarId> = params.iter()
            .filter(|(_, is_heap)| *is_heap)
            .map(|(vid, _)| *vid)
            .collect();
        let ownerships = analyze_fn(params, &heap_vars, body);
        info.fn_params.insert(name.clone(), ownerships);
    }

    // Fixpoint iteration: re-analyze with callee borrow info
    // Convergence guaranteed: params can only change Borrow → Owned (monotone)
    for _ in 0..10 {
        let mut changed = false;
        for (name, params, body, vt) in &fn_decls {
            let heap_vars: HashSet<VarId> = params.iter()
                .filter(|(_, is_heap)| *is_heap)
                .map(|(vid, _)| *vid)
                .collect();
            let ownerships = analyze_fn_ip(params, &heap_vars, body, &info, vt);
            if let Some(old) = info.fn_params.get(name) {
                if *old != ownerships { changed = true; }
            }
            info.fn_params.insert(name.clone(), ownerships);
        }
        if !changed { break; }
    }

    info
}

/// Analyze a single function: for each heap-type param, determine if it escapes.
fn analyze_fn(
    params: &[(VarId, bool)],
    heap_vars: &HashSet<VarId>,
    body: &IrExpr,
) -> Vec<ParamOwnership> {
    if heap_vars.is_empty() {
        return params.iter().map(|_| ParamOwnership::Owned).collect();
    }
    let mut escaped = HashSet::new();
    check_escape_expr(body, heap_vars, &mut escaped, true, None);
    params.iter().map(|(vid, is_heap)| {
        if !is_heap { ParamOwnership::Owned }
        else if escaped.contains(vid) { ParamOwnership::Owned }
        else { ParamOwnership::Borrow }
    }).collect()
}

/// Analyze with inter-procedure info: user fn calls check callee's borrow info
/// to determine if args escape (instead of conservatively marking all as escaped).
fn analyze_fn_ip(
    params: &[(VarId, bool)],
    heap_vars: &HashSet<VarId>,
    body: &IrExpr,
    info: &BorrowInfo,
    _vt: &VarTable,
) -> Vec<ParamOwnership> {
    if heap_vars.is_empty() {
        return params.iter().map(|_| ParamOwnership::Owned).collect();
    }
    let mut escaped = HashSet::new();
    check_escape_expr(body, heap_vars, &mut escaped, true, Some(info));
    params.iter().map(|(vid, is_heap)| {
        if !is_heap { ParamOwnership::Owned }
        else if escaped.contains(vid) { ParamOwnership::Owned }
        else { ParamOwnership::Borrow }
    }).collect()
}

/// Check if any heap param VarIds escape through an expression.
fn check_escape_expr(
    expr: &IrExpr,
    heap_vars: &HashSet<VarId>,
    escaped: &mut HashSet<VarId>,
    is_tail: bool,
    info: Option<&BorrowInfo>,
) {
    match &expr.kind {
        IrExprKind::Var { id } => {
            if is_tail && heap_vars.contains(id) {
                escaped.insert(*id);
            }
        }

        // Literals — no escape
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        | IrExprKind::Break | IrExprKind::Continue => {}

        // StringInterp — no escape (params used for display only)
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr: e } = part {
                    check_escape_expr(e, heap_vars, escaped, false, info);
                }
            }
        }

        // Collections — params stored in data structures escape
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { mark_all(e, heap_vars, escaped); }
        }

        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { mark_all(e, heap_vars, escaped); }
        }

        IrExprKind::SpreadRecord { base, fields } => {
            mark_all(base, heap_vars, escaped);
            for (_, e) in fields { mark_all(e, heap_vars, escaped); }
        }

        // Binary ops: Concat consumes ownership; others are OK
        IrExprKind::BinOp { op, left, right } => {
            if matches!(op, BinOp::ConcatStr | BinOp::ConcatList) {
                mark_all(left, heap_vars, escaped);
                mark_all(right, heap_vars, escaped);
            } else {
                check_escape_expr(left, heap_vars, escaped, false, info);
                check_escape_expr(right, heap_vars, escaped, false, info);
            }
        }

        IrExprKind::UnOp { operand, .. } => {
            check_escape_expr(operand, heap_vars, escaped, false, info);
        }

        // Calls — check callee's borrow info
        IrExprKind::Call { target, args, .. } => {
            let is_safe = match target {
                CallTarget::Named { name } => matches!(name.as_str(),
                    "println" | "eprintln" | "assert" | "assert_eq" | "assert_ne"
                ),
                CallTarget::Module { module, .. } => {
                    almide::stdlib::is_stdlib_module(module)
                }
                _ => false,
            };

            if is_safe {
                for a in args { check_escape_expr(a, heap_vars, escaped, false, info); }
            } else if let Some(borrow_info) = info {
                let callee_name = match target {
                    CallTarget::Named { name } => Some(name.clone()),
                    CallTarget::Module { module, func } => Some(format!("{}.{}", module, func)),
                    _ => None,
                };
                match target {
                    CallTarget::Method { object, .. } => check_escape_expr(object, heap_vars, escaped, false, info),
                    CallTarget::Computed { callee } => check_escape_expr(callee, heap_vars, escaped, false, info),
                    _ => {}
                }
                for (i, a) in args.iter().enumerate() {
                    if let Some(ref fn_name) = callee_name {
                        let ownership = borrow_info.param_ownership(fn_name, i);
                        if ownership == ParamOwnership::Borrow {
                            check_escape_expr(a, heap_vars, escaped, false, info);
                        } else {
                            mark_all(a, heap_vars, escaped);
                        }
                    } else {
                        mark_all(a, heap_vars, escaped);
                    }
                }
            } else {
                match target {
                    CallTarget::Method { object, .. } => check_escape_expr(object, heap_vars, escaped, false, info),
                    CallTarget::Computed { callee } => check_escape_expr(callee, heap_vars, escaped, false, info),
                    _ => {}
                }
                for a in args { mark_all(a, heap_vars, escaped); }
            }
        }

        IrExprKind::If { cond, then, else_ } => {
            check_escape_expr(cond, heap_vars, escaped, false, info);
            check_escape_expr(then, heap_vars, escaped, is_tail, info);
            check_escape_expr(else_, heap_vars, escaped, is_tail, info);
        }

        IrExprKind::Match { subject, arms } => {
            check_escape_expr(subject, heap_vars, escaped, false, info);
            for arm in arms {
                check_escape_expr(&arm.body, heap_vars, escaped, is_tail, info);
            }
        }

        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { check_escape_stmt(s, heap_vars, escaped, info); }
            if let Some(e) = expr {
                check_escape_expr(e, heap_vars, escaped, is_tail, info);
            }
        }

        IrExprKind::ForIn { iterable, body, .. } => {
            check_escape_expr(iterable, heap_vars, escaped, false, info);
            for s in body { check_escape_stmt(s, heap_vars, escaped, info); }
        }

        IrExprKind::While { cond, body } => {
            check_escape_expr(cond, heap_vars, escaped, false, info);
            for s in body { check_escape_stmt(s, heap_vars, escaped, info); }
        }

        IrExprKind::Lambda { params, body } => {
            let shadow: HashSet<VarId> = params.iter().map(|(vid, _)| *vid).collect();
            let filtered: HashSet<VarId> = heap_vars.difference(&shadow).cloned().collect();
            mark_all_filtered(body, &filtered, escaped);
        }

        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            check_escape_expr(object, heap_vars, escaped, false, info);
        }

        IrExprKind::IndexAccess { object, index } => {
            check_escape_expr(object, heap_vars, escaped, false, info);
            check_escape_expr(index, heap_vars, escaped, false, info);
        }

        IrExprKind::Range { start, end, .. } => {
            check_escape_expr(start, heap_vars, escaped, false, info);
            check_escape_expr(end, heap_vars, escaped, false, info);
        }

        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } => {
            if is_tail {
                mark_all(e, heap_vars, escaped);
            } else {
                check_escape_expr(e, heap_vars, escaped, false, info);
            }
        }

        IrExprKind::Try { expr: e } | IrExprKind::Await { expr: e } => {
            check_escape_expr(e, heap_vars, escaped, is_tail, info);
        }
    }
}

/// Check escape in a statement.
fn check_escape_stmt(
    stmt: &IrStmt,
    heap_vars: &HashSet<VarId>,
    escaped: &mut HashSet<VarId>,
    info: Option<&BorrowInfo>,
) {
    match &stmt.kind {
        IrStmtKind::Bind { value, mutability, .. } => {
            if *mutability == Mutability::Var {
                mark_all(value, heap_vars, escaped);
            } else {
                // Let binding: treat as tail-like (param flowing into binding may need owned)
                check_escape_expr(value, heap_vars, escaped, true, info);
            }
        }
        IrStmtKind::BindDestructure { value, .. } => {
            check_escape_expr(value, heap_vars, escaped, true, info);
        }
        IrStmtKind::Assign { value, .. } => {
            mark_all(value, heap_vars, escaped);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            check_escape_expr(index, heap_vars, escaped, false, info);
            mark_all(value, heap_vars, escaped);
        }
        IrStmtKind::FieldAssign { value, .. } => {
            mark_all(value, heap_vars, escaped);
        }
        IrStmtKind::Expr { expr } => {
            check_escape_expr(expr, heap_vars, escaped, false, info);
        }
        IrStmtKind::Guard { cond, else_ } => {
            check_escape_expr(cond, heap_vars, escaped, false, info);
            check_escape_expr(else_, heap_vars, escaped, false, info);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

/// Mark all heap VarIds that appear anywhere in the expression as escaped.
fn mark_all(expr: &IrExpr, heap_vars: &HashSet<VarId>, escaped: &mut HashSet<VarId>) {
    mark_all_filtered(expr, heap_vars, escaped);
}

fn mark_all_filtered(expr: &IrExpr, vars: &HashSet<VarId>, escaped: &mut HashSet<VarId>) {
    match &expr.kind {
        IrExprKind::Var { id } => {
            if vars.contains(id) { escaped.insert(*id); }
        }
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        | IrExprKind::Break | IrExprKind::Continue => {}

        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr: e } = part {
                    mark_all_filtered(e, vars, escaped);
                }
            }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { mark_all_filtered(e, vars, escaped); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, e) in fields { mark_all_filtered(e, vars, escaped); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            mark_all_filtered(base, vars, escaped);
            for (_, e) in fields { mark_all_filtered(e, vars, escaped); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            mark_all_filtered(left, vars, escaped);
            mark_all_filtered(right, vars, escaped);
        }
        IrExprKind::UnOp { operand, .. } => mark_all_filtered(operand, vars, escaped),
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => mark_all_filtered(object, vars, escaped),
                CallTarget::Computed { callee } => mark_all_filtered(callee, vars, escaped),
                _ => {}
            }
            for a in args { mark_all_filtered(a, vars, escaped); }
        }
        IrExprKind::If { cond, then, else_ } => {
            mark_all_filtered(cond, vars, escaped);
            mark_all_filtered(then, vars, escaped);
            mark_all_filtered(else_, vars, escaped);
        }
        IrExprKind::Match { subject, arms } => {
            mark_all_filtered(subject, vars, escaped);
            for arm in arms { mark_all_filtered(&arm.body, vars, escaped); }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { mark_all_in_stmt(s, vars, escaped); }
            if let Some(e) = expr { mark_all_filtered(e, vars, escaped); }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            mark_all_filtered(iterable, vars, escaped);
            for s in body { mark_all_in_stmt(s, vars, escaped); }
        }
        IrExprKind::While { cond, body } => {
            mark_all_filtered(cond, vars, escaped);
            for s in body { mark_all_in_stmt(s, vars, escaped); }
        }
        IrExprKind::Lambda { body, .. } => mark_all_filtered(body, vars, escaped),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            mark_all_filtered(object, vars, escaped);
        }
        IrExprKind::IndexAccess { object, index } => {
            mark_all_filtered(object, vars, escaped);
            mark_all_filtered(index, vars, escaped);
        }
        IrExprKind::Range { start, end, .. } => {
            mark_all_filtered(start, vars, escaped);
            mark_all_filtered(end, vars, escaped);
        }
        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Await { expr: e } => {
            mark_all_filtered(e, vars, escaped);
        }
    }
}

fn mark_all_in_stmt(stmt: &IrStmt, vars: &HashSet<VarId>, escaped: &mut HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } => {
            mark_all_filtered(value, vars, escaped);
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            mark_all_filtered(index, vars, escaped);
            mark_all_filtered(value, vars, escaped);
        }
        IrStmtKind::FieldAssign { value, .. } => {
            mark_all_filtered(value, vars, escaped);
        }
        IrStmtKind::Expr { expr } => mark_all_filtered(expr, vars, escaped),
        IrStmtKind::Guard { cond, else_ } => {
            mark_all_filtered(cond, vars, escaped);
            mark_all_filtered(else_, vars, escaped);
        }
        IrStmtKind::Comment { .. } => {}
    }
}
