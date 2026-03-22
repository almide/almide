//! Nanopass framework for semantic rewriting.
//!
//! Each pass does ONE thing. Passes compose into a pipeline.
//! Target-specific passes are enabled/disabled per target.
//!
//! Inspired by:
//! - Nanopass framework (Indiana University, Chez Scheme)
//! - MLIR dialect conversion patterns
//! - NLLB-200 Mixture of Experts (shared + language-specific)

use crate::ir::IrProgram;
use crate::types::Ty;

// ── Scope Context ──
// Tracks where we are in the program during IR traversal.

#[derive(Debug, Clone)]
pub struct ScopeContext {
    /// Does this function auto-unwrap Results (effect fn, not test)?
    pub auto_unwrap: bool,
    /// Are we inside a loop body?
    pub in_loop: bool,
    /// Are we at the top level (module scope)?
    pub is_top_level: bool,
    /// Type of the current match subject (if inside a match)
    pub match_subject_ty: Option<Ty>,
    /// Target we're generating for
    pub target: Target,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Rust,
    TypeScript,
    JavaScript,
    Go,
    Python,
    Wasm,
}

// ── Target Attributes ──
// Attached to IR nodes by passes. Template renderer reads these.
// Following CrossTL's attribute system + NMT interlingua insight:
// "language-specific info is a removable offset on top of shared meaning"

#[derive(Debug, Clone, Default)]
pub struct TargetAttrs {
    /// Rust: wrap in `?` for auto-propagation
    pub needs_try: bool,
    /// Rust: emit `.clone()` before use
    pub needs_clone: bool,
    /// Rust: emit `&` reference instead of value
    pub needs_borrow: bool,
    /// Rust: wrap type in `Box<T>` for recursive types
    pub needs_box: bool,
    /// Rust: emit `None::<T>` with explicit type (when inference fails)
    pub none_type_hint: Option<String>,
    /// Rust: emit `.as_str()` on match subject
    pub match_as_str: bool,
    /// Rust: top-level let → LazyLock
    pub lazy_init: bool,
    /// TS: Option erasure — `some(x)` becomes just `x`
    pub option_erased: bool,
    /// TS: Result wrapped in `{ ok, value/error }` object
    pub result_wrapped: bool,
}

// ── Nanopass Trait ──
// Each pass implements this trait. Passes are composable and independent.
// A pass receives the full program (for global analysis) but should
// only modify what it's responsible for.

pub trait NanoPass: std::fmt::Debug {
    /// Human-readable name for this pass (for logging/debugging)
    fn name(&self) -> &str;

    /// Which targets does this pass apply to?
    /// Return `None` for all targets, or `Some(vec)` for specific ones.
    fn targets(&self) -> Option<Vec<Target>>;

    /// Run the pass. Receives the program and target, returns modified program.
    /// Global passes analyze the whole program.
    /// Local passes walk the IR with scope context.
    fn run(&self, program: &mut IrProgram, target: Target);
}

// ── Pass Pipeline ──
// Ordered list of passes. Target-specific passes are skipped for other targets.

pub struct Pipeline {
    passes: Vec<Box<dyn NanoPass>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    pub fn add<P: NanoPass + 'static>(mut self, pass: P) -> Self {
        self.passes.push(Box::new(pass));
        self
    }

    pub fn run(&self, program: &mut IrProgram, target: Target) {
        for pass in &self.passes {
            // Skip passes not relevant to this target
            if let Some(targets) = pass.targets() {
                if !targets.contains(&target) {
                    continue;
                }
            }
            pass.run(program, target);
        }
    }
}

// ── Built-in Passes ──
// Placeholder implementations. Each will be fleshed out during migration.

#[derive(Debug)]
pub struct OptionErasurePass;

impl NanoPass for OptionErasurePass {
    fn name(&self) -> &str { "OptionErasure" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::TypeScript, Target::JavaScript, Target::Python])
    }
    fn run(&self, _program: &mut IrProgram, _target: Target) {
        // TS/Python: some(x) → x, none → null/None
        // Walks IR and sets `option_erased = true` on relevant nodes
        // TODO: implement during Phase 2 migration
    }
}

#[derive(Debug)]
pub struct ResultPropagationPass;

impl NanoPass for ResultPropagationPass {
    fn name(&self) -> &str { "ResultPropagation" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }
    fn run(&self, _program: &mut IrProgram, _target: Target) {
        // Rust: insert `?` on Result-returning calls inside effect fn
        // TODO: implement during Phase 2 migration
    }
}

#[derive(Debug)]
pub struct BorrowInsertionPass;

impl NanoPass for BorrowInsertionPass {
    fn name(&self) -> &str { "BorrowInsertion" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }
    fn run(&self, _program: &mut IrProgram, _target: Target) {
        // Rust: analyze parameter usage, mark &T vs T
        // TODO: implement during Phase 2 migration (move from borrow.rs)
    }
}

#[derive(Debug)]
pub struct CloneInsertionPass;

impl NanoPass for CloneInsertionPass {
    fn name(&self) -> &str { "CloneInsertion" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }
    fn run(&self, _program: &mut IrProgram, _target: Target) {
        // Rust: use-count analysis, insert .clone() where needed
        // TODO: implement during Phase 2 migration
    }
}

#[derive(Debug)]
pub struct FanLoweringPass;

impl NanoPass for FanLoweringPass {
    fn name(&self) -> &str { "FanLowering" }
    fn targets(&self) -> Option<Vec<Target>> {
        None // All targets need this
    }
    fn run(&self, program: &mut IrProgram, _target: Target) {
        super::pass_fan_lowering::strip_fan_auto_try(program);
    }
}

#[derive(Debug)]
pub struct TypeConcretizationPass;

impl NanoPass for TypeConcretizationPass {
    fn name(&self) -> &str { "TypeConcretization" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }
    fn run(&self, _program: &mut IrProgram, _target: Target) {
        // Rust: Box recursive types, generate AnonRecord structs
        // TODO: implement during Phase 2 migration
    }
}

// ── Default Pipeline ──

#[derive(Debug)]
pub struct StreamFusionPass;
impl NanoPass for StreamFusionPass {
    fn name(&self) -> &str { "StreamFusion" }
    fn targets(&self) -> Option<Vec<Target>> { None }
    fn run(&self, program: &mut IrProgram, _target: Target) {
        super::pass_stream_fusion::StreamFusionPass.run(program, _target);
    }
}

pub fn default_pipeline() -> Pipeline {
    Pipeline::new()
        // Global passes first (need whole-program analysis)
        .add(TypeConcretizationPass)
        .add(BorrowInsertionPass)
        .add(CloneInsertionPass)
        // Optimization passes (analysis only in Phase 1)
        .add(StreamFusionPass)
        // Local passes (scope-dependent)
        .add(OptionErasurePass)
        .add(ResultPropagationPass)
        // Target-specific lowering
        .add(FanLoweringPass)
}
