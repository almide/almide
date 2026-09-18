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

    /// A REPRESENTATION BOUNDARY: every pass declared before this one stays
    /// before it and every pass declared after stays after, without each of
    /// them naming it. `UnifyVarTables` (per-module var tables → one table)
    /// and `IrLinkFlatten` (modules → root) change what a VarId or a fn name
    /// MEANS, so an edge to each of them would be on every pass; a barrier
    /// says it once. The shuffle (`ALMIDE_SHUFFLE_PASSES`) never crosses one.
    /// Default: not a barrier.
    fn barrier(&self) -> bool { false }

    /// Postconditions: structural invariants guaranteed after this pass runs
    /// — and from then on: they are MONOTONE, re-verified after every later
    /// pass in every profile (release included — the per-pass walk costs
    /// ~8 ms per file, measured over spec/lang), so a later pass that undoes
    /// them is named. A violation is a compiler bug and fails the build.
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

/// splitmix64 — the shuffle's seeded generator: deterministic per seed and
/// dependency-free, so `ALMIDE_SHUFFLE_PASSES=<seed>` names one order.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Pipeline {
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    pub fn add<P: NanoPass + 'static>(mut self, pass: P) -> Self {
        self.passes.push(Box::new(pass));
        self
    }

    /// The pass names in the order `target` would run them under `seed`
    /// (the declared order when `seed` is `None`) — what the shuffle gate
    /// prints, and what the shuffle's own tests read.
    pub fn order_names(&self, target: Target, seed: Option<&str>) -> Vec<&str> {
        self.order(target, seed).into_iter().map(|i| self.passes[i].name()).collect()
    }

    /// The indices of the passes that run for `target`, in the DECLARED order
    /// — or, under a shuffle seed, in a random order consistent with every
    /// declared `depends_on` / `run_before` edge and every barrier. Kahn's
    /// algorithm over those edges, the ready set drawn by a seeded
    /// generator; a seed that does not parse runs the declared order.
    pub(crate) fn order(&self, target: Target, seed: Option<&str>) -> Vec<usize> {
        let live: Vec<usize> = (0..self.passes.len())
            .filter(|&i| self.passes[i].targets().map_or(true, |ts| ts.contains(&target)))
            .collect();
        let Some(seed) = seed.and_then(|s| s.trim().parse::<u64>().ok()) else { return live; };
        let after = self.declared_edges(&live);
        let n = live.len();
        let mut indegree = vec![0usize; n];
        for succs in &after {
            for &b in succs { indegree[b] += 1; }
        }
        let mut rng = SplitMix64(seed ^ 0x9E37_79B9_7F4A_7C15);
        let mut ready: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
        let mut out = Vec::with_capacity(n);
        while !ready.is_empty() {
            let pick = (rng.next() % ready.len() as u64) as usize;
            let a = ready.swap_remove(pick);
            out.push(live[a]);
            for &b in &after[a] {
                indegree[b] -= 1;
                if indegree[b] == 0 { ready.push(b); }
            }
        }
        assert!(out.len() == n, "[ICE] the declared pass edges form a cycle");
        out
    }

    /// The edges the shuffle must respect, over positions in `live`:
    /// `after[a]` holds every live `b` that must run AFTER `a` — from each
    /// pass's `depends_on` / `run_before`, from every barrier (everything
    /// declared before it precedes it, everything after follows it), and
    /// from `ALMIDE_PASS_EDGES=A<B,C<D`, the extra edges the bisection
    /// instrument (`scripts/pass-shuffle-bisect.py`) forces to find the
    /// pair a divergence needs declared.
    fn declared_edges(&self, live: &[usize]) -> Vec<Vec<usize>> {
        let position = |name: &str| live.iter().position(|&i| self.passes[i].name() == name);
        let n = live.len();
        let mut after: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut edge = |a: usize, b: usize| if a != b && !after[a].contains(&b) { after[a].push(b); };
        for (bi, &b) in live.iter().enumerate() {
            for dep in self.passes[b].depends_on() {
                if let Some(ai) = position(dep) { edge(ai, bi); }
            }
            for succ in self.passes[b].run_before() {
                if let Some(ci) = position(succ) { edge(bi, ci); }
            }
            if self.passes[b].barrier() {
                for ai in 0..bi { edge(ai, bi); }
                for ci in bi + 1..n { edge(bi, ci); }
            }
        }
        let extra = almide_base::env::var("ALMIDE_PASS_EDGES").unwrap_or_default();
        for (a, b) in extra.split(',').filter_map(|pair| pair.split_once('<')) {
            if let (Some(ai), Some(bi)) = (position(a.trim()), position(b.trim())) {
                edge(ai, bi);
            }
        }
        after
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
        if almide_base::env::flag("ALMIDE_TRACE_PASSES") {
            eprintln!("[pass:start] {}", pass_name);
        }
        // Time only through the wasm-safe shim (raw std::time is forbidden in
        // this crate — it panics on the wasm32-unknown-unknown playground).
        let _pass_t = almide_base::profile::ProfileTimer::start(
            almide_base::env::flag("ALMIDE_PROFILE"),
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

    /// Inter-pass IR verification, after every pass in every profile: the IR
    /// verifier, then the postconditions of the pass that just ran (`idx`)
    /// AND of every pass that ran before it (`done`).
    fn verify_after_pass(passes: &[Box<dyn NanoPass>], idx: usize, done: &[usize], program: &IrProgram) {
        let pass_name = passes[idx].name();
        let errors = almide_ir::verify_program(program);
        if !errors.is_empty() {
            eprintln!("[IR CHECK] {} error(s) after pass '{}':", errors.len(), pass_name);
            for e in &errors {
                eprintln!("  {}", e);
            }
            // No warn-mode: a DETECTED violation is fatal in every
            // profile (release-parity §10 — the v0.25.0 lesson:
            // a warning's audience cannot fix a compiler bug).
            panic!("IR verification failed after pass '{}'", pass_name);
        }

        // Postcondition verification: the pass's own, then every one
        // established earlier that must still hold.
        let mut violations = verify_postconditions(pass_name, program, &passes[idx].postconditions());
        violations.extend(Self::established_violations(passes, done, pass_name, program));
        for v in &violations {
            eprintln!("[POSTCONDITION VIOLATION] {}", v);
        }
        if !violations.is_empty() {
            panic!("Postcondition violation after pass '{}'", pass_name);
        }
    }

    /// A postcondition is MONOTONE: it holds from the pass that establishes
    /// it to the end of the pipeline (rustc's `validate_body` is indexed by
    /// `MirPhase`, Swift's SIL verifier by `SILStage`, for the same reason).
    /// A later pass that reintroduces a shape an earlier pass lowered breaks
    /// the invariant downstream passes rely on; this names both passes.
    fn established_violations(passes: &[Box<dyn NanoPass>], done: &[usize], after: &str, program: &IrProgram) -> Vec<String> {
        done.iter()
            .flat_map(|&i| {
                let by = passes[i].name();
                verify_postconditions(by, program, &passes[i].postconditions())
                    .into_iter()
                    .map(move |v| format!("{v} — established by '{by}', no longer holds after '{after}'"))
            })
            .collect()
    }

    pub fn run(&self, program: IrProgram, target: Target) -> IrProgram {
        let mut program = program;
        let mut executed: Vec<&str> = Vec::new();
        let mut done: Vec<usize> = Vec::new();

        // ALMIDE_DUMP_IR: dump IR after specified passes (comma-separated, or "all")
        let dump_filter = almide_base::env::var("ALMIDE_DUMP_IR");
        let dump_all = dump_filter.as_deref() == Some("all");
        let dump_passes: Vec<&str> = dump_filter.as_deref()
            .filter(|s| *s != "all")
            .map(|s| s.split(',').map(str::trim).collect())
            .unwrap_or_default();
        // Contract-level checks (the IR verifier + every established pass
        // postcondition) run after EVERY pass in EVERY profile, and a
        // detected violation is fatal (§10 release parity). Until now the
        // per-pass walk was debug / `ALMIDE_VERIFY_IR` only, on a cost
        // claim of ~1.2 s per file that no longer held: measured over the
        // 218 files of spec/lang, `--target rust` emission takes 5.75 s
        // without the walk and 7.54 s with it — ~8 ms per file, a rounding
        // error next to rustc. A release binary that skipped the walk was
        // the one profile whose IR nobody checked; that profile is the one
        // that ships. `ALMIDE_IR_FAULT=<pass>` (harness) injects a
        // violation after the named pass so the gate can be watched turning
        // red in the release binary (tests/ir_verify_every_profile_test.rs).
        let fault_after = almide_base::env::var("ALMIDE_IR_FAULT");

        // #912 pass-ordering lens: `ALMIDE_SKIP_PASS=Name[,Name…]` skips the
        // named passes. A hunt instrument, not a user feature: the spec suite
        // run with an OPTIONAL pass skipped must not change program OUTPUT —
        // a silent value diff is a pass-dependency hole, while a dep-edge
        // panic or compile error is the system refusing loudly. Zero-cost
        // when unset (parsed once, out of the loop).
        let skip_passes: Vec<String> = almide_base::env::var("ALMIDE_SKIP_PASS")
            .map(|s| s.split(',').map(|x| x.trim().to_string()).collect())
            .unwrap_or_default();

        // #559: the names of passes that WILL run in THIS target's pipeline.
        // Dep edges are enforced ONLY against this set, so a wasm-arm pass can
        // depend on a Rust-only pass (absent here) without panicking — the edge
        // is vacuous, not violated.
        let in_pipeline: std::collections::HashSet<&str> = self.passes.iter()
            .filter(|p| p.targets().map_or(true, |ts| ts.contains(&target)))
            .map(|p| p.name())
            .collect();
        // #2186 step 4: `ALMIDE_SHUFFLE_PASSES=<seed>` runs the passes in a
        // random order that respects every DECLARED edge and barrier — and
        // nothing else. The declared order is one such order; if another
        // one emits different Rust, a dependency is missing its declaration.
        // `scripts/check-pass-shuffle.sh` byte-diffs the corpus under it.
        let shuffle_seed = almide_base::env::var("ALMIDE_SHUFFLE_PASSES");
        let order = self.order(target, shuffle_seed.as_deref());
        if let Some(seed) = &shuffle_seed {
            // The order under this seed, so a byte-diff names its witness.
            let names: Vec<&str> = order.iter().map(|&i| self.passes[i].name()).collect();
            eprintln!("[almide] ALMIDE_SHUFFLE_PASSES={seed} order: {}", names.join(" "));
        }
        for &idx in &order {
            let pass = &self.passes[idx];
            // #912 lens skip: the skipped pass stays OUT of `executed`, so a
            // later pass that declared a dep edge on it panics loudly in
            // `validate_pass_deps` — an undeclared dependency is exactly what
            // the hunt is for.
            if skip_passes.iter().any(|s| s.eq_ignore_ascii_case(pass.name())) {
                continue;
            }
            Self::validate_pass_deps(pass.as_ref(), &self.passes, target, &executed, &in_pipeline);

            let pass_name = pass.name();
            program = Self::run_pass_with_dump(pass.as_ref(), program, target, dump_all, &dump_passes);
            if fault_after.as_deref().is_some_and(|p| p.eq_ignore_ascii_case(pass_name)) {
                Self::inject_ir_fault(&mut program);
            }

            // Inter-pass IR verification, every pass, every profile.
            Self::verify_after_pass(&self.passes, idx, &done, &program);

            executed.push(pass_name);
            done.push(idx);
        }

        // The last pass's walk above IS the end-of-pipeline verification
        // (#532): the IR the emitter reads verified, in every profile.
        program
    }

    /// The `ALMIDE_IR_FAULT` fault: bind one function's locals in a second
    /// function, the `verify_binder_ownership` violation (#2186) — a
    /// duplicate of the first function that has a param or a binder. The
    /// program then fails the walk after the named pass, in the profile
    /// that runs it: the release binary's evidence that the walk runs.
    fn inject_ir_fault(program: &mut IrProgram) {
        let Some(victim) = program.functions.iter().find(|f| !f.params.is_empty()).cloned() else {
            panic!("ALMIDE_IR_FAULT: no function with a param to duplicate");
        };
        let mut twin = victim;
        twin.name = almide_base::intern::sym("__ir_fault_twin");
        program.functions.push(twin);
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
            super::pass_borrow_inference::commit_chain_source_modes(&mut program, &sigs);
            super::pass_borrow_inference::insert_borrows_at_call_sites(&mut program, &sigs);
            super::pass_borrow_inference::note_borrowed_lambda_params(&mut program);
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

    /// Strips the auto-`Try` inside fan arms, so the `Try` has to be there.
    fn depends_on(&self) -> Vec<&'static str> { vec!["ResultPropagation"] }
    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        super::pass_fan_lowering::strip_fan_auto_try(&mut program);
        PassResult { program, changed: true }
    }
}

#[cfg(test)]
mod shuffle_tests {
    //! The seeded order respects every declared edge and barrier, is
    //! deterministic per seed, actually varies across seeds, and refuses a
    //! cycle — the properties `ALMIDE_SHUFFLE_PASSES` rests on (#2186).
    use super::*;

    #[derive(Debug)]
    struct P { name: &'static str, after: Vec<&'static str>, before: Vec<&'static str>, barrier: bool, rust_only: bool }
    impl NanoPass for P {
        fn name(&self) -> &str { self.name }
        fn targets(&self) -> Option<Vec<Target>> { if self.rust_only { Some(vec![Target::Rust]) } else { None } }
        fn depends_on(&self) -> Vec<&'static str> { self.after.clone() }
        fn run_before(&self) -> Vec<&'static str> { self.before.clone() }
        fn barrier(&self) -> bool { self.barrier }
        fn run(&self, program: IrProgram, _: Target) -> PassResult { PassResult { program, changed: false } }
    }
    fn p(name: &'static str) -> P { P { name, after: vec![], before: vec![], barrier: false, rust_only: false } }

    fn pipeline() -> Pipeline {
        Pipeline::new()
            .add(p("Unify"))
            .add(P { name: "Barrier", after: vec![], before: vec![], barrier: true, rust_only: false })
            .add(p("A"))
            .add(P { name: "B", after: vec!["A"], before: vec![], barrier: false, rust_only: false })
            .add(P { name: "C", after: vec![], before: vec!["D"], barrier: false, rust_only: false })
            .add(p("D"))
            .add(p("E"))
    }

    fn pos(order: &[&str], name: &str) -> usize { order.iter().position(|n| *n == name).unwrap_or_else(|| panic!("{name} missing from {order:?}")) }

    #[test]
    fn the_declared_order_is_the_order_without_a_seed() {
        assert_eq!(pipeline().order_names(Target::Rust, None), vec!["Unify", "Barrier", "A", "B", "C", "D", "E"]);
        // An unparsable seed is no seed.
        assert_eq!(pipeline().order_names(Target::Rust, Some("x")), vec!["Unify", "Barrier", "A", "B", "C", "D", "E"]);
    }

    #[test]
    fn every_seed_respects_the_edges_and_the_barrier_and_some_seed_moves_something() {
        let pl = pipeline();
        let mut moved = false;
        for seed in 0..64u64 {
            let o = pl.order_names(Target::Rust, Some(&seed.to_string()));
            assert_eq!(o.len(), 7, "every pass runs once: {o:?}");
            assert_eq!(o[0], "Unify", "everything declared before the barrier stays before it: {o:?}");
            assert_eq!(o[1], "Barrier", "{o:?}");
            assert!(pos(&o, "A") < pos(&o, "B"), "depends_on: {o:?}");
            assert!(pos(&o, "C") < pos(&o, "D"), "run_before: {o:?}");
            moved |= o != vec!["Unify", "Barrier", "A", "B", "C", "D", "E"];
            assert_eq!(o, pl.order_names(Target::Rust, Some(&seed.to_string())), "deterministic per seed");
        }
        assert!(moved, "the free passes never moved — the shuffle would be decorative");
    }

    #[test]
    fn a_pass_absent_from_the_target_is_neither_run_nor_an_edge() {
        // A Rust-only pass that must precede `X`: the Wgsl arm runs `X`
        // alone, and the edge to the absent pass is vacuous, not a panic.
        let pl = Pipeline::new()
            .add(P { name: "R", after: vec![], before: vec!["X"], barrier: false, rust_only: true })
            .add(p("X"));
        assert_eq!(pl.order_names(Target::Wgsl, Some("1")), vec!["X"]);
        assert_eq!(pl.order_names(Target::Rust, Some("1")), vec!["R", "X"]);
    }

    #[test]
    #[should_panic(expected = "form a cycle")]
    fn a_cycle_in_the_declared_edges_is_an_ice() {
        let pl = Pipeline::new()
            .add(P { name: "A", after: vec!["B"], before: vec![], barrier: false, rust_only: false })
            .add(P { name: "B", after: vec!["A"], before: vec![], barrier: false, rust_only: false });
        let _ = pl.order_names(Target::Rust, Some("1"));
    }
}
