//! Target configuration: defines which passes and templates each target uses.
//!
//! Adding a new target = implement this module:
//! 1. Create templates in codegen/templates/<target>.toml
//! 2. Select which Nanopass passes to enable
//! 3. Implement any target-specific passes
//!
//! Target addition cost (estimated):
//! - GC language (Python, TS): ~500 LOC (no borrow/clone passes)
//! - Ownership language (Rust, Go): ~800 LOC (need borrow analysis)

use super::pass::{
    BorrowInsertionPass, FanLoweringPass, Pipeline, Target,
};
use super::pass_auto_parallel::AutoParallelPass;
use super::pass_box_deref::BoxDerefPass;
use super::pass_capture_clone::CaptureClonePass;
use super::pass_clone::CloneInsertionPass;
use super::pass_builtin_lowering::BuiltinLoweringPass;
use super::pass_result_propagation::ResultPropagationPass;
use super::pass_intrinsic_lowering::IntrinsicLoweringPass;
use super::pass_normalize_runtime_calls::NormalizeRuntimeCallsPass;
use super::pass_stdlib_lowering::StdlibLoweringPass;
use super::pass_match_subject::MatchSubjectPass;
use super::pass_pattern_literal_guard::PatternLiteralGuardPass;
use super::pass_effect_inference::EffectInferencePass;
use super::pass_tco::TailCallOptPass;
use super::pass_licm::LICMPass;
use super::pass_peephole::PeepholePass;
use super::pass_egg_saturation::EggSaturationPass;
use super::pass_matrix_shape_spec::MatrixShapeSpecPass;
use super::pass_const_fold::ConstFoldPass;
use super::pass_rust_lowering::RustLoweringPass;
use super::pass_lambda_type_resolve::LambdaTypeResolvePass;
use super::pass_concretize_types::ConcretizeTypesPass;
use super::pass_resolve_calls::ResolveCallsPass;
use super::pass_list_pattern::ListPatternLoweringPass;
use super::pass_unify_var_tables::UnifyVarTablesPass;
use super::pass_top_let_storage::TopLetStoragePass;
use super::pass_ir_link_flatten::IrLinkFlattenPass;
use super::template::TemplateSet;

/// Full configuration for a codegen target.
pub struct TargetConfig {
    pub target: Target,
    pub pipeline: Pipeline,
    pub templates: TemplateSet,
}

/// Build the pipeline and templates for a target.
pub fn configure(target: Target) -> TargetConfig {
    let pipeline = build_pipeline(target);
    let templates = build_templates(target);
    TargetConfig {
        target,
        pipeline,
        templates,
    }
}

fn build_pipeline(target: Target) -> Pipeline {
    // Stage 1 egg flip landed: `EggSaturationPass` is the sole
    // fusion driver for both matrix and list combinator chains.
    // The imperative `MatrixFusionPass` and `StreamFusionPass`
    // have been retired. The `fma / fma3` legacy optimisations
    // the imperative matrix pass also handled are not yet ported
    // to egg; they were performance-only (no spec depends on
    // them) and are earmarked for Stage 4's profile-guided cost
    // function.
    match target {
        Target::Rust => {
            Pipeline::new()
                // Merge every `IrModule.var_table` into `program.var_table` up
                // front so downstream passes see a single unified table.
                // See `active/var-table-unification.md`.
                .add(UnifyVarTablesPass)
                // ListPatternLowering: desugar list patterns to if/else before any other pass
                .add(ListPatternLoweringPass)
                // LambdaTypeResolve runs FIRST (before ResolveCalls /
                // IntrinsicLowering) so that `CallTarget::Module { list, fold }`
                // is still present. Both `ResolveCalls` (bundled stdlib →
                // `Named { almide_rt_... }`) and `IntrinsicLowering` (→
                // `RuntimeCall`) rewrite the target; after either, the
                // `resolve_call_lambdas` helper can't recognise the
                // stdlib method any more, which leaves closure params
                // as `TypeVar` and breaks `MatchSubject` on
                // string-folding lambdas. (#559: this is the SOLE
                // LambdaTypeResolvePass in the Rust pipeline — the stale
                // "a second invocation runs later" note was removed; the
                // Wgsl/Wasm arms have their own single invocation.)
                .add(LambdaTypeResolvePass)
                .add(ConcretizeTypesPass)
                // Hoist payload-nested string literals (`ok("x")`, `Word("hi")`)
                // into Bind + `==` guards BEFORE MatchSubject so the as_deref /
                // &* subject deref only ever sees top-level / already-handled
                // literals — one reconciliation path per literal. Runs after
                // ConcretizeTypes so the literal/payload type is resolved.
                .add(PatternLiteralGuardPass)
                // Verify all user-module calls resolve to known IrFunctions.
                .add(ResolveCallsPass)
                // BoxDeref: insert Deref IR nodes for Box'd pattern vars (before CloneInsertion)
                .add(BoxDerefPass)
                // LICM: hoist loop-invariant expressions before loops
                .add(LICMPass)
                // Equality-saturation fusion: matrix + list combinators.
                // Single driver, lifted from `stdlib/matrix.almd` + list rules.
                .add(EggSaturationPass)
                // MatrixShapeSpec: unroll small-shape matmuls inline before
                // the stdlib lowering pass turns them into `InlineRust`
                // blobs that no longer carry structural info.
                .add(MatrixShapeSpecPass)
                // Clean up arithmetic on numeric literals (e.g. (kb * -1.0) → -kb)
                .add(ConstFoldPass)
                // @intrinsic(symbol) → RuntimeCall must run BEFORE
                // BorrowInsertion so the subsequent pass can look up the
                // borrow signature by the mangled runtime symbol
                // (`almide_rt_<m>_<f>`) and wrap args with the right
                // Borrow IR node. BorrowInsertion's signature table is
                // seeded from bundled `@intrinsic` declarations at
                // `infer_borrow_signatures` entry.
                .add(IntrinsicLoweringPass)
        .add(BorrowInsertionPass)
        // TCO: convert self-recursive tail calls to loops AFTER BorrowInsertion
        // (so that param types are already finalized — avoids String/&str mismatch)
        .add(TailCallOptPass)
        .add(CaptureClonePass)
        .add(CloneInsertionPass)
        // Match subject transforms: String → .as_str(), Option<String> → .as_deref()
        .add(MatchSubjectPass)
        // Analysis passes (before lowering, while Module calls still visible)
        .add(EffectInferencePass)
        // Semantic lowering (order matters!)
        // 1. Stdlib first: Module calls → Named calls with arg decoration
        .add(StdlibLoweringPass)
        // 2. AutoParallel: rewrite pure list ops to parallel variants
        .add(AutoParallelPass)
        // 3. ResultPropagation: insert Try (?) for effect fn calls
        .add(ResultPropagationPass)
        // 3. Builtin last: Named calls (assert_eq, println, etc.) → RustMacro
        .add(BuiltinLoweringPass)
        // Peephole: swap/reverse/rotate/copy → specialized IR nodes
                .add(PeepholePass)
                // Rust-specific: push optimization, borrow index lift
                .add(RustLoweringPass)
                // Shared passes
                .add(FanLoweringPass)
                // Final normalization: collapse legacy `Named { "almide_rt_*" }`
                // into `RuntimeCall { symbol }`. Establishes the walker
                // invariant `Named.name does NOT start with "almide_rt_"`.
                .add(NormalizeRuntimeCallsPass)
                // Final: flatten modules into root (after UnifyVarTables)
                .add(IrLinkFlattenPass)
                // §4 Stage 1: compute the unified top-let storage attribute
                // at pipeline end (VarIds final, modules flattened); the
                // walker asserts every legacy predicate agrees with it.
                .add(TopLetStoragePass)
        }

        Target::Wgsl => Pipeline::new()
            .add(UnifyVarTablesPass)
            .add(LambdaTypeResolvePass)
            .add(ConcretizeTypesPass)
            .add(FanLoweringPass),
    }
}

fn build_templates(target: Target) -> TemplateSet {
    match target {
        Target::Rust => super::template::rust_templates(),
        Target::Wgsl => TemplateSet::new("wgsl"),
    }
}
