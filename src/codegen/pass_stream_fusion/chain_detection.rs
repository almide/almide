//! Pipe chain detection for debug analysis.

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

#[allow(dead_code)] // Used in tests; will be called from fusion pass once pipe chain detection is wired up
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
