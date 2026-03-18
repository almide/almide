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
    self, BorrowInsertionPass, CloneInsertionPass, FanLoweringPass, NanoPass,
    OptionErasurePass, Pipeline, Target, TypeConcretizationPass,
};
use super::pass_builtin_lowering::BuiltinLoweringPass;
use super::pass_match_lowering::MatchLoweringPass;
use super::pass_result_erasure::ResultErasurePass;
use super::pass_result_propagation::ResultPropagationPass;
use super::pass_shadow_resolve::ShadowResolvePass;
use super::pass_stdlib_lowering::StdlibLoweringPass;
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
            // Global passes
            .add(TypeConcretizationPass)
            .add(BorrowInsertionPass)
            .add(CloneInsertionPass)
            // Semantic lowering (order matters!)
            // 1. Stdlib first: Module calls → Named calls with arg decoration
            .add(StdlibLoweringPass)
            // 2. ResultPropagation: insert Try (?) for effect fn calls
            .add(ResultPropagationPass)
            // 3. Builtin last: Named calls (assert_eq, println, etc.) → RustMacro
            .add(BuiltinLoweringPass)
            // Shared passes
            .add(FanLoweringPass),

        Target::TypeScript => Pipeline::new()
            // Semantic lowering
            .add(MatchLoweringPass)
            // Result/Option erasure: ok(x)→x, err(e)→throw, some(x)→x, none→null
            .add(ResultErasurePass)
            // Shadow resolution: let x = 1; let x = 2 → let x = 1; x = 2
            .add(ShadowResolvePass)
            // Shared passes
            .add(FanLoweringPass),

        Target::Go => Pipeline::new()
            // Go-specific passes will go here
            // .add(ResultToTuplePass)
            // .add(GoroutineLoweringPass)
            .add(FanLoweringPass),

        Target::Python => Pipeline::new()
            // Python-specific passes will go here
            .add(OptionErasurePass)
            // .add(ResultToExceptionPass)
            .add(FanLoweringPass),
    }
}

fn build_templates(target: Target) -> TemplateSet {
    match target {
        Target::Rust => super::template::rust_templates(),
        Target::TypeScript => super::template::typescript_templates(),
        // TODO: load from TOML files once template loader is implemented
        Target::Go => TemplateSet::new("go"),
        Target::Python => TemplateSet::new("python"),
    }
}
