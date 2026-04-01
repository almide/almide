//! AutoParallelPass: rewrite pure list operations to parallel variants.
//!
//! Runs AFTER StdlibLowering (which converts Module calls to Named calls).
//! Looks for Named calls to `almide_rt_list_{map,filter,any,all}` where
//! the lambda argument is pure (no effect fn calls, no mutable captures).
//! Rewrites the call target to `almide_rt_list_par_{map,filter,any,all}`.
//!
//! Rust target only. Uses `std::thread::scope` in the runtime — no external crates.

use almide_ir::*;
use almide_base::intern::{Sym, sym};
use super::pass::PassResult;
use almide_lang::types::Ty;
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct AutoParallelPass;

impl NanoPass for AutoParallelPass {
    fn name(&self) -> &str { "AutoParallel" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }

    fn depends_on(&self) -> Vec<&'static str> {
        vec!["StdlibLowering"]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Collect effect function names for purity analysis
        let mut effect_fns: std::collections::HashSet<Sym> = std::collections::HashSet::new();
        for func in &program.functions {
            if func.is_effect {
                effect_fns.insert(func.name);
            }
        }
        for module in &program.modules {
            for func in &module.functions {
                if func.is_effect {
                    effect_fns.insert(sym(&format!("{}.{}", module.name, func.name)));
                    effect_fns.insert(func.name);
                }
            }
        }

        // Collect mutable variable IDs from var_table
        let mut mutable_vars = std::collections::HashSet::new();
        for i in 0..program.var_table.len() {
            let id = VarId(i as u32);
            if program.var_table.get(id).mutability == Mutability::Var {
                mutable_vars.insert(id);
            }
        }

        for func in &mut program.functions {
            func.body = rewrite_expr(func.body.clone(), &effect_fns, &mutable_vars);
        }
        for tl in &mut program.top_lets {
            tl.value = rewrite_expr(tl.value.clone(), &effect_fns, &mutable_vars);
        }
        for module in &mut program.modules {
            // Each module has its own var_table for mutable var tracking
            let mut mod_mutable = std::collections::HashSet::new();
            for i in 0..module.var_table.len() {
                let id = VarId(i as u32);
                if module.var_table.get(id).mutability == Mutability::Var {
                    mod_mutable.insert(id);
                }
            }
            for func in &mut module.functions {
                func.body = rewrite_expr(func.body.clone(), &effect_fns, &mod_mutable);
            }
            for tl in &mut module.top_lets {
                tl.value = rewrite_expr(tl.value.clone(), &effect_fns, &mod_mutable);
            }
        }
        PassResult { program, changed: true }
    }
}

/// Map sequential runtime names to their parallel counterparts.
fn parallel_name(name: &str) -> Option<&'static str> {
    match name {
        "almide_rt_list_map" => Some("almide_rt_list_par_map"),
        "almide_rt_list_filter" => Some("almide_rt_list_par_filter"),
        "almide_rt_list_any" => Some("almide_rt_list_par_any"),
        "almide_rt_list_all" => Some("almide_rt_list_par_all"),
        _ => None,
    }
}

/// Check if a lambda body is pure: no effect fn calls, no mutable variable captures.
fn is_pure_lambda(
    body: &IrExpr,
    params: &[(VarId, Ty)],
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    let param_ids: std::collections::HashSet<VarId> = params.iter().map(|(id, _)| *id).collect();
    is_pure_expr(body, &param_ids, effect_fns, mutable_vars)
}

/// Recursively check expression purity.
/// A pure expression:
/// - Contains no calls to effect functions
/// - Does not reference mutable variables outside its own lambda params
/// - Contains no Assign statements
fn is_pure_expr(
    expr: &IrExpr,
    local_vars: &std::collections::HashSet<VarId>,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    match &expr.kind {
        // Variable reference: impure if it captures a mutable variable
        IrExprKind::Var { id } => {
            if local_vars.contains(id) {
                return true;
            }
            // Captured variable — impure only if it's mutable
            !mutable_vars.contains(id)
        }

        // Calls: check if the target is an effect fn
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Named { name } => {
                    if effect_fns.contains(name) { return false; }
                    // Stdlib effect functions (fs, http, etc.)
                    if name.starts_with("almide_rt_") {
                        let rest = &name["almide_rt_".len()..];
                        let module = rest.split('_').next().unwrap_or("");
                        if matches!(module, "fs" | "http" | "env" | "process" | "time") {
                            return false;
                        }
                    }
                }
                CallTarget::Module { module, .. } => {
                    if matches!(&**module, "fs" | "http" | "env" | "process" | "time") {
                        return false;
                    }
                }
                _ => {}
            }
            args.iter().all(|a| is_pure_expr(a, local_vars, effect_fns, mutable_vars))
        }

        // Literals: always pure
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } |
        IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. } |
        IrExprKind::Unit | IrExprKind::Hole | IrExprKind::OptionNone => true,

        // Operators
        IrExprKind::BinOp { left, right, .. } => {
            is_pure_expr(left, local_vars, effect_fns, mutable_vars) &&
            is_pure_expr(right, local_vars, effect_fns, mutable_vars)
        }
        IrExprKind::UnOp { operand, .. } => {
            is_pure_expr(operand, local_vars, effect_fns, mutable_vars)
        }

        // Control flow
        IrExprKind::If { cond, then, else_ } => {
            is_pure_expr(cond, local_vars, effect_fns, mutable_vars) &&
            is_pure_expr(then, local_vars, effect_fns, mutable_vars) &&
            is_pure_expr(else_, local_vars, effect_fns, mutable_vars)
        }
        IrExprKind::Match { subject, arms } => {
            is_pure_expr(subject, local_vars, effect_fns, mutable_vars) &&
            arms.iter().all(|arm| {
                let mut arm_vars = local_vars.clone();
                collect_pattern_bindings(&arm.pattern, &mut arm_vars);
                arm.guard.as_ref().map_or(true, |g| is_pure_expr(g, &arm_vars, effect_fns, mutable_vars)) &&
                is_pure_expr(&arm.body, &arm_vars, effect_fns, mutable_vars)
            })
        }
        IrExprKind::Block { stmts, expr } => {
            let mut block_vars = local_vars.clone();
            for stmt in stmts {
                if !is_pure_stmt(stmt, &block_vars, effect_fns, mutable_vars) {
                    return false;
                }
                collect_stmt_bindings(stmt, &mut block_vars);
            }
            expr.as_ref().map_or(true, |e| is_pure_expr(e, &block_vars, effect_fns, mutable_vars))
        }

        // Collections
        IrExprKind::List { elements } => {
            elements.iter().all(|e| is_pure_expr(e, local_vars, effect_fns, mutable_vars))
        }
        IrExprKind::Tuple { elements } => {
            elements.iter().all(|e| is_pure_expr(e, local_vars, effect_fns, mutable_vars))
        }
        IrExprKind::Record { fields, .. } => {
            fields.iter().all(|(_, e)| is_pure_expr(e, local_vars, effect_fns, mutable_vars))
        }
        IrExprKind::MapLiteral { entries } => {
            entries.iter().all(|(k, v)|
                is_pure_expr(k, local_vars, effect_fns, mutable_vars) &&
                is_pure_expr(v, local_vars, effect_fns, mutable_vars)
            )
        }

        // Access
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => {
            is_pure_expr(object, local_vars, effect_fns, mutable_vars)
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            is_pure_expr(object, local_vars, effect_fns, mutable_vars) &&
            is_pure_expr(index, local_vars, effect_fns, mutable_vars)
        }

        // Wrapping
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } |
        IrExprKind::OptionSome { expr } | IrExprKind::Clone { expr } |
        IrExprKind::Deref { expr } | IrExprKind::Borrow { expr, .. } |
        IrExprKind::BoxNew { expr } | IrExprKind::ToVec { expr } |
        IrExprKind::Try { expr } | IrExprKind::Await { expr } |
        IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr } => {
            is_pure_expr(expr, local_vars, effect_fns, mutable_vars)
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            is_pure_expr(expr, local_vars, effect_fns, mutable_vars) &&
            is_pure_expr(fallback, local_vars, effect_fns, mutable_vars)
        }

        // Nested lambda: treat as pure boundary (it captures, but we check its own refs)
        IrExprKind::Lambda { params, body, .. } => {
            let mut inner_vars = local_vars.clone();
            for (id, _) in params {
                inner_vars.insert(*id);
            }
            is_pure_expr(body, &inner_vars, effect_fns, mutable_vars)
        }

        // String interpolation
        IrExprKind::StringInterp { parts } => {
            parts.iter().all(|p| match p {
                IrStringPart::Lit { .. } => true,
                IrStringPart::Expr { expr } => is_pure_expr(expr, local_vars, effect_fns, mutable_vars),
            })
        }

        // Range
        IrExprKind::Range { start, end, .. } => {
            is_pure_expr(start, local_vars, effect_fns, mutable_vars) &&
            is_pure_expr(end, local_vars, effect_fns, mutable_vars)
        }

        // Spread record
        IrExprKind::SpreadRecord { base, fields } => {
            is_pure_expr(base, local_vars, effect_fns, mutable_vars) &&
            fields.iter().all(|(_, e)| is_pure_expr(e, local_vars, effect_fns, mutable_vars))
        }

        // FnRef: pure (it's just a reference to a function)
        IrExprKind::FnRef { .. } => true,

        // Macro invocations and rendered calls: assume impure (conservative)
        IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. } => false,

        // Loops in a lambda body: could mutate, be conservative
        IrExprKind::ForIn { .. } | IrExprKind::While { .. } => false,
        IrExprKind::Break | IrExprKind::Continue => true,

        // Fan (concurrent): impure by definition
        IrExprKind::Fan { .. } => false,

        IrExprKind::EmptyMap => true,
        IrExprKind::Todo { .. } => false,
        IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. } => true,
        IrExprKind::IterChain { .. } => true,
    }
}

/// Check statement purity.
fn is_pure_stmt(
    stmt: &IrStmt,
    local_vars: &std::collections::HashSet<VarId>,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => is_pure_expr(value, local_vars, effect_fns, mutable_vars),
        IrStmtKind::BindDestructure { value, .. } => is_pure_expr(value, local_vars, effect_fns, mutable_vars),
        // Assignment to a variable: impure (mutation)
        IrStmtKind::Assign { .. } | IrStmtKind::IndexAssign { .. } |
        IrStmtKind::MapInsert { .. } | IrStmtKind::FieldAssign { .. } |
        IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. } |
        IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => false,
        IrStmtKind::Expr { expr } => is_pure_expr(expr, local_vars, effect_fns, mutable_vars),
        IrStmtKind::Guard { cond, else_ } => {
            is_pure_expr(cond, local_vars, effect_fns, mutable_vars) &&
            is_pure_expr(else_, local_vars, effect_fns, mutable_vars)
        }
        IrStmtKind::Comment { .. } => true,
    }
}

/// Collect variable bindings introduced by a pattern.
fn collect_pattern_bindings(pattern: &IrPattern, vars: &mut std::collections::HashSet<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => { vars.insert(*var); }
        IrPattern::Constructor { args, .. } => {
            for p in args { collect_pattern_bindings(p, vars); }
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern { collect_pattern_bindings(p, vars); }
            }
        }
        IrPattern::Tuple { elements } => {
            for p in elements { collect_pattern_bindings(p, vars); }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            collect_pattern_bindings(inner, vars);
        }
        IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => {}
    }
}

/// Collect variable bindings from a statement (for block scope tracking).
fn collect_stmt_bindings(stmt: &IrStmt, vars: &mut std::collections::HashSet<VarId>) {
    match &stmt.kind {
        IrStmtKind::Bind { var, .. } => { vars.insert(*var); }
        IrStmtKind::BindDestructure { pattern, .. } => {
            collect_pattern_bindings(pattern, vars);
        }
        _ => {}
    }
}

// ── IR rewriting ────────────────────────────────────────────────

fn rewrite_expr(
    expr: IrExpr,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        // Target pattern: Named call to a parallelizable list function with a lambda arg
        IrExprKind::Call { target: CallTarget::Named { ref name }, .. }
            if parallel_name(name).is_some() =>
        {
            let orig_name = *name; // Sym is Copy
            let par_name = parallel_name(name).unwrap();
            // Extract the call (we matched the ref above)
            let IrExprKind::Call { target: CallTarget::Named { name: _ }, args, type_args } = expr.kind else {
                unreachable!()
            };

            // Recurse into args first
            let args: Vec<IrExpr> = args.into_iter()
                .map(|a| rewrite_expr(a, effect_fns, mutable_vars))
                .collect();

            // Find the lambda argument (last arg for map/filter/any/all)
            let lambda_arg = args.last();
            let is_pure = match lambda_arg {
                Some(IrExpr { kind: IrExprKind::Lambda { params, body, .. }, .. }) => {
                    is_pure_lambda(body, params, effect_fns, mutable_vars)
                }
                // If the lambda is wrapped in Clone (from CloneInsertionPass), peek inside
                Some(IrExpr { kind: IrExprKind::Clone { expr }, .. }) => {
                    match &expr.kind {
                        IrExprKind::Lambda { params, body, .. } => {
                            is_pure_lambda(body, params, effect_fns, mutable_vars)
                        }
                        _ => false,
                    }
                }
                _ => false,
            };

            if is_pure {
                IrExprKind::Call {
                    target: CallTarget::Named { name: sym(par_name) },
                    args,
                    type_args,
                }
            } else {
                // Not pure — keep original sequential call
                IrExprKind::Call {
                    target: CallTarget::Named { name: orig_name },
                    args,
                    type_args,
                }
            }
        }

        // Recurse into all other expressions
        IrExprKind::Call { target, args, type_args } => {
            let args = args.into_iter().map(|a| rewrite_expr(a, effect_fns, mutable_vars)).collect();
            let target = match target {
                CallTarget::Method { object, method } =>
                    CallTarget::Method { object: Box::new(rewrite_expr(*object, effect_fns, mutable_vars)), method },
                CallTarget::Computed { callee } =>
                    CallTarget::Computed { callee: Box::new(rewrite_expr(*callee, effect_fns, mutable_vars)) },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_expr(*cond, effect_fns, mutable_vars)),
            then: Box::new(rewrite_expr(*then, effect_fns, mutable_vars)),
            else_: Box::new(rewrite_expr(*else_, effect_fns, mutable_vars)),
        },
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: rewrite_stmts(stmts, effect_fns, mutable_vars),
            expr: expr.map(|e| Box::new(rewrite_expr(*e, effect_fns, mutable_vars))),
        },

        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_expr(*subject, effect_fns, mutable_vars)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| rewrite_expr(g, effect_fns, mutable_vars)),
                body: rewrite_expr(arm.body, effect_fns, mutable_vars),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(rewrite_expr(*left, effect_fns, mutable_vars)),
            right: Box::new(rewrite_expr(*right, effect_fns, mutable_vars)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op,
            operand: Box::new(rewrite_expr(*operand, effect_fns, mutable_vars)),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params,
            body: Box::new(rewrite_expr(*body, effect_fns, mutable_vars)),
            lambda_id,
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| rewrite_expr(e, effect_fns, mutable_vars)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| rewrite_expr(e, effect_fns, mutable_vars)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name,
            fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v, effect_fns, mutable_vars))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rewrite_expr(*base, effect_fns, mutable_vars)),
            fields: fields.into_iter().map(|(k, v)| (k, rewrite_expr(v, effect_fns, mutable_vars))).collect(),
        },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_expr(*object, effect_fns, mutable_vars)),
            field,
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)),
            field,
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(rewrite_expr(*object, effect_fns, mutable_vars)),
            index,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rewrite_expr(*object, effect_fns, mutable_vars)),
            index: Box::new(rewrite_expr(*index, effect_fns, mutable_vars)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(rewrite_expr(*object, effect_fns, mutable_vars)),
            key: Box::new(rewrite_expr(*key, effect_fns, mutable_vars)),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rewrite_expr(*iterable, effect_fns, mutable_vars)),
            body: rewrite_stmts(body, effect_fns, mutable_vars),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_expr(*cond, effect_fns, mutable_vars)),
            body: rewrite_stmts(body, effect_fns, mutable_vars),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_expr(expr, effect_fns, mutable_vars) },
                other => other,
            }).collect(),
        },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)), fallback: Box::new(rewrite_expr(*fallback, effect_fns, mutable_vars)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)), as_str, mutable },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(rewrite_expr(*expr, effect_fns, mutable_vars)) },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (rewrite_expr(k, effect_fns, mutable_vars), rewrite_expr(v, effect_fns, mutable_vars))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rewrite_expr(*start, effect_fns, mutable_vars)),
            end: Box::new(rewrite_expr(*end, effect_fns, mutable_vars)),
            inclusive,
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| rewrite_expr(e, effect_fns, mutable_vars)).collect(),
        },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name,
            args: args.into_iter().map(|a| rewrite_expr(a, effect_fns, mutable_vars)).collect(),
        },
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn rewrite_stmts(
    stmts: Vec<IrStmt>,
    effect_fns: &std::collections::HashSet<Sym>,
    mutable_vars: &std::collections::HashSet<VarId>,
) -> Vec<IrStmt> {
    stmts.into_iter().map(|s| {
        let kind = match s.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                var, mutability, ty,
                value: rewrite_expr(value, effect_fns, mutable_vars),
            },
            IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
                var,
                value: rewrite_expr(value, effect_fns, mutable_vars),
            },
            IrStmtKind::Expr { expr } => IrStmtKind::Expr {
                expr: rewrite_expr(expr, effect_fns, mutable_vars),
            },
            IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
                cond: rewrite_expr(cond, effect_fns, mutable_vars),
                else_: rewrite_expr(else_, effect_fns, mutable_vars),
            },
            IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
                pattern,
                value: rewrite_expr(value, effect_fns, mutable_vars),
            },
            IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
                target,
                index: rewrite_expr(index, effect_fns, mutable_vars),
                value: rewrite_expr(value, effect_fns, mutable_vars),
            },
            IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
                target,
                key: rewrite_expr(key, effect_fns, mutable_vars),
                value: rewrite_expr(value, effect_fns, mutable_vars),
            },
            IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
                target, field,
                value: rewrite_expr(value, effect_fns, mutable_vars),
            },
            other => other,
        };
        IrStmt { kind, span: s.span }
    }).collect()
}
