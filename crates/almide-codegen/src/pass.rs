//! Nanopass framework for semantic rewriting.
//!
//! Each pass does ONE thing. Passes compose into a pipeline.
//! Target-specific passes are enabled/disabled per target.
//!
//! Inspired by:
//! - Nanopass framework (Indiana University, Chez Scheme)
//! - MLIR dialect conversion patterns
//! - NLLB-200 Mixture of Experts (shared + language-specific)

use almide_ir::IrProgram;
use almide_lang::types::Ty;

// ── Pass Result ──
// Returned by each pass: the transformed program + whether anything changed.

pub struct PassResult {
    pub program: IrProgram,
    pub changed: bool,
}

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

    /// Passes that must have executed before this one.
    /// Returns pass names (matching `NanoPass::name()`).
    /// Default: no dependencies.
    fn depends_on(&self) -> Vec<&'static str> { vec![] }

    /// Run the pass. Takes ownership of the program, returns modified program
    /// and whether any changes were made.
    fn run(&self, program: IrProgram, target: Target) -> PassResult;
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

    pub fn run(&self, program: IrProgram, target: Target) -> IrProgram {
        let mut program = program;
        let mut executed: Vec<&str> = Vec::new();
        for pass in &self.passes {
            // Skip passes not relevant to this target
            if let Some(targets) = pass.targets() {
                if !targets.contains(&target) {
                    continue;
                }
            }
            // Validate dependencies: every declared dep must have already executed
            for dep in pass.depends_on() {
                if !executed.contains(&dep) {
                    panic!(
                        "Pass '{}' depends on '{}', but '{}' has not been executed. \
                         Check pipeline ordering.",
                        pass.name(), dep, dep
                    );
                }
            }
            let result = pass.run(program, target);
            program = result.program;

            // Inter-pass IR verification (opt-in via ALMIDE_VERIFY_IR=1)
            if std::env::var("ALMIDE_VERIFY_IR").is_ok() {
                let errors = almide_ir::verify_program(&program);
                if !errors.is_empty() {
                    eprintln!("[IR VERIFY] {} error(s) after pass '{}':", errors.len(), pass.name());
                    for e in &errors {
                        eprintln!("  {}", e);
                    }
                    panic!("IR verification failed after pass '{}'", pass.name());
                }
            }

            executed.push(pass.name());
        }
        program
    }
}

// ── Built-in Passes ──
// Placeholder implementations. Each will be fleshed out during migration.

#[derive(Debug)]
pub struct OptionErasurePass;

impl NanoPass for OptionErasurePass {
    fn name(&self) -> &str { "OptionErasure" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::TypeScript, Target::Python])
    }
    fn run(&self, program: IrProgram, _target: Target) -> PassResult {
        // TS/Python: some(x) → x, none → null/None
        // Walks IR and sets `option_erased = true` on relevant nodes
        // TODO: implement during Phase 2 migration
        PassResult { program, changed: false }
    }
}

#[derive(Debug)]
pub struct ResultPropagationPass;

impl NanoPass for ResultPropagationPass {
    fn name(&self) -> &str { "ResultPropagation" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }
    fn run(&self, program: IrProgram, _target: Target) -> PassResult {
        // Rust: insert `?` on Result-returning calls inside effect fn
        // TODO: implement during Phase 2 migration
        PassResult { program, changed: false }
    }
}

#[derive(Debug)]
pub struct BorrowInsertionPass;

impl NanoPass for BorrowInsertionPass {
    fn name(&self) -> &str { "BorrowInsertion" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }
    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let sigs = super::pass_borrow_inference::infer_borrow_signatures(&mut program);
        let changed = !sigs.is_empty();
        if changed {
            super::pass_borrow_inference::insert_borrows_at_call_sites(&mut program, &sigs);
        }
        PassResult { program, changed }
    }
}

#[derive(Debug)]
pub struct CloneInsertionPass;

impl NanoPass for CloneInsertionPass {
    fn name(&self) -> &str { "CloneInsertion" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }
    fn run(&self, program: IrProgram, _target: Target) -> PassResult {
        // Rust: use-count analysis, insert .clone() where needed
        // TODO: implement during Phase 2 migration
        PassResult { program, changed: false }
    }
}

#[derive(Debug)]
pub struct FanLoweringPass;

impl NanoPass for FanLoweringPass {
    fn name(&self) -> &str { "FanLowering" }
    fn targets(&self) -> Option<Vec<Target>> {
        None // All targets need this
    }
    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        super::pass_fan_lowering::strip_fan_auto_try(&mut program);
        PassResult { program, changed: true }
    }
}

#[derive(Debug)]
pub struct TypeConcretizationPass;

impl NanoPass for TypeConcretizationPass {
    fn name(&self) -> &str { "TypeConcretization" }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Rust])
    }
    fn run(&self, program: IrProgram, _target: Target) -> PassResult {
        // Rust: Box recursive types, generate AnonRecord structs
        // TODO: implement during Phase 2 migration
        PassResult { program, changed: false }
    }
}

// ── Default Pipeline ──

#[derive(Debug)]
pub struct StreamFusionPass;
impl NanoPass for StreamFusionPass {
    fn name(&self) -> &str { "StreamFusion" }
    fn targets(&self) -> Option<Vec<Target>> { None }
    fn run(&self, program: IrProgram, _target: Target) -> PassResult {
        super::pass_stream_fusion::StreamFusionPass.run(program, _target)
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
