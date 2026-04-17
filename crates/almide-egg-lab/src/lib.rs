//! almide-egg-lab — Feasibility PoC for egg (equality saturation) on a
//! minimal Almide IR subset.
//!
//! **Goal**: validate that egg can express and apply Almide's existing
//! stream-fusion rewrite rules before we commit to the 10-month
//! `mlir-backend-adoption` arc. This crate is intentionally isolated
//! from the main compiler pipeline — if the experiment fails, we
//! delete this crate with no blast radius.
//!
//! ## What this PoC proves / disproves
//!
//! - [x] `egg::define_language!` can model Almide-ish expressions
//! - [x] Fusion rules port cleanly from imperative passes
//! - [x] A custom cost function picks the fused form
//! - [x] Equality saturation terminates inside a small iteration budget
//!
//! ## What this PoC intentionally omits
//!
//! - Real lambda substitution (we use `compose` / `and-pred` pseudo-ops
//!   as markers; a later lowering step would beta-reduce them into
//!   honest lambdas). egg supports substitution via custom `Applier`
//!   impls, but that's Stage-1 work — not feasibility.
//! - Full Almide IR coverage (we model only list combinators + lambda
//!   shape needed for the 3 target rules).
//! - Integration with `almide-ir::IrExpr` (the real lowering will
//!   happen in Stage 1 if this PoC passes).

use egg::*;

pub mod bridge;
pub use bridge::{Bridge, LowerError};

define_language! {
    /// Minimal Almide IR fragment for fusion experiments.
    ///
    /// `map`, `filter`, `fold` are modeled as loop-bearing ops (high
    /// cost). `compose` / `and-pred` are zero-cost markers that stand
    /// in for "the optimizer has fused two stages into one body"
    /// without actually rebuilding the lambda.
    ///
    /// Atoms (`xs`, `identity`, variable names) are represented as
    /// the default `Symbol` variant. Named lambda references use the
    /// unary `(lam f)` form, where `f` is itself a `Symbol` node.
    pub enum AlmideExpr {
        // ── Numeric literal ───────────────────────────────────────
        Num(i64),

        // ── List combinators (each represents one traversal) ──────
        "map" = Map([Id; 2]),        // (map xs f)
        "filter" = Filter([Id; 2]),  // (filter xs p)
        "fold" = Fold([Id; 3]),      // (fold xs init f)

        // ── Lambda reference ──────────────────────────────────────
        // `(lam f)` — opaque reference to a lambda named `f`. In a
        // real lowering these would be IrExpr::Lambda nodes; the PoC
        // treats them as atoms keyed by their Symbol payload.
        "lam" = Lam([Id; 1]),

        // ── Fusion markers (zero-cost pseudo-ops) ─────────────────
        // `(compose g f)` means `λx. g(f(x))`. A later pass would
        // replace this with a real lambda IR node.
        "compose" = Compose([Id; 2]),
        // `(and-pred p q)` means `λx. p(x) and q(x)`.
        "and-pred" = AndPred([Id; 2]),

        // ── Default variant ───────────────────────────────────────
        // Any bare atom: variable name (`xs`), lambda marker
        // (`identity`), user-supplied fn symbol (`f`, `g`, `p`).
        Symbol(Symbol),
    }
}

/// Rewrite rules mirroring the behavior of the imperative
/// `pass_stream_fusion` in `almide-codegen`.
///
/// These three are the minimum needed to demonstrate that egg
/// subsumes the existing fusion logic. Adding more (`map_fold`,
/// `flatmap_flatmap`, `filter_map_fold`, …) follows the same shape.
pub fn fusion_rules() -> Vec<Rewrite<AlmideExpr, ()>> {
    vec![
        // FunctorIdentity: map(xs, (x => x)) ≡ xs
        rewrite!("identity-map"; "(map ?xs identity)" => "?xs"),

        // Functor law: map(map(xs, f), g) ≡ map(xs, g ∘ f)
        rewrite!("map-map-fuse";
            "(map (map ?xs ?f) ?g)"
            => "(map ?xs (compose ?g ?f))"),

        // Predicate conjunction: filter(filter(xs, p), q) ≡ filter(xs, p ∧ q)
        rewrite!("filter-filter-fuse";
            "(filter (filter ?xs ?p) ?q)"
            => "(filter ?xs (and-pred ?p ?q))"),
    ]
}

/// Cost function that reflects Almide's real target preference:
/// each list-traversing op (`map` / `filter` / `fold`) pays a loop
/// penalty, while fusion markers (`compose`, `and-pred`) are free.
///
/// The penalty picks "one loop" over "two loops" even when the node
/// count is equal, which is what the StreamFusion imperative pass
/// does implicitly.
pub struct FusionCost;

impl CostFunction<AlmideExpr> for FusionCost {
    type Cost = u64;

    fn cost<C>(&mut self, enode: &AlmideExpr, mut costs: C) -> u64
    where
        C: FnMut(Id) -> u64,
    {
        let self_cost: u64 = match enode {
            AlmideExpr::Map(_) | AlmideExpr::Filter(_) | AlmideExpr::Fold(_) => 100,
            AlmideExpr::Compose(_) | AlmideExpr::AndPred(_) => 1,
            _ => 1,
        };
        enode.fold(self_cost, |acc, id| acc.saturating_add(costs(id)))
    }
}

/// Run equality saturation on `input` and extract the optimal form
/// under `FusionCost`. Returns the best expression plus the number of
/// iterations used.
pub fn optimize(input: &str) -> (RecExpr<AlmideExpr>, usize) {
    let expr: RecExpr<AlmideExpr> = input.parse().expect("parse AlmideExpr");
    let runner = Runner::default()
        .with_iter_limit(64)
        .with_node_limit(10_000)
        .with_expr(&expr)
        .run(&fusion_rules());
    let root = runner.roots[0];
    let extractor = Extractor::new(&runner.egraph, FusionCost);
    let (_cost, best) = extractor.find_best(root);
    (best, runner.iterations.len())
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_rewrite(input: &str, expected: &str) {
        let (best, iters) = optimize(input);
        let best_str = best.to_string();
        let expected_expr: RecExpr<AlmideExpr> = expected.parse().expect("parse expected");
        assert_eq!(
            best_str,
            expected_expr.to_string(),
            "\n  input:    {}\n  expected: {}\n  got:      {}\n  iters:    {}",
            input, expected, best_str, iters,
        );
        // Feasibility sanity: saturation should converge quickly for
        // these tiny fragments. If this ever explodes we want to know.
        assert!(iters < 32, "unexpectedly many iterations: {}", iters);
    }

    #[test]
    fn identity_map_collapses() {
        expect_rewrite("(map xs identity)", "xs");
    }

    #[test]
    fn map_map_fuses() {
        expect_rewrite(
            "(map (map xs (lam f)) (lam g))",
            "(map xs (compose (lam g) (lam f)))",
        );
    }

    #[test]
    fn filter_filter_fuses() {
        expect_rewrite(
            "(filter (filter xs (lam p)) (lam q))",
            "(filter xs (and-pred (lam p) (lam q)))",
        );
    }

    /// Transitive fusion: three chained maps should fold into one
    /// with a nested compose marker. This is the property that makes
    /// egg interesting — imperative fusion needs an explicit loop to
    /// converge, equality saturation handles it by fixpoint.
    #[test]
    fn triple_map_fuses_transitively() {
        let (best, _) = optimize("(map (map (map xs (lam f)) (lam g)) (lam h))");
        let best_str = best.to_string();
        // We don't care about the exact parenthesization of the
        // compose chain, only that the outer form is a single map
        // and both inner lambdas appear inside a compose.
        assert!(
            best_str.starts_with("(map xs "),
            "expected single outer map, got: {}",
            best_str,
        );
        assert!(
            best_str.contains("compose"),
            "expected compose marker, got: {}",
            best_str,
        );
    }

    /// Identity interacts with fusion: map(map(xs, id), f) should
    /// collapse to map(xs, f) by way of (identity-map ∘ nothing).
    /// This checks that equality saturation composes rules without
    /// manual ordering — the feasibility proof for phase-order-free
    /// optimization.
    #[test]
    fn identity_inside_map_chain_eliminates() {
        let (best, _) = optimize("(map (map xs identity) (lam f))");
        assert_eq!(best.to_string(), "(map xs (lam f))");
    }
}
