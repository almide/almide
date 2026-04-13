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
use super::pass_stdlib_lowering::StdlibLoweringPass;
use super::pass_match_subject::MatchSubjectPass;
use super::pass_effect_inference::EffectInferencePass;
use super::pass_stream_fusion::StreamFusionPass;
use super::pass_tco::TailCallOptPass;
use super::pass_licm::LICMPass;
use super::pass_peephole::PeepholePass;
use super::pass_rust_lowering::RustLoweringPass;
use super::pass_lambda_type_resolve::LambdaTypeResolvePass;
use super::pass_concretize_types::ConcretizeTypesPass;
use super::pass_closure_conversion::ClosureConversionPass;
use super::pass_resolve_calls::ResolveCallsPass;
use super::pass_list_pattern::ListPatternLoweringPass;
use super::pass_tail_call_mark::TailCallMarkPass;
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
    match target {
        Target::Rust => Pipeline::new()
            // ListPatternLowering: desugar list patterns to if/else before any other pass
            .add(ListPatternLoweringPass)
            // Verify all user-module calls resolve to known IrFunctions.
            .add(ResolveCallsPass)
            // BoxDeref: insert Deref IR nodes for Box'd pattern vars (before CloneInsertion)
            .add(BoxDerefPass)
            // LICM: hoist loop-invariant expressions before loops
            .add(LICMPass)
            // Stream fusion BEFORE borrow/clone (decorators break pattern matching)
            .add(StreamFusionPass)
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
            .add(FanLoweringPass),

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
            .add(ListPatternLoweringPass)
            // Verify all user-module calls resolve to known IrFunctions.
            // Runs early so violations surface before deep transformations.
            .add(ResolveCallsPass)
            .add(LICMPass)
            .add(EffectInferencePass)
            // StreamFusion not included: WASM emitter has its own lowering paths
            .add(ResultPropagationPass)
            // Peephole: swap/reverse/rotate/copy → specialized IR nodes
            .add(PeepholePass)
            // Lambda type resolution: top-down propagation of lambda param types
            .add(LambdaTypeResolvePass)
            // Concretize types: sync every IrExpr.ty with VarTable / parent context,
            // so downstream emit code can trust expr.ty.
            .add(ConcretizeTypesPass)
            // Closure conversion: lift lambdas to top-level functions with explicit env
            .add(ClosureConversionPass)
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
