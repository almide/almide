//! The reference-interpreter oracle (#516) — the third judge.
//!
//! Implements `ReferenceOracle` over `almide::interp` (the pre-codegen IR
//! tree-walker). With it, the ladder can catch the class the 2-way
//! differential is structurally blind to: BOTH targets wrong identically.
//!
//! Abstention discipline: any pipeline failure (parse / type errors the
//! check rung somehow let through, lowering panics on generator shapes,
//! `Unsupported` interp features, fuel exhaustion) returns `None` — the
//! ladder then skips the third vote rather than emitting a bogus one.

use super::ReferenceOracle;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub struct InterpOracle {
    /// Evaluation budget. Generated programs can loop arbitrarily; the
    /// native/wasm rungs are wall-clock bounded, the interp is fuel
    /// bounded. Exhaustion = abstain, never a finding.
    fuel: u64,
}

impl InterpOracle {
    pub fn new() -> Self {
        // Generous: the generator's corpus programs are small; anything
        // that needs more than this is a loop the wall-clock rungs already
        // bound at seconds.
        const DEFAULT_FUEL: u64 = 50_000_000;
        Self { fuel: DEFAULT_FUEL }
    }

    fn evaluate_inner(&self, source: &str) -> Option<String> {
        use almide::interp::{Interpreter, RunStatus};

        let tokens = almide::lexer::Lexer::tokenize(source);
        let mut parser = almide::parser::Parser::new(tokens);
        let mut prog = parser.parse().ok()?;
        if !parser.errors.is_empty() {
            return None;
        }

        let canon = almide::canonicalize::canonicalize_program(&prog, std::iter::empty());
        let mut checker = almide::check::Checker::from_env(canon.env);
        let diags = checker.infer_program(&mut prog);
        if diags
            .iter()
            .any(|d| d.level == almide::diagnostic::Level::Error)
        {
            return None;
        }

        let mut ir = almide::lower::lower_program(&prog, &checker.env, &checker.type_map);
        almide::optimize::optimize_program(&mut ir);
        almide::mono::monomorphize(&mut ir);
        almide::ir_link::ir_link(&mut ir);

        let out = Interpreter::new(&ir).with_fuel(self.fuel).run_main();
        match out.status {
            // Only a CLEAN run casts a vote: the ladder compares stdout of
            // clean native/wasm runs, and abort stderr text is not part of
            // the cross-target byte contract at this rung.
            RunStatus::Ok => Some(out.stdout),
            RunStatus::Aborted | RunStatus::Unsupported(_) | RunStatus::FuelExhausted => None,
        }
    }
}

impl ReferenceOracle for InterpOracle {
    fn evaluate(&self, source: &str) -> Option<String> {
        // The frontend is panic-free on the fuzz corpus by its own gates,
        // but the oracle must never kill the campaign: a panic = abstain
        // (and the shape will surface through the panic-fuzz lane instead).
        catch_unwind(AssertUnwindSafe(|| self.evaluate_inner(source)))
            .ok()
            .flatten()
    }
}
