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
use super::pass_closure_conversion::ClosureConversionPass;
use super::pass_resolve_calls::ResolveCallsPass;
use super::pass_list_pattern::ListPatternLoweringPass;
use super::pass_tail_call_mark::TailCallMarkPass;
use super::pass_unify_var_tables::UnifyVarTablesPass;
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
                // string-folding lambdas. A second
                // `LambdaTypeResolvePass` invocation runs later for
                // `Ty::Fn` wrapper refresh; this first one is purely
                // about pre-rewrite information.
                .add(LambdaTypeResolvePass)
                .add(ConcretizeTypesPass)
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
        }

        Target::TypeScript => Pipeline::new(), // TS codegen removed — use --target wasm for JS runtimes

        Target::Go => Pipeline::new()
            .add(TailCallOptPass)
            .add(LICMPass)
            // Go-specific passes will go here
            // .add(ResultToTuplePass)
            // .add(GoroutineLoweringPass)
            .add(FanLoweringPass),

        Target::Python => Pipeline::new()
            .add(TailCallOptPass)
            .add(LICMPass)
            // Python-specific passes will go here when the target is activated:
            // .add(OptionErasurePass)      // some(x) → x, none → None
            // .add(ResultToExceptionPass)  // ok/err → try/except
            .add(FanLoweringPass),

        Target::Wasm => Pipeline::new()
            // Merge every `IrModule.var_table` into `program.var_table` up
            // front so downstream passes see a single unified table.
            .add(UnifyVarTablesPass)
            .add(ListPatternLoweringPass)
            // Lambda type resolution runs BEFORE IntrinsicLowering so that
            // `Call { target: Module { list, map } }` still carries the
            // stdlib call-site signature used to propagate list-elem types
            // into the lambda param. `IntrinsicLoweringPass` rewrites the
            // Call into a `RuntimeCall` that no longer exposes the (module,
            // fn) pair, which would leave closures with `List[TypeVar(A)]`
            // param types and break ConcretizeTypes / WASM emit downstream.
            .add(LambdaTypeResolvePass)
            // @intrinsic(symbol) → RuntimeCall. See Rust pipeline comment.
            .add(IntrinsicLoweringPass)
            // Verify all user-module calls resolve to known IrFunctions.
            // Runs early so violations surface before deep transformations.
            .add(ResolveCallsPass)
            .add(LICMPass)
            // Equality-saturation fusion (matrix + list combinators).
            .add(EggSaturationPass)
            // Clean up arithmetic on numeric literals (e.g. (kb * -1.0) → -kb)
            .add(ConstFoldPass)
            .add(EffectInferencePass)
            // StreamFusion not included: WASM emitter has its own lowering paths
            .add(ResultPropagationPass)
        // Peephole: swap/reverse/rotate/copy → specialized IR nodes
        .add(PeepholePass)
        // Concretize types: sync every IrExpr.ty with VarTable / parent context,
        // so downstream emit code can trust expr.ty.
        .add(ConcretizeTypesPass)
        // Closure conversion: lift lambdas to top-level functions with explicit env
        .add(ClosureConversionPass)
        // Re-concretize after closure conversion: lifted functions' bodies
        // carry their original expr.ty, but the SymbolTable now contains
        // lifted signatures too — running ConcretizeTypes again resolves
        // Call return types inside closures (e.g. map.get inside a lifted lambda).
        .add(ConcretizeTypesPass)
        .add(FanLoweringPass)
        // TailCallMark: mark tail-position calls for WASM return_call emission.
        // Must run last — after all passes that may create or transform calls.
        .add(TailCallMarkPass),
    }
}

fn build_templates(target: Target) -> TemplateSet {
    match target {
        Target::Rust => super::template::rust_templates(),
        Target::TypeScript => TemplateSet::new("typescript"),
        Target::Go => TemplateSet::new("go"),
        Target::Python => TemplateSet::new("python"),
        Target::Wasm => TemplateSet::new("wasm"),
    }
}
