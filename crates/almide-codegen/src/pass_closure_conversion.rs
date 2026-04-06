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
    let mut ty = expr.ty.clone();

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
            // Use the outer Fn type (if available) as an authoritative source for
            // param/ret types — type inference may leave individual Lambda param
            // annotations as Unknown/TypeVar even when the enclosing Ty::Fn is
            // fully resolved (e.g. when passed to a stdlib callback).
            let fn_params: Option<Vec<Ty>> = match &ty {
                Ty::Fn { params, .. } => Some(params.clone()),
                _ => None,
            };
            let mut func_params = vec![IrParam {
                var: env_var,
                ty: env_ty.clone(), // env pointer (i32 in WASM, maps via ty_to_valtype)
                name: sym("__env"),
                borrow: ParamBorrow::Own,
                open_record: None,
                default: None,
            }];
            for (i, (vid, vty)) in params.iter().enumerate() {
                let info_name = var_table.get(*vid).name;
                let info_ty = var_table.get(*vid).ty.clone();
                // Priority: own annotation → Fn type → VarTable → body usage → fallback
                let resolved_ty = if !vty.is_unresolved_structural() {
                    vty.clone()
                } else if let Some(fp) = fn_params.as_ref().and_then(|ps| ps.get(i)).filter(|t| !t.is_unresolved_structural()) {
                    fp.clone()
                } else if !info_ty.is_unresolved_structural() {
                    info_ty.clone()
                } else if let Some(inferred) = infer_param_ty_from_body(&body, *vid) {
                    inferred
                } else {
                    info_ty.clone()
                };
                // Propagate the resolved type back to the VarTable so later
                // emit phases (Member access resolution, etc.) see a concrete
                // type instead of the original Unknown/TypeVar.
                if info_ty.is_unresolved_structural() && !resolved_ty.is_unresolved_structural() {
                    var_table.entries[vid.0 as usize].ty = resolved_ty.clone();
                }
                func_params.push(IrParam {
                    var: *vid,
                    ty: resolved_ty,
                    name: info_name,
                    borrow: ParamBorrow::Own,
                    open_record: None,
                    default: None,
                });
            }

            let ret_ty = match &ty {
                Ty::Fn { ret, .. } if !ret.is_unresolved() => *ret.clone(),
                _ if !body.ty.is_unresolved() => body.ty.clone(),
                _ => infer_body_result_ty(&body).unwrap_or_else(|| body.ty.clone()),
            };

            // Propagate the resolved signature back to the enclosing ClosureCreate
            // expression's type so downstream list/map/fold call sites see
            // the concrete param/ret types (they read from fn_arg.ty).
            let resolved_params: Vec<Ty> = func_params.iter().skip(1).map(|p| p.ty.clone()).collect();
            ty = Ty::Fn { params: resolved_params, ret: Box::new(ret_ty.clone()) };

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
        IrExprKind::Call { target, args, type_args } => {
            // Before descending, propagate list element type → inline Lambda
            // param types. Type inference may leave list closure callbacks'
            // params as Unknown/TypeVar even when the list's element type is
            // fully resolved, which breaks downstream Member/sort/map emit.
            let propagated_args = propagate_list_elem_to_lambda_params(&target, args, var_table);
            IrExprKind::Call {
                target: convert_target(target, lifted, counter, var_table),
                args: propagated_args.into_iter().map(|a| convert_expr(a, lifted, counter, var_table)).collect(),
                type_args,
            }
        }
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

/// For list stdlib calls whose callback Lambda has unresolved param types,
/// seed the Lambda param from the list's element type. This plugs the gap
/// where type inference didn't push the list element through to the lambda
/// param (most commonly with anonymous record element types).
///
/// Mutates the `vty` entries of inline Lambdas in `args` and updates the
/// VarTable entries for the param VarIds so downstream passes see the
/// propagated type.
fn propagate_list_elem_to_lambda_params(
    target: &almide_ir::CallTarget,
    args: Vec<IrExpr>,
    var_table: &mut VarTable,
) -> Vec<IrExpr> {
    use almide_ir::CallTarget;
    // Extract method name + the "list" arg (object or first positional arg).
    let (method_name, list_arg_idx): (Option<String>, usize) = match target {
        CallTarget::Method { method, .. } => (Some(method.to_string()), 0),
        CallTarget::Module { module, func } if module.as_str() == "list" => {
            (Some(func.to_string()), 0)
        }
        _ => (None, 0),
    };
    let Some(name) = method_name else { return args; };
    // Only methods that take `(list, ..., lambda)` benefit from this.
    if !matches!(
        name.as_str(),
        "map" | "filter" | "filter_map" | "flat_map" | "fold" | "reduce"
        | "find" | "any" | "all" | "each" | "count" | "partition"
        | "sort_by" | "group_by" | "unique_by" | "take_while" | "drop_while"
        | "min_by" | "max_by" | "scan" | "chunk_by" | "dedup_by"
    ) {
        return args;
    }
    // Resolve list element type from the list arg (or via VarTable).
    let list_elem = args.get(list_arg_idx).and_then(|a| match &a.ty {
        Ty::Applied(_, ta) => ta.first().cloned().filter(|t| !t.is_unresolved_structural()),
        _ => None,
    }).or_else(|| {
        args.get(list_arg_idx).and_then(|a| {
            if let IrExprKind::Var { id } = &a.kind {
                if let Ty::Applied(_, ta) = &var_table.get(*id).ty {
                    return ta.first().cloned().filter(|t| !t.is_unresolved_structural());
                }
            }
            None
        })
    });
    let Some(elem_ty) = list_elem else { return args; };
    // Walk args and update any inline Lambda whose first param is unresolved.
    args.into_iter().map(|arg| {
        match arg.kind {
            IrExprKind::Lambda { mut params, body, lambda_id } => {
                if let Some((vid, pty)) = params.first_mut() {
                    if pty.is_unresolved_structural() {
                        *pty = elem_ty.clone();
                        if var_table.get(*vid).ty.is_unresolved_structural() {
                            var_table.entries[vid.0 as usize].ty = elem_ty.clone();
                        }
                    }
                }
                // Also refresh the Lambda's outer `Ty::Fn.params[0]` so later
                // lookups of `lambda.ty` see the resolved element type.
                let refreshed_ty = match arg.ty {
                    Ty::Fn { params: fparams, ret } => {
                        let new_params: Vec<Ty> = fparams.into_iter().enumerate().map(|(i, p)| {
                            if i == 0 && p.is_unresolved_structural() { elem_ty.clone() } else { p }
                        }).collect();
                        Ty::Fn { params: new_params, ret }
                    }
                    other => other,
                };
                IrExpr {
                    kind: IrExprKind::Lambda { params, body, lambda_id },
                    ty: refreshed_ty,
                    span: arg.span,
                }
            }
            _ => arg,
        }
    }).collect()
}

/// Infer a Lambda body's result type when both its own `.ty` and the
/// enclosing `Ty::Fn` `ret` are unresolved. Traces the "tail" of the
/// expression tree (final value of blocks, branches of if/match, binop
/// results) to find a concrete type.
fn infer_body_result_ty(expr: &IrExpr) -> Option<Ty> {
    match &expr.kind {
        IrExprKind::Block { expr: Some(tail), .. } => infer_body_result_ty(tail),
        IrExprKind::If { then, else_, .. } => {
            infer_body_result_ty(then)
                .filter(|t| !t.is_unresolved())
                .or_else(|| infer_body_result_ty(else_))
        }
        IrExprKind::Match { arms, .. } => {
            arms.iter()
                .find_map(|arm| infer_body_result_ty(&arm.body).filter(|t| !t.is_unresolved()))
        }
        IrExprKind::BinOp { op, left, right } => {
            // Most ops have a fixed result type; ConcatList inherits from operands.
            op.result_ty().or_else(|| {
                if !left.ty.is_unresolved() { Some(left.ty.clone()) }
                else if !right.ty.is_unresolved() { Some(right.ty.clone()) }
                else { None }
            })
        }
        IrExprKind::LitInt { .. } => Some(Ty::Int),
        IrExprKind::LitFloat { .. } => Some(Ty::Float),
        IrExprKind::LitBool { .. } => Some(Ty::Bool),
        IrExprKind::LitStr { .. } => Some(Ty::String),
        _ => {
            if !expr.ty.is_unresolved() { Some(expr.ty.clone()) } else { None }
        }
    }
}

/// Infer a Lambda parameter's type by scanning the body for operations that
/// constrain it. Used as a last-resort fallback when type inference leaves
/// a param as Unknown/TypeVar.
///
/// Handles the common case where `p` is one operand of a homogeneous binop
/// (`a + b`, comparisons, etc.) and the sibling operand has a resolved type.
fn infer_param_ty_from_body(body: &IrExpr, target: VarId) -> Option<Ty> {
    fn walk(expr: &IrExpr, target: VarId) -> Option<Ty> {
        match &expr.kind {
            IrExprKind::BinOp { left, right, .. } => {
                // If one side is Var(target) and the other has a known type, use it.
                if let IrExprKind::Var { id } = &left.kind {
                    if *id == target && !right.ty.is_unresolved_structural() {
                        return Some(right.ty.clone());
                    }
                }
                if let IrExprKind::Var { id } = &right.kind {
                    if *id == target && !left.ty.is_unresolved_structural() {
                        return Some(left.ty.clone());
                    }
                }
                // Recurse
                walk(left, target).or_else(|| walk(right, target))
            }
            IrExprKind::If { cond, then, else_ } => {
                walk(cond, target)
                    .or_else(|| walk(then, target))
                    .or_else(|| walk(else_, target))
            }
            IrExprKind::Block { stmts, expr } => {
                for stmt in stmts {
                    if let Some(t) = walk_stmt_for_target(stmt, target) {
                        return Some(t);
                    }
                }
                expr.as_ref().and_then(|e| walk(e, target))
            }
            IrExprKind::Call { args, .. } => {
                args.iter().find_map(|a| walk(a, target))
            }
            IrExprKind::Match { subject, arms } => {
                walk(subject, target)
                    .or_else(|| arms.iter().find_map(|a| walk(&a.body, target)))
            }
            IrExprKind::UnOp { operand, .. } => walk(operand, target),
            IrExprKind::Member { object, .. } => walk(object, target),
            IrExprKind::IndexAccess { object, index } => {
                walk(object, target).or_else(|| walk(index, target))
            }
            _ => None,
        }
    }
    fn walk_stmt_for_target(stmt: &IrStmt, target: VarId) -> Option<Ty> {
        match &stmt.kind {
            IrStmtKind::Bind { value, .. } => walk(value, target),
            IrStmtKind::BindDestructure { value, .. } => walk(value, target),
            IrStmtKind::Assign { value, .. } => walk(value, target),
            IrStmtKind::Expr { expr } => walk(expr, target),
            _ => None,
        }
    }
    walk(body, target)
}
