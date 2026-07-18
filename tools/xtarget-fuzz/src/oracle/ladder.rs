//! The ladder driver: applies each rung in order and classifies the
//! first failure.

use std::path::Path;

use almide::fmt::format_program;

use super::runner::Toolchain;
use super::ReferenceOracle;

/// Which rung a program reached / failed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    Check,
    FmtRoundTrip,
    NativeBuild,
    WasmBuild,
    Run,
}

/// The classified result of running the full ladder on one program.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Passed every rung; native and WASM agreed byte-for-byte. Carries
    /// the native evidence so post-rungs (the metamorphic gate) can
    /// compare variant behavior without re-running the original.
    Clean { native: RunEvidence },

    /// `almide check` rejected the program. This is a *generator* bug
    /// (we promised well-typed-by-construction), not a compiler finding.
    /// The driver buckets these separately and they gate generator
    /// quality. `diagnostics` is the check stderr.
    GeneratorReject { diagnostics: String },

    /// A genuine compiler/runtime finding worth a repro.
    Finding(Finding),

    /// The program could not be evaluated to a comparison (e.g. wasm
    /// runtime missing) — skipped, not counted against anything.
    Skipped { reason: String },
}

/// A reproducible finding: the rung it surfaced at plus the evidence.
#[derive(Debug, Clone)]
pub struct Finding {
    pub rung: Rung,
    pub kind: FindingKind,
    /// Human-readable summary for the issue/report.
    pub summary: String,
    /// Native side evidence (stdout/stderr/exit), when relevant.
    pub native: Option<RunEvidence>,
    /// WASM side evidence, when relevant.
    pub wasm: Option<RunEvidence>,
}

/// The category of a finding — drives triage and dedup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    /// fmt(parse(fmt(parse(src)))) was not stable.
    FmtInstability,
    /// Native build failed (rustc rejected generated Rust, or ICE).
    NativeBuildFailure,
    /// WASM build failed or the module did not validate.
    WasmBuildFailure,
    /// One side hung (timed out).
    Hang,
    /// Native and WASM produced different observable output.
    OutputDivergence,
    /// One side ran, the other failed to run though it built.
    RunFailureDivergence,
    /// A binding-shape rewrite (let⟺var⟺assign) changed acceptance or
    /// observable behavior (#515, completeness §3).
    MetamorphicDivergence,
}

/// Captured observable behaviour of one execution.
#[derive(Debug, Clone)]
pub struct RunEvidence {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

impl RunEvidence {
    fn from(p: &super::runner::ProcResult) -> Self {
        RunEvidence {
            stdout: String::from_utf8_lossy(&p.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&p.stderr).into_owned(),
            exit_code: p.exit_code,
            timed_out: p.timed_out,
        }
    }
}

/// Run the full ladder against a program already written to `file`.
/// `wasm_out` is a scratch path for the WASM build artifact. `reference`
/// is an optional future interpreter oracle (currently always `None`).
pub fn run_ladder(
    tc: &Toolchain,
    source: &str,
    file: &Path,
    wasm_out: &Path,
    reference: Option<&dyn ReferenceOracle>,
) -> Outcome {
    // ── Rung (a): check ──
    let check = tc.check(file);
    if check.timed_out {
        return Outcome::Finding(Finding {
            rung: Rung::Check,
            kind: FindingKind::Hang,
            summary: "almide check hung".into(),
            native: None,
            wasm: None,
        });
    }
    if !check.success() {
        return Outcome::GeneratorReject {
            diagnostics: String::from_utf8_lossy(&check.stderr).into_owned(),
        };
    }

    // ── Rung (b): fmt round-trip stability ──
    if let Some(finding) = fmt_round_trip(source) {
        return Outcome::Finding(finding);
    }

    // ── Rung (c): native build + run ──
    let native = tc.run_native(file);
    if native.spawn_failed {
        return Outcome::Skipped {
            reason: format!(
                "could not spawn almide: {}",
                String::from_utf8_lossy(&native.stderr)
            ),
        };
    }
    if native.timed_out {
        // A native hang is not, by itself, a cross-target finding: a mutation
        // can synthesize a genuinely non-terminating program (`pos + 0` in a
        // recursion step — seed 20260718 index 198), which hangs on both
        // targets. Only a hang DIVERGENCE is evidence: build + run the wasm
        // leg; if wasm CLEANLY SUCCEEDS while native hung, that IS a finding.
        // A wasm failure exit is NOT termination evidence — an unbounded
        // allocator loop traps at wasm's 4GB memory ceiling long before
        // native's (the index-198 shape: both non-terminating, wasm merely
        // OOMs first) — so it skips like a double hang.
        let wasm_build = tc.build_wasm(file, wasm_out);
        if wasm_build.success() {
            let wasm_run = tc.run_wasm(wasm_out);
            if native_hang_is_finding(true, wasm_run.timed_out, wasm_run.success()) {
                return Outcome::Finding(Finding {
                    rung: Rung::Run,
                    kind: FindingKind::Hang,
                    summary: "native run hung while wasm succeeded".into(),
                    native: Some(RunEvidence::from(&native)),
                    wasm: Some(RunEvidence::from(&wasm_run)),
                });
            }
        }
        return Outcome::Skipped {
            reason: "native hung and wasm did not cleanly succeed (a non-terminating or \
                     resource-unbounded program by construction) — no divergence oracle"
                .into(),
        };
    }
    if !native.success() {
        // A COMPILE failure is an immediate finding: rung (a) accepted the
        // program, so failing to build it is a check-vs-build gap. A RUNTIME
        // error is NOT — a corpus MUTATION can synthesize a program that
        // ABORTS BY DESIGN (a bounds/div-fixture variant in the mutation
        // pool), and the abort form is itself a cross-target contract
        // (ALS-T6): the ORACLE is the comparison below — wasm must reach the
        // same observables, divergence surfaces there.
        let stderr = String::from_utf8_lossy(&native.stderr);
        if stderr.contains("Compile error") || stderr.contains("error[E") {
            return Outcome::Finding(Finding {
                rung: Rung::NativeBuild,
                kind: FindingKind::NativeBuildFailure,
                summary: "native build failed after check accepted".into(),
                native: Some(RunEvidence::from(&native)),
                wasm: None,
            });
        }
    }

    // ── Rung (d): wasm build ──
    let wasm_build = tc.build_wasm(file, wasm_out);
    if !wasm_build.success() {
        return Outcome::Finding(Finding {
            rung: Rung::WasmBuild,
            kind: FindingKind::WasmBuildFailure,
            summary: "wasm build failed".into(),
            native: Some(RunEvidence::from(&native)),
            wasm: Some(RunEvidence::from(&wasm_build)),
        });
    }

    // ── Rung (e): wasm run + differential compare ──
    let wasm = tc.run_wasm(wasm_out);
    if wasm.spawn_failed {
        // wasmtime not installed ⇒ we cannot do the differential compare.
        return Outcome::Skipped {
            reason: "could not spawn wasmtime (is it installed?)".into(),
        };
    }
    if wasm.timed_out {
        return Outcome::Finding(Finding {
            rung: Rung::Run,
            kind: FindingKind::Hang,
            summary: "wasm run hung".into(),
            native: Some(RunEvidence::from(&native)),
            wasm: Some(RunEvidence::from(&wasm)),
        });
    }

    // Compare observable behaviour: stdout, exit code, and run-success.
    let nat_ev = RunEvidence::from(&native);
    let wasm_ev = RunEvidence::from(&wasm);

    if native.success() != wasm.success() {
        // One leg ran cleanly and the other did not — a run-failure
        // divergence in either direction (native can non-zero-exit BY DESIGN
        // now that intended-abort corpus mutants flow through to the compare).
        let summary = if native.success() {
            "wasm run failed while native succeeded"
        } else {
            "native run failed while wasm succeeded"
        };
        return Outcome::Finding(Finding {
            rung: Rung::Run,
            kind: FindingKind::RunFailureDivergence,
            summary: summary.into(),
            native: Some(nat_ev),
            wasm: Some(wasm_ev),
        });
    }

    if nat_ev.stdout != wasm_ev.stdout || nat_ev.exit_code != wasm_ev.exit_code {
        return Outcome::Finding(Finding {
            rung: Rung::Run,
            kind: FindingKind::OutputDivergence,
            summary: divergence_summary(&nat_ev, &wasm_ev),
            native: Some(nat_ev),
            wasm: Some(wasm_ev),
        });
    }

    // Optional future rung: compare both against a reference interpreter.
    if let Some(reference) = reference {
        if let Some(expected) = reference.evaluate(source) {
            if expected != nat_ev.stdout {
                return Outcome::Finding(Finding {
                    rung: Rung::Run,
                    kind: FindingKind::OutputDivergence,
                    summary: "both targets disagree with reference interpreter".into(),
                    native: Some(nat_ev),
                    wasm: Some(wasm_ev),
                });
            }
        }
    }

    Outcome::Clean { native: nat_ev }
}

/// fmt round-trip: `parse → fmt → parse → fmt` must be a fixed point.
/// Returns a finding if it is not (formatter instability), or `None` if
/// the source could not be re-parsed (which the check rung would already
/// have caught — treated as no-finding here).
fn fmt_round_trip(source: &str) -> Option<Finding> {
    let first = parse_then_format(source)?;
    let second = parse_then_format(&first)?;
    if first != second {
        return Some(Finding {
            rung: Rung::FmtRoundTrip,
            kind: FindingKind::FmtInstability,
            summary: "fmt is not idempotent (parse∘fmt∘parse∘fmt diverged)".into(),
            native: None,
            wasm: None,
        });
    }
    None
}

/// Parse `src` and format it, or `None` on parse failure.
fn parse_then_format(src: &str) -> Option<String> {
    let tokens = almide::lexer::Lexer::tokenize(src);
    let mut parser = almide::parser::Parser::new(tokens);
    let program = parser.parse().ok()?;
    Some(format_program(&program))
}

/// Build a short, scannable description of an output divergence —
/// the first line that differs.
fn divergence_summary(native: &RunEvidence, wasm: &RunEvidence) -> String {
    if native.exit_code != wasm.exit_code {
        return format!(
            "exit code differs: native={:?} wasm={:?}",
            native.exit_code, wasm.exit_code
        );
    }
    for (n, w) in native.stdout.lines().zip(wasm.stdout.lines()) {
        if n != w {
            return format!("stdout differs: native={n:?} wasm={w:?}");
        }
    }
    format!(
        "stdout length differs: native={}B wasm={}B",
        native.stdout.len(),
        wasm.stdout.len()
    )
}

/// Pure classification for the native-hang rung: a HANG is a finding IFF the
/// wasm leg built, did not itself time out, and CLEANLY SUCCEEDED — a wasm
/// failure exit is not termination evidence (an unbounded allocator loop traps
/// at wasm's 4GB ceiling long before native's; both are non-terminating).
fn native_hang_is_finding(wasm_built: bool, wasm_timed_out: bool, wasm_succeeded: bool) -> bool {
    wasm_built && !wasm_timed_out && wasm_succeeded
}

#[cfg(test)]
mod hang_classification_tests {
    use super::native_hang_is_finding;

    #[test]
    fn wasm_clean_success_is_a_finding() {
        assert!(native_hang_is_finding(true, false, true));
    }

    #[test]
    fn wasm_hang_is_a_skip() {
        // Both targets hang — a non-terminating program by construction
        // (seed 20260718 index 198's `pos + 0` mutation).
        assert!(!native_hang_is_finding(true, true, false));
    }

    #[test]
    fn wasm_failure_exit_is_a_skip() {
        // wasm OOM-trapped at its 4GB ceiling while native was still
        // allocating — resource race, not a semantic divergence.
        assert!(!native_hang_is_finding(true, false, false));
    }

    #[test]
    fn wasm_build_failure_is_a_skip() {
        assert!(!native_hang_is_finding(false, false, false));
    }
}
