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
        let registry = TypeConstructorRegistry::new();
        for func in &program.functions {
            let chains = detect_pipe_chains(&func.body, &registry);
            for chain in &chains {
                if chain.fusible_pairs > 0 {
                    // Phase 1: detection only. In Phase 2, we'll rewrite.
                    // For now, annotate the program with optimization opportunities.
                    eprintln!(
                        "[StreamFusion] {}: {} ops, {} fusible pairs",
                        func.name, chain.ops.len(), chain.fusible_pairs
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
        // or as Named calls with the result of another Named call as first arg
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
            if let Some(op) = classify_stdlib_op(name) {
                // Check if the first argument is also a pipe-chain operation
                let mut chain_ops = vec![op];
                let mut current = args.first();

                while let Some(arg) = current {
                    if let IrExprKind::Call {
                        target: CallTarget::Named { name: inner_name },
                        args: inner_args,
                        ..
                    } = &arg.kind
                    {
                        if let Some(inner_op) = classify_stdlib_op(inner_name) {
                            chain_ops.push(inner_op);
                            current = inner_args.first();
                            continue;
                        }
                    }
                    break;
                }

                if chain_ops.len() >= 2 {
                    chain_ops.reverse(); // From inner to outer
                    let container_name = detect_container_type(&expr.ty);
                    let fusible_pairs = count_fusible_pairs(&chain_ops, &container_name, registry);
                    chains.push(PipeChain {
                        ops: chain_ops,
                        fusible_pairs,
                        container_name,
                    });
                    return; // Don't recurse into this chain's sub-expressions
                }
            }

            // Recurse into args
            for arg in args {
                detect_pipe_chains_inner(arg, registry, chains);
            }
        }

        // Recurse into all sub-expressions
        IrExprKind::Block { stmts, expr: body } => {
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } => detect_pipe_chains_inner(value, registry, chains),
                    IrStmtKind::Assign { value, .. } => detect_pipe_chains_inner(value, registry, chains),
                    IrStmtKind::Expr { expr } => detect_pipe_chains_inner(expr, registry, chains),
                    IrStmtKind::Guard { cond, else_ } => {
                        detect_pipe_chains_inner(cond, registry, chains);
                        detect_pipe_chains_inner(else_, registry, chains);
                    }
                    IrStmtKind::BindDestructure { value, .. } => detect_pipe_chains_inner(value, registry, chains),
                    _ => {}
                }
            }
            if let Some(e) = body {
                detect_pipe_chains_inner(e, registry, chains);
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

/// Classify a runtime function name as a pipe operation.
fn classify_stdlib_op(name: &str) -> Option<PipeOp> {
    if name.contains("_map") && !name.contains("flat_map") {
        Some(PipeOp::Map)
    } else if name.contains("_filter") {
        Some(PipeOp::Filter)
    } else if name.contains("_fold") || name.contains("_reduce") {
        Some(PipeOp::Fold)
    } else if name.contains("_flat_map") {
        Some(PipeOp::FlatMap)
    } else {
        None
    }
}

/// Detect the container type from the expression's type.
fn detect_container_type(ty: &Ty) -> Option<String> {
    ty.constructor_name().map(|s| s.to_string())
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
