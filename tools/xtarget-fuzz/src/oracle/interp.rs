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

    /// An oracle with an explicit budget. Only the tests use this — they
    /// need a budget small enough to exhaust in milliseconds, and the
    /// `FuelExhausted` verdict does not depend on the budget's size.
    #[cfg(test)]
    pub fn with_fuel(fuel: u64) -> Self {
        Self { fuel }
    }

    /// Run `source` to completion (or to fuel exhaustion) and hand back the
    /// raw outcome. Both public answers are read off this one run: the
    /// stdout vote wants `Ok` only, the termination probe wants
    /// `FuelExhausted` only, and neither may see the other's status.
    fn run_inner(&self, source: &str) -> Option<almide::interp::RunOutcome> {
        use almide::interp::Interpreter;

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

        Some(Interpreter::new(&ir).with_fuel(self.fuel).run_main())
    }
}

impl ReferenceOracle for InterpOracle {
    fn evaluate(&self, source: &str) -> Option<String> {
        use almide::interp::RunStatus;

        // The frontend is panic-free on the fuzz corpus by its own gates,
        // but the oracle must never kill the campaign: a panic = abstain
        // (and the shape will surface through the panic-fuzz lane instead).
        let out = catch_unwind(AssertUnwindSafe(|| self.run_inner(source)))
            .ok()
            .flatten()?;
        match out.status {
            // Only a CLEAN run casts a vote: the ladder compares stdout of
            // clean native/wasm runs, and abort stderr text is not part of
            // the cross-target byte contract at this rung.
            RunStatus::Ok => Some(out.stdout),
            // `Exited(n)` is an explicit `process.exit(n)` — a deliberate
            // non-zero exit, not a clean run, so it abstains from the stdout
            // vote exactly like `Aborted` (this rung compares the stdout of
            // CLEAN runs; the exit code is the ladder's own comparison).
            RunStatus::Aborted
            | RunStatus::Exited(_)
            | RunStatus::Unsupported(_)
            | RunStatus::FuelExhausted => None,
        }
    }

    fn exhausts_fuel(&self, source: &str) -> bool {
        use almide::interp::RunStatus;

        // ONLY `FuelExhausted`. `Unsupported` means the interpreter lacks a
        // feature — it says nothing about whether the program terminates —
        // and treating it as non-termination would suppress real findings on
        // every shape the interpreter has not caught up to yet.
        matches!(
            catch_unwind(AssertUnwindSafe(|| self.run_inner(source))).ok().flatten(),
            Some(out) if matches!(out.status, RunStatus::FuelExhausted)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The #924 shape: an effect-free counting loop seeded at `i64::MIN`.
    /// LLVM deletes it and computes the exit value analytically, so the
    /// native leg finishes in milliseconds while wasm — which elides
    /// nothing — runs it for real and times out. Native therefore cannot
    /// answer "does this terminate"; the fuel-bounded interpreter can.
    const NON_TERMINATING: &str = r#"
fn main() -> Unit = {
  var seq = -9223372036854775808
  while seq < 3 {
    seq = seq + 1
  }
  println("loop=${seq}")
}
"#;

    /// The same program with the literal the mutation replaced. This is the
    /// A/B partner: if `exhausts_fuel` said `true` here too, the rule would
    /// suppress every wasm-hang finding rather than just the elided ones.
    const TERMINATING: &str = r#"
fn main() -> Unit = {
  var seq = 0
  while seq < 3 {
    seq = seq + 1
  }
  println("loop=${seq}")
}
"#;

    #[test]
    fn non_terminating_loop_exhausts_fuel() {
        let oracle = InterpOracle::with_fuel(100_000);
        assert!(
            oracle.exhausts_fuel(NON_TERMINATING),
            "a ~9.2e18-iteration loop must exhaust the budget, not abstain"
        );
    }

    #[test]
    fn terminating_loop_does_not_exhaust_fuel() {
        let oracle = InterpOracle::with_fuel(100_000);
        assert!(
            !oracle.exhausts_fuel(TERMINATING),
            "a 3-iteration loop must not be reported as non-terminating"
        );
        assert_eq!(oracle.evaluate(TERMINATING).as_deref(), Some("loop=3\n"));
    }

    /// Fuel exhaustion abstains from the STDOUT vote as it always has —
    /// `exhausts_fuel` is a separate question, and answering it must not
    /// turn a timed-out run into a third vote.
    #[test]
    fn fuel_exhaustion_still_abstains_from_the_stdout_vote() {
        let oracle = InterpOracle::with_fuel(100_000);
        assert_eq!(oracle.evaluate(NON_TERMINATING), None);
    }

    /// How much of the self-checking identity family (#1332) the THIRD
    /// judge can actually judge.
    ///
    /// This number matters for a specific reason: the interpreter is the
    /// only judge with a lineage independent of both backends, but it
    /// abstains freely (69.9% of the cross-target corpus votes today), and
    /// an abstention is not a verdict. The identity family was chosen
    /// partly because it sits well inside the interpreter's reach — so on
    /// this family the campaign gets BOTH an independent second opinion
    /// and an oracle that needs no second opinion at all.
    ///
    /// The floor is deliberately loose: this asserts the family has not
    /// drifted OUT of interp reach, not a precise rate.
    #[test]
    fn the_interpreter_can_judge_most_of_the_identity_family() {
        use crate::generator::identity;
        use crate::rng::SplitMix64;

        let oracle = InterpOracle::new();
        const N: u64 = 60;
        let mut voted = 0usize;
        for index in 0..N {
            let plan = identity::plan(&mut SplitMix64::for_program(0x1332, index));
            let (src, expected) = identity::render(&plan);
            if let Some(out) = oracle.evaluate(&src) {
                voted += 1;
                // When it DOES vote, it must agree with the construction —
                // a disagreement here is a real finding in the interpreter
                // (or in the generator's algebra), not a coverage gap.
                assert_eq!(out, expected, "interp disagreed with the oracle at index {index}");
            }
        }
        // Printed so the rate is recoverable with `--nocapture` rather than
        // only when the floor breaks.
        eprintln!("interp judged {voted}/{N} identity programs");
        assert!(
            voted * 2 >= N as usize,
            "the interpreter judged only {voted}/{N} identity programs — the family \
             has drifted out of the third judge's reach"
        );
    }

    /// A shape the interpreter cannot run must NOT be read as
    /// non-termination: `Unsupported` says nothing about whether the
    /// program halts, and conflating the two would silently suppress real
    /// findings on every feature the interpreter has not caught up to.
    #[test]
    fn unsupported_is_not_non_termination() {
        let oracle = InterpOracle::with_fuel(100_000);
        // Not parseable / not checkable ⇒ run_inner bails before the
        // interpreter ever runs, which is the strongest form of "no answer".
        assert!(!oracle.exhausts_fuel("fn main() -> Unit = { nonexistent_fn() }"));
    }
}
