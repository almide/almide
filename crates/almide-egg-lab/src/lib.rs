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
    /// Three ledgers share the language:
    /// - **List combinators** — `map` / `filter` / `fold` + fusion
    ///   markers (`compose` / `and-pred`). Original feasibility PoC.
    /// - **Matrix atomic ops** — direct counterparts of the stdlib
    ///   `matrix.<op>` intrinsics. Op names use underscores here
    ///   (`matrix_mul`) because egg's S-expr tokenizer reads `.` as a
    ///   separator; the buildscript that translates `@rewrite` from
    ///   stdlib will perform the `matrix.mul` → `matrix_mul` rename.
    /// - **Matrix fused ops** — one variant per `@rewrite` RHS. Cost
    ///   function treats these as cheaper than the unfused chain so
    ///   equality saturation picks them.
    ///
    /// Atoms (`xs`, `identity`, capture names) use the default
    /// `Symbol` variant.
    pub enum AlmideExpr {
        // ── Numeric literal ───────────────────────────────────────
        Num(i64),

        // ── List combinators ──────────────────────────────────────
        "map" = Map([Id; 2]),
        "filter" = Filter([Id; 2]),
        "fold" = Fold([Id; 3]),

        "lam" = Lam([Id; 1]),
        "compose" = Compose([Id; 2]),
        "and-pred" = AndPred([Id; 2]),

        // ── Matrix atomic ops (LHS of fusion rules) ───────────────
        "matrix_mul" = MatrixMul([Id; 2]),
        "matrix_add" = MatrixAdd([Id; 2]),
        "matrix_scale" = MatrixScale([Id; 2]),
        "matrix_gelu" = MatrixGelu([Id; 1]),
        "matrix_softmax_rows" = MatrixSoftmaxRows([Id; 1]),
        "matrix_linear_row" = MatrixLinearRow([Id; 3]),
        "matrix_layer_norm_rows" = MatrixLayerNormRows([Id; 4]),

        // ── Matrix fused targets (RHS of fusion rules) ────────────
        "matrix_fused_gemm_bias_scale_gelu" = MatrixFusedGemmBiasScaleGelu([Id; 4]),
        "matrix_attention_weights" = MatrixAttentionWeights([Id; 3]),
        "matrix_scaled_dot_product_attention" = MatrixScaledDotProductAttention([Id; 4]),
        "matrix_pre_norm_linear" = MatrixPreNormLinear([Id; 6]),
        "matrix_linear_row_gelu" = MatrixLinearRowGelu([Id; 3]),
        "matrix_mul_scaled" = MatrixMulScaled([Id; 3]),

        // ── Default variant (atoms, identifiers) ──────────────────
        Symbol(Symbol),
    }
}

/// List fusion rules (the original feasibility PoC).
///
/// These three are the minimum needed to demonstrate that egg
/// subsumes the existing imperative `pass_stream_fusion`. Adding
/// more (`map_fold`, `flatmap_flatmap`, `filter_map_fold`, …) follows
/// the same shape.
pub fn list_fusion_rules() -> Vec<Rewrite<AlmideExpr, ()>> {
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

/// Matrix fusion rules — direct egg transcription of the seven
/// `@rewrite` attributes currently living in `stdlib/matrix.almd`.
/// When Stage 1's buildscript-to-egg emitter lands, this table will
/// be **generated** from those same attributes (one of the PR #215
/// skeleton's payoffs).
///
/// Names match the `name = "..."` arg on each stdlib attribute so
/// that future regression tooling can pair egg vs imperative firings
/// by rule name.
pub fn matrix_fusion_rules() -> Vec<Rewrite<AlmideExpr, ()>> {
    vec![
        rewrite!("gemm_bias_scale_gelu";
            "(matrix_gelu (matrix_scale (matrix_add (matrix_mul ?a ?b) ?bias) ?alpha))"
            => "(matrix_fused_gemm_bias_scale_gelu ?a ?b ?bias ?alpha)"),

        rewrite!("attention_weights";
            "(matrix_softmax_rows (matrix_scale (matrix_mul ?q ?kt) ?scale))"
            => "(matrix_attention_weights ?q ?kt ?scale)"),

        rewrite!("scaled_dot_product_attention";
            "(matrix_mul (matrix_attention_weights ?q ?kt ?scale) ?v)"
            => "(matrix_scaled_dot_product_attention ?q ?kt ?v ?scale)"),

        rewrite!("pre_norm_linear";
            "(matrix_linear_row (matrix_layer_norm_rows ?x ?gamma ?beta ?eps) ?w ?bias)"
            => "(matrix_pre_norm_linear ?x ?gamma ?beta ?eps ?w ?bias)"),

        rewrite!("linear_row_gelu";
            "(matrix_gelu (matrix_linear_row ?x ?w ?bias))"
            => "(matrix_linear_row_gelu ?x ?w ?bias)"),

        rewrite!("mul_scaled_rhs";
            "(matrix_mul ?a (matrix_scale ?b ?s))"
            => "(matrix_mul_scaled ?a ?s ?b)"),

        rewrite!("mul_scaled_lhs";
            "(matrix_mul (matrix_scale ?a ?s) ?b)"
            => "(matrix_mul_scaled ?a ?s ?b)"),
    ]
}

/// All rules usable by the saturator. Call sites that only need the
/// list fragment (existing bridge tests) can still use
/// `list_fusion_rules()` directly; callers that expect mixed
/// expressions use this union.
pub fn fusion_rules() -> Vec<Rewrite<AlmideExpr, ()>> {
    let mut rules = list_fusion_rules();
    rules.extend(matrix_fusion_rules());
    rules
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
        // Rough cost model:
        //   - List loop-bearing ops (map/filter/fold): 100 each — one
        //     traversal per node.
        //   - Matrix atomic ops: 200 each. Bigger than list loops
        //     because each stems from a BLAS call or a full matrix
        //     allocation; fusion avoiding even one of these is worth
        //     dozens of scalar ops.
        //   - Fused matrix ops: 110 each. One BLAS call + in-place
        //     post-pass. Cheaper than the ~4-op chain it replaces
        //     (the 4-op gemm_bias_scale_gelu chain = 4×200 = 800)
        //     so extraction always prefers the fused form.
        //   - Fusion markers (compose / and-pred / lam): 1. Bookkeeping.
        //   - Atoms (Num / Symbol): 1.
        let self_cost: u64 = match enode {
            AlmideExpr::Map(_) | AlmideExpr::Filter(_) | AlmideExpr::Fold(_) => 100,

            AlmideExpr::MatrixMul(_) | AlmideExpr::MatrixAdd(_)
            | AlmideExpr::MatrixScale(_) | AlmideExpr::MatrixGelu(_)
            | AlmideExpr::MatrixSoftmaxRows(_) | AlmideExpr::MatrixLinearRow(_)
            | AlmideExpr::MatrixLayerNormRows(_) => 200,

            AlmideExpr::MatrixFusedGemmBiasScaleGelu(_)
            | AlmideExpr::MatrixAttentionWeights(_)
            | AlmideExpr::MatrixScaledDotProductAttention(_)
            | AlmideExpr::MatrixPreNormLinear(_)
            | AlmideExpr::MatrixLinearRowGelu(_)
            | AlmideExpr::MatrixMulScaled(_) => 110,

            AlmideExpr::Compose(_) | AlmideExpr::AndPred(_) | AlmideExpr::Lam(_) => 1,
            AlmideExpr::Num(_) | AlmideExpr::Symbol(_) => 1,
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

    // ── Matrix fusion rules (Stage 1 skeleton) ────────────────────
    //
    // Each `@rewrite` attribute currently driving the imperative
    // `MatrixFusionPass` has a matching egg `rewrite!` in
    // `matrix_fusion_rules()`. The tests below prove saturation
    // reaches the fused form from the unfused chain for every rule,
    // which is the go-signal for migrating the pass to egg.

    #[test]
    fn matrix_gemm_bias_scale_gelu_fuses() {
        let (best, _) = optimize(
            "(matrix_gelu (matrix_scale (matrix_add (matrix_mul a b) bias) alpha))"
        );
        assert_eq!(
            best.to_string(),
            "(matrix_fused_gemm_bias_scale_gelu a b bias alpha)"
        );
    }

    #[test]
    fn matrix_attention_weights_fuses() {
        let (best, _) = optimize(
            "(matrix_softmax_rows (matrix_scale (matrix_mul q kt) scale))"
        );
        assert_eq!(
            best.to_string(),
            "(matrix_attention_weights q kt scale)"
        );
    }

    /// Scaled dot-product attention. The outer rule depends on the
    /// inner `attention_weights` fusion having already fired — this
    /// is where phase-order-free saturation shines: both rewrites
    /// live in the same e-graph and extraction picks the cheapest
    /// form without us sequencing them manually.
    #[test]
    fn matrix_sdpa_fuses_through_intermediate() {
        let (best, _) = optimize(
            "(matrix_mul \
                (matrix_softmax_rows (matrix_scale (matrix_mul q kt) scale)) \
                v)"
        );
        assert_eq!(
            best.to_string(),
            "(matrix_scaled_dot_product_attention q kt v scale)"
        );
    }

    #[test]
    fn matrix_pre_norm_linear_fuses() {
        let (best, _) = optimize(
            "(matrix_linear_row \
                (matrix_layer_norm_rows x gamma beta eps) \
                w bias)"
        );
        assert_eq!(
            best.to_string(),
            "(matrix_pre_norm_linear x gamma beta eps w bias)"
        );
    }

    #[test]
    fn matrix_linear_row_gelu_fuses() {
        let (best, _) = optimize(
            "(matrix_gelu (matrix_linear_row x w bias))"
        );
        assert_eq!(
            best.to_string(),
            "(matrix_linear_row_gelu x w bias)"
        );
    }

    #[test]
    fn matrix_mul_scaled_rhs_fuses() {
        let (best, _) = optimize(
            "(matrix_mul a (matrix_scale b s))"
        );
        assert_eq!(
            best.to_string(),
            "(matrix_mul_scaled a s b)"
        );
    }

    #[test]
    fn matrix_mul_scaled_lhs_fuses() {
        let (best, _) = optimize(
            "(matrix_mul (matrix_scale a s) b)"
        );
        assert_eq!(
            best.to_string(),
            "(matrix_mul_scaled a s b)"
        );
    }
}
