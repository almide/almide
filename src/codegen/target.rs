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
    BorrowInsertionPass, FanLoweringPass,
    OptionErasurePass, Pipeline, Target, TypeConcretizationPass,
};
use super::pass_clone::CloneInsertionPass;
use super::pass_builtin_lowering::BuiltinLoweringPass;
use super::pass_match_lowering::MatchLoweringPass;
use super::pass_result_erasure::ResultErasurePass;
use super::pass_result_propagation::ResultPropagationPass;
use super::pass_shadow_resolve::ShadowResolvePass;
use super::pass_stdlib_lowering::StdlibLoweringPass;
use super::pass_match_subject::MatchSubjectPass;
use super::pass_effect_inference::EffectInferencePass;
use super::pass_stream_fusion::StreamFusionPass;
use super::pass_tco::TailCallOptPass;
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
            // TCO: convert self-recursive tail calls to loops (before any lowering)
            .add(TailCallOptPass)
            // Global passes
            .add(TypeConcretizationPass)
            // Stream fusion BEFORE borrow/clone (decorators break pattern matching)
            .add(StreamFusionPass)
            .add(BorrowInsertionPass)
            .add(CloneInsertionPass)
            // Match subject transforms: String → .as_str(), Option<String> → .as_deref()
            .add(MatchSubjectPass)
            // Analysis passes (before lowering, while Module calls still visible)
            .add(EffectInferencePass)
            // Semantic lowering (order matters!)
            // 1. Stdlib first: Module calls → Named calls with arg decoration
            .add(StdlibLoweringPass)
            // 2. ResultPropagation: insert Try (?) for effect fn calls
            .add(ResultPropagationPass)
            // 3. Builtin last: Named calls (assert_eq, println, etc.) → RustMacro
            .add(BuiltinLoweringPass)
            // Shared passes
            .add(FanLoweringPass),

        Target::TypeScript | Target::JavaScript => Pipeline::new()
            // TCO: convert self-recursive tail calls to loops
            .add(TailCallOptPass)
            // Analysis passes
            .add(EffectInferencePass)
            .add(StreamFusionPass)
            // Semantic lowering
            .add(MatchLoweringPass)
            // Result/Option erasure: ok(x)→x, err(e)→throw, some(x)→x, none→null
            .add(ResultErasurePass)
            // Shadow resolution: let x = 1; let x = 2 → let x = 1; x = 2
            .add(ShadowResolvePass)
            // Shared passes
            .add(FanLoweringPass),

        Target::Go => Pipeline::new()
            .add(TailCallOptPass)
            // Go-specific passes will go here
            // .add(ResultToTuplePass)
            // .add(GoroutineLoweringPass)
            .add(FanLoweringPass),

        Target::Python => Pipeline::new()
            .add(TailCallOptPass)
            // Python-specific passes will go here
            .add(OptionErasurePass)
            // .add(ResultToExceptionPass)
            .add(FanLoweringPass),

        Target::Wasm => Pipeline::new()
            .add(TailCallOptPass)
            .add(EffectInferencePass)
            // StreamFusion not included: WASM emitter has its own lowering paths
            .add(ResultPropagationPass)
            .add(FanLoweringPass),
    }
}

fn build_templates(target: Target) -> TemplateSet {
    match target {
        Target::Rust => super::template::rust_templates(),
        Target::TypeScript => super::template::typescript_templates(),
        Target::JavaScript => super::template::javascript_templates(),
        Target::Go => TemplateSet::new("go"),
        Target::Python => TemplateSet::new("python"),
        Target::Wasm => TemplateSet::new("wasm"),
    }
}
