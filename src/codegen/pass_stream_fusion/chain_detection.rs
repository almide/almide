//! Pipe chain detection for debug analysis.

use crate::ir::*;
use crate::types::constructor::{TypeConstructorRegistry, AlgebraicLaw};

#[derive(Debug)]
pub struct PipeChain {
    pub ops: Vec<PipeOp>,
    pub fusible_pairs: usize,
    pub container_name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PipeOp {
    Map, Filter, Fold, FlatMap, Other(String),
}

pub(super) fn unwrap_decorators(expr: &IrExpr) -> &IrExpr {
    match &expr.kind {
        IrExprKind::Borrow { expr: inner, .. }
        | IrExprKind::ToVec { expr: inner }
        | IrExprKind::Clone { expr: inner } => unwrap_decorators(inner),
        _ => expr,
    }
}

pub(super) fn detect_pipe_chains(expr: &IrExpr, registry: &TypeConstructorRegistry) -> Vec<PipeChain> {
    let mut chains = Vec::new();
    detect_pipe_chains_inner(expr, registry, &mut chains);
    chains
}

fn detect_pipe_chains_inner(
    expr: &IrExpr, registry: &TypeConstructorRegistry, chains: &mut Vec<PipeChain>,
) {
    match &expr.kind {
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
                        chain_ops.reverse();
                        let container_name = detect_container_type_from_call(expr);
                        let fusible_pairs = count_fusible_pairs(&chain_ops, &container_name, registry);
                        chains.push(PipeChain { ops: chain_ops, fusible_pairs, container_name });
                        return;
                    }
                }
            }
            for arg in args { detect_pipe_chains_inner(arg, registry, chains); }
        }
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
        IrExprKind::Lambda { body, .. } => detect_pipe_chains_inner(body, registry, chains),
        _ => {}
    }
}

fn extract_call_name(expr: &IrExpr) -> Option<&str> {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Module { func, .. }, .. } => Some(func.as_str()),
        IrExprKind::Call { target: CallTarget::Named { name }, .. } => Some(name.as_str()),
        _ => None,
    }
}

fn try_extend_chain<'a>(
    expr: &'a IrExpr, var: VarId, let_chain: &mut Vec<(VarId, PipeOp, &'a IrExpr)>,
) -> bool {
    let Some(name) = extract_call_name(expr) else { return false };
    let Some(op) = classify_stdlib_op(name) else { return false };
    let is_chained = let_chain.last()
        .map_or(true, |(prev_var, _, _)| first_arg_is_var(expr, *prev_var));
    if is_chained { let_chain.push((var, op, expr)); true } else { false }
}

fn first_arg_is_var(expr: &IrExpr, var: VarId) -> bool {
    if let IrExprKind::Call { args, .. } = &expr.kind {
        if let Some(first) = args.first() {
            if let IrExprKind::Var { id } = &unwrap_decorators(first).kind {
                return *id == var;
            }
        }
    }
    false
}

fn flush_let_chain(
    let_chain: &[(VarId, PipeOp, &IrExpr)],
    registry: &TypeConstructorRegistry, chains: &mut Vec<PipeChain>,
) {
    if let_chain.len() >= 2 {
        let ops: Vec<PipeOp> = let_chain.iter().map(|(_, op, _)| op.clone()).collect();
        let container_name = let_chain.first()
            .map(|(_, _, e)| detect_container_type_from_call(e))
            .unwrap_or(None);
        let fusible_pairs = count_fusible_pairs(&ops, &container_name, registry);
        chains.push(PipeChain { ops, fusible_pairs, container_name });
    }
}

pub(super) fn classify_stdlib_op(name: &str) -> Option<PipeOp> {
    if name.ends_with("flat_map") { return Some(PipeOp::FlatMap); }
    if name.ends_with("filter_map") { return None; }
    let func = name.rsplit('_').next().unwrap_or(name);
    match func {
        "map" => Some(PipeOp::Map),
        "filter" => Some(PipeOp::Filter),
        "fold" | "reduce" => Some(PipeOp::Fold),
        _ => None,
    }
}

pub(super) fn detect_container_type_from_call(expr: &IrExpr) -> Option<String> {
    if let IrExprKind::Call { args, .. } = &expr.kind {
        if let Some(first_arg) = args.first() {
            let unwrapped = unwrap_decorators(first_arg);
            if let Some(name) = unwrapped.ty.constructor_name() {
                if matches!(name, "List" | "Option" | "Result") {
                    return Some(name.to_string());
                }
            }
            return detect_container_type_from_call(unwrapped);
        }
    }
    expr.ty.constructor_name().map(|s| s.to_string())
}

pub(super) fn count_fusible_pairs(
    ops: &[PipeOp], container_name: &Option<String>, registry: &TypeConstructorRegistry,
) -> usize {
    let name = match container_name { Some(n) => n.as_str(), None => return 0 };
    ops.windows(2).filter(|pair| {
        match (&pair[0], &pair[1]) {
            (PipeOp::Map, PipeOp::Map) => registry.satisfies(name, AlgebraicLaw::FunctorComposition),
            (PipeOp::Filter, PipeOp::Filter) => registry.satisfies(name, AlgebraicLaw::FilterComposition),
            (PipeOp::Map, PipeOp::Fold) => registry.satisfies(name, AlgebraicLaw::MapFoldFusion),
            (PipeOp::Map, PipeOp::Filter) => registry.satisfies(name, AlgebraicLaw::MapFilterFusion),
            (PipeOp::FlatMap, PipeOp::FlatMap) => registry.satisfies(name, AlgebraicLaw::MonadAssociativity),
            _ => false,
        }
    }).count()
}
