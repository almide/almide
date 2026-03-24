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
//! - FilterMapFoldFusion: `fold(filter_map(x, fm), init, g)` → single-pass fold
//! - RangeFoldFusion: `fold(range(s, e), init, g)` → for loop (no allocation)

#[allow(dead_code)]
mod chain_detection;
mod fusion_rules;
mod ir_transform;
mod lambda_composition;

use crate::ir::*;
use super::pass::{NanoPass, PassResult, Target};

pub use chain_detection::{PipeChain, PipeOp};

use chain_detection::classify_stdlib_op;
use fusion_rules::*;
use ir_transform::recursive_transform;

// ── NanoPass entry point ──────────────────────────────────────────

#[derive(Debug)]
pub struct StreamFusionPass;

impl NanoPass for StreamFusionPass {
    fn name(&self) -> &str { "StreamFusion" }
    fn targets(&self) -> Option<Vec<Target>> { None }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Pre-pass: inline single-use collection lets to expose nested call patterns
        for func in &mut program.functions {
            inline_single_use_collection_lets(&mut func.body, &program.var_table);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                inline_single_use_collection_lets(&mut func.body, &module.var_table);
            }
        }

        // Phase 2: fuse chains using algebraic laws
        let mut totals = FusionCounts::default();
        for func in &mut program.functions {
            let c = fuse_all(func, &mut program.var_table);
            totals.add(&c);
        }

        PassResult { program, changed: totals.total() > 0 }
    }
}

// ── Fusion orchestrator ──────────────────────────────────────────

/// Run all fusion passes on a function.
fn fuse_all(func: &mut IrFunction, var_table: &mut VarTable) -> FusionCounts {
    let mut counts = FusionCounts::default();

    // FunctorIdentity: map(x, id) → x  (must run BEFORE map+map fusion)
    counts.identity = fuse_counting(&mut func.body, try_eliminate_identity_map);

    // FunctorComposition: map(map(x, f), g) → map(x, f >> g)
    counts.map_map = fuse_counting(&mut func.body, try_fuse_map_map);

    // FilterComposition: filter(filter(x, p), q) → filter(x, p && q)
    counts.filter_filter = fuse_counting(&mut func.body, try_fuse_filter_filter);

    // MapFoldFusion: fold(map(x, f), init, g) → fold(x, init, (acc, x) => g(acc, f(x)))
    counts.map_fold = fuse_counting(&mut func.body, try_fuse_map_fold);

    // MonadAssociativity: flat_map(flat_map(x, f), g) → flat_map(x, x => flat_map(f(x), g))
    counts.flatmap_flatmap = fuse_counting(&mut func.body, try_fuse_flatmap_flatmap);

    // MapFilterFusion: filter(map(x, f), p) → filter_map(x, ...)
    counts.map_filter = fuse_counting(&mut func.body, try_fuse_map_filter);

    // FilterMapFoldFusion: fold(filter_map(x, fm), init, g) → single-pass fold
    {
        let mut count = 0;
        func.body = recursive_transform(std::mem::take(&mut func.body), &mut |e| {
            try_fuse_filter_map_fold(e, &mut count, var_table)
        });
        counts.filter_map_fold = count;
    }

    // RangeFoldFusion: fold(range(start, end), init, g) → for loop
    {
        let mut count = 0;
        func.body = recursive_transform(std::mem::take(&mut func.body), &mut |e| {
            try_fuse_range_fold(e, &mut count, var_table)
        });
        counts.range_fold = count;
    }

    counts
}

/// Apply a fusion transform repeatedly (bottom-up) and count how many times it fired.
fn fuse_counting(
    body: &mut IrExpr,
    try_transform: fn(IrExpr) -> Option<IrExpr>,
) -> usize {
    let mut count = 0usize;
    *body = recursive_transform(std::mem::take(body), &mut |e| {
        if let Some(fused) = try_transform(e) {
            count += 1;
            Some(fused)
        } else {
            None
        }
    });
    count
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
    range_fold: usize,
}

impl FusionCounts {
    fn total(&self) -> usize {
        self.map_map + self.filter_filter + self.map_fold + self.identity
            + self.flatmap_flatmap + self.map_filter + self.filter_map_fold + self.range_fold
    }

    fn add(&mut self, other: &FusionCounts) {
        self.map_map += other.map_map;
        self.filter_filter += other.filter_filter;
        self.map_fold += other.map_fold;
        self.identity += other.identity;
        self.flatmap_flatmap += other.flatmap_flatmap;
        self.map_filter += other.map_filter;
        self.filter_map_fold += other.filter_map_fold;
        self.range_fold += other.range_fold;
    }

    #[allow(dead_code)]
    fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.identity > 0 { parts.push(format!("{} identity-map", self.identity)); }
        if self.map_map > 0 { parts.push(format!("{} map+map", self.map_map)); }
        if self.filter_filter > 0 { parts.push(format!("{} filter+filter", self.filter_filter)); }
        if self.map_fold > 0 { parts.push(format!("{} map+fold", self.map_fold)); }
        if self.flatmap_flatmap > 0 { parts.push(format!("{} flat_map+flat_map", self.flatmap_flatmap)); }
        if self.map_filter > 0 { parts.push(format!("{} map+filter", self.map_filter)); }
        if self.filter_map_fold > 0 { parts.push(format!("{} filter_map+fold", self.filter_map_fold)); }
        if self.range_fold > 0 { parts.push(format!("{} range+fold", self.range_fold)); }
        parts.join(", ")
    }
}

// ── Let-inlining pre-pass ────────────────────────────────────────

fn inline_single_use_collection_lets(body: &mut IrExpr, var_table: &VarTable) {
    if let IrExprKind::Block { stmts, expr } = &mut body.kind {
        let mut inlined_vars: std::collections::HashSet<VarId> = std::collections::HashSet::new();
        loop {
            let mut did_inline = false;
            for i in 0..stmts.len() {
                let (var, value) = match &stmts[i].kind {
                    IrStmtKind::Bind { var, value, .. } if !inlined_vars.contains(var) => (*var, value.clone()),
                    _ => continue,
                };
                let is_list_collection_op = matches!(&value.kind,
                    IrExprKind::Call { target: CallTarget::Module { module, func }, .. }
                    if module == "list" && (classify_stdlib_op(func).is_some() || func == "range")
                );
                if !is_list_collection_op || var_table.use_count(var) != 1 { continue; }
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
                break;
            }
            if !did_inline { break; }
        }
        stmts.retain(|s| {
            if let IrStmtKind::Bind { var, .. } = &s.kind { !inlined_vars.contains(var) } else { true }
        });
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

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::chain_detection::*;
    use crate::types::constructor::TypeConstructorRegistry;

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
        assert_eq!(count_fusible_pairs(&ops, &Some("Map".into()), &registry), 0);
    }

    #[test]
    fn count_fusible_option_flatmap() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::FlatMap, PipeOp::FlatMap];
        assert_eq!(count_fusible_pairs(&ops, &Some("Option".into()), &registry), 1);
    }
}
