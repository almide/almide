//! Closure Conversion pass (bottom-up): lifts all lambdas to top-level
//! functions with explicit environments.
//!
//! After this pass, no `Lambda` nodes remain in the IR. Each lambda becomes:
//! - A new `IrFunction` with an env pointer as its first parameter
//! - A `ClosureCreate` node at the original lambda site
//!
//! Inside lifted functions, captured variables are accessed via `EnvLoad` nodes.
//!
//! **Precondition**: `LambdaTypeResolve` has already run — all lambda
//! parameter types are concrete in VarTable. This pass does NO type inference.
//!
//! Inspired by Elm/Haskell/Gleam closure conversion.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_lang::types::Ty;
use almide_base::intern::sym;
use super::pass::{NanoPass, Postcondition, PassResult, Target};

#[derive(Debug)]
pub struct ClosureConversionPass;

impl NanoPass for ClosureConversionPass {
    fn name(&self) -> &str { "ClosureConversion" }

    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Wasm])
    }

    fn depends_on(&self) -> Vec<&'static str> { vec!["LambdaTypeResolve"] }

    fn postconditions(&self) -> Vec<Postcondition> {
        vec![Postcondition::Custom(verify_env_load_indices)]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut program_lifted = Vec::new();
        let mut counter = 0u32;

        // With the unified `program.var_table` (see
        // `pass_unify_var_tables`) every VarId across the program is a
        // single global namespace; lifted closures can be placed
        // wherever they're most convenient. Keeping module-local
        // closures inside their source module preserves the existing
        // WASM function-table layout and the per-module emission order.
        let IrProgram { functions, top_lets, modules, var_table, .. } = &mut program;
        for func in functions.iter_mut() {
            func.body = convert_expr(
                std::mem::take(&mut func.body),
                &mut program_lifted, &mut counter, var_table,
            );
        }
        for tl in top_lets.iter_mut() {
            tl.value = convert_expr(
                std::mem::take(&mut tl.value),
                &mut program_lifted, &mut counter, var_table,
            );
        }
        let any_changed = !program_lifted.is_empty();
        functions.extend(program_lifted);

        let mut module_changed = false;
        for module in modules.iter_mut() {
            let mut module_lifted = Vec::new();
            for func in module.functions.iter_mut() {
                func.body = convert_expr(
                    std::mem::take(&mut func.body),
                    &mut module_lifted, &mut counter, var_table,
                );
            }
            for tl in module.top_lets.iter_mut() {
                tl.value = convert_expr(
                    std::mem::take(&mut tl.value),
                    &mut module_lifted, &mut counter, var_table,
                );
            }
            if !module_lifted.is_empty() { module_changed = true; }
            module.functions.extend(module_lifted);
        }

        PassResult { program, changed: any_changed || module_changed }
    }
}

// ── Bottom-up Lambda → ClosureCreate conversion ─────────────────────

/// Arg index of an inline-eligible list-combinator's lambda for the `Module`
/// call form (e.g. egg's fused output), or None.
fn module_inline_lambda_arg(target: &CallTarget) -> Option<usize> {
    if let CallTarget::Module { module, func, .. } = target {
        if module.as_str() == "list" {
            return list_combinator_inline_arg(func.as_str());
        }
    }
    None
}

/// Same, for the usual post-IntrinsicLowering `RuntimeCall` form
/// (`almide_rt_list_<method>`). These are the only combinators the WASM emitter
/// inline-splices (map/filter at arg 1, fold at arg 2); leaving exactly their
/// lambda args raw keeps the fast path AND inline-context Perceus RC. A missed
/// form only costs a perf regression (the arg becomes a ClosureCreate that
/// dispatches via call_indirect), never correctness.
fn runtime_inline_lambda_arg(symbol: &almide_base::intern::Sym) -> Option<usize> {
    match symbol.as_str() {
        "almide_rt_list_map" | "almide_rt_list_filter" => Some(1),
        "almide_rt_list_fold" => Some(2),
        _ => None,
    }
}

fn list_combinator_inline_arg(func: &str) -> Option<usize> {
    match func {
        "map" | "filter" => Some(1),
        "fold" => Some(2),
        _ => None,
    }
}

/// Convert nested closures inside a (potential) inline lambda's body, but keep
/// the lambda itself RAW so the emitter can splice it. Non-lambda args fall back
/// to normal conversion.
fn keep_lambda_raw(
    expr: IrExpr,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    vt: &mut VarTable,
) -> IrExpr {
    let IrExpr { kind, ty, span, def_id } = expr;
    match kind {
        IrExprKind::Lambda { params, body, lambda_id } => IrExpr {
            kind: IrExprKind::Lambda {
                params,
                body: Box::new(convert_expr(*body, lifted, counter, vt)),
                lambda_id,
            },
            ty, span, def_id,
        },
        other => convert_expr(IrExpr { kind: other, ty, span, def_id }, lifted, counter, vt),
    }
}

fn convert_expr(
    expr: IrExpr,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    vt: &mut VarTable,
) -> IrExpr {
    let span = expr.span;
    let mut ty = expr.ty.clone();

    let kind = match expr.kind {
        IrExprKind::Lambda { params, body, lambda_id } => {
            // 1. Bottom-up: convert nested lambdas first
            let body = convert_expr(*body, lifted, counter, vt);

            // 2. Free variable analysis (single source of truth in almide-ir).
            let param_ids: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
            let free = almide_ir::free_vars::free_vars(&body, &param_ids);

            // A value-position lambda is always lifted to a ClosureCreate with a
            // stable, globally-unique function name — even with NO captures (the
            // emitter builds [table_idx, 0]). Inline-combinator args never reach
            // here: they were kept raw at the Call/RuntimeCall site above. So we no
            // longer leave capture-free *value* lambdas raw (that was the source of
            // the fragile lambda_id-matched value path). (Closure v2, P2b/A.)

            // Mutable captures stay raw: the emitter boxes them as heap cells in the
            // Lambda path, and a lifted env doesn't yet thread the cell correctly
            // (a P3 concern). So a mutate-captured lambda keeps the raw representation.
            if free.iter().any(|vid| body_assigns_to(&body, *vid)) {
                return IrExpr {
                    kind: IrExprKind::Lambda { params, body: Box::new(body), lambda_id },
                    ty, span, def_id: None,
                };
            }

            // 4. Build captures (sorted for deterministic env layout)
            let mut captures: Vec<(VarId, Ty)> = free.into_iter()
                .map(|vid| (vid, vt.get(vid).ty.clone()))
                .collect();
            captures.sort_by_key(|(vid, _)| vid.0);

            // 5. Generate lifted function
            let id = *counter;
            *counter = id + 1;
            let func_name = sym(&format!("__closure_{}", id));

            // Env parameter (i32 pointer in WASM, proxied as Ty::String)
            let env_ty = Ty::String;
            let env_var = vt.alloc(sym("__env"), env_ty.clone(), Mutability::Let, None);

            // Create EnvLoad bindings for each capture
            let mut prologue = Vec::new();
            let mut cap_locals: Vec<(VarId, VarId, Ty)> = Vec::new();
            for (idx, (vid, cap_ty)) in captures.iter().enumerate() {
                let local = vt.alloc(
                    sym(&format!("__cap_{}", idx)), cap_ty.clone(), Mutability::Let, None,
                );
                prologue.push(IrStmt {
                    kind: IrStmtKind::Bind {
                        var: local, mutability: Mutability::Let, ty: cap_ty.clone(),
                        value: IrExpr {
                            kind: IrExprKind::EnvLoad { env_var, index: idx as u32 },
                            ty: cap_ty.clone(), span: None, def_id: None,
                        },
                    },
                    span: None,
                });
                cap_locals.push((*vid, local, cap_ty.clone()));
            }

            // Rewrite body: replace captured VarIds with local VarIds.
            // Clone because `body.ty` is still referenced below for ret_ty.
            let mut rewritten = body.clone();
            rewrite_var_ids(&mut rewritten, &cap_locals);
            let final_body = if prologue.is_empty() {
                rewritten
            } else {
                let ty = rewritten.ty.clone();
                let span = rewritten.span;
                IrExpr {
                    kind: IrExprKind::Block {
                        stmts: prologue,
                        expr: Some(Box::new(rewritten)),
                    },
                    ty, span, def_id: None,
                }
            };

            // 6. Build lifted function params
            // Types come from: IR annotation → Ty::Fn wrapper → VarTable
            // (all should be resolved by LambdaTypeResolve)
            let fn_params: Option<Vec<Ty>> = match &ty {
                Ty::Fn { params, .. } => Some(params.clone()),
                _ => None,
            };
            let mut func_params = vec![IrParam {
                var: env_var, ty: env_ty, name: sym("__env"),
                borrow: ParamBorrow::Own, open_record: None, default: None, attrs: vec![],
            }];
            for (i, (vid, vty)) in params.iter().enumerate() {
                let info_name = vt.get(*vid).name;
                let info_ty = vt.get(*vid).ty.clone();
                let resolved = if !vty.is_unresolved_structural() {
                    vty.clone()
                } else if let Some(fp) = fn_params.as_ref().and_then(|ps| ps.get(i))
                    .filter(|t| !t.is_unresolved_structural())
                {
                    fp.clone()
                } else {
                    info_ty.clone()
                };
                // Sync back to VarTable
                if info_ty.is_unresolved_structural() && !resolved.is_unresolved_structural() {
                    vt.entries[vid.0 as usize].ty = resolved.clone();
                }
                func_params.push(IrParam {
                    var: *vid, ty: resolved, name: info_name,
                    borrow: ParamBorrow::Own, open_record: None, default: None, attrs: vec![],
                });
            }

            // Return type
            let ret_ty = match &ty {
                Ty::Fn { ret, .. } if !ret.is_unresolved() => *ret.clone(),
                _ => body.ty.clone(),
            };

            // Update enclosing expression type to resolved Fn signature
            let resolved_params: Vec<Ty> = func_params.iter().skip(1).map(|p| p.ty.clone()).collect();
            ty = Ty::Fn { params: resolved_params, ret: Box::new(ret_ty.clone()) };

            lifted.push(IrFunction {
                name: func_name, params: func_params, ret_ty,
                body: final_body,
                is_effect: false, is_async: false, is_test: false,
                generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
                visibility: IrVisibility::Private, doc: None, blank_lines_before: 0,
                def_id: None,
                mutated_params: vec![], module_origin: None,
            });

            IrExprKind::ClosureCreate { func_name, captures }
        }

        // ── Recursive conversion for all other nodes ──

        IrExprKind::Block { stmts, expr: tail } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| convert_stmt(s, lifted, counter, vt)).collect(),
            expr: tail.map(|e| Box::new(convert_expr(*e, lifted, counter, vt))),
        },
        IrExprKind::Call { target, args, type_args } => {
            // An inline-eligible list-combinator's lambda arg stays RAW so the WASM
            // emitter splices it (no alloc / no call_indirect) and Perceus processes
            // its body in the INLINE context — lifting it and re-inlining would
            // corrupt RC. Every OTHER lambda becomes a ClosureCreate value.
            // (Closure v2, P2b/A.)
            let inline_idx = module_inline_lambda_arg(&target);
            let args = args.into_iter().enumerate()
                .map(|(i, a)| {
                    if Some(i) == inline_idx { keep_lambda_raw(a, lifted, counter, vt) }
                    else { convert_expr(a, lifted, counter, vt) }
                })
                .collect();
            IrExprKind::Call { target: convert_target(target, lifted, counter, vt), args, type_args }
        }
        IrExprKind::RuntimeCall { symbol, args } => {
            let inline_idx = runtime_inline_lambda_arg(&symbol);
            let args = args.into_iter().enumerate()
                .map(|(i, a)| {
                    if Some(i) == inline_idx { keep_lambda_raw(a, lifted, counter, vt) }
                    else { convert_expr(a, lifted, counter, vt) }
                })
                .collect();
            IrExprKind::RuntimeCall { symbol, args }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(convert_expr(*cond, lifted, counter, vt)),
            then: Box::new(convert_expr(*then, lifted, counter, vt)),
            else_: Box::new(convert_expr(*else_, lifted, counter, vt)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(convert_expr(*subject, lifted, counter, vt)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| convert_expr(g, lifted, counter, vt)),
                body: convert_expr(arm.body, lifted, counter, vt),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(convert_expr(*iterable, lifted, counter, vt)),
            body: body.into_iter().map(|s| convert_stmt(s, lifted, counter, vt)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(convert_expr(*cond, lifted, counter, vt)),
            body: body.into_iter().map(|s| convert_stmt(s, lifted, counter, vt)).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(convert_expr(*left, lifted, counter, vt)),
            right: Box::new(convert_expr(*right, lifted, counter, vt)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(convert_expr(*operand, lifted, counter, vt)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| convert_expr(e, lifted, counter, vt)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| convert_expr(e, lifted, counter, vt)).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| convert_expr(e, lifted, counter, vt)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, convert_expr(v, lifted, counter, vt))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(convert_expr(*base, lifted, counter, vt)),
            fields: fields.into_iter().map(|(k, v)| (k, convert_expr(v, lifted, counter, vt))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (convert_expr(k, lifted, counter, vt), convert_expr(v, lifted, counter, vt))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(convert_expr(*start, lifted, counter, vt)),
            end: Box::new(convert_expr(*end, lifted, counter, vt)),
            inclusive,
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(convert_expr(*object, lifted, counter, vt)), field,
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(convert_expr(*object, lifted, counter, vt)), index,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(convert_expr(*object, lifted, counter, vt)),
            index: Box::new(convert_expr(*index, lifted, counter, vt)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(convert_expr(*object, lifted, counter, vt)),
            key: Box::new(convert_expr(*key, lifted, counter, vt)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: convert_expr(expr, lifted, counter, vt) },
                other => other,
            }).collect(),
        },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(convert_expr(*expr, lifted, counter, vt)),
            fallback: Box::new(convert_expr(*fallback, lifted, counter, vt)),
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(convert_expr(*expr, lifted, counter, vt)), field,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow {
            expr: Box::new(convert_expr(*expr, lifted, counter, vt)), as_str, mutable,
        },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(convert_expr(*expr, lifted, counter, vt)) },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|a| convert_expr(a, lifted, counter, vt)).collect(),
        },

        // Leaf nodes — no conversion needed
        other => other,
    };

    IrExpr { kind, ty, span, def_id: None }
}

fn convert_stmt(
    stmt: IrStmt,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    vt: &mut VarTable,
) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: convert_expr(value, lifted, counter, vt),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: convert_expr(value, lifted, counter, vt),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
            var, value: convert_expr(value, lifted, counter, vt),
        },
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target,
            index: convert_expr(index, lifted, counter, vt),
            value: convert_expr(value, lifted, counter, vt),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target,
            key: convert_expr(key, lifted, counter, vt),
            value: convert_expr(value, lifted, counter, vt),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target, field, value: convert_expr(value, lifted, counter, vt),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: convert_expr(cond, lifted, counter, vt),
            else_: convert_expr(else_, lifted, counter, vt),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr {
            expr: convert_expr(expr, lifted, counter, vt),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

fn convert_target(
    target: CallTarget,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    vt: &mut VarTable,
) -> CallTarget {
    match target {
        CallTarget::Method { object, method } => CallTarget::Method {
            object: Box::new(convert_expr(*object, lifted, counter, vt)), method,
        },
        CallTarget::Computed { callee } => CallTarget::Computed {
            callee: Box::new(convert_expr(*callee, lifted, counter, vt)),
        },
        other => other,
    }
}

// Free-variable / capture analysis now lives in `almide_ir::free_vars` — the
// single source of truth shared by this pass and the WASM emitter. (Closure v2, P1.)

// ── Mutable capture detection ───────────────────────────────────────

fn body_assigns_to(expr: &IrExpr, target: VarId) -> bool {
    struct Checker { target: VarId, found: bool }
    impl IrVisitor for Checker {
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            match &stmt.kind {
                IrStmtKind::Assign { var, .. }
                | IrStmtKind::IndexAssign { target: var, .. }
                | IrStmtKind::MapInsert { target: var, .. }
                | IrStmtKind::FieldAssign { target: var, .. }
                | IrStmtKind::ListSwap { target: var, .. } => {
                    if *var == self.target { self.found = true; }
                }
                _ => {}
            }
            if !self.found { walk_stmt(self, stmt); }
        }
        fn visit_expr(&mut self, expr: &IrExpr) {
            if !self.found { walk_expr(self, expr); }
        }
    }
    let mut c = Checker { target, found: false };
    c.visit_expr(expr);
    c.found
}

// ── Variable ID rewriting (IrMutVisitor-based) ──────────────────────
//
// Replace original captured VarIds with new local VarIds (EnvLoad targets).
// Uses `IrMutVisitor` to recurse, so new IrExprKind variants are handled
// automatically via walk_expr_mut.

use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut};
use almide_ir::IrMutVisitor;

struct VarIdRewriter<'a> {
    mappings: &'a [(VarId, VarId, Ty)],
}

impl<'a> VarIdRewriter<'a> {
    fn find(&self, id: VarId) -> Option<VarId> {
        self.mappings.iter().find(|(orig, _, _)| *orig == id).map(|(_, new, _)| *new)
    }
}

impl<'a> IrMutVisitor for VarIdRewriter<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        match &mut expr.kind {
            IrExprKind::Var { id } => {
                if let Some(new) = self.find(*id) { *id = new; }
            }
            IrExprKind::ClosureCreate { captures, .. } => {
                for (vid, _) in captures.iter_mut() {
                    if let Some(new) = self.find(*vid) { *vid = new; }
                }
            }
            _ => {}
        }
        walk_expr_mut(self, expr);
    }

    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        // Rewrite the VarId targets of assignments (mutations go to the local,
        // not the captured env slot).
        match &mut stmt.kind {
            IrStmtKind::Assign { var, .. }
            | IrStmtKind::IndexAssign { target: var, .. }
            | IrStmtKind::MapInsert { target: var, .. }
            | IrStmtKind::FieldAssign { target: var, .. }
            | IrStmtKind::ListSwap { target: var, .. } => {
                if let Some(new) = self.find(*var) { *var = new; }
            }
            _ => {}
        }
        walk_stmt_mut(self, stmt);
    }
}

fn rewrite_var_ids(expr: &mut IrExpr, mappings: &[(VarId, VarId, Ty)]) {
    let mut rewriter = VarIdRewriter { mappings };
    rewriter.visit_expr_mut(expr);
}

// ── Postcondition: verify EnvLoad indices are within bounds ──────────

fn verify_env_load_indices(program: &IrProgram) -> Vec<String> {
    let mut violations = Vec::new();

    let check_funcs = |funcs: &[IrFunction], violations: &mut Vec<String>| {
        for func in funcs {
            // Lifted closures have __env as first param and ClosureCreate captures
            // tell us the env size. But we can also verify structurally: collect
            // the max EnvLoad index in each function and ensure it's consistent
            // with the number of __cap_N bindings in the prologue.
            let cap_count = count_cap_bindings(&func.body);
            if cap_count == 0 { continue; }

            let max_index = max_env_load_index(&func.body);
            if let Some(max_idx) = max_index {
                if max_idx >= cap_count as u32 {
                    violations.push(format!(
                        "[ClosureConversion] in {}: EnvLoad index {} >= cap_count {} (offset would be {})",
                        func.name.as_str(), max_idx, cap_count, max_idx * 8,
                    ));
                }
            }
        }
    };

    check_funcs(&program.functions, &mut violations);
    for module in &program.modules {
        check_funcs(&module.functions, &mut violations);
    }
    violations
}

fn count_cap_bindings(expr: &IrExpr) -> usize {
    // Count __cap_N let bindings in the top-level block prologue
    if let IrExprKind::Block { stmts, .. } = &expr.kind {
        stmts.iter().filter(|s| {
            if let IrStmtKind::Bind { value, .. } = &s.kind {
                matches!(&value.kind, IrExprKind::EnvLoad { .. })
            } else {
                false
            }
        }).count()
    } else {
        0
    }
}

fn max_env_load_index(expr: &IrExpr) -> Option<u32> {
    struct MaxIdx(Option<u32>);
    impl IrVisitor for MaxIdx {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::EnvLoad { index, .. } = &expr.kind {
                self.0 = Some(self.0.map_or(*index, |m| m.max(*index)));
            }
            walk_expr(self, expr);
        }
    }
    let mut finder = MaxIdx(None);
    finder.visit_expr(expr);
    finder.0
}
