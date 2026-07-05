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
        // Detect captured vars a closure MUTATES, before conversion rewrites their
        // references to `EnvLoad`s (after which the original VarId is gone). On WASM
        // such a var must become a shared heap cell (`mutable_captures`), which the
        // emitter seeds from `shared_mut_vars`. The IR `Mutability` flag is unreliable
        // here — a non-Copy var mutated only via a method like `list.push` is recorded
        // `Let` (it is never reassigned). (Closure v2 P6.)
        for v in detect_mutated_captures(&program) {
            program.codegen_annotations.shared_mut_vars.insert(v);
        }

        // A lambda that CAPTURES a shared cell var — even one it only reads — must
        // stay raw, not be lifted to a ClosureCreate/EnvLoad. The lifted env path
        // does not thread the heap cell (a reader closure would load the raw cell
        // ptr and use it as the object → garbage; sibling closures would not share
        // one cell). The raw Lambda path boxes captures as heap cells correctly, so
        // keeping these lambdas raw is what makes sibling/reader closures observe a
        // shared mutable capture consistently on WASM. (Closure v2 P6.)
        let shared: HashSet<VarId> = program.codegen_annotations.shared_mut_vars.clone();

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
                &mut program_lifted, &mut counter, var_table, &shared,
            );
        }
        for tl in top_lets.iter_mut() {
            tl.value = convert_expr(
                std::mem::take(&mut tl.value),
                &mut program_lifted, &mut counter, var_table, &shared,
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
                    &mut module_lifted, &mut counter, var_table, &shared,
                );
            }
            for tl in module.top_lets.iter_mut() {
                tl.value = convert_expr(
                    std::mem::take(&mut tl.value),
                    &mut module_lifted, &mut counter, var_table, &shared,
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
    shared: &HashSet<VarId>,
) -> IrExpr {
    let IrExpr { kind, ty, span, def_id } = expr;
    match kind {
        IrExprKind::Lambda { params, body, lambda_id } => IrExpr {
            kind: IrExprKind::Lambda {
                params,
                body: Box::new(convert_expr(*body, lifted, counter, vt, shared)),
                lambda_id,
            },
            ty, span, def_id,
        },
        other => convert_expr(IrExpr { kind: other, ty, span, def_id }, lifted, counter, vt, shared),
    }
}

fn convert_expr(
    expr: IrExpr,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    vt: &mut VarTable,
    shared: &HashSet<VarId>,
) -> IrExpr {
    let span = expr.span;
    let mut ty = expr.ty.clone();

    let kind = match expr.kind {
        IrExprKind::Lambda { params, body, lambda_id } => {
            // 1. Bottom-up: convert nested lambdas first
            let body = convert_expr(*body, lifted, counter, vt, shared);

            // 2. Free variable analysis (single source of truth in almide-ir).
            let param_ids: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
            let free = almide_ir::free_vars::free_vars(&body, &param_ids);

            // A value-position lambda is always lifted to a ClosureCreate with a
            // stable, globally-unique function name — even with NO captures (the
            // emitter builds [table_idx, 0]). Inline-combinator args never reach
            // here: they were kept raw at the Call/RuntimeCall site above. So we no
            // longer leave capture-free *value* lambdas raw (that was the source of
            // the fragile lambda_id-matched value path). (Closure v2, P2b/A.)

            // Shared-cell captures stay raw: the emitter boxes them as heap cells in
            // the Lambda path, and a lifted env doesn't thread the cell correctly
            // (a P3/P6 concern). A lambda is kept raw if it captures any var that is a
            // shared cell — whether THIS lambda mutates it (`list.push(acc, …)` or
            // `acc = …`) or merely READS it while a sibling closure mutates it. The
            // read-only case matters: `let read = () => list.len(xs)` beside
            // `let wipe = () => list.clear(xs)` — if `read` were lifted it would load
            // the raw cell ptr and use it as the list (garbage), and the two closures
            // would not share one cell. `shared` is the program-wide shared-cell set
            // (every var some closure mutates), so it already covers this lambda's own
            // mutations; `body_mutates` stays as a belt-and-suspenders fallback for a
            // var the global detection might miss. (Closure v2 P6.)
            if free.iter().any(|vid| shared.contains(vid) || body_mutates(&body, *vid)) {
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
                borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
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
                    borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![],
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
            stmts: stmts.into_iter().map(|s| convert_stmt(s, lifted, counter, vt, shared)).collect(),
            expr: tail.map(|e| Box::new(convert_expr(*e, lifted, counter, vt, shared))),
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
                    if Some(i) == inline_idx { keep_lambda_raw(a, lifted, counter, vt, shared) }
                    else { convert_expr(a, lifted, counter, vt, shared) }
                })
                .collect();
            IrExprKind::Call { target: convert_target(target, lifted, counter, vt, shared), args, type_args }
        }
        IrExprKind::RuntimeCall { symbol, args } => {
            let inline_idx = runtime_inline_lambda_arg(&symbol);
            let args = args.into_iter().enumerate()
                .map(|(i, a)| {
                    if Some(i) == inline_idx { keep_lambda_raw(a, lifted, counter, vt, shared) }
                    else { convert_expr(a, lifted, counter, vt, shared) }
                })
                .collect();
            IrExprKind::RuntimeCall { symbol, args }
        }
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(convert_expr(*cond, lifted, counter, vt, shared)),
            then: Box::new(convert_expr(*then, lifted, counter, vt, shared)),
            else_: Box::new(convert_expr(*else_, lifted, counter, vt, shared)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(convert_expr(*subject, lifted, counter, vt, shared)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| convert_expr(g, lifted, counter, vt, shared)),
                body: convert_expr(arm.body, lifted, counter, vt, shared),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(convert_expr(*iterable, lifted, counter, vt, shared)),
            body: body.into_iter().map(|s| convert_stmt(s, lifted, counter, vt, shared)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(convert_expr(*cond, lifted, counter, vt, shared)),
            body: body.into_iter().map(|s| convert_stmt(s, lifted, counter, vt, shared)).collect(),
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op,
            left: Box::new(convert_expr(*left, lifted, counter, vt, shared)),
            right: Box::new(convert_expr(*right, lifted, counter, vt, shared)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(convert_expr(*operand, lifted, counter, vt, shared)),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| convert_expr(e, lifted, counter, vt, shared)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| convert_expr(e, lifted, counter, vt, shared)).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| convert_expr(e, lifted, counter, vt, shared)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, convert_expr(v, lifted, counter, vt, shared))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(convert_expr(*base, lifted, counter, vt, shared)),
            fields: fields.into_iter().map(|(k, v)| (k, convert_expr(v, lifted, counter, vt, shared))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (convert_expr(k, lifted, counter, vt, shared), convert_expr(v, lifted, counter, vt, shared))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(convert_expr(*start, lifted, counter, vt, shared)),
            end: Box::new(convert_expr(*end, lifted, counter, vt, shared)),
            inclusive,
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(convert_expr(*object, lifted, counter, vt, shared)), field,
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(convert_expr(*object, lifted, counter, vt, shared)), index,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(convert_expr(*object, lifted, counter, vt, shared)),
            index: Box::new(convert_expr(*index, lifted, counter, vt, shared)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(convert_expr(*object, lifted, counter, vt, shared)),
            key: Box::new(convert_expr(*key, lifted, counter, vt, shared)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: convert_expr(expr, lifted, counter, vt, shared) },
                lit @ IrStringPart::Lit { .. } => lit,
            }).collect(),
        },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)),
            fallback: Box::new(convert_expr(*fallback, lifted, counter, vt, shared)),
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)), field,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow {
            expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)), as_str, mutable,
        },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(convert_expr(*expr, lifted, counter, vt, shared)) },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|a| convert_expr(a, lifted, counter, vt, shared)).collect(),
        },

        // Any other kind: recurse into every child so a Lambda nested in a
        // not-yet-listed node is still lifted (total by construction).
        other => return IrExpr { kind: other, ty, span, def_id: None }
            .map_children(&mut |e| convert_expr(e, lifted, counter, vt, shared)),
    };

    IrExpr { kind, ty, span, def_id: None }
}

fn convert_stmt(
    stmt: IrStmt,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    vt: &mut VarTable,
    shared: &HashSet<VarId>,
) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: convert_expr(value, lifted, counter, vt, shared),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: convert_expr(value, lifted, counter, vt, shared),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
            var, value: convert_expr(value, lifted, counter, vt, shared),
        },
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target,
            index: convert_expr(index, lifted, counter, vt, shared),
            value: convert_expr(value, lifted, counter, vt, shared),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target,
            key: convert_expr(key, lifted, counter, vt, shared),
            value: convert_expr(value, lifted, counter, vt, shared),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target, field, value: convert_expr(value, lifted, counter, vt, shared),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: convert_expr(cond, lifted, counter, vt, shared),
            else_: convert_expr(else_, lifted, counter, vt, shared),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr {
            expr: convert_expr(expr, lifted, counter, vt, shared),
        },
        other => return IrStmt { kind: other, span: stmt.span }
            .map_exprs(&mut |e| convert_expr(e, lifted, counter, vt, shared)),
    };
    IrStmt { kind, span: stmt.span }
}

fn convert_target(
    target: CallTarget,
    lifted: &mut Vec<IrFunction>,
    counter: &mut u32,
    vt: &mut VarTable,
    shared: &HashSet<VarId>,
) -> CallTarget {
    match target {
        CallTarget::Method { object, method } => CallTarget::Method {
            object: Box::new(convert_expr(*object, lifted, counter, vt, shared)), method,
        },
        CallTarget::Computed { callee } => CallTarget::Computed {
            callee: Box::new(convert_expr(*callee, lifted, counter, vt, shared)),
        },
        other @ (CallTarget::Named { .. } | CallTarget::Module { .. }) => other,
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

// ── P6: captured-and-mutated var detection (need a shared cell on WASM) ──

/// In-place stdlib mutators — runtime fns that take `&mut args[0]`. The single
/// source of truth for "this call mutates its receiver": used both to mark a
/// captured var a shared cell (WASM) and to route a mutator on a `ModuleRc`
/// global through `Rc::make_mut(&mut *c.borrow_mut())` instead of a clone (Rust).
pub(crate) fn is_inplace_mutator(symbol: &str) -> bool {
    // ONLY the runtime fns that take `&mut` on `args[0]` (verified against
    // runtime/rs/src/*.rs). The `list.set/insert/sort/reverse`, `map.set/remove`,
    // and all `set.*` ops return a NEW value (pure) — calling them in a closure and
    // discarding the result is a no-op, not a captured mutation.
    matches!(symbol,
        "almide_rt_list_push" | "almide_rt_list_pop" | "almide_rt_list_clear"
        | "almide_rt_map_insert" | "almide_rt_map_delete" | "almide_rt_map_clear"
        | "almide_rt_string_push" | "almide_rt_string_push_char" | "almide_rt_string_clear"
    )
    // Bytes builders mutate their buffer in place (the runtime takes `&mut`): push,
    // clear, fill, copy_within, set_at, as_mut_ptr, plus every append_*/set_*/write_*.
    // Matched by shape — the read side is read_*/get/slice/len/… (disjoint). This is
    // the complete &mut set in runtime/rs/src/bytes.rs; note bytes' stdlib `mut`
    // annotations are incomplete (only push/set_at/copy_within), so we cannot key off
    // the `mut` keyword here and instead encode the runtime's actual mutation surface.
    || symbol.strip_prefix("almide_rt_bytes_").is_some_and(|m| {
        matches!(m, "push" | "clear" | "fill" | "copy_within" | "set_at" | "as_mut_ptr")
            || m.starts_with("append_") || m.starts_with("set_") || m.starts_with("write_")
    })
}

/// Collects vars an expression mutates: assignment targets, `&mut`-borrows, and
/// `args[0]` of an in-place stdlib mutator call.
struct MutatedCollector { out: HashSet<VarId> }
impl IrVisitor for MutatedCollector {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Borrow { expr: inner, mutable: true, .. } => {
                if let IrExprKind::Var { id } = &inner.kind { self.out.insert(*id); }
            }
            IrExprKind::RuntimeCall { symbol, args } if is_inplace_mutator(symbol) => {
                if let Some(a) = args.first() {
                    if let IrExprKind::Var { id } = &a.kind { self.out.insert(*id); }
                }
            }
            _ => {}
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Assign { var, .. } => { self.out.insert(*var); }
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => { self.out.insert(*target); }
            _ => {}
        }
        walk_stmt(self, s);
    }
}

/// Local `var`s that are CAPTURED by some lambda AND MUTATED ANYWHERE (in that
/// lambda, another lambda, or the enclosing scope after capture) — these become
/// shared heap cells so every closure capturing them observes the live value:
/// capture-by-reference, matching Swift/JS/Python/Ruby/Go. (Previously only a
/// mutation INSIDE the capturing lambda's body counted, so `let f = () => x;
/// x = 42` snapshotted x at capture time.) Top-level globals are EXCLUDED — they
/// have their own storage (module Cell/RefCell, read via top_let_globals), not a
/// per-closure cell. Run before conversion rewrites captures to `EnvLoad`s.
fn detect_mutated_captures(program: &IrProgram) -> HashSet<VarId> {
    // (1) every var mutated anywhere in the program
    let mut mutated = MutatedCollector { out: HashSet::new() };
    for f in &program.functions { mutated.visit_expr(&f.body); }
    for tl in &program.top_lets { mutated.visit_expr(&tl.value); }
    for m in &program.modules {
        for f in &m.functions { mutated.visit_expr(&f.body); }
        for tl in &m.top_lets { mutated.visit_expr(&tl.value); }
    }
    // (2) every var captured (free in some lambda body)
    struct CapW { out: HashSet<VarId> }
    impl IrVisitor for CapW {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Lambda { params, body, .. } = &expr.kind {
                let param_set: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
                for v in almide_ir::free_vars::free_vars(body, &param_set) {
                    self.out.insert(v);
                }
            }
            walk_expr(self, expr);
        }
    }
    let mut cap = CapW { out: HashSet::new() };
    for f in &program.functions { cap.visit_expr(&f.body); }
    for tl in &program.top_lets { cap.visit_expr(&tl.value); }
    for m in &program.modules {
        for f in &m.functions { cap.visit_expr(&f.body); }
        for tl in &m.top_lets { cap.visit_expr(&tl.value); }
    }
    // (3) top-level globals are stored as Cell/RefCell, not per-closure cells
    let top_vars: HashSet<VarId> = program.top_lets.iter().map(|tl| tl.var)
        .chain(program.modules.iter().flat_map(|m| m.top_lets.iter().map(|tl| tl.var)))
        .collect();
    cap.out.intersection(&mutated.out).copied()
        .filter(|v| !top_vars.contains(v))
        .collect()
}

/// Does `body` mutate `var` (assignment, `&mut`-borrow, or in-place mutator call)?
fn body_mutates(body: &IrExpr, var: VarId) -> bool {
    let mut mc = MutatedCollector { out: HashSet::new() };
    mc.visit_expr(body);
    mc.out.contains(&var)
}
