//! Closure Conversion pass: lifts all lambdas to top-level functions with explicit environments.
//!
//! After this pass, no `Lambda` nodes remain in the IR. Each lambda becomes:
//! - A new `IrFunction` with an env pointer as its first parameter
//! - A `ClosureCreate` node at the original lambda site
//!
//! Inside lifted functions, captured variables are accessed via `EnvLoad` nodes.
//! This eliminates the need for WASM codegen to understand closures — it only sees
//! plain functions and explicit environment allocation.
//!
//! Inspired by Elm/Haskell/Gleam closure conversion.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_lang::types::Ty;
use almide_base::intern::sym;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct ClosureConversionPass;

impl NanoPass for ClosureConversionPass {
    fn name(&self) -> &str { "ClosureConversion" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Wasm])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut lifted = Vec::new();
        let mut counter = 0u32;

        // Convert lambdas in main program functions
        for func in &mut program.functions {
            func.body = convert_expr(
                std::mem::take(&mut func.body),
                &mut lifted, &mut counter, &mut program.var_table,
            );
        }
        for tl in &mut program.top_lets {
            tl.value = convert_expr(
                std::mem::take(&mut tl.value),
                &mut lifted, &mut counter, &mut program.var_table,
            );
        }

        // Convert lambdas in module functions
        for module in &mut program.modules {
            for func in &mut module.functions {
                func.body = convert_expr(
                    std::mem::take(&mut func.body),
                    &mut lifted, &mut counter, &mut module.var_table,
                );
            }
            for tl in &mut module.top_lets {
                tl.value = convert_expr(
                    std::mem::take(&mut tl.value),
                    &mut lifted, &mut counter, &mut module.var_table,
                );
            }
        }

        // Add all lifted functions to the main program
        let changed = !lifted.is_empty();
        program.functions.extend(lifted);

        PassResult { program, changed }
    }
}

/// Recursively convert all Lambda nodes in an expression (bottom-up).
/// Inner lambdas are converted first, so their ClosureCreate captures are
/// visible when processing outer lambdas.
fn convert_expr(
    expr: IrExpr,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    var_table: &mut VarTable,
) -> IrExpr {
    let span = expr.span;
    let ty = expr.ty.clone();

    let kind = match expr.kind {
        IrExprKind::Lambda { params, body, lambda_id } => {
            // 1. Recursively convert nested lambdas first (bottom-up)
            let body = convert_expr(*body, lifted, counter, var_table);

            // 2. Compute free variables of the lambda body
            let param_ids: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
            let mut free = HashSet::new();
            collect_free_vars(&body, &param_ids, &mut free);

            // Sort captures for deterministic env layout
            let mut captures: Vec<(VarId, Ty)> = free.into_iter()
                .map(|vid| {
                    let info = var_table.get(vid);
                    (vid, info.ty.clone())
                })
                .collect();
            captures.sort_by_key(|(vid, _)| vid.0);

            // 3. Generate lifted function name
            let id = lambda_id.unwrap_or_else(|| { let c = *counter; *counter = c + 1; c });
            let func_name = sym(&format!("__closure_{}", id));

            // 4. Create env parameter (i32 pointer in WASM)
            // Use Ty::String as a proxy for i32 pointer type (maps to ValType::I32)
            let env_ty = Ty::String;
            let env_var = var_table.alloc(
                sym("__env"), env_ty.clone(), Mutability::Let, None,
            );

            // 5. Create local bindings for each capture: let __cap_N = EnvLoad(N)
            //    Then rewrite Var references to use the new local VarIds.
            //    This ensures inner ClosureCreate nodes reference locals in var_map.
            let mut prologue_stmts = Vec::new();
            let mut cap_locals: Vec<(VarId, VarId, Ty)> = Vec::new(); // (original, new_local, ty)
            for (idx, (vid, cap_ty)) in captures.iter().enumerate() {
                let local_name = sym(&format!("__cap_{}", idx));
                let local_var = var_table.alloc(local_name, cap_ty.clone(), Mutability::Let, None);
                prologue_stmts.push(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: local_var,
                        mutability: Mutability::Let,
                        ty: cap_ty.clone(),
                        value: IrExpr {
                            kind: IrExprKind::EnvLoad { env_var, index: idx as u32 },
                            ty: cap_ty.clone(),
                            span: None,
                        },
                    },
                    span: None,
                });
                cap_locals.push((*vid, local_var, cap_ty.clone()));
            }

            // Rewrite body: replace original captured VarIds with new local VarIds
            let rewritten_body = rewrite_var_ids(&body, &cap_locals);

            // Wrap body with prologue
            let final_body = if prologue_stmts.is_empty() {
                rewritten_body
            } else {
                IrExpr {
                    kind: IrExprKind::Block {
                        stmts: prologue_stmts,
                        expr: Some(Box::new(rewritten_body.clone())),
                    },
                    ty: rewritten_body.ty.clone(),
                    span: rewritten_body.span,
                }
            };

            // Build the ClosureCreate with original VarIds (they exist in the enclosing scope)
            // The captures list uses the original VarIds because at the call site,
            // these vars are in scope (as locals or as cap_locals from the enclosing lifted fn).

            // 6. Build the lifted function
            let mut func_params = vec![IrParam {
                var: env_var,
                ty: env_ty.clone(), // env pointer (i32 in WASM, maps via ty_to_valtype)
                name: sym("__env"),
                borrow: ParamBorrow::Own,
                open_record: None,
                default: None,
            }];
            for (vid, vty) in &params {
                let info = var_table.get(*vid);
                func_params.push(IrParam {
                    var: *vid,
                    ty: vty.clone(),
                    name: info.name,
                    borrow: ParamBorrow::Own,
                    open_record: None,
                    default: None,
                });
            }

            let ret_ty = match &ty {
                Ty::Fn { ret, .. } => *ret.clone(),
                _ => body.ty.clone(),
            };

            lifted.push(IrFunction {
                name: func_name,
                params: func_params,
                ret_ty,
                body: final_body,
                is_effect: false,
                is_async: false,
                is_test: false,
                generics: None,
                extern_attrs: vec![], export_attrs: vec![],
                visibility: IrVisibility::Private,
                doc: None,
                blank_lines_before: 0,
            });

            // 7. Replace the Lambda with ClosureCreate
            IrExprKind::ClosureCreate {
                func_name,
                captures,
            }
        }

        // ── Recursive conversion for all other nodes ──

        IrExprKind::Block { stmts, expr: tail } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| convert_stmt(s, lifted, counter, var_table)).collect(),
            expr: tail.map(|e| Box::new(convert_expr(*e, lifted, counter, var_table))),
        },
        IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
            target: convert_target(target, lifted, counter, var_table),
            args: args.into_iter().map(|a| convert_expr(a, lifted, counter, var_table)).collect(),
            type_args,
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(convert_expr(*cond, lifted, counter, var_table)),
            then: Box::new(convert_expr(*then, lifted, counter, var_table)),
            else_: Box::new(convert_expr(*else_, lifted, counter, var_table)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(convert_expr(*subject, lifted, counter, var_table)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| convert_expr(g, lifted, counter, var_table)),
                body: convert_expr(arm.body, lifted, counter, var_table),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(convert_expr(*iterable, lifted, counter, var_table)),
            body: body.into_iter().map(|s| convert_stmt(s, lifted, counter, var_table)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(convert_expr(*cond, lifted, counter, var_table)),
            body: body.into_iter().map(|s| convert_stmt(s, lifted, counter, var_table)).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(convert_expr(*left, lifted, counter, var_table)),
            right: Box::new(convert_expr(*right, lifted, counter, var_table)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(convert_expr(*operand, lifted, counter, var_table)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| convert_expr(e, lifted, counter, var_table)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| convert_expr(e, lifted, counter, var_table)).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| convert_expr(e, lifted, counter, var_table)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, convert_expr(v, lifted, counter, var_table))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(convert_expr(*base, lifted, counter, var_table)),
            fields: fields.into_iter().map(|(k, v)| (k, convert_expr(v, lifted, counter, var_table))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (convert_expr(k, lifted, counter, var_table), convert_expr(v, lifted, counter, var_table))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(convert_expr(*start, lifted, counter, var_table)),
            end: Box::new(convert_expr(*end, lifted, counter, var_table)),
            inclusive,
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(convert_expr(*object, lifted, counter, var_table)), field,
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(convert_expr(*object, lifted, counter, var_table)), index,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(convert_expr(*object, lifted, counter, var_table)),
            index: Box::new(convert_expr(*index, lifted, counter, var_table)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(convert_expr(*object, lifted, counter, var_table)),
            key: Box::new(convert_expr(*key, lifted, counter, var_table)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: convert_expr(expr, lifted, counter, var_table) },
                other => other,
            }).collect(),
        },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(convert_expr(*expr, lifted, counter, var_table)),
            fallback: Box::new(convert_expr(*fallback, lifted, counter, var_table)),
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(convert_expr(*expr, lifted, counter, var_table)), field,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow {
            expr: Box::new(convert_expr(*expr, lifted, counter, var_table)), as_str, mutable,
        },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(convert_expr(*expr, lifted, counter, var_table)) },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|a| convert_expr(a, lifted, counter, var_table)).collect(),
        },

        // Leaf nodes — no conversion needed
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn convert_stmt(
    stmt: IrStmt,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    var_table: &mut VarTable,
) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: convert_expr(value, lifted, counter, var_table),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: convert_expr(value, lifted, counter, var_table),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
            var, value: convert_expr(value, lifted, counter, var_table),
        },
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target, index: convert_expr(index, lifted, counter, var_table),
            value: convert_expr(value, lifted, counter, var_table),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target, key: convert_expr(key, lifted, counter, var_table),
            value: convert_expr(value, lifted, counter, var_table),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target, field, value: convert_expr(value, lifted, counter, var_table),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: convert_expr(cond, lifted, counter, var_table),
            else_: convert_expr(else_, lifted, counter, var_table),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr {
            expr: convert_expr(expr, lifted, counter, var_table),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

fn convert_target(
    target: CallTarget,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    var_table: &mut VarTable,
) -> CallTarget {
    match target {
        CallTarget::Method { object, method } => CallTarget::Method {
            object: Box::new(convert_expr(*object, lifted, counter, var_table)), method,
        },
        CallTarget::Computed { callee } => CallTarget::Computed {
            callee: Box::new(convert_expr(*callee, lifted, counter, var_table)),
        },
        other => other,
    }
}

// ── Free variable analysis ──────────────────────────────────────

// ── FreeVarCollector: IrVisitor-based free variable analysis ────────
//
// Computes free variables of an expression by tracking bound variables
// through scopes. Uses walk_expr/walk_stmt for exhaustive traversal of
// non-scope-introducing nodes.

struct FreeVarCollector {
    bound: HashSet<VarId>,
    free: HashSet<VarId>,
}

impl IrVisitor for FreeVarCollector {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Var { id } => {
                if !self.bound.contains(id) { self.free.insert(*id); }
            }
            IrExprKind::ClosureCreate { captures, .. } => {
                for (vid, _) in captures {
                    if !self.bound.contains(vid) { self.free.insert(*vid); }
                }
            }
            IrExprKind::Lambda { params, body, .. } => {
                let saved = self.bound.clone();
                for (v, _) in params { self.bound.insert(*v); }
                self.visit_expr(body);
                self.bound = saved;
            }
            IrExprKind::Block { stmts, expr: tail } => {
                let saved = self.bound.clone();
                for stmt in stmts {
                    self.visit_stmt(stmt);
                    match &stmt.kind {
                        IrStmtKind::Bind { var, .. } => { self.bound.insert(*var); }
                        IrStmtKind::BindDestructure { pattern, .. } => {
                            collect_pattern_bindings(pattern, &mut self.bound);
                        }
                        _ => {}
                    }
                }
                if let Some(e) = tail { self.visit_expr(e); }
                self.bound = saved;
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                for arm in arms {
                    let saved = self.bound.clone();
                    collect_pattern_bindings(&arm.pattern, &mut self.bound);
                    if let Some(g) = &arm.guard { self.visit_expr(g); }
                    self.visit_expr(&arm.body);
                    self.bound = saved;
                }
            }
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.visit_expr(iterable);
                let saved = self.bound.clone();
                self.bound.insert(*var);
                if let Some(vt) = var_tuple { for v in vt { self.bound.insert(*v); } }
                for s in body { self.visit_stmt(s); }
                self.bound = saved;
            }
            _ => walk_expr(self, expr),
        }
    }
}

fn collect_free_vars(expr: &IrExpr, bound: &HashSet<VarId>, free: &mut HashSet<VarId>) {
    let mut collector = FreeVarCollector { bound: bound.clone(), free: std::mem::take(free) };
    collector.visit_expr(expr);
    *free = collector.free;
}

fn collect_pattern_bindings(pattern: &IrPattern, bound: &mut HashSet<VarId>) {
    match pattern {
        IrPattern::Bind { var, .. } => { bound.insert(*var); }
        IrPattern::Constructor { args, .. } => {
            for p in args { collect_pattern_bindings(p, bound); }
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern {
                    collect_pattern_bindings(p, bound);
                }
            }
        }
        IrPattern::Tuple { elements } => {
            for p in elements { collect_pattern_bindings(p, bound); }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            collect_pattern_bindings(inner, bound);
        }
        _ => {}
    }
}

// ── Variable ID rewriting ────────────────────────────────────────

/// Rewrite Var references: replace original captured VarIds with new local VarIds.
fn rewrite_var_ids(expr: &IrExpr, mappings: &[(VarId, VarId, Ty)]) -> IrExpr {
    let kind = match &expr.kind {
        IrExprKind::Var { id } => {
            if let Some((_, new_id, _)) = mappings.iter().find(|(orig, _, _)| orig == id) {
                IrExprKind::Var { id: *new_id }
            } else {
                expr.kind.clone()
            }
        }
        // ClosureCreate captures: also rewrite VarIds
        IrExprKind::ClosureCreate { func_name, captures } => {
            let rewritten: Vec<(VarId, Ty)> = captures.iter().map(|(vid, ty)| {
                if let Some((_, new_id, _)) = mappings.iter().find(|(orig, _, _)| orig == vid) {
                    (*new_id, ty.clone())
                } else {
                    (*vid, ty.clone())
                }
            }).collect();
            IrExprKind::ClosureCreate { func_name: *func_name, captures: rewritten }
        }
        IrExprKind::Block { stmts, expr: tail } => IrExprKind::Block {
            stmts: stmts.iter().map(|s| rewrite_var_ids_stmt(s, mappings)).collect(),
            expr: tail.as_ref().map(|e| Box::new(rewrite_var_ids(e, mappings))),
        },
        IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
            target: match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(rewrite_var_ids(object, mappings)), method: *method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(rewrite_var_ids(callee, mappings)),
                },
                other => other.clone(),
            },
            args: args.iter().map(|a| rewrite_var_ids(a, mappings)).collect(),
            type_args: type_args.clone(),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_var_ids(cond, mappings)),
            then: Box::new(rewrite_var_ids(then, mappings)),
            else_: Box::new(rewrite_var_ids(else_, mappings)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_var_ids(subject, mappings)),
            arms: arms.iter().map(|arm| IrMatchArm {
                pattern: arm.pattern.clone(),
                guard: arm.guard.as_ref().map(|g| rewrite_var_ids(g, mappings)),
                body: rewrite_var_ids(&arm.body, mappings),
            }).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op: *op,
            left: Box::new(rewrite_var_ids(left, mappings)),
            right: Box::new(rewrite_var_ids(right, mappings)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op: *op, operand: Box::new(rewrite_var_ids(operand, mappings)),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var: *var, var_tuple: var_tuple.clone(),
            iterable: Box::new(rewrite_var_ids(iterable, mappings)),
            body: body.iter().map(|s| rewrite_var_ids_stmt(s, mappings)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_var_ids(cond, mappings)),
            body: body.iter().map(|s| rewrite_var_ids_stmt(s, mappings)).collect(),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.iter().map(|e| rewrite_var_ids(e, mappings)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.iter().map(|e| rewrite_var_ids(e, mappings)).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.iter().map(|e| rewrite_var_ids(e, mappings)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name: *name, fields: fields.iter().map(|(k, v)| (*k, rewrite_var_ids(v, mappings))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rewrite_var_ids(base, mappings)),
            fields: fields.iter().map(|(k, v)| (*k, rewrite_var_ids(v, mappings))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.iter().map(|(k, v)| (rewrite_var_ids(k, mappings), rewrite_var_ids(v, mappings))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rewrite_var_ids(start, mappings)),
            end: Box::new(rewrite_var_ids(end, mappings)),
            inclusive: *inclusive,
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_var_ids(object, mappings)), field: *field,
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(rewrite_var_ids(object, mappings)), index: *index,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rewrite_var_ids(object, mappings)),
            index: Box::new(rewrite_var_ids(index, mappings)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(rewrite_var_ids(object, mappings)),
            key: Box::new(rewrite_var_ids(key, mappings)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_var_ids(expr, mappings) },
                other => other.clone(),
            }).collect(),
        },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(rewrite_var_ids(expr, mappings)),
            fallback: Box::new(rewrite_var_ids(fallback, mappings)),
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(rewrite_var_ids(expr, mappings)), field: *field,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow {
            expr: Box::new(rewrite_var_ids(expr, mappings)), as_str: *as_str, mutable: *mutable,
        },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(rewrite_var_ids(expr, mappings)) },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name: *name, args: args.iter().map(|a| rewrite_var_ids(a, mappings)).collect(),
        },
        // ClosureCreate inside a lifted body: captures might reference outer captures
        IrExprKind::ClosureCreate { func_name, captures: inner_captures } => {
            let rewritten: Vec<(VarId, Ty)> = inner_captures.iter().map(|(vid, ty)| {
                // This captured var might itself be a capture from the enclosing scope
                // We don't rewrite VarIds here — the EnvLoad already replaced the Var node
                // in the inner lambda's body. The ClosureCreate stores values, which are
                // resolved at the call site (where the var is in scope as a local or env load).
                (*vid, ty.clone())
            }).collect();
            IrExprKind::ClosureCreate { func_name: *func_name, captures: rewritten }
        },
        // Leaf nodes
        _ => expr.kind.clone(),
    };
    IrExpr { kind, ty: expr.ty.clone(), span: expr.span }
}

fn rewrite_var_ids_stmt(stmt: &IrStmt, mappings: &[(VarId, VarId, Ty)]) -> IrStmt {
    let rw = |e: &IrExpr| rewrite_var_ids(e, mappings);
    let kind = match &stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var: *var, mutability: *mutability, ty: ty.clone(), value: rw(value),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern: pattern.clone(), value: rw(value),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
            var: *var, value: rw(value),
        },
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target: *target, index: rw(index), value: rw(value),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target: *target, key: rw(key), value: rw(value),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target: *target, field: *field, value: rw(value),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: rw(cond), else_: rw(else_),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rw(expr) },
        IrStmtKind::ListSwap { target, a, b } => IrStmtKind::ListSwap {
            target: *target, a: rw(a), b: rw(b),
        },
        other => other.clone(),
    };
    IrStmt { kind, span: stmt.span }
}
