/// Borrow inference: Lobster-style automatic escape analysis.
/// Determines which function parameters can be passed by reference (&str, &[T])
/// instead of by value (String, Vec<T>), eliminating unnecessary clones.

use std::collections::{HashMap, HashSet};
use crate::ast::*;

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

/// Check if a TypeExpr represents a heap-allocated type that benefits from borrowing.
fn is_heap_type(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Simple { name } => name == "String",
        TypeExpr::Generic { name, .. } => name == "List" || name == "Map",
        _ => false,
    }
}

/// Analyze all functions in a program and its modules.
/// Uses fixpoint iteration: starts with all params as Borrow, then refines
/// using inter-procedure analysis until stable.
pub fn analyze_program(program: &Program, modules: &[(String, Program, Option<crate::project::PkgId>, bool)]) -> BorrowInfo {
    // Collect all function declarations (name → (params, body))
    let mut fn_decls: Vec<(String, &[Param], &Expr)> = Vec::new();
    for decl in &program.decls {
        if let Decl::Fn { name, params, body: Some(body), .. } = decl {
            if name == "main" { continue; }
            fn_decls.push((name.clone(), params, body));
        }
    }
    for (mod_name, mod_prog, _, _) in modules {
        for decl in &mod_prog.decls {
            if let Decl::Fn { name, params, body: Some(body_expr), .. } = decl {
                let qualified = format!("{}.{}", mod_name, name);
                fn_decls.push((qualified, params, body_expr));
            }
        }
    }

    // Initial pass: analyze without inter-procedure info
    let mut info = BorrowInfo::new();
    for (name, params, body) in &fn_decls {
        let ownerships = analyze_fn(params, body);
        info.fn_params.insert(name.clone(), ownerships);
    }

    // Fixpoint iteration: re-analyze with callee borrow info
    // Convergence guaranteed: params can only change Borrow → Owned (monotone)
    for _ in 0..10 {
        let mut changed = false;
        for (name, params, body) in &fn_decls {
            let ownerships = analyze_fn_with_info(params, body, &info);
            if let Some(old) = info.fn_params.get(name) {
                if *old != ownerships {
                    changed = true;
                }
            }
            info.fn_params.insert(name.clone(), ownerships);
        }
        if !changed { break; }
    }

    info
}

/// Analyze a single function: for each heap-type param, determine if it escapes.
fn analyze_fn(params: &[Param], body: &Expr) -> Vec<ParamOwnership> {
    // Collect names of heap-type parameters
    let heap_params: HashSet<String> = params.iter()
        .filter(|p| is_heap_type(&p.ty))
        .map(|p| p.name.clone())
        .collect();

    if heap_params.is_empty() {
        // No heap params — all Owned (no borrow needed for primitives)
        return params.iter().map(|_| ParamOwnership::Owned).collect();
    }

    // Find which heap params escape
    let mut escaped = HashSet::new();
    check_escape_expr(body, &heap_params, &mut escaped, true);

    params.iter().map(|p| {
        if !is_heap_type(&p.ty) {
            ParamOwnership::Owned // primitives: no need to borrow (they're Copy)
        } else if escaped.contains(&p.name) {
            ParamOwnership::Owned
        } else {
            ParamOwnership::Borrow
        }
    }).collect()
}

/// Analyze with inter-procedure info: user fn calls check callee's borrow info
/// to determine if args escape (instead of conservatively marking all as escaped).
fn analyze_fn_with_info(params: &[Param], body: &Expr, info: &BorrowInfo) -> Vec<ParamOwnership> {
    let heap_params: HashSet<String> = params.iter()
        .filter(|p| is_heap_type(&p.ty))
        .map(|p| p.name.clone())
        .collect();

    if heap_params.is_empty() {
        return params.iter().map(|_| ParamOwnership::Owned).collect();
    }

    let mut escaped = HashSet::new();
    check_escape_expr_ip(body, &heap_params, &mut escaped, true, info);

    params.iter().map(|p| {
        if !is_heap_type(&p.ty) {
            ParamOwnership::Owned
        } else if escaped.contains(&p.name) {
            ParamOwnership::Owned
        } else {
            ParamOwnership::Borrow
        }
    }).collect()
}

/// Inter-procedure escape analysis: delegates to check_escape_expr with borrow info.
fn check_escape_expr_ip(expr: &Expr, heap_params: &HashSet<String>, escaped: &mut HashSet<String>, is_tail: bool, info: &BorrowInfo) {
    check_escape_expr_inner(expr, heap_params, escaped, is_tail, Some(info));
}

/// Check if any heap params escape through an expression (no inter-procedure info).
fn check_escape_expr(expr: &Expr, heap_params: &HashSet<String>, escaped: &mut HashSet<String>, is_tail: bool) {
    check_escape_expr_inner(expr, heap_params, escaped, is_tail, None);
}

/// Unified escape analysis. When `info` is Some, uses inter-procedure callee
/// borrow info to avoid conservatively marking user fn args as escaped.
fn check_escape_expr_inner(expr: &Expr, heap_params: &HashSet<String>, escaped: &mut HashSet<String>, is_tail: bool, info: Option<&BorrowInfo>) {
    match expr {
        // Direct use of a param in return position → escapes
        Expr::Ident { name, .. } => {
            if is_tail && heap_params.contains(name) {
                escaped.insert(name.clone());
            }
        }

        // Literals — no escape
        Expr::Int { .. } | Expr::Float { .. } | Expr::String { .. }
        | Expr::Bool { .. } | Expr::Unit { .. } | Expr::None { .. }
        | Expr::Hole { .. } | Expr::Todo { .. } | Expr::Placeholder { .. }
        | Expr::TypeName { .. } | Expr::InterpolatedString { .. } => {}

        // List literal — any param inside escapes (stored in data structure)
        Expr::List { elements, .. } => {
            for e in elements {
                mark_all_params(e, heap_params, escaped);
            }
        }

        // Tuple — same as list (stored in data structure)
        Expr::Tuple { elements, .. } => {
            for e in elements {
                mark_all_params(e, heap_params, escaped);
            }
        }

        // Record — fields store values, params escape
        Expr::Record { fields, .. } => {
            for f in fields {
                mark_all_params(&f.value, heap_params, escaped);
            }
        }

        Expr::SpreadRecord { base, fields, .. } => {
            mark_all_params(base, heap_params, escaped);
            for f in fields {
                mark_all_params(&f.value, heap_params, escaped);
            }
        }

        // Binary ops: ++ consumes ownership; others are OK for borrows
        Expr::Binary { op, left, right, .. } => {
            if op == "++" {
                mark_all_params(left, heap_params, escaped);
                mark_all_params(right, heap_params, escaped);
            } else {
                check_escape_expr_inner(left, heap_params, escaped, false, info);
                check_escape_expr_inner(right, heap_params, escaped, false, info);
            }
        }

        Expr::Unary { operand, .. } => {
            check_escape_expr_inner(operand, heap_params, escaped, false, info);
        }

        // Function call — check if callee borrows its args
        Expr::Call { callee, args, .. } => {
            check_escape_expr_inner(callee, heap_params, escaped, false, info);
            // Identify safe calls: builtins and stdlib module calls
            let is_safe = match callee.as_ref() {
                Expr::Ident { name, .. } => matches!(name.as_str(),
                    "println" | "eprintln" | "assert" | "assert_eq" | "assert_ne"
                ),
                Expr::Member { object, .. } => {
                    if let Expr::Ident { name, .. } = object.as_ref() {
                        crate::stdlib::is_stdlib_module(name)
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if is_safe {
                for a in args { check_escape_expr_inner(a, heap_params, escaped, false, info); }
            } else if let Some(borrow_info) = info {
                // Inter-procedure: check each arg against callee's param ownership
                let callee_name = match callee.as_ref() {
                    Expr::Ident { name, .. } => Some(name.as_str()),
                    _ => None,
                };
                for (i, a) in args.iter().enumerate() {
                    if let Some(fn_name) = callee_name {
                        let ownership = borrow_info.param_ownership(fn_name, i);
                        if ownership == ParamOwnership::Borrow {
                            // Callee borrows this param — arg doesn't escape
                            check_escape_expr_inner(a, heap_params, escaped, false, info);
                        } else {
                            // Callee owns this param — arg escapes
                            mark_all_params(a, heap_params, escaped);
                        }
                    } else {
                        mark_all_params(a, heap_params, escaped);
                    }
                }
            } else {
                // Conservative: user fn args escape
                for a in args { mark_all_params(a, heap_params, escaped); }
            }
        }

        Expr::If { cond, then, else_, .. } => {
            check_escape_expr_inner(cond, heap_params, escaped, false, info);
            check_escape_expr_inner(then, heap_params, escaped, is_tail, info);
            check_escape_expr_inner(else_, heap_params, escaped, is_tail, info);
        }

        Expr::Match { subject, arms, .. } => {
            check_escape_expr_inner(subject, heap_params, escaped, false, info);
            for arm in arms {
                check_escape_expr_inner(&arm.body, heap_params, escaped, is_tail, info);
            }
        }

        Expr::Block { stmts, expr, .. } | Expr::DoBlock { stmts, expr, .. } => {
            for s in stmts { check_escape_stmt(s, heap_params, escaped); }
            if let Some(e) = expr {
                check_escape_expr_inner(e, heap_params, escaped, is_tail, info);
            }
        }

        Expr::ForIn { iterable, body, .. } => {
            check_escape_expr_inner(iterable, heap_params, escaped, false, info);
            for s in body { check_escape_stmt(s, heap_params, escaped); }
        }

        Expr::Lambda { body, params: lparams, .. } => {
            let shadow: HashSet<String> = lparams.iter().map(|p| p.name.clone()).collect();
            let filtered: HashSet<String> = heap_params.difference(&shadow).cloned().collect();
            mark_all_params_filtered(body, &filtered, escaped);
        }

        Expr::Member { object, .. } | Expr::TupleIndex { object, .. } => {
            check_escape_expr_inner(object, heap_params, escaped, false, info);
        }

        Expr::Pipe { left, right, .. } => {
            check_escape_expr_inner(left, heap_params, escaped, false, info);
            check_escape_expr_inner(right, heap_params, escaped, is_tail, info);
        }

        Expr::Range { start, end, .. } => {
            check_escape_expr_inner(start, heap_params, escaped, false, info);
            check_escape_expr_inner(end, heap_params, escaped, false, info);
        }

        Expr::Some { expr, .. } | Expr::Ok { expr, .. } | Expr::Err { expr, .. } => {
            if is_tail {
                mark_all_params(expr, heap_params, escaped);
            } else {
                check_escape_expr_inner(expr, heap_params, escaped, false, info);
            }
        }

        Expr::Paren { expr, .. } => {
            check_escape_expr_inner(expr, heap_params, escaped, is_tail, info);
        }

        Expr::Try { expr, .. } | Expr::Await { expr, .. } => {
            check_escape_expr_inner(expr, heap_params, escaped, is_tail, info);
        }
    }
}

/// Check escape in a statement. Statements are never in tail position.
fn check_escape_stmt(stmt: &Stmt, heap_params: &HashSet<String>, escaped: &mut HashSet<String>) {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Var { value, .. }
        | Stmt::LetDestructure { value, .. } => {
            // Var assignment: if RHS is a param, it escapes (stored in mutable var)
            if matches!(stmt, Stmt::Var { .. }) {
                mark_all_params(value, heap_params, escaped);
            } else {
                check_escape_expr(value, heap_params, escaped, false);
            }
        }
        Stmt::Assign { value, .. } => {
            // Assignment to var: value escapes
            mark_all_params(value, heap_params, escaped);
        }
        Stmt::IndexAssign { index, value, .. } => {
            check_escape_expr(index, heap_params, escaped, false);
            mark_all_params(value, heap_params, escaped);
        }
        Stmt::FieldAssign { value, .. } => {
            mark_all_params(value, heap_params, escaped);
        }
        Stmt::Expr { expr, .. } => {
            check_escape_expr(expr, heap_params, escaped, false);
        }
        Stmt::Guard { cond, else_, .. } => {
            check_escape_expr(cond, heap_params, escaped, false);
            check_escape_expr(else_, heap_params, escaped, false);
        }
        Stmt::Comment { .. } => {}
    }
}

/// Mark all heap params that appear anywhere in the expression as escaped.
fn mark_all_params(expr: &Expr, heap_params: &HashSet<String>, escaped: &mut HashSet<String>) {
    mark_all_params_filtered(expr, heap_params, escaped);
}

fn mark_all_params_filtered(expr: &Expr, params: &HashSet<String>, escaped: &mut HashSet<String>) {
    match expr {
        Expr::Ident { name, .. } => {
            if params.contains(name) {
                escaped.insert(name.clone());
            }
        }
        Expr::Int { .. } | Expr::Float { .. } | Expr::String { .. }
        | Expr::Bool { .. } | Expr::Unit { .. } | Expr::None { .. }
        | Expr::Hole { .. } | Expr::Todo { .. } | Expr::Placeholder { .. }
        | Expr::TypeName { .. } | Expr::InterpolatedString { .. } => {}
        Expr::List { elements, .. } | Expr::Tuple { elements, .. } => {
            for e in elements { mark_all_params_filtered(e, params, escaped); }
        }
        Expr::Record { fields, .. } => {
            for f in fields { mark_all_params_filtered(&f.value, params, escaped); }
        }
        Expr::SpreadRecord { base, fields, .. } => {
            mark_all_params_filtered(base, params, escaped);
            for f in fields { mark_all_params_filtered(&f.value, params, escaped); }
        }
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right, .. } => {
            mark_all_params_filtered(left, params, escaped);
            mark_all_params_filtered(right, params, escaped);
        }
        Expr::Unary { operand, .. } => mark_all_params_filtered(operand, params, escaped),
        Expr::Call { callee, args, .. } => {
            mark_all_params_filtered(callee, params, escaped);
            for a in args { mark_all_params_filtered(a, params, escaped); }
        }
        Expr::If { cond, then, else_, .. } => {
            mark_all_params_filtered(cond, params, escaped);
            mark_all_params_filtered(then, params, escaped);
            mark_all_params_filtered(else_, params, escaped);
        }
        Expr::Match { subject, arms, .. } => {
            mark_all_params_filtered(subject, params, escaped);
            for arm in arms { mark_all_params_filtered(&arm.body, params, escaped); }
        }
        Expr::Block { stmts, expr, .. } | Expr::DoBlock { stmts, expr, .. } => {
            for s in stmts { mark_all_params_in_stmt(s, params, escaped); }
            if let Some(e) = expr { mark_all_params_filtered(e, params, escaped); }
        }
        Expr::ForIn { iterable, body, .. } => {
            mark_all_params_filtered(iterable, params, escaped);
            for s in body { mark_all_params_in_stmt(s, params, escaped); }
        }
        Expr::Lambda { body, .. } => mark_all_params_filtered(body, params, escaped),
        Expr::Member { object, .. } | Expr::TupleIndex { object, .. } => {
            mark_all_params_filtered(object, params, escaped);
        }
        Expr::Range { start, end, .. } => {
            mark_all_params_filtered(start, params, escaped);
            mark_all_params_filtered(end, params, escaped);
        }
        Expr::Paren { expr, .. } | Expr::Try { expr, .. } | Expr::Await { expr, .. }
        | Expr::Some { expr, .. } | Expr::Ok { expr, .. } | Expr::Err { expr, .. } => {
            mark_all_params_filtered(expr, params, escaped);
        }
    }
}

fn mark_all_params_in_stmt(stmt: &Stmt, params: &HashSet<String>, escaped: &mut HashSet<String>) {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Var { value, .. }
        | Stmt::LetDestructure { value, .. } | Stmt::Assign { value, .. } => {
            mark_all_params_filtered(value, params, escaped);
        }
        Stmt::IndexAssign { index, value, .. } => {
            mark_all_params_filtered(index, params, escaped);
            mark_all_params_filtered(value, params, escaped);
        }
        Stmt::FieldAssign { value, .. } => {
            mark_all_params_filtered(value, params, escaped);
        }
        Stmt::Expr { expr, .. } => mark_all_params_filtered(expr, params, escaped),
        Stmt::Guard { cond, else_, .. } => {
            mark_all_params_filtered(cond, params, escaped);
            mark_all_params_filtered(else_, params, escaped);
        }
        Stmt::Comment { .. } => {}
    }
}
