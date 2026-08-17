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
    /// The by-construction oracle (#1332): the program's own source
    /// declares the stdout it must produce, so a leg is judged ALONE.
    SelfCheck,
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

    /// The v1 wasm renderer declined the program with an HONEST wall
    /// (#782: a wall is a clean error, never a silent fallback). This is
    /// SUBSET-COVERAGE debt, not a divergence bug: the program has no wasm
    /// leg to compare. Counted separately (the wall rate is its own
    /// metric); `reason` is the wall reason for the burn-down histogram.
    /// Recognized by the `almide::WASM_WALL_MARKER` stderr line — a shared
    /// constant, not a copied string: the first classifier here matched a
    /// diagnostic form (`wall: Unsupported(...)`) that #931's rework then
    /// removed, so every wall was misfiled as a WasmBuildFailure finding
    /// and the nightly went red on subset debt.
    Walled { reason: String },

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
    /// One side outran the budget AND a confirm re-run at
    /// [`SLOW_CONFIRM_FACTOR`]× the budget still did not finish.
    Hang,
    /// One side outran the budget but COMPLETED, byte-identical, within the
    /// [`SLOW_CONFIRM_FACTOR`]× confirm re-run (#1235). Perf-class: the night
    /// verdict routes it to the perf ledger instead of failing on it — the
    /// 0.57.0 release gate classified a 21.7s quadratic-concat run (#1229)
    /// as a Hang, a wrong CLASSIFIER verdict on a right detector.
    Slow,
    /// Native and WASM produced different observable output.
    OutputDivergence,
    /// One side ran, the other failed to run though it built.
    RunFailureDivergence,
    /// A binding-shape rewrite (let⟺var⟺assign) changed acceptance or
    /// observable behavior (#515, completeness §3).
    MetamorphicDivergence,
    /// A leg's observable output did not match the output the program is
    /// KNOWN by construction to produce (#1332). Unlike every other kind
    /// here this needs no second execution to be a verdict, so it fires
    /// even when native and wasm agree — the shared-lowering blind spot
    /// the 2-way vote cannot see.
    SelfCheckFailure,
}

/// Captured observable behaviour of one execution.
#[derive(Debug, Clone)]
pub struct RunEvidence {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// Measured wall clock of this execution — the load-bearing number on
    /// a Slow finding (#1235), informational elsewhere.
    pub duration_secs: f64,
}

impl RunEvidence {
    fn from(p: &super::runner::ProcResult) -> Self {
        RunEvidence {
            stdout: String::from_utf8_lossy(&p.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&p.stderr).into_owned(),
            exit_code: p.exit_code,
            timed_out: p.timed_out,
            duration_secs: p.duration.as_secs_f64(),
        }
    }
}

/// The hang-vs-slow confirm multiplier (#1235): a leg that outruns the
/// per-program budget is re-run ONCE at this multiple of it before being
/// classified. Completing there is an order-of-magnitude perf regression
/// ([`FindingKind::Slow`]) — a real finding, but not nontermination.
/// Outrunning even that stays a [`FindingKind::Hang`]. The re-run only
/// fires on legs that already blew the budget, so its cost lands on the
/// rare suspect, not the campaign; genuinely non-terminating mutants are
/// mostly absorbed EARLIER (double-hang skip, interp fuel skip) and never
/// reach it.
const SLOW_CONFIRM_FACTOR: u32 = 10;

/// Run the full ladder against a program already written to `file`.
/// `wasm_out` is a scratch path for the WASM build artifact. `reference`
/// is the interpreter oracle (the third judge, which abstains freely).
///
/// `expected` is the BY-CONSTRUCTION oracle (#1332): when the generator
/// knows what the program must print, every leg is judged against it
/// individually. That is the only rung here that can convict two legs at
/// once, so it runs before the differential comparison.
pub fn run_ladder(
    tc: &Toolchain,
    source: &str,
    file: &Path,
    wasm_out: &Path,
    reference: Option<&dyn ReferenceOracle>,
    expected: Option<&str>,
) -> Outcome {
    let tc_confirm = tc.with_timeout(tc.timeout * SLOW_CONFIRM_FACTOR);

    // ── Rung (a): check ──
    let check = tc.check(file);
    if check.timed_out {
        // Hung, or merely slow? One confirm re-run decides (#1235). A check
        // that completes only at 10× the budget is a frontend perf finding,
        // not nontermination — the ladder stops either way, because a
        // program the frontend cannot process in budget yields no run legs.
        let check2 = tc_confirm.check(file);
        if !check2.timed_out && !check2.spawn_failed {
            return Outcome::Finding(Finding {
                rung: Rung::Check,
                kind: FindingKind::Slow,
                summary: "almide check outlived the budget but completed at 10x (slow, not hung)"
                    .into(),
                native: Some(RunEvidence::from(&check2)),
                wasm: None,
            });
        }
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

    // ── Rung (c): native build + run — TWO timed steps, so rustc's compile
    // time can never masquerade as the program's run time (see
    // `Toolchain::build_native`). ──
    let native_bin = wasm_out.with_extension("nativebin");
    let nbuild = tc.build_native(file, &native_bin);
    if nbuild.spawn_failed {
        return Outcome::Skipped {
            reason: format!(
                "could not spawn almide: {}",
                String::from_utf8_lossy(&nbuild.stderr)
            ),
        };
    }
    if nbuild.timed_out {
        // rustc outran the budget — a toolchain/load event with no program
        // observable in it. The one-step `almide run` form classified this as
        // a run HANG whenever the (rustc-free) wasm leg finished, minting a
        // phantom nightly-red finding under machine load.
        return Outcome::Skipped {
            reason: "native BUILD timed out (rustc under load) — no program ran, \
                     so there is no hang or divergence oracle"
                .into(),
        };
    }
    if !nbuild.success() {
        // Rung (a) accepted the program, so a real COMPILE failure here is the
        // check-vs-build gap finding. Anything else (a transient cargo/linker
        // failure with no compile diagnostic) has no program observable — skip,
        // matching the one-step form, which only ever minted this finding on
        // the diagnostic markers.
        let stderr = String::from_utf8_lossy(&nbuild.stderr);
        if stderr.contains("Compile error") || stderr.contains("error[E") {
            return Outcome::Finding(Finding {
                rung: Rung::NativeBuild,
                kind: FindingKind::NativeBuildFailure,
                summary: "native build failed after check accepted".into(),
                native: Some(RunEvidence::from(&nbuild)),
                wasm: None,
            });
        }
        return Outcome::Skipped {
            reason: format!(
                "native build failed without a compile diagnostic (toolchain \
                 event): {}",
                stderr.lines().last().unwrap_or("")
            ),
        };
    }
    let native = tc.run_native_bin(&native_bin);
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
                // wasm cleanly succeeded — hung, or merely slow (#1235)? The
                // confirm re-run decides; a completed re-run has real
                // observables, so it flows through the SAME comparison as a
                // normal run (inheriting every skip rule and divergence
                // class), with agreement mapping to Slow instead of Clean.
                let native2 = tc_confirm.run_native_bin(&native_bin);
                if !native2.timed_out && !native2.spawn_failed {
                    return match compare_runs(source, &native2, &wasm_run, reference, expected) {
                        Outcome::Clean { .. } => Outcome::Finding(Finding {
                            rung: Rung::Run,
                            kind: FindingKind::Slow,
                            summary: "native run outlived the budget but completed at 10x, \
                                      byte-identical to wasm (slow, not hung)"
                                .into(),
                            native: Some(RunEvidence::from(&native2)),
                            wasm: Some(RunEvidence::from(&wasm_run)),
                        }),
                        other => other,
                    };
                }
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
    // A RUNTIME failure is NOT a finding by itself — a corpus MUTATION can
    // synthesize a program that ABORTS BY DESIGN (a bounds/div-fixture variant
    // in the mutation pool), and the abort form is itself a cross-target
    // contract (ALS-T6): the ORACLE is the comparison below — wasm must reach
    // the same observables, divergence surfaces there. (Compile failures were
    // classified at the build step above; with the two-step native leg they can
    // no longer be conflated with a program whose OWN stderr contains a
    // compiler-diagnostic-shaped string.)

    // ── Rung (d): wasm build ──
    let wasm_build = tc.build_wasm(file, wasm_out);
    if !wasm_build.success() {
        // An HONEST wall is subset-coverage debt, not a finding — there is
        // no wasm leg to diverge. Anything else (validator failure, panic,
        // missing-wall crash) stays a finding. The marker is the shared
        // `almide::WASM_WALL_MARKER` contract line the CLI emits on every
        // wall path (pinned by tests/wall_shape_rendering_test.rs) — never
        // a locally copied string, which is how the first classifier here
        // silently rotted when #931 reworked the wall diagnostic.
        let stderr = String::from_utf8_lossy(&wasm_build.stderr);
        if let Some(line) =
            stderr.lines().map(str::trim).find(|l| l.starts_with(almide::WASM_WALL_MARKER))
        {
            return Outcome::Walled {
                reason: line[almide::WASM_WALL_MARKER.len()..].to_string(),
            };
        }
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
        // The SYMMETRIC rule to the native-hang skip above: a wasm hang is
        // only evidence when native CLEANLY SUCCEEDED. A mutation can
        // synthesize a genuinely non-terminating program whose two legs
        // merely FAIL at different speeds — native blows its stack in
        // milliseconds while the wasm leg (TCO'd into a loop) grinds past
        // the timeout toward the 4GB ceiling. Both diverge only in failure
        // form/speed — no divergence oracle.
        if !native.success() {
            return Outcome::Skipped {
                reason: "wasm hung while native also failed (a non-terminating or \
                         resource-unbounded program by construction) — no divergence \
                         oracle"
                    .into(),
            };
        }
        // Native SUCCEEDED — but is that termination evidence, or LLVM?
        // #924, seed 1785908634988319229 index 724 seeded
        // `spec/wasm_cross/effect_assign_unwrap.almd`'s `var seq = 0` with
        // `i64::MIN`, making its counting loop run ~9.2e18 times. The loop is
        // effect-free, so LLVM deletes it and computes the exit value
        // analytically: native finishes in 3ms. That is NOT lowerable — the
        // generated cargo project pins `[profile.dev] opt-level = 1` because
        // mutual tail calls become loops only via LLVM's TCO
        // (`src/cli/cargo_build.rs`), so there is no unoptimized native leg to
        // fall back on. The wasm renderer elides nothing, runs the loop, and
        // times out honestly — and the ladder mints a divergence for a
        // program that simply does not terminate.
        //
        // The reference interpreter is the one optimizer-free judge here, so
        // it decides: fuel exhaustion on a program this small means
        // non-termination, and then native's completion is elision, not an
        // oracle. Abstention (`Unsupported`) deliberately does NOT suppress.
        if reference.is_some_and(|r| r.exhausts_fuel(source)) {
            return Outcome::Skipped {
                reason: "wasm hung on a program the reference interpreter cannot \
                         terminate either (native completes only because LLVM elides \
                         the loop) — no termination oracle"
                    .into(),
            };
        }
        // Native succeeded and the interp terminates it, so the program IS
        // finite — hung, or merely slow (#1235)? This is the exact site of
        // the 0.57.0 release-gate misclassification: a quadratic
        // string-concat run (#1229) completed at 21.7s, byte-identical,
        // and was minted a Hang. The fuel skip above runs FIRST so
        // interp-provable non-terminators never pay the 10× re-run.
        let wasm2 = tc_confirm.run_wasm(wasm_out);
        if !wasm2.timed_out && !wasm2.spawn_failed {
            return match compare_runs(source, &native, &wasm2, reference, expected) {
                Outcome::Clean { .. } => Outcome::Finding(Finding {
                    rung: Rung::Run,
                    kind: FindingKind::Slow,
                    summary: "wasm run outlived the budget but completed at 10x, \
                              byte-identical to native (slow, not hung)"
                        .into(),
                    native: Some(RunEvidence::from(&native)),
                    wasm: Some(RunEvidence::from(&wasm2)),
                }),
                other => other,
            };
        }
        return Outcome::Finding(Finding {
            rung: Rung::Run,
            kind: FindingKind::Hang,
            summary: "wasm run hung".into(),
            native: Some(RunEvidence::from(&native)),
            wasm: Some(RunEvidence::from(&wasm)),
        });
    }

    compare_runs(source, &native, &wasm, reference, expected)
}

/// The differential comparison of two COMPLETED runs — every rule between
/// "both legs produced observables" and the verdict. Pure over the two
/// [`ProcResult`]s and the reference oracle (no toolchain), so the confirm
/// re-run (#1235) can reuse it verbatim: a suspect leg that completes at
/// 10× flows through the same skip rules (C-196 stack, C-197 memory) and
/// the same divergence classes as any other run, with `Clean` mapped to
/// `Slow` at the call site.
fn compare_runs(
    source: &str,
    native: &super::runner::ProcResult,
    wasm: &super::runner::ProcResult,
    reference: Option<&dyn ReferenceOracle>,
    expected: Option<&str>,
) -> Outcome {
    // Compare observable behaviour: stdout, exit code, and run-success.
    let nat_ev = RunEvidence::from(native);
    let wasm_ev = RunEvidence::from(wasm);

    // BOTH legs died of CALL-STACK exhaustion (a mutation-synthesized unbounded
    // recursion): native hits Rust's guard page ("fatal runtime error: stack
    // overflow"), wasm traps at its own depth limit — different codes, same
    // non-semantic cause. Stack DEPTH is a resource limit, not an observable
    // the ALS specifies (normalizing it to a T6 abort is the depth-guard
    // follow-up), so there is no divergence oracle here — skip, like the
    // double-hang rule above.
    if !native.success()
        && !wasm.success()
        && String::from_utf8_lossy(&native.stderr).contains("stack overflow")
    {
        return Outcome::Skipped {
            reason: "both legs exhausted the call stack (unbounded recursion by \
                     construction) — depth limits are resource-bound, no semantic oracle"
                .into(),
        };
    }
    // ONE leg exhausted its call stack while the other terminated (C-196): the
    // terminating leg's optimizer legally transformed the unbounded recursion into
    // iteration (LLVM's accumulator TRE on native — Wave 4 finding 57), or vice
    // versa. Stack depth is a RESOURCE limit, not an observable the ALS specifies,
    // so the contracted divergence is a skip — mirroring the both-legs rule above.
    if one_sided_stack_exhaustion(
        native.success(),
        wasm.success(),
        String::from_utf8_lossy(&native.stderr).contains("stack overflow"),
        String::from_utf8_lossy(&wasm.stderr).contains("call stack exhausted"),
    ) {
        return Outcome::Skipped {
            reason: "one leg exhausted its call stack while the other's optimizer \
                     transformed the recursion to termination — the C-196 \
                     resource-limit divergence, not a semantic oracle"
                .into(),
        };
    }
    // C-197, the memory sibling: wasm32 exhausted its linear memory (the DEFINED
    // "Error: out of memory" abort — the $oom primitive, never an OOB fault) while
    // native's 64-bit address space satisfied the same program. A resource limit,
    // not a semantic oracle — mirroring the stack rule above.
    if one_sided_memory_exhaustion(
        native.success(),
        wasm.success(),
        String::from_utf8_lossy(&wasm.stderr).contains("out of memory"),
    ) {
        return Outcome::Skipped {
            reason: "wasm32 exhausted its linear memory (the defined out-of-memory \
                     abort) while native's larger address space satisfied the \
                     program — the C-197 resource-limit divergence, not a \
                     semantic oracle"
                .into(),
        };
    }
    // ── The BY-CONSTRUCTION oracle (#1332) ──
    //
    // This runs BEFORE the differential comparison for one reason: it is
    // the only judge here that can convict both legs at once. The
    // native↔wasm vote is structurally blind to a bug in anything the two
    // legs share (the frontend, almide-mir, the linked IR), because a
    // shared miscompile makes both legs identically wrong and the vote
    // comes back unanimous — the #1322 failure mode. An identity-family
    // program's expected stdout is a literal in its own source, so a leg
    // is judged ALONE and agreement proves nothing.
    //
    // Placed after the resource-limit skips above (C-196 stack, C-197
    // memory) so a wasm32 OOM is still a skip, not a bogus self-check
    // failure.
    if let Some(exp) = expected {
        let nat_ok = native.success() && nat_ev.stdout == exp;
        let wasm_ok = wasm.success() && wasm_ev.stdout == exp;
        if !nat_ok || !wasm_ok {
            let which = match (nat_ok, wasm_ok) {
                // The blind spot, caught: both backends agree on the WRONG
                // answer. No differential oracle can see this.
                (false, false) => "both legs",
                (false, true) => "native",
                (true, false) => "wasm",
                (true, true) => unreachable!("guarded by the branch above"),
            };
            let summary = format!(
                "self-check failed on {which}: {}",
                self_check_diff(exp, if nat_ok { &wasm_ev } else { &nat_ev })
            );
            return Outcome::Finding(Finding {
                rung: Rung::SelfCheck,
                kind: FindingKind::SelfCheckFailure,
                summary,
                native: Some(nat_ev),
                wasm: Some(wasm_ev),
            });
        }
    }

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
                // Name the first differing line: without it the finding's evidence
                // carried both TARGET outputs but never what the interp expected,
                // so adjudicating "which judge is wrong" required rebuilding the
                // oracle by hand (Wave 4 L3's reporting gap).
                return Outcome::Finding(Finding {
                    rung: Rung::Run,
                    kind: FindingKind::OutputDivergence,
                    summary: format!(
                        "both targets disagree with reference interpreter ({})",
                        first_line_diff(&expected, &nat_ev.stdout)
                    ),
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

/// Render a self-check failure (#1332) for the finding summary: the first
/// line where the leg's output departs from the output the program is
/// known by construction to produce, or the failure mode when the leg
/// did not produce clean output at all.
fn self_check_diff(expected: &str, actual: &RunEvidence) -> String {
    if actual.timed_out {
        return "the leg outran the budget without producing its known output".to_string();
    }
    if actual.exit_code != Some(0) {
        let last = actual.stderr.lines().last().unwrap_or("").trim();
        return format!(
            "the leg exited {:?} (expected a clean run): {last:?}",
            actual.exit_code
        );
    }
    for (i, (e, a)) in expected.lines().zip(actual.stdout.lines()).enumerate() {
        if e != a {
            return format!("line {}: expected {e:?} got {a:?}", i + 1);
        }
    }
    format!(
        "line counts differ: expected {} got {}",
        expected.lines().count(),
        actual.stdout.lines().count()
    )
}

/// The first line where the interp's expected stdout and the targets' agreed stdout
/// differ, rendered for a finding summary. A pure helper so the diff logic is
/// unit-testable; falls back to a length note when one output is a prefix of the other.
fn first_line_diff(expected: &str, actual: &str) -> String {
    for (i, (e, a)) in expected.lines().zip(actual.lines()).enumerate() {
        if e != a {
            return format!("line {}: interp={e:?} vs targets={a:?}", i + 1);
        }
    }
    format!(
        "line counts differ: interp={} vs targets={}",
        expected.lines().count(),
        actual.lines().count()
    )
}

#[cfg(test)]
mod first_line_diff_tests {
    use super::first_line_diff;

    #[test]
    fn names_the_first_differing_line() {
        assert_eq!(
            first_line_diff("a\nb\nc\n", "a\nX\nc\n"),
            "line 2: interp=\"b\" vs targets=\"X\""
        );
    }

    #[test]
    fn prefix_case_reports_line_counts() {
        assert_eq!(
            first_line_diff("a\n", "a\nb\n"),
            "line counts differ: interp=1 vs targets=2"
        );
    }
}

/// C-196's decision, extracted pure so it is unit-testable like
/// [`native_hang_is_finding`]: true iff exactly one leg succeeded AND the failing
/// leg's stderr carries its stack-exhaustion signature (`call stack exhausted` on
/// wasmtime, `stack overflow` on the native guard page).
fn one_sided_stack_exhaustion(
    native_ok: bool,
    wasm_ok: bool,
    native_stack_overflow: bool,
    wasm_stack_exhausted: bool,
) -> bool {
    (native_ok && !wasm_ok && wasm_stack_exhausted)
        || (wasm_ok && !native_ok && native_stack_overflow)
}

/// C-197's decision, pure like its stack sibling: true iff wasm failed with the
/// defined out-of-memory abort while native succeeded. (The reverse direction —
/// native OOM while wasm succeeds — has no single stable native signature and
/// stays a finding until one exists; wasm32 being the SMALLER space, the forward
/// direction is the one the resource asymmetry actually produces.)
fn one_sided_memory_exhaustion(native_ok: bool, wasm_ok: bool, wasm_oom: bool) -> bool {
    native_ok && !wasm_ok && wasm_oom
}

#[cfg(test)]
mod memory_exhaustion_classification_tests {
    use super::one_sided_memory_exhaustion;

    #[test]
    fn wasm_oom_native_completed_is_contracted() {
        // Wave 4 L5: ~34 GB of pushes — native's 64-bit space completed,
        // wasm32 aborted with the defined line. C-197 — a skip, not a finding.
        assert!(one_sided_memory_exhaustion(true, false, true));
    }

    #[test]
    fn wasm_failure_without_the_oom_line_is_still_a_finding() {
        assert!(!one_sided_memory_exhaustion(true, false, false));
    }

    #[test]
    fn both_ok_is_not_this_rule() {
        assert!(!one_sided_memory_exhaustion(true, true, false));
    }
}

#[cfg(test)]
mod stack_exhaustion_classification_tests {
    use super::one_sided_stack_exhaustion;

    #[test]
    fn wasm_exhausted_native_terminated_is_contracted() {
        // Wave 4 finding 57: LLVM's accumulator TRE terminated native; wasm
        // recursed faithfully and trapped. C-196 — a skip, not a finding.
        assert!(one_sided_stack_exhaustion(true, false, false, true));
    }

    #[test]
    fn native_exhausted_wasm_terminated_is_contracted() {
        assert!(one_sided_stack_exhaustion(false, true, true, false));
    }

    #[test]
    fn wasm_failure_without_the_signature_is_still_a_finding() {
        assert!(!one_sided_stack_exhaustion(true, false, false, false));
    }

    #[test]
    fn both_ok_is_not_this_rule() {
        assert!(!one_sided_stack_exhaustion(true, true, false, false));
    }
}

#[cfg(test)]
mod compare_runs_tests {
    use super::super::runner::ProcResult;
    use super::{compare_runs, FindingKind, Outcome};

    fn run(stdout: &str, stderr: &str, exit: i32) -> ProcResult {
        ProcResult {
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            exit_code: Some(exit),
            timed_out: false,
            spawn_failed: false,
            duration: std::time::Duration::from_millis(5),
        }
    }

    #[test]
    fn identical_completed_runs_are_clean() {
        // The confirm re-run's agreement case (#1235): the call site maps
        // THIS outcome to a Slow finding, so Clean here is load-bearing.
        let out = compare_runs("", &run("a\n", "", 0), &run("a\n", "", 0), None, None);
        assert!(matches!(out, Outcome::Clean { .. }));
    }

    #[test]
    fn differing_stdout_is_an_output_divergence() {
        let out = compare_runs("", &run("a\n", "", 0), &run("b\n", "", 0), None, None);
        let Outcome::Finding(f) = out else { panic!("expected a finding") };
        assert_eq!(f.kind, FindingKind::OutputDivergence);
    }

    #[test]
    fn wasm_oom_skip_survives_the_extraction() {
        // C-197 must apply to a confirm re-run exactly as to a first run.
        let out = compare_runs(
            "",
            &run("a\n", "", 0),
            &run("", "Error: out of memory", 134),
            None,
            None,
        );
        assert!(matches!(out, Outcome::Skipped { .. }));
    }

    /// THE point of #1332: both legs agree, and both are wrong. Every
    /// differential rule in this function returns `Clean` here — only the
    /// by-construction oracle convicts.
    #[test]
    fn agreeing_legs_that_are_both_wrong_are_convicted() {
        let both = run("a0=99\n", "", 0);
        // Without the oracle: unanimous, therefore clean.
        assert!(matches!(
            compare_runs("", &both, &both, None, None),
            Outcome::Clean { .. }
        ));
        // With it: a self-check failure naming BOTH legs.
        let out = compare_runs("", &both, &both, None, Some("a0=41\n"));
        let Outcome::Finding(f) = out else { panic!("expected a finding") };
        assert_eq!(f.kind, FindingKind::SelfCheckFailure);
        assert!(f.summary.contains("both legs"), "summary: {}", f.summary);
        assert!(f.summary.contains("a0=41"), "summary: {}", f.summary);
    }

    /// A one-sided self-check failure names the guilty leg — that is the
    /// information the 2-way vote never had.
    #[test]
    fn one_sided_self_check_names_the_leg() {
        let out = compare_runs(
            "",
            &run("a0=41\n", "", 0),
            &run("a0=42\n", "", 0),
            None,
            Some("a0=41\n"),
        );
        let Outcome::Finding(f) = out else { panic!("expected a finding") };
        assert_eq!(f.kind, FindingKind::SelfCheckFailure);
        assert!(f.summary.contains("wasm"), "summary: {}", f.summary);
    }

    /// Matching the oracle keeps the program clean — the oracle must not
    /// manufacture findings out of correct runs.
    #[test]
    fn legs_matching_the_oracle_stay_clean() {
        let ok = run("a0=41\n", "", 0);
        assert!(matches!(
            compare_runs("", &ok, &ok, None, Some("a0=41\n")),
            Outcome::Clean { .. }
        ));
    }

    /// The C-197 resource skip still wins over the oracle: wasm32 running
    /// out of linear memory is not a miscompile, and the skip rule runs
    /// first by construction.
    #[test]
    fn resource_skips_still_precede_the_oracle() {
        let out = compare_runs(
            "",
            &run("a0=41\n", "", 0),
            &run("", "Error: out of memory", 134),
            None,
            Some("a0=41\n"),
        );
        assert!(matches!(out, Outcome::Skipped { .. }));
    }
}

#[cfg(test)]
mod self_check_diff_tests {
    use super::{self_check_diff, RunEvidence};

    fn ev(stdout: &str, stderr: &str, exit: i32) -> RunEvidence {
        RunEvidence {
            stdout: stdout.into(),
            stderr: stderr.into(),
            exit_code: Some(exit),
            timed_out: false,
            duration_secs: 0.01,
        }
    }

    #[test]
    fn names_the_first_wrong_line() {
        assert_eq!(
            self_check_diff("a0=41\na1=-7\n", &ev("a0=41\na1=13\n", "", 0)),
            "line 2: expected \"a1=-7\" got \"a1=13\""
        );
    }

    #[test]
    fn a_nonzero_exit_is_reported_as_such() {
        let s = self_check_diff("a0=41\n", &ev("", "Error: division by zero", 1));
        assert!(s.contains("exited Some(1)"), "{s}");
        assert!(s.contains("division by zero"), "{s}");
    }

    #[test]
    fn a_truncated_run_reports_line_counts() {
        assert_eq!(
            self_check_diff("a0=41\na1=-7\n", &ev("a0=41\n", "", 0)),
            "line counts differ: expected 2 got 1"
        );
    }
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
