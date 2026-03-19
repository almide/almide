//! StreamFusionPass: fuse pipe chains (map |> filter |> fold) into single loops.
//!
//! Uses the TypeConstructorRegistry's algebraic law table to determine
//! which fusions are valid. This ensures all optimizations are mathematically
//! guaranteed to preserve semantics.
//!
//! ## Fusion Rules (from algebraic laws)
//!
//! - FunctorComposition: `map(f) |> map(g)` → `map(f >> g)`
//! - FilterComposition: `filter(p) |> filter(q)` → `filter(x => p(x) && q(x))`
//! - MapFoldFusion: `map(f) |> fold(init, g)` → `fold(init, (acc, x) => g(acc, f(x)))`
//! - MapFilterFusion: `map(f) |> filter(p)` → single-pass filter_map
//!
//! ## Current Status
//!
//! Phase 1: Detection and analysis only (no rewriting yet).
//! Reports fusible chains for debugging/optimization planning.

use crate::ir::*;
use crate::types::Ty;
use crate::types::constructor::{TypeConstructorRegistry, AlgebraicLaw};
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct StreamFusionPass;

impl NanoPass for StreamFusionPass {
    fn name(&self) -> &str { "StreamFusion" }
    fn targets(&self) -> Option<Vec<Target>> { None } // All targets

    fn run(&self, program: &mut IrProgram, _target: Target) {
        // Pre-pass: inline single-use collection lets to expose nested call patterns
        for func in &mut program.functions {
            inline_single_use_collection_lets(&mut func.body, &program.var_table);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                inline_single_use_collection_lets(&mut func.body, &module.var_table);
            }
        }

        // Debug: dump IR after inlining
        if std::env::var("ALMIDE_DEBUG_FUSION").is_ok() {
            for func in &program.functions {
                dump_calls(&func.body, &func.name);
            }
        }

        // Phase 2: fuse chains using algebraic laws
        let mut totals = FusionCounts::default();
        for func in &mut program.functions {
            let c = fuse_all(func, &mut program.var_table);
            totals.map_map += c.map_map;
            totals.filter_filter += c.filter_filter;
            totals.map_fold += c.map_fold;
            totals.identity += c.identity;
            totals.flatmap_flatmap += c.flatmap_flatmap;
            totals.map_filter += c.map_filter;
        }
        let fused_count = totals.total();

        // Debug output
        if std::env::var("ALMIDE_DEBUG_FUSION").is_ok() {
            let registry = &program.type_registry;
            for func in &program.functions {
                let chains = detect_pipe_chains(&func.body, registry);
                if !chains.is_empty() {
                    for chain in &chains {
                        eprintln!(
                            "[StreamFusion] {}: {} ({} fusible, container={:?})",
                            func.name,
                            chain.ops.iter().map(|o| format!("{:?}", o)).collect::<Vec<_>>().join(" → "),
                            chain.fusible_pairs,
                            chain.container_name
                        );
                    }
                }
            }
            if fused_count > 0 {
                let mut parts = Vec::new();
                if totals.identity > 0 { parts.push(format!("{} identity-map", totals.identity)); }
                if totals.map_map > 0 { parts.push(format!("{} map+map", totals.map_map)); }
                if totals.filter_filter > 0 { parts.push(format!("{} filter+filter", totals.filter_filter)); }
                if totals.map_fold > 0 { parts.push(format!("{} map+fold", totals.map_fold)); }
                if totals.flatmap_flatmap > 0 { parts.push(format!("{} flat_map+flat_map", totals.flatmap_flatmap)); }
                if totals.map_filter > 0 { parts.push(format!("{} map+filter", totals.map_filter)); }
                if totals.filter_map_fold > 0 { parts.push(format!("{} filter_map+fold", totals.filter_map_fold)); }
                eprintln!("[StreamFusion] fused: {}", parts.join(", "));
            }
        }
    }
}

/// A detected pipe chain with analysis results.
#[derive(Debug)]
pub struct PipeChain {
    /// The operations in the chain (map, filter, fold, etc.)
    pub ops: Vec<PipeOp>,
    /// Number of adjacent pairs that can be fused
    pub fusible_pairs: usize,
    /// The type constructor being operated on (e.g., List)
    pub container_name: Option<String>,
}

/// A single operation in a pipe chain.
#[derive(Debug, Clone)]
pub enum PipeOp {
    Map,
    Filter,
    Fold,
    FlatMap,
    Other(String),
}

/// Unwrap decorator IR nodes (Borrow, ToVec, Clone) to find the inner expression.
/// StdlibLoweringPass wraps args in these; we need to see through them for chain detection.
fn unwrap_decorators(expr: &IrExpr) -> &IrExpr {
    match &expr.kind {
        IrExprKind::Borrow { expr: inner, .. } => unwrap_decorators(inner),
        IrExprKind::ToVec { expr: inner } => unwrap_decorators(inner),
        IrExprKind::Clone { expr: inner } => unwrap_decorators(inner),
        _ => expr,
    }
}

/// Detect pipe chains in an expression tree.
/// A pipe chain is a sequence of stdlib calls on a container type
/// connected via pipes (`|>`) or method chaining.
fn detect_pipe_chains(expr: &IrExpr, registry: &TypeConstructorRegistry) -> Vec<PipeChain> {
    let mut chains = Vec::new();
    detect_pipe_chains_inner(expr, registry, &mut chains);
    chains
}

fn detect_pipe_chains_inner(
    expr: &IrExpr,
    registry: &TypeConstructorRegistry,
    chains: &mut Vec<PipeChain>,
) {
    match &expr.kind {
        // Pipe chains appear as nested calls:
        // fold(filter(map(list, f), p), init, g)
        // StdlibLoweringPass wraps args in Borrow/ToVec nodes, so we unwrap those.
        IrExprKind::Call { target, args, .. } => {
            let call_name = match target {
                CallTarget::Named { name } => Some(name.as_str()),
                CallTarget::Module { func, .. } => Some(func.as_str()),
                _ => None,
            };
            if let Some(name) = call_name {
            if let Some(op) = classify_stdlib_op(name) {
                let mut chain_ops = vec![op];
                let mut current = args.first().map(|a| unwrap_decorators(a));

                while let Some(arg) = current {
                    let inner_op = extract_call_name(arg).and_then(classify_stdlib_op);
                    let inner_args_opt = match &arg.kind {
                        IrExprKind::Call { args, .. } => Some(args),
                        _ => None,
                    };
                    if let (Some(op), Some(inner_args)) = (inner_op, inner_args_opt) {
                        chain_ops.push(op);
                        current = inner_args.first().map(|a| unwrap_decorators(a));
                        continue;
                    }
                    break;
                }

                if chain_ops.len() >= 2 {
                    chain_ops.reverse(); // From inner to outer
                    let container_name = detect_container_type_from_call(expr);
                    let fusible_pairs = count_fusible_pairs(&chain_ops, &container_name, registry);
                    chains.push(PipeChain {
                        ops: chain_ops,
                        fusible_pairs,
                        container_name,
                    });
                    return;
                }
            }
            }

            // Recurse into args
            for arg in args {
                detect_pipe_chains_inner(arg, registry, chains);
            }
        }

        // Detect let-binding chains: let a = map(x); let b = filter(a); fold(b)
        IrExprKind::Block { stmts, expr: body } => {
            let mut let_chain: Vec<(VarId, PipeOp, &IrExpr)> = Vec::new();
            for stmt in stmts {
                let extended = match &stmt.kind {
                    IrStmtKind::Bind { var, value, .. } => try_extend_chain(value, *var, &mut let_chain),
                    IrStmtKind::Expr { expr } => try_extend_chain(expr, VarId(0), &mut let_chain),
                    _ => false,
                };
                if !extended {
                    flush_let_chain(&let_chain, registry, chains);
                    let_chain.clear();
                    match &stmt.kind {
                        IrStmtKind::Bind { value, .. } => detect_pipe_chains_inner(value, registry, chains),
                        IrStmtKind::Expr { expr } => detect_pipe_chains_inner(expr, registry, chains),
                        _ => {}
                    }
                }
            }
            let body_appended = body.as_ref()
                .map_or(false, |e| try_extend_chain(e, VarId(0), &mut let_chain));
            flush_let_chain(&let_chain, registry, chains);
            if let Some(e) = body {
                if !body_appended { detect_pipe_chains_inner(e, registry, chains); }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            detect_pipe_chains_inner(cond, registry, chains);
            detect_pipe_chains_inner(then, registry, chains);
            detect_pipe_chains_inner(else_, registry, chains);
        }
        IrExprKind::Lambda { body, .. } => {
            detect_pipe_chains_inner(body, registry, chains);
        }
        _ => {}
    }
}



/// Extract the function name from a call expression (Module or Named).
fn extract_call_name(expr: &IrExpr) -> Option<&str> {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Module { func, .. }, .. } => Some(func.as_str()),
        IrExprKind::Call { target: CallTarget::Named { name }, .. } => Some(name.as_str()),
        _ => None,
    }
}

/// Try to extend a let-binding chain with an expression. Returns true if appended.
fn try_extend_chain<'a>(
    expr: &'a IrExpr,
    var: VarId,
    let_chain: &mut Vec<(VarId, PipeOp, &'a IrExpr)>,
) -> bool {
    let Some(name) = extract_call_name(expr) else { return false };
    let Some(op) = classify_stdlib_op(name) else { return false };
    let is_chained = let_chain.last()
        .map_or(true, |(prev_var, _, _)| first_arg_is_var(expr, *prev_var));
    if is_chained {
        let_chain.push((var, op, expr));
        true
    } else {
        false
    }
}

/// Check if the first argument of a call expression is a variable reference.
fn first_arg_is_var(expr: &IrExpr, var: VarId) -> bool {
    if let IrExprKind::Call { args, .. } = &expr.kind {
        if let Some(first) = args.first() {
            let unwrapped = unwrap_decorators(first);
            if let IrExprKind::Var { id } = &unwrapped.kind {
                return *id == var;
            }
        }
    }
    false
}

/// Flush a collected let-binding chain into detected chains.
fn flush_let_chain(
    let_chain: &[(VarId, PipeOp, &IrExpr)],
    registry: &TypeConstructorRegistry,
    chains: &mut Vec<PipeChain>,
) {
    if let_chain.len() >= 2 {
        let ops: Vec<PipeOp> = let_chain.iter().map(|(_, op, _)| op.clone()).collect();
        // Detect container from first call's first argument type
        let container_name = if let Some((_, _, first_expr)) = let_chain.first() {
            detect_container_type_from_call(first_expr)
        } else {
            None
        };
        let fusible_pairs = count_fusible_pairs(&ops, &container_name, registry);
        chains.push(PipeChain {
            ops,
            fusible_pairs,
            container_name,
        });
    }
}

/// Classify a runtime function name or stdlib func name as a pipe operation.
fn classify_stdlib_op(name: &str) -> Option<PipeOp> {
    // Check multi-word ops first (before splitting by _)
    if name.ends_with("flat_map") {
        return Some(PipeOp::FlatMap);
    }
    if name.ends_with("filter_map") {
        return None; // filter_map is already fused, not a chain candidate
    }
    // Handle both "almide_rt_list_map" and plain "map"
    let func = name.rsplit('_').next().unwrap_or(name);
    match func {
        "map" => Some(PipeOp::Map),
        "filter" => Some(PipeOp::Filter),
        "fold" | "reduce" => Some(PipeOp::Fold),
        _ => None,
    }
}

/// Detect the container type from the expression or its first argument's type.
/// For fold/reduce (which returns a scalar), we look at the input list type instead.
fn detect_container_type_from_call(expr: &IrExpr) -> Option<String> {
    // Try the first argument — for pipe chains, it's the container being operated on
    if let IrExprKind::Call { args, .. } = &expr.kind {
        if let Some(first_arg) = args.first() {
            let unwrapped = unwrap_decorators(first_arg);
            if let Some(name) = unwrapped.ty.constructor_name() {
                if matches!(name, "List" | "Option" | "Result") {
                    return Some(name.to_string());
                }
            }
            // Recurse into nested call
            return detect_container_type_from_call(unwrapped);
        }
    }
    expr.ty.constructor_name().map(|s| s.to_string())
}

/// Count how many adjacent pairs in the chain can be fused,
/// using the algebraic law table.
fn count_fusible_pairs(
    ops: &[PipeOp],
    container_name: &Option<String>,
    registry: &TypeConstructorRegistry,
) -> usize {
    let name = match container_name {
        Some(n) => n.as_str(),
        None => return 0,
    };

    let mut count = 0;
    for pair in ops.windows(2) {
        let fusible = match (&pair[0], &pair[1]) {
            (PipeOp::Map, PipeOp::Map) => registry.satisfies(name, AlgebraicLaw::FunctorComposition),
            (PipeOp::Filter, PipeOp::Filter) => registry.satisfies(name, AlgebraicLaw::FilterComposition),
            (PipeOp::Map, PipeOp::Fold) => registry.satisfies(name, AlgebraicLaw::MapFoldFusion),
            (PipeOp::Map, PipeOp::Filter) => registry.satisfies(name, AlgebraicLaw::MapFilterFusion),
            (PipeOp::FlatMap, PipeOp::FlatMap) => registry.satisfies(name, AlgebraicLaw::MonadAssociativity),
            _ => false,
        };
        if fusible {
            count += 1;
        }
    }
    count
}

/// Inline single-use let bindings whose value is a collection operation (map/filter/fold/flat_map).
/// Converts `let a = map(x, f); fold(a, ...)` → `fold(map(x, f), ...)`.
/// This enables the nested-call fusion patterns to fire on pipe chains.
fn inline_single_use_collection_lets(body: &mut IrExpr, var_table: &VarTable) {
    if let IrExprKind::Block { stmts, expr } = &mut body.kind {
        // Iteratively inline single-use collection bindings from top to bottom.
        // Each round substitutes one variable into all subsequent stmts/expr,
        // so chained references (a → b → c) are resolved in order.
        let mut inlined_vars: std::collections::HashSet<VarId> = std::collections::HashSet::new();

        loop {
            let mut did_inline = false;
            for i in 0..stmts.len() {
                let (var, value) = match &stmts[i].kind {
                    IrStmtKind::Bind { var, value, .. } if !inlined_vars.contains(var) => (*var, value.clone()),
                    _ => continue,
                };
                // Only inline List module calls (not Result.map, Option.map, etc.)
                let is_list_collection_op = matches!(&value.kind,
                    IrExprKind::Call { target: CallTarget::Module { module, func }, .. }
                    if module == "list" && classify_stdlib_op(func).is_some()
                );
                if !is_list_collection_op || var_table.use_count(var) != 1 {
                    continue;
                }

                // Substitute this var into all subsequent stmts and tail expr
                for j in (i + 1)..stmts.len() {
                    let s = &mut stmts[j];
                    match &mut s.kind {
                        IrStmtKind::Bind { value: v, .. } => *v = substitute_var_in_expr(v, var, &value),
                        IrStmtKind::Expr { expr: e } => *e = substitute_var_in_expr(e, var, &value),
                        _ => {}
                    }
                }
                if let Some(e) = expr.as_mut() {
                    **e = substitute_var_in_expr(e, var, &value);
                }
                inlined_vars.insert(var);
                did_inline = true;
                break; // restart — substitution may have changed later stmts
            }
            if !did_inline { break; }
        }

        // Remove inlined bindings
        stmts.retain(|s| {
            if let IrStmtKind::Bind { var, .. } = &s.kind {
                !inlined_vars.contains(var)
            } else {
                true
            }
        });

        // Recurse into remaining stmts and expressions
        for stmt in stmts.iter_mut() {
            match &mut stmt.kind {
                IrStmtKind::Bind { value, .. } => inline_single_use_collection_lets(value, var_table),
                IrStmtKind::Expr { expr } => inline_single_use_collection_lets(expr, var_table),
                _ => {}
            }
        }
        if let Some(e) = expr { inline_single_use_collection_lets(e, var_table); }
    }
}

/// Run all fusion passes on a function. Returns (map_map_count, filter_filter_count, map_fold_count).
/// Fusion results: (map+map, filter+filter, map+fold, identity_map, flat_map+flat_map, map+filter)
fn fuse_all(func: &mut IrFunction, var_table: &mut VarTable) -> FusionCounts {
    let mut counts = FusionCounts::default();

    // FunctorIdentity: map(x, id) → x  (must run BEFORE map+map fusion)
    counts.identity = eliminate_identity_maps(&mut func.body);

    // FunctorComposition: map(map(x, f), g) → map(x, f >> g)
    let mm_before = count_map_calls(&func.body);
    func.body = fuse_map_map(func.body.clone());
    let mm_after = count_map_calls(&func.body);
    counts.map_map = if mm_before > mm_after { mm_before - mm_after } else { 0 };

    // FilterComposition: filter(filter(x, p), q) → filter(x, p && q)
    let ff_before = count_filter_calls(&func.body);
    func.body = fuse_filter_filter(func.body.clone());
    let ff_after = count_filter_calls(&func.body);
    counts.filter_filter = if ff_before > ff_after { ff_before - ff_after } else { 0 };

    // MapFoldFusion: fold(map(x, f), init, g) → fold(x, init, (acc, x) => g(acc, f(x)))
    counts.map_fold = fuse_map_fold_pass(&mut func.body);

    // MonadAssociativity: flat_map(flat_map(x, f), g) → flat_map(x, x => flat_map(f(x), g))
    let fm_before = count_calls_by_name_total(&func.body, "flat_map");
    func.body = fuse_flatmap_flatmap(func.body.clone());
    let fm_after = count_calls_by_name_total(&func.body, "flat_map");
    counts.flatmap_flatmap = if fm_before > fm_after { fm_before - fm_after } else { 0 };

    // MapFilterFusion: filter(map(x, f), p) → filter_map(x, x => { let y = f(x); if p(y) { some(y) } else { none } })
    counts.map_filter = fuse_map_filter_pass(&mut func.body);

    // FilterMapFoldFusion: fold(filter_map(x, fm), init, g) → fold(x, init, (acc, x) => match fm(x) { Some(v) => g(acc, v), None => acc })
    counts.filter_map_fold = fuse_filter_map_fold_pass(&mut func.body, var_table);

    counts
}

#[derive(Default)]
struct FusionCounts {
    map_map: usize,
    filter_filter: usize,
    map_fold: usize,
    identity: usize,
    flatmap_flatmap: usize,
    map_filter: usize,
    filter_map_fold: usize,
}

impl FusionCounts {
    fn total(&self) -> usize {
        self.map_map + self.filter_filter + self.map_fold + self.identity + self.flatmap_flatmap + self.map_filter + self.filter_map_fold
    }
}

fn count_filter_calls(expr: &IrExpr) -> usize {
    let mut count = 0;
    count_calls_by_name(expr, "filter", &mut count);
    count
}

fn count_calls_by_name(expr: &IrExpr, name: &str, count: &mut usize) {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            let matches = match target {
                CallTarget::Module { func, .. } => func == name,
                CallTarget::Named { name: n } => n.ends_with(&format!("_{}", name)),
                _ => false,
            };
            if matches { *count += 1; }
            for arg in args { count_calls_by_name(arg, name, count); }
        }
        IrExprKind::Block { stmts, expr } => {
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => count_calls_by_name(value, name, count),
                    IrStmtKind::Expr { expr } => count_calls_by_name(expr, name, count),
                    _ => {}
                }
            }
            if let Some(e) = expr { count_calls_by_name(e, name, count); }
        }
        IrExprKind::If { cond, then, else_ } => {
            count_calls_by_name(cond, name, count);
            count_calls_by_name(then, name, count);
            count_calls_by_name(else_, name, count);
        }
        IrExprKind::Lambda { body, .. } => count_calls_by_name(body, name, count),
        _ => {}
    }
}

fn is_filter_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "filter",
        CallTarget::Named { name } => name.ends_with("_filter") && !name.ends_with("_filter_map"),
        _ => false,
    }
}

fn is_fold_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "fold",
        CallTarget::Named { name } => name.ends_with("_fold"),
        _ => false,
    }
}

/// Fuse filter(filter(x, p), q) → filter(x, x => p(x) && q(x))
fn fuse_filter_filter(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    match expr.kind {
        IrExprKind::Call {
            ref target,
            ref args,
            ref type_args,
        } if is_filter_call(target) && args.len() >= 2 => {
            let inner = &args[0];
            if let IrExprKind::Call {
                target: ref inner_target,
                args: ref inner_args,
                ..
            } = inner.kind {
                if is_filter_call(inner_target) && inner_args.len() >= 2 {
                    let source = &inner_args[0];
                    let p = &inner_args[1]; // First predicate
                    let q = &args[1];       // Second predicate

                    // Compose: (x) => p(x) && q(x)
                    if let Some(composed) = compose_predicates(p, q) {
                        let fused = IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![fuse_filter_filter(source.clone()), composed],
                                type_args: type_args.clone(),
                            },
                            ty,
                            span,
                        };
                        return fuse_filter_filter(fused);
                    }
                }
            }
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_filter_filter(a.clone())).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        IrExprKind::Block { stmts, expr: body } => IrExpr {
            kind: IrExprKind::Block {
                stmts: stmts.into_iter().map(|s| fuse_filter_stmt(s)).collect(),
                expr: body.map(|e| Box::new(fuse_filter_filter(*e))),
            },
            ty, span,
        },
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(fuse_filter_filter(*cond)),
                then: Box::new(fuse_filter_filter(*then)),
                else_: Box::new(fuse_filter_filter(*else_)),
            },
            ty, span,
        },
        IrExprKind::Lambda { params, body } => IrExpr {
            kind: IrExprKind::Lambda { params, body: Box::new(fuse_filter_filter(*body)) },
            ty, span,
        },
        // Generic Call: recurse into args
        IrExprKind::Call { ref target, ref args, ref type_args } => {
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_filter_filter(a.clone())).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        other => IrExpr { kind: other, ty, span },
    }
}

fn fuse_filter_stmt(stmt: IrStmt) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: fuse_filter_filter(value),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: fuse_filter_filter(expr) },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

/// Compose two predicates: p and q → (x) => p(x) && q(x)
fn compose_predicates(p: &IrExpr, q: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: p_params, body: p_body },
        IrExprKind::Lambda { params: q_params, body: q_body },
    ) = (&p.kind, &q.kind) {
        if p_params.len() != 1 || q_params.len() != 1 {
            return None;
        }
        let (p_param_id, p_param_ty) = &p_params[0];
        let (q_param_id, _) = &q_params[0];

        // Substitute q's param with p's param in q's body
        let q_body_subst = substitute_var_in_expr(q_body, *q_param_id, &IrExpr {
            kind: IrExprKind::Var { id: *p_param_id },
            ty: p_param_ty.clone(),
            span: None,
        });

        // (x) => p_body && q_body_subst
        let composed_body = IrExpr {
            kind: IrExprKind::BinOp {
                op: crate::ir::BinOp::And,
                left: p_body.clone(),
                right: Box::new(q_body_subst),
            },
            ty: crate::types::Ty::Bool,
            span: None,
        };

        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*p_param_id, p_param_ty.clone())],
                body: Box::new(composed_body),
            },
            ty: p.ty.clone(),
            span: p.span,
        });
    }
    None
}

/// Fuse fold(map(x, f), init, g) → fold(x, init, (acc, x) => g(acc, f(x)))
/// Returns count of fusions performed.
fn fuse_map_fold_pass(body: &mut IrExpr) -> usize {
    let mut count = 0;
    *body = fuse_map_fold(body.clone(), &mut count);
    count
}

fn fuse_map_fold(expr: IrExpr, count: &mut usize) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    match expr.kind {
        IrExprKind::Call {
            ref target,
            ref args,
            ref type_args,
        } if is_fold_call(target) && args.len() >= 3 => {
            // fold(map(x, f), init, g) → fold(x, init, (acc, x) => g(acc, f(x)))
            let inner = &args[0];
            if let IrExprKind::Call {
                target: ref inner_target,
                args: ref inner_args,
                ..
            } = inner.kind {
                if is_map_call(inner_target) && inner_args.len() >= 2 {
                    let source = &inner_args[0]; // Original list
                    let f = &inner_args[1];       // Map function
                    let init = &args[1];           // Fold initial value
                    let g = &args[2];              // Fold reducer

                    if let Some(fused_reducer) = compose_map_into_fold(f, g) {
                        *count += 1;
                        return IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![
                                    fuse_map_fold(source.clone(), count),
                                    init.clone(),
                                    fused_reducer,
                                ],
                                type_args: type_args.clone(),
                            },
                            ty, span,
                        };
                    }
                }
            }
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_map_fold(a.clone(), count)).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        IrExprKind::Block { stmts, expr: body } => IrExpr {
            kind: IrExprKind::Block {
                stmts: stmts.into_iter().map(|s| {
                    let kind = match s.kind {
                        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                            var, mutability, ty, value: fuse_map_fold(value, count),
                        },
                        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: fuse_map_fold(expr, count) },
                        other => other,
                    };
                    IrStmt { kind, span: s.span }
                }).collect(),
                expr: body.map(|e| Box::new(fuse_map_fold(*e, count))),
            },
            ty, span,
        },
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(fuse_map_fold(*cond, count)),
                then: Box::new(fuse_map_fold(*then, count)),
                else_: Box::new(fuse_map_fold(*else_, count)),
            },
            ty, span,
        },
        IrExprKind::Lambda { params, body } => IrExpr {
            kind: IrExprKind::Lambda { params, body: Box::new(fuse_map_fold(*body, count)) },
            ty, span,
        },
        // Generic Call: recurse into args
        IrExprKind::Call { ref target, ref args, ref type_args } => {
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_map_fold(a.clone(), count)).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        other => IrExpr { kind: other, ty, span },
    }
}

/// Compose a map function into a fold reducer:
/// map f, fold g → (acc, x) => g(acc, f(x))
fn compose_map_into_fold(f: &IrExpr, g: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: f_params, body: f_body },
        IrExprKind::Lambda { params: g_params, body: g_body },
    ) = (&f.kind, &g.kind) {
        if f_params.len() != 1 || g_params.len() != 2 {
            return None;
        }
        let (f_param_id, f_param_ty) = &f_params[0];
        let (g_acc_id, g_acc_ty) = &g_params[0];
        let (g_elem_id, _) = &g_params[1];

        // In fold's reducer: g(acc, elem)
        // We want: g(acc, f(elem))
        // So substitute g_elem with f_body, and f_param with the new elem param
        //
        // New reducer: (acc, x) => g_body[g_elem := f_body[f_param := x]]
        // But f_param IS the x, so: (g_acc, f_param) => g_body[g_elem := f_body]
        let g_body_subst = substitute_var_in_expr(g_body, *g_elem_id, f_body);

        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![
                    (*g_acc_id, g_acc_ty.clone()),
                    (*f_param_id, f_param_ty.clone()),
                ],
                body: Box::new(g_body_subst),
            },
            ty: g.ty.clone(),
            span: g.span,
        });
    }
    None
}


/// Count map calls in an expression (for measuring fusion effectiveness).
fn count_map_calls(expr: &IrExpr) -> usize {
    let mut count = 0;
    count_map_calls_inner(expr, &mut count);
    count
}

fn count_map_calls_inner(expr: &IrExpr, count: &mut usize) {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            let is_map = match target {
                CallTarget::Module { func, .. } => func == "map",
                CallTarget::Named { name } => name.ends_with("_map") && !name.ends_with("flat_map") && !name.ends_with("filter_map"),
                _ => false,
            };
            if is_map { *count += 1; }
            for arg in args { count_map_calls_inner(arg, count); }
        }
        IrExprKind::Block { stmts, expr } => {
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => count_map_calls_inner(value, count),
                    IrStmtKind::Expr { expr } => count_map_calls_inner(expr, count),
                    _ => {}
                }
            }
            if let Some(e) = expr { count_map_calls_inner(e, count); }
        }
        IrExprKind::If { cond, then, else_ } => {
            count_map_calls_inner(cond, count);
            count_map_calls_inner(then, count);
            count_map_calls_inner(else_, count);
        }
        IrExprKind::Lambda { body, .. } => count_map_calls_inner(body, count),
        _ => {}
    }
}

/// Fuse consecutive map(map(x, f), g) → map(x, compose(f, g))
/// This eliminates one intermediate list allocation.
fn fuse_map_map(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    match expr.kind {
        // Nested map(map(x, f), g) → map(x, x => g(f(x)))
        IrExprKind::Call {
            target: ref outer_target,
            ref args,
            ref type_args,
        } if is_map_call(outer_target) && args.len() >= 2 => {
            // Check if first arg is also a map call
            let inner = &args[0];
            if let IrExprKind::Call {
                target: ref inner_target,
                args: ref inner_args,
                ..
            } = inner.kind {
                if is_map_call(inner_target) && inner_args.len() >= 2 {
                    let source = &inner_args[0];    // Original list
                    let f = &inner_args[1];          // First lambda: f
                    let g = &args[1];                // Second lambda: g

                    // Compose: (x) => g(f(x))
                    if let Some(composed) = compose_lambdas(f, g) {
                        let fused = IrExpr {
                            kind: IrExprKind::Call {
                                target: outer_target.clone(),
                                args: vec![fuse_map_map(source.clone()), composed],
                                type_args: type_args.clone(),
                            },
                            ty,
                            span,
                        };
                        return fuse_map_map(fused); // Recursively fuse further
                    }
                }
            }

            // No fusion possible — recurse into sub-expressions
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_map_map(a.clone())).collect();
            IrExpr {
                kind: IrExprKind::Call {
                    target: outer_target.clone(),
                    args: new_args,
                    type_args: type_args.clone(),
                },
                ty, span,
            }
        }

        // Recurse into blocks
        IrExprKind::Block { stmts, expr: body } => IrExpr {
            kind: IrExprKind::Block {
                stmts: stmts.into_iter().map(|s| fuse_stmt(s)).collect(),
                expr: body.map(|e| Box::new(fuse_map_map(*e))),
            },
            ty, span,
        },
        IrExprKind::DoBlock { stmts, expr: body } => IrExpr {
            kind: IrExprKind::DoBlock {
                stmts: stmts.into_iter().map(|s| fuse_stmt(s)).collect(),
                expr: body.map(|e| Box::new(fuse_map_map(*e))),
            },
            ty, span,
        },
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(fuse_map_map(*cond)),
                then: Box::new(fuse_map_map(*then)),
                else_: Box::new(fuse_map_map(*else_)),
            },
            ty, span,
        },
        IrExprKind::Lambda { params, body } => IrExpr {
            kind: IrExprKind::Lambda {
                params,
                body: Box::new(fuse_map_map(*body)),
            },
            ty, span,
        },
        IrExprKind::Match { subject, arms } => IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(fuse_map_map(*subject)),
                arms: arms.into_iter().map(|arm| IrMatchArm {
                    pattern: arm.pattern,
                    guard: arm.guard.map(fuse_map_map),
                    body: fuse_map_map(arm.body),
                }).collect(),
            },
            ty, span,
        },
        // Generic Call: recurse into args
        IrExprKind::Call { ref target, ref args, ref type_args } => {
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_map_map(a.clone())).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        // All other expressions: pass through
        other => IrExpr { kind: other, ty, span },
    }
}

fn fuse_stmt(stmt: IrStmt) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: fuse_map_map(value),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign {
            var, value: fuse_map_map(value),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr {
            expr: fuse_map_map(expr),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

fn is_map_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "map",
        CallTarget::Named { name } => name.ends_with("_map") && !name.ends_with("flat_map") && !name.ends_with("filter_map"),
        _ => false,
    }
}

/// Compose two lambdas: f and g → (x) => g(f(x))
/// Only works when both are simple lambda expressions.
fn compose_lambdas(f: &IrExpr, g: &IrExpr) -> Option<IrExpr> {
    // Both must be lambdas with exactly one parameter
    if let (
        IrExprKind::Lambda { params: f_params, body: f_body },
        IrExprKind::Lambda { params: g_params, body: g_body },
    ) = (&f.kind, &g.kind) {
        if f_params.len() != 1 || g_params.len() != 1 {
            return None;
        }
        let (f_param_id, f_param_ty) = &f_params[0];
        let (g_param_id, _g_param_ty) = &g_params[0];

        // Create composed lambda: (x) => g_body[g_param := f_body[f_param := x]]
        // Since f_param is already used in f_body, we just need to:
        // 1. Use f's parameter as the composed lambda's parameter
        // 2. Substitute g_param with f_body in g_body
        let composed_body = substitute_var_in_expr(g_body, *g_param_id, f_body);

        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*f_param_id, f_param_ty.clone())],
                body: Box::new(composed_body),
            },
            ty: g.ty.clone(), // The composed lambda has g's type
            span: f.span,
        });
    }
    None
}

/// Substitute all occurrences of a variable with an expression.
fn substitute_var_in_expr(expr: &IrExpr, var: VarId, replacement: &IrExpr) -> IrExpr {
    match &expr.kind {
        IrExprKind::Var { id } if *id == var => replacement.clone(),
        IrExprKind::Call { target, args, type_args } => IrExpr {
            kind: IrExprKind::Call {
                target: match target {
                    CallTarget::Method { object, method } => CallTarget::Method {
                        object: Box::new(substitute_var_in_expr(object, var, replacement)),
                        method: method.clone(),
                    },
                    CallTarget::Computed { callee } => CallTarget::Computed {
                        callee: Box::new(substitute_var_in_expr(callee, var, replacement)),
                    },
                    other => other.clone(),
                },
                args: args.iter().map(|a| substitute_var_in_expr(a, var, replacement)).collect(),
                type_args: type_args.clone(),
            },
            ty: expr.ty.clone(),
            span: expr.span,
        },
        IrExprKind::BinOp { op, left, right } => IrExpr {
            kind: IrExprKind::BinOp {
                op: op.clone(),
                left: Box::new(substitute_var_in_expr(left, var, replacement)),
                right: Box::new(substitute_var_in_expr(right, var, replacement)),
            },
            ty: expr.ty.clone(),
            span: expr.span,
        },
        IrExprKind::UnOp { op, operand } => IrExpr {
            kind: IrExprKind::UnOp {
                op: op.clone(),
                operand: Box::new(substitute_var_in_expr(operand, var, replacement)),
            },
            ty: expr.ty.clone(),
            span: expr.span,
        },
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(substitute_var_in_expr(cond, var, replacement)),
                then: Box::new(substitute_var_in_expr(then, var, replacement)),
                else_: Box::new(substitute_var_in_expr(else_, var, replacement)),
            },
            ty: expr.ty.clone(),
            span: expr.span,
        },
        IrExprKind::Member { object, field } => IrExpr {
            kind: IrExprKind::Member {
                object: Box::new(substitute_var_in_expr(object, var, replacement)),
                field: field.clone(),
            },
            ty: expr.ty.clone(),
            span: expr.span,
        },
        // For other expression kinds, return as-is (conservative)
        _ => expr.clone(),
    }
}


fn count_calls_by_name_total(expr: &IrExpr, name: &str) -> usize {
    let mut count = 0;
    count_calls_by_name(expr, name, &mut count);
    count
}

// ── FunctorIdentity: map(x, (x) => x) → x ──

/// Eliminate identity map calls: map(x, (x) => x) → x
/// Returns count of eliminations.
fn eliminate_identity_maps(body: &mut IrExpr) -> usize {
    let mut count = 0;
    *body = elim_identity(body.clone(), &mut count);
    count
}

fn elim_identity(expr: IrExpr, count: &mut usize) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    match expr.kind {
        IrExprKind::Call { ref target, ref args, .. }
            if is_map_call(target) && args.len() >= 2 =>
        {
            if is_identity_lambda(&args[1]) {
                *count += 1;
                // map(x, id) → x
                return elim_identity(args[0].clone(), count);
            }
            let new_args: Vec<IrExpr> = args.iter().map(|a| elim_identity(a.clone(), count)).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: match &expr.kind { IrExprKind::Call { type_args, .. } => type_args.clone(), _ => vec![] } },
                ty, span,
            }
        }
        IrExprKind::Block { stmts, expr: body } => IrExpr {
            kind: IrExprKind::Block {
                stmts: stmts.into_iter().map(|s| {
                    let kind = match s.kind {
                        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind { var, mutability, ty, value: elim_identity(value, count) },
                        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: elim_identity(expr, count) },
                        other => other,
                    };
                    IrStmt { kind, span: s.span }
                }).collect(),
                expr: body.map(|e| Box::new(elim_identity(*e, count))),
            },
            ty, span,
        },
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(elim_identity(*cond, count)),
                then: Box::new(elim_identity(*then, count)),
                else_: Box::new(elim_identity(*else_, count)),
            },
            ty, span,
        },
        IrExprKind::Lambda { params, body } => IrExpr {
            kind: IrExprKind::Lambda { params, body: Box::new(elim_identity(*body, count)) },
            ty, span,
        },
        // Generic Call: recurse into args
        IrExprKind::Call { ref target, ref args, ref type_args } => {
            let new_args: Vec<IrExpr> = args.iter().map(|a| elim_identity(a.clone(), count)).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        other => IrExpr { kind: other, ty, span },
    }
}

/// Check if a lambda is the identity function: (x) => x
fn is_identity_lambda(expr: &IrExpr) -> bool {
    if let IrExprKind::Lambda { params, body } = &expr.kind {
        if params.len() == 1 {
            let (param_id, _) = &params[0];
            if let IrExprKind::Var { id } = &body.kind {
                return id == param_id;
            }
        }
    }
    false
}

// ── MonadAssociativity: flat_map(flat_map(x, f), g) → flat_map(x, x => flat_map(f(x), g)) ──

fn is_flatmap_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "flat_map",
        CallTarget::Named { name } => name.ends_with("_flat_map"),
        _ => false,
    }
}

/// Fuse flat_map(flat_map(x, f), g) → flat_map(x, x => { let inner = f(x); flat_map(inner, g) })
fn fuse_flatmap_flatmap(expr: IrExpr) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    match expr.kind {
        IrExprKind::Call {
            ref target, ref args, ref type_args,
        } if is_flatmap_call(target) && args.len() >= 2 => {
            let inner = &args[0];
            if let IrExprKind::Call {
                target: ref inner_target, args: ref inner_args, ..
            } = inner.kind {
                if is_flatmap_call(inner_target) && inner_args.len() >= 2 {
                    let source = &inner_args[0];
                    let f = &inner_args[1];
                    let g = &args[1];

                    // Compose: (x) => flat_map(f(x), g)
                    if let Some(composed) = compose_flatmaps(f, g, target, type_args) {
                        let fused = IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![fuse_flatmap_flatmap(source.clone()), composed],
                                type_args: type_args.clone(),
                            },
                            ty, span,
                        };
                        return fuse_flatmap_flatmap(fused);
                    }
                }
            }
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_flatmap_flatmap(a.clone())).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        IrExprKind::Block { stmts, expr: body } => IrExpr {
            kind: IrExprKind::Block {
                stmts: stmts.into_iter().map(|s| {
                    let kind = match s.kind {
                        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind { var, mutability, ty, value: fuse_flatmap_flatmap(value) },
                        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: fuse_flatmap_flatmap(expr) },
                        other => other,
                    };
                    IrStmt { kind, span: s.span }
                }).collect(),
                expr: body.map(|e| Box::new(fuse_flatmap_flatmap(*e))),
            },
            ty, span,
        },
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(fuse_flatmap_flatmap(*cond)),
                then: Box::new(fuse_flatmap_flatmap(*then)),
                else_: Box::new(fuse_flatmap_flatmap(*else_)),
            },
            ty, span,
        },
        IrExprKind::Lambda { params, body } => IrExpr {
            kind: IrExprKind::Lambda { params, body: Box::new(fuse_flatmap_flatmap(*body)) },
            ty, span,
        },
        // Generic Call: recurse into args
        IrExprKind::Call { ref target, ref args, ref type_args } => {
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_flatmap_flatmap(a.clone())).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        other => IrExpr { kind: other, ty, span },
    }
}

/// Compose two flat_map functions: f and g → (x) => flat_map(f(x), g)
fn compose_flatmaps(f: &IrExpr, g: &IrExpr, flat_map_target: &CallTarget, type_args: &[crate::types::Ty]) -> Option<IrExpr> {
    if let IrExprKind::Lambda { params: f_params, body: f_body } = &f.kind {
        if f_params.len() != 1 {
            return None;
        }
        let (f_param_id, f_param_ty) = &f_params[0];

        // (x) => flat_map(f_body, g)
        let inner_call = IrExpr {
            kind: IrExprKind::Call {
                target: flat_map_target.clone(),
                args: vec![*f_body.clone(), g.clone()],
                type_args: type_args.to_vec(),
            },
            ty: f.ty.clone(), // approximate
            span: f.span,
        };

        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*f_param_id, f_param_ty.clone())],
                body: Box::new(inner_call),
            },
            ty: f.ty.clone(),
            span: f.span,
        });
    }
    None
}

// ── MapFilterFusion: filter(map(x, f), p) → filter_map(x, ...) ──

/// Fuse filter(map(x, f), p) → filter_map(x, (x) => { let y = f(x); if p(y) { some(y) } else { none } })
fn fuse_map_filter_pass(body: &mut IrExpr) -> usize {
    let mut count = 0;
    *body = fuse_map_filter(body.clone(), &mut count);
    count
}

fn fuse_map_filter(expr: IrExpr, count: &mut usize) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    match expr.kind {
        IrExprKind::Call {
            ref target, ref args, ref type_args,
        } if is_filter_call(target) && args.len() >= 2 => {
            let inner = &args[0];
            // Check if first arg is a map call
            if let IrExprKind::Call {
                target: ref inner_target, args: ref inner_args, ..
            } = inner.kind {
                if is_map_call(inner_target) && inner_args.len() >= 2 {
                    let source = &inner_args[0];
                    let f = &inner_args[1];  // map function
                    let p = &args[1];         // filter predicate

                    if let Some(filter_map_lambda) = compose_map_filter(f, p) {
                        *count += 1;
                        let fm_target = match inner_target {
                            CallTarget::Module { module, .. } => CallTarget::Module {
                                module: module.clone(),
                                func: "filter_map".to_string(),
                            },
                            CallTarget::Named { name } => {
                                let base = name.replace("_map", "_filter_map");
                                CallTarget::Named { name: base }
                            }
                            other => other.clone(),
                        };

                        return IrExpr {
                            kind: IrExprKind::Call {
                                target: fm_target,
                                args: vec![fuse_map_filter(source.clone(), count), filter_map_lambda],
                                type_args: type_args.clone(),
                            },
                            ty, span,
                        };
                    }
                }
            }
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_map_filter(a.clone(), count)).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        // Generic Call: recurse into args (so fusion fires inside println(list.filter(list.map(...))))
        IrExprKind::Call { ref target, ref args, ref type_args } => {
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_map_filter(a.clone(), count)).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        IrExprKind::Block { stmts, expr: body } => IrExpr {
            kind: IrExprKind::Block {
                stmts: stmts.into_iter().map(|s| {
                    let kind = match s.kind {
                        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind { var, mutability, ty, value: fuse_map_filter(value, count) },
                        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: fuse_map_filter(expr, count) },
                        other => other,
                    };
                    IrStmt { kind, span: s.span }
                }).collect(),
                expr: body.map(|e| Box::new(fuse_map_filter(*e, count))),
            },
            ty, span,
        },
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(fuse_map_filter(*cond, count)),
                then: Box::new(fuse_map_filter(*then, count)),
                else_: Box::new(fuse_map_filter(*else_, count)),
            },
            ty, span,
        },
        IrExprKind::Lambda { params, body } => IrExpr {
            kind: IrExprKind::Lambda { params, body: Box::new(fuse_map_filter(*body, count)) },
            ty, span,
        },
        other => IrExpr { kind: other, ty, span },
    }
}

/// Compose map function f and filter predicate p into a filter_map lambda:
/// (x) => { let y = f(x); if p(y) { some(y) } else { none } }
fn compose_map_filter(f: &IrExpr, p: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: f_params, body: f_body },
        IrExprKind::Lambda { params: p_params, body: p_body },
    ) = (&f.kind, &p.kind) {
        if f_params.len() != 1 || p_params.len() != 1 {
            return None;
        }
        let (f_param_id, f_param_ty) = &f_params[0];
        let (p_param_id, _) = &p_params[0];

        // Substitute p's param with f_body in p_body (p applied to f(x))
        let p_applied = substitute_var_in_expr(p_body, *p_param_id, f_body);

        // Build: if p(f(x)) { Some(f(x)) } else { None }
        let result_ty = f_body.ty.clone();
        let composed_body = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(p_applied),
                then: Box::new(IrExpr {
                    kind: IrExprKind::OptionSome { expr: f_body.clone() },
                    ty: Ty::option(result_ty.clone()),
                    span: None,
                }),
                else_: Box::new(IrExpr {
                    kind: IrExprKind::OptionNone,
                    ty: Ty::option(result_ty),
                    span: None,
                }),
            },
            ty: f_body.ty.clone(), // approximate
            span: None,
        };

        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*f_param_id, f_param_ty.clone())],
                body: Box::new(composed_body),
            },
            ty: f.ty.clone(),
            span: f.span,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_map() {
        assert!(matches!(classify_stdlib_op("almide_rt_list_map"), Some(PipeOp::Map)));
    }

    #[test]
    fn classify_filter() {
        assert!(matches!(classify_stdlib_op("almide_rt_list_filter"), Some(PipeOp::Filter)));
    }

    #[test]
    fn classify_fold() {
        assert!(matches!(classify_stdlib_op("almide_rt_list_fold"), Some(PipeOp::Fold)));
    }

    #[test]
    fn classify_flat_map() {
        assert!(matches!(classify_stdlib_op("almide_rt_list_flat_map"), Some(PipeOp::FlatMap)));
    }

    #[test]
    fn classify_unknown() {
        assert!(classify_stdlib_op("almide_rt_list_length").is_none());
    }

    #[test]
    fn count_fusible_map_map() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::Map, PipeOp::Map];
        assert_eq!(count_fusible_pairs(&ops, &Some("List".into()), &registry), 1);
    }

    #[test]
    fn count_fusible_map_filter_fold() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::Map, PipeOp::Filter, PipeOp::Fold];
        // map→filter = fusible (MapFilterFusion), filter→fold = not fusible
        assert_eq!(count_fusible_pairs(&ops, &Some("List".into()), &registry), 1);
    }

    #[test]
    fn count_fusible_map_fold() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::Map, PipeOp::Fold];
        assert_eq!(count_fusible_pairs(&ops, &Some("List".into()), &registry), 1);
    }

    #[test]
    fn count_fusible_none_for_map() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::Map, PipeOp::Map];
        // Map (key-value) has no FunctorComposition law
        assert_eq!(count_fusible_pairs(&ops, &Some("Map".into()), &registry), 0);
    }

    #[test]
    fn count_fusible_option_flatmap() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::FlatMap, PipeOp::FlatMap];
        assert_eq!(count_fusible_pairs(&ops, &Some("Option".into()), &registry), 1);
    }
}

// ── FilterMapFoldFusion: fold(filter_map(x, fm), init, g) → fold(x, init, fused_reducer) ──

fn is_filter_map_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "filter_map",
        CallTarget::Named { name } => name.ends_with("_filter_map"),
        _ => false,
    }
}

fn fuse_filter_map_fold_pass(body: &mut IrExpr, vt: &mut VarTable) -> usize {
    let mut count = 0;
    *body = fuse_filter_map_fold(body.clone(), &mut count, vt);
    count
}

fn fuse_filter_map_fold(expr: IrExpr, count: &mut usize, vt: &mut VarTable) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    match expr.kind {
        IrExprKind::Call {
            ref target, ref args, ref type_args,
        } if is_fold_call(target) && args.len() >= 3 => {
            let inner = &args[0];
            if let IrExprKind::Call {
                target: ref inner_target,
                args: ref inner_args,
                ..
            } = inner.kind {
                if is_filter_map_call(inner_target) && inner_args.len() >= 2 {
                    let source = &inner_args[0];    // Original list
                    let fm = &inner_args[1];         // filter_map lambda: (x) => Option<B>
                    let init = &args[1];             // Fold initial value
                    let g = &args[2];                // Fold reducer: (acc, v) => acc'

                    if let Some(fused_reducer) = compose_filter_map_into_fold(fm, g, vt) {
                        *count += 1;
                        return IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![
                                    fuse_filter_map_fold(source.clone(), count, vt),
                                    init.clone(),
                                    fused_reducer,
                                ],
                                type_args: type_args.clone(),
                            },
                            ty, span,
                        };
                    }
                }
            }
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_filter_map_fold(a.clone(), count, vt)).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        // Generic Call: recurse into args
        IrExprKind::Call { ref target, ref args, ref type_args } => {
            let new_args: Vec<IrExpr> = args.iter().map(|a| fuse_filter_map_fold(a.clone(), count, vt)).collect();
            IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: new_args, type_args: type_args.clone() },
                ty, span,
            }
        }
        IrExprKind::Block { stmts, expr: body } => IrExpr {
            kind: IrExprKind::Block {
                stmts: stmts.into_iter().map(|s| {
                    let kind = match s.kind {
                        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
                            var, mutability, ty, value: fuse_filter_map_fold(value, count, vt),
                        },
                        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: fuse_filter_map_fold(expr, count, vt) },
                        other => other,
                    };
                    IrStmt { kind, span: s.span }
                }).collect(),
                expr: body.map(|e| Box::new(fuse_filter_map_fold(*e, count, vt))),
            },
            ty, span,
        },
        IrExprKind::If { cond, then, else_ } => IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(fuse_filter_map_fold(*cond, count, vt)),
                then: Box::new(fuse_filter_map_fold(*then, count, vt)),
                else_: Box::new(fuse_filter_map_fold(*else_, count, vt)),
            },
            ty, span,
        },
        IrExprKind::Lambda { params, body } => IrExpr {
            kind: IrExprKind::Lambda { params, body: Box::new(fuse_filter_map_fold(*body, count, vt)) },
            ty, span,
        },
        other => IrExpr { kind: other, ty, span },
    }
}

/// Compose filter_map lambda fm and fold reducer g into a single reducer:
/// fm: (x) => Option<B>,  g: (acc, v) => acc'
/// → (acc, x) => { match fm(x) { Some(v) => g(acc, v), None => acc } }
fn compose_filter_map_into_fold(fm: &IrExpr, g: &IrExpr, vt: &mut VarTable) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: fm_params, body: _fm_body },
        IrExprKind::Lambda { params: g_params, body: g_body },
    ) = (&fm.kind, &g.kind) {
        if fm_params.len() != 1 || g_params.len() != 2 {
            return None;
        }
        let (fm_param_id, fm_param_ty) = &fm_params[0];
        let (g_acc_id, g_acc_ty) = &g_params[0];
        let (g_elem_id, _g_elem_ty) = &g_params[1];

        // Build: (acc, x) => match fm(x) { Some(v) => g_body[g_elem := v], None => acc }
        // We apply fm's body directly: substitute fm_param with x, then match the result.
        // But fm is a full lambda — we call it: fm(x)
        let fm_call = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(fm.clone()) },
                args: vec![IrExpr {
                    kind: IrExprKind::Var { id: *fm_param_id },
                    ty: fm_param_ty.clone(),
                    span: None,
                }],
                type_args: vec![],
            },
            ty: Ty::Unknown, // Option<B>, approximate
            span: None,
        };

        // Match arms: Some(v) => g(acc, v), None => acc
        let v_var = vt.alloc("__fused_v".into(), g_acc_ty.clone(), Mutability::Let, None); // temporary — will be unique enough for codegen
        let some_arm = IrMatchArm {
            pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: v_var }) },
            guard: None,
            body: substitute_var_in_expr(g_body, *g_elem_id, &IrExpr {
                kind: IrExprKind::Var { id: v_var },
                ty: g_acc_ty.clone(), // approximate
                span: None,
            }),
        };
        let none_arm = IrMatchArm {
            pattern: IrPattern::None,
            guard: None,
            body: IrExpr {
                kind: IrExprKind::Var { id: *g_acc_id },
                ty: g_acc_ty.clone(),
                span: None,
            },
        };

        let match_expr = IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(fm_call),
                arms: vec![some_arm, none_arm],
            },
            ty: g_acc_ty.clone(),
            span: None,
        };

        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![
                    (*g_acc_id, g_acc_ty.clone()),
                    (*fm_param_id, fm_param_ty.clone()),
                ],
                body: Box::new(match_expr),
            },
            ty: g.ty.clone(),
            span: g.span,
        });
    }
    None
}

fn dump_calls(expr: &IrExpr, ctx: &str) {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            let name = match target {
                CallTarget::Module { module, func } => format!("Module({}.{})", module, func),
                CallTarget::Named { name } => format!("Named({})", name),
                _ => "Other".to_string(),
            };
            eprintln!("[Fusion-IR] {}: {} with {} args", ctx, name, args.len());
            for a in args { dump_calls(a, ctx); }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                match &s.kind {
                    IrStmtKind::Bind { var, value, .. } => {
                        eprintln!("[Fusion-IR] {}: Bind var={}", ctx, var.0);
                        dump_calls(value, ctx);
                    }
                    IrStmtKind::Expr { expr } => dump_calls(expr, ctx),
                    _ => {}
                }
            }
            if let Some(e) = expr { dump_calls(e, ctx); }
        }
        _ => {}
    }
}
