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
use crate::types::constructor::{TypeConstructorRegistry, AlgebraicLaw};
use super::pass::{NanoPass, Target};

#[derive(Debug)]
pub struct StreamFusionPass;

impl NanoPass for StreamFusionPass {
    fn name(&self) -> &str { "StreamFusion" }
    fn targets(&self) -> Option<Vec<Target>> { None } // All targets

    fn run(&self, program: &mut IrProgram, _target: Target) {
        let registry = &program.type_registry;
        for func in &program.functions {
            let chains = detect_pipe_chains(&func.body, registry);
            // Debug: log function analysis
            if std::env::var("ALMIDE_DEBUG_FUSION").is_ok() {
                eprintln!("[StreamFusion] analyzing {} — {} chain(s)", func.name, chains.len());
            }
            for chain in &chains {
                {
                    // Phase 1: detection only. In Phase 2, we'll rewrite.
                    // For now, annotate the program with optimization opportunities.
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
                    let inner_name = match &arg.kind {
                        IrExprKind::Call { target: CallTarget::Named { name }, .. } => Some(name.as_str()),
                        IrExprKind::Call { target: CallTarget::Module { func, .. }, .. } => Some(func.as_str()),
                        _ => None,
                    };
                    if let Some(iname) = inner_name {
                        if let Some(inner_op) = classify_stdlib_op(iname) {
                            if let IrExprKind::Call { args: inner_args, .. } = &arg.kind {
                                chain_ops.push(inner_op);
                                current = inner_args.first().map(|a| unwrap_decorators(a));
                                continue;
                            }
                        }
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
        // This is how |> desugars in Almide's IR
        IrExprKind::Block { stmts, expr: body } => {
            // Collect sequential let bindings that are pipe operations
            let mut let_chain: Vec<(VarId, PipeOp, &IrExpr)> = Vec::new();
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { var, value, .. } => {
                        let call_name = match &value.kind {
                            IrExprKind::Call { target: CallTarget::Module { func, .. }, .. } => Some(func.as_str()),
                            IrExprKind::Call { target: CallTarget::Named { name }, .. } => Some(name.as_str()),
                            _ => None,
                        };
                        if let Some(name) = call_name {
                            if let Some(op) = classify_stdlib_op(name) {
                                // Check if first arg references the previous let binding
                                let is_chained = if let Some((prev_var, _, _)) = let_chain.last() {
                                    first_arg_is_var(value, *prev_var)
                                } else {
                                    true // First in chain
                                };
                                if is_chained {
                                    let_chain.push((*var, op, value));
                                    continue;
                                }
                            }
                        }
                        // Not a chain op — flush and reset
                        flush_let_chain(&let_chain, registry, chains);
                        let_chain.clear();
                        detect_pipe_chains_inner(value, registry, chains);
                    }
                    IrStmtKind::Expr { expr } => {
                        // Check if this expression continues the chain
                        let call_name = match &expr.kind {
                            IrExprKind::Call { target: CallTarget::Module { func, .. }, .. } => Some(func.as_str()),
                            IrExprKind::Call { target: CallTarget::Named { name }, .. } => Some(name.as_str()),
                            _ => None,
                        };
                        if let Some(name) = call_name {
                            if let Some(op) = classify_stdlib_op(name) {
                                if let Some((prev_var, _, _)) = let_chain.last() {
                                    if first_arg_is_var(expr, *prev_var) {
                                        let_chain.push((VarId(0), op, expr)); // VarId doesn't matter for last
                                        continue;
                                    }
                                }
                            }
                        }
                        flush_let_chain(&let_chain, registry, chains);
                        let_chain.clear();
                        detect_pipe_chains_inner(expr, registry, chains);
                    }
                    _ => {
                        flush_let_chain(&let_chain, registry, chains);
                        let_chain.clear();
                    }
                }
            }
            // Also check if body expr continues the chain
            if let Some(e) = body {
                let call_name = match &e.kind {
                    IrExprKind::Call { target: CallTarget::Module { func, .. }, .. } => Some(func.as_str()),
                    IrExprKind::Call { target: CallTarget::Named { name }, .. } => Some(name.as_str()),
                    _ => None,
                };
                let appended = if let Some(name) = call_name {
                    if let Some(op) = classify_stdlib_op(name) {
                        if let Some((prev_var, _, _)) = let_chain.last() {
                            if first_arg_is_var(e, *prev_var) {
                                let_chain.push((VarId(0), op, e));
                                true
                            } else { false }
                        } else { false }
                    } else { false }
                } else { false };
                flush_let_chain(&let_chain, registry, chains);
                if !appended {
                    detect_pipe_chains_inner(e, registry, chains);
                }
            } else {
                flush_let_chain(&let_chain, registry, chains);
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
