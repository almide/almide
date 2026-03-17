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
    OptionErasurePass, Pipeline, ResultPropagationPass, Target, TypeConcretizationPass,
};
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
            // Local passes
            .add(ResultPropagationPass)
            // Shared passes
            .add(FanLoweringPass),

        Target::TypeScript => Pipeline::new()
            // Local passes
            .add(OptionErasurePass)
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
