//! Nanopass framework for semantic rewriting.
//!
//! Each pass does ONE thing. Passes compose into a pipeline.
//! Target-specific passes are enabled/disabled per target.
//!
//! Inspired by:
//! - Nanopass framework (Indiana University, Chez Scheme)
//! - MLIR dialect conversion patterns
//! - NLLB-200 Mixture of Experts (shared + language-specific)

use almide_ir::{IrProgram, IrExprKind, IrPattern, IrVisitor};
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
    Wasm,
    Wgsl,
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

    /// Passes that must have executed before this one (after-deps).
    /// Returns pass names (matching `NanoPass::name()`).
    /// TARGET-CONDITIONAL: a dep is only enforced when that dep is itself in
    /// THIS target's pipeline — so a wasm-arm pass may depend on a Rust-only
    /// pass without panicking (the dep is simply absent and the edge is
    /// vacuous). Default: no dependencies.
    fn depends_on(&self) -> Vec<&'static str> { vec![] }

    /// Passes that must run AFTER this one (before-deps) — the converse of
    /// `depends_on`, which the after-only mechanism could not express (#559).
    /// Enforced TARGET-CONDITIONALLY: when a named successor runs, this pass
    /// must already have executed. Use for "X must run before Y" invariants
    /// (e.g. StackBalance before Perceus) where Y does not, or should not,
    /// declare the reverse dependency. Default: none.
    fn run_before(&self) -> Vec<&'static str> { vec![] }

    /// Postconditions: structural invariants guaranteed after this pass runs.
    /// Verified on every build. Debug builds panic on violation; release
    /// builds print a `[POSTCONDITION VIOLATION]` diagnostic and keep
    /// running. Violations are compiler bugs — downstream passes may rely
    /// on the invariants unconditionally.
    fn postconditions(&self) -> Vec<Postcondition> { vec![] }

    /// Run the pass. Takes ownership of the program, returns modified program
    /// and whether any changes were made.
    fn run(&self, program: IrProgram, target: Target) -> PassResult;
}

/// Structural invariants a pass guarantees after execution.
#[derive(Debug)]
pub enum Postcondition {
    /// No IR pattern of this kind remains (e.g., "List" after ListPatternLowering)
    NoPatternKind(&'static str),
    /// No TypeVar remains in any function signature or body type
    NoTypeVars,
    /// All Ty nodes are concrete (no Unknown, no TypeVar)
    AllTypesConcrete,
    /// Custom check: returns list of violation messages (empty = OK)
    Custom(fn(&IrProgram) -> Vec<String>),
}

/// Verify postconditions for a pass. Returns list of violations.
pub fn verify_postconditions(pass_name: &str, program: &IrProgram, postconditions: &[Postcondition]) -> Vec<String> {
    use almide_lang::types::Ty;
    let mut violations = Vec::new();

    for pc in postconditions {
        match pc {
            Postcondition::NoPatternKind(kind) => {
                let count = count_pattern_kind(program, kind);
                if count > 0 {
                    violations.push(format!(
                        "[{}] {} '{}' pattern(s) remain after pass (expected 0)",
                        pass_name, count, kind
                    ));
                }
            }
            Postcondition::NoTypeVars => {
                let count = count_typevars_in_functions(program);
                if count > 0 {
                    violations.push(format!(
                        "[{}] {} TypeVar(s) remain in active functions (expected 0)",
                        pass_name, count
                    ));
                }
            }
            Postcondition::AllTypesConcrete => {
                let (unknowns, typevars) = count_incomplete_types(program);
                if unknowns > 0 {
                    violations.push(format!(
                        "[{}] {} Unknown type(s) in IR (expected 0)",
                        pass_name, unknowns
                    ));
                }
                if typevars > 0 {
                    violations.push(format!(
                        "[{}] {} TypeVar(s) in IR (expected 0)",
                        pass_name, typevars
                    ));
                }
            }
            Postcondition::Custom(check) => {
                violations.extend(check(program));
            }
        }
    }
    violations
}

fn count_pattern_kind(program: &IrProgram, kind: &str) -> usize {
    use almide_ir::*;
    struct PatternCounter { kind: String, count: usize }
    impl IrVisitor for PatternCounter {
        fn visit_pattern(&mut self, pat: &IrPattern) {
            let matches = match (&pat, self.kind.as_str()) {
                (IrPattern::List { .. }, "List") => true,
                _ => false,
            };
            if matches { self.count += 1; }
            almide_ir::walk_pattern(self, pat);
        }
    }
    let mut counter = PatternCounter { kind: kind.to_string(), count: 0 };
    for func in &program.functions { counter.visit_expr(&func.body); }
    counter.count
}

fn count_typevars_in_functions(program: &IrProgram) -> usize {
    use almide_lang::types::Ty;
    fn has_typevar(ty: &Ty) -> bool {
        match ty {
            Ty::TypeVar(_) => true,
            _ => ty.children().iter().any(|c| has_typevar(c)),
        }
    }
    let mut count = 0;
    for func in &program.functions {
        if has_typevar(&func.ret_ty) { count += 1; }
        for p in &func.params { if has_typevar(&p.ty) { count += 1; } }
    }
    count
}

fn count_incomplete_types(program: &IrProgram) -> (usize, usize) {
    use almide_lang::types::Ty;
    let mut unknowns = 0;
    let mut typevars = 0;
    for func in &program.functions {
        if func.ret_ty.contains_unknown() { unknowns += 1; }
        if func.ret_ty.contains_typevar() { typevars += 1; }
        for p in &func.params {
            if p.ty.contains_unknown() { unknowns += 1; }
            if p.ty.contains_typevar() { typevars += 1; }
        }
    }
    (unknowns, typevars)
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

    // ── `Pipeline::run` main-loop-body extraction (cog>100 decomposition,
    // pattern 2) ──
    //
    // Each of these is a 1:1 text-move of one step of the per-pass loop
    // body. `executed`/`program` are threaded through explicitly (by `&`/
    // `&mut`/return-and-reassign) exactly as the inline loop did — nothing
    // reads a value a LATER iteration's step produces, and within one
    // iteration the steps run in the same fixed order as before.

    /// Validate a pass's declared `depends_on`/`run_before` dependency
    /// edges against what has executed so far. Panics on violation exactly
    /// like the original inline checks. `executed`/`in_pipeline` are
    /// read-only here.
    fn validate_pass_deps(
        pass: &dyn NanoPass,
        all_passes: &[Box<dyn NanoPass>],
        target: Target,
        executed: &[&str],
        in_pipeline: &std::collections::HashSet<&str>,
    ) {
        // After-deps: every declared dep PRESENT in this pipeline must
        // already have executed (target-conditional, #559).
        for dep in pass.depends_on() {
            if in_pipeline.contains(dep) && !executed.contains(&dep) {
                panic!(
                    "Pass '{}' depends on '{}', but '{}' has not been executed. \
                     Check pipeline ordering.",
                    pass.name(), dep, dep
                );
            }
        }
        // Before-deps: any PRESENT pass declaring `run_before(this)` must
        // already have executed when `this` runs (#559).
        let this_name = pass.name();
        for other in all_passes {
            if !other.targets().map_or(true, |ts| ts.contains(&target)) { continue; }
            if other.run_before().contains(&this_name) && !executed.contains(&other.name()) {
                panic!(
                    "Pass '{}' declares run_before('{}'), but it has not executed \
                     before '{}'. Check pipeline ordering.",
                    other.name(), this_name, this_name
                );
            }
        }
    }

    /// Run one pass with profiling + optional IR dump.
    fn run_pass_with_dump(
        pass: &dyn NanoPass,
        program: IrProgram,
        target: Target,
        dump_all: bool,
        dump_passes: &[&str],
    ) -> IrProgram {
        let pass_name = pass.name();
        // Debug aid: name each pass BEFORE it runs, so a pass that never
        // returns (infinite recursion → stack overflow) is identifiable —
        // the ALMIDE_PROFILE line only prints on completion.
        if std::env::var_os("ALMIDE_TRACE_PASSES").is_some() {
            eprintln!("[pass:start] {}", pass_name);
        }
        // Time only through the wasm-safe shim (raw std::time is forbidden in
        // this crate — it panics on the wasm32-unknown-unknown playground).
        let _pass_t = almide_base::profile::ProfileTimer::start(
            std::env::var_os("ALMIDE_PROFILE").is_some(),
        );
        let result = pass.run(program, target);
        if let Some(t) = &_pass_t {
            let dt = t.elapsed_secs();
            if dt > 0.01 { eprintln!("[prof:pass] {:30} {:.3}s", pass_name, dt); }
        }
        let program = result.program;

        // IR dump (opt-in via ALMIDE_DUMP_IR=all or ALMIDE_DUMP_IR=pass1,pass2)
        if dump_all || dump_passes.iter().any(|p| p.eq_ignore_ascii_case(pass_name)) {
            eprintln!("── IR after {} ──{}──",
                pass_name,
                if result.changed { " (changed) " } else { " (unchanged) " });
            if let Ok(json) = serde_json::to_string_pretty(&program) {
                eprintln!("{}", json);
            } else {
                // Fallback: debug format
                eprintln!("{:#?}", program);
            }
            eprintln!("── end {} ──\n", pass_name);
        }
        program
    }

    /// Inter-pass IR verification (debug / opt-in only — see `verify_ir`
    /// in `run`).
    fn verify_after_pass(pass: &dyn NanoPass, program: &IrProgram) {
        let pass_name = pass.name();
        let errors = almide_ir::verify_program(program);
        if !errors.is_empty() {
            eprintln!("[IR CHECK] {} error(s) after pass '{}':", errors.len(), pass_name);
            for e in &errors {
                eprintln!("  {}", e);
            }
            // No warn-mode: a DETECTED violation is fatal in every
            // profile (release-parity §10 — the v0.25.0 lesson:
            // a warning's audience cannot fix a compiler bug).
            // Release cost is unchanged: the verifier itself stays
            // debug/opt-in (the measured ~1.2s/file walk).
            panic!("IR verification failed after pass '{}'", pass_name);
        }

        // Postcondition verification.
        let postconds = pass.postconditions();
        if !postconds.is_empty() {
            let violations = verify_postconditions(pass_name, program, &postconds);
            for v in &violations {
                eprintln!("[POSTCONDITION VIOLATION] {}", v);
            }
            if !violations.is_empty() {
                panic!("Postcondition violation after pass '{}'", pass_name);
            }
        }
    }

    pub fn run(&self, program: IrProgram, target: Target) -> IrProgram {
        let mut program = program;
        let mut executed: Vec<&str> = Vec::new();

        // ALMIDE_DUMP_IR: dump IR after specified passes (comma-separated, or "all")
        let dump_filter = std::env::var("ALMIDE_DUMP_IR").ok();
        let dump_all = dump_filter.as_deref() == Some("all");
        let dump_passes: Vec<&str> = dump_filter.as_deref()
            .filter(|s| *s != "all")
            .map(|s| s.split(',').map(str::trim).collect())
            .unwrap_or_default();
        // Contract-level checks (IR verifier + pass postconditions) run on
        // every build. Debug builds escalate violations to `panic!` so CI
        // and local `cargo test` catch them; release builds print the same
        // diagnostic and keep running so an end-user `almide build` does
        // not crash on a compiler bug. `ALMIDE_CHECK_IR` /
        // `ALMIDE_VERIFY_IR` used to gate this — removed in S2 flip
        // (v0.14.7-phase3.2); `expr.ty` is now trustworthy by contract.
        let hard_fail = cfg!(debug_assertions);
        let _ = hard_fail; // escalation is unconditional now; the flag only gates whether verification RUNS
        // The inter-pass IR verifier + postcondition checks walk the entire
        // (merged) program after EVERY pass. That's a developer safety net —
        // it catches compiler bugs but does nothing for a correct build. In
        // release it can't even fail hard, yet it dominates codegen time
        // (~1.2s/file: 20 passes × verify(user + all merged stdlib functions)).
        // Run it only in debug (cargo test / CI) or when explicitly requested.
        let verify_ir = hard_fail || std::env::var_os("ALMIDE_VERIFY_IR").is_some();

        // #559: the names of passes that WILL run in THIS target's pipeline.
        // Dep edges are enforced ONLY against this set, so a wasm-arm pass can
        // depend on a Rust-only pass (absent here) without panicking — the edge
        // is vacuous, not violated.
        let in_pipeline: std::collections::HashSet<&str> = self.passes.iter()
            .filter(|p| p.targets().map_or(true, |ts| ts.contains(&target)))
            .map(|p| p.name())
            .collect();
        for pass in &self.passes {
            // Skip passes not relevant to this target
            if let Some(targets) = pass.targets() {
                if !targets.contains(&target) {
                    continue;
                }
            }
            Self::validate_pass_deps(pass.as_ref(), &self.passes, target, &executed, &in_pipeline);

            let pass_name = pass.name();
            program = Self::run_pass_with_dump(pass.as_ref(), program, target, dump_all, &dump_passes);

            // Inter-pass IR verification (debug / opt-in only — see verify_ir).
            if verify_ir {
                Self::verify_after_pass(pass.as_ref(), &program);
            }

            executed.push(pass_name);
        }

        // §10 release promotion (#532): one FINAL verification runs in EVERY
        // profile. The per-pass walk above stays debug/opt-in (it dominates
        // release codegen time, ~1.2s/file across ~20 passes), but the
        // END-of-pipeline IR must verify before emission — one walk, ~60 ms,
        // the same trade wasmparser::validate makes on the wasm side. A
        // violation is a compiler bug and fails the build in release too.
        if !verify_ir {
            let errors = almide_ir::verify_program(&program);
            if !errors.is_empty() {
                eprintln!("[IR CHECK] {} error(s) at end of pipeline:", errors.len());
                for e in &errors {
                    eprintln!("  {}", e);
                }
                panic!("final IR verification failed (release gate, #532)");
            }
        }
        program
    }
}

// ── Built-in Passes ──
//
// Each concrete pass lives in its own `pass_*.rs` file. This file defines
// the trait, the pipeline runner, and the thin wrappers for passes whose
// logic also lives elsewhere (BorrowInsertion, FanLowering, ...).

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
            super::pass_borrow_inference::hoist_conflicting_reads(&mut program);
        }
        PassResult { program, changed }
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
