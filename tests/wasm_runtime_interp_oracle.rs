//! The 3-way oracle GATE: the reference interpreter (which evaluates the
//! linked IR before any target lowering and shares no codegen pass with either
//! backend) must agree with the native == wasm consensus on every
//! `spec/wasm_cross/*.almd` fixture it can evaluate. A dissent is a
//! both-backends-wrong suspect — the bug class the 2-way gate is blind to. See
//! crates/almide-interp/CLAUDE.md.
//!
//! One binary per corpus gate (wasm_runtime_test_parts/corpus.rs says why):
//! this one, `wasm_runtime_cross_target` and `wasm_runtime_opt_parity` share
//! the corpus SOURCE and each build their own table, so the CI shard packer
//! can place each corpus build on its own runner.
//!
//! Requires: the `almide` binary and wasmtime for the native and wasm legs;
//! the gate self-skips without them (CI sets `ALMIDE_EXPECT_TOOLS` so a
//! missing tool fails the tripwire instead).

// common.rs and corpus.rs serve every wasm_runtime_* binary; this gate reads
// the native, wasm and interp legs.
#![allow(dead_code)]

include!("wasm_runtime_test_parts/common.rs");
include!("wasm_runtime_test_parts/interp_leg.rs");
include!("wasm_runtime_test_parts/corpus.rs");

// ── The corpus gate ──
//
// Keeps its own `#[test]`, its own name and its own assertions; only the
// source of the legs changed (see corpus.rs). The comparison logic below is
// the same as when the gate walked the corpus itself — a refactor that quietly
// weakened it would be far worse than the builds it saved, so the
// classification arms were moved verbatim.

#[test]
fn interp_cross_target_spec() {
    // The 3-way oracle. A 2-way native==wasm vote is structurally blind to a
    // bug both backends share through a common lowering pass; the interpreter
    // evaluates the linked IR BEFORE any target lowering, so its vote is
    // independent. See crates/almide-interp/CLAUDE.md.
    let Some(legs) = corpus() else {
        eprintln!("interp_cross_target_spec: toolchain unavailable — skipping");
        return;
    };
    let total = legs.len();

    let mut agreed = 0usize; // interp == native == wasm
    let mut skipped: Vec<(String, String)> = Vec::new(); // (fixture, reason)
    // interp disagrees with a native==wasm consensus → a both-backends-wrong
    // suspect (or an interp bug). The headline of this whole test.
    let mut both_backends_wrong: Vec<String> = Vec::new();
    // native != wasm: a 2-way divergence already owned by wasm_cross_target_spec.
    // We do NOT fail here (that would double-report and fight the @xt-allow
    // ratchet); instead the interp casts a tie-breaking vote and we log which
    // backend it corroborates, as a diagnostic aid for that gate.
    let mut backend_split: Vec<String> = Vec::new();

    for l in legs {
        let name = &l.name;
        let (ic, iout, ierr) = match &l.interp {
            InterpLeg::Ran(c, o, e) => (*c, o.clone(), e.clone()),
            InterpLeg::Skip(reason) => {
                skipped.push((name.clone(), reason.clone()));
                continue;
            }
        };
        let (nc, nout, nerr) = (&l.native.0, &l.native.1, &l.native.2);
        let (nc, nout, nerr) = (*nc, nout.clone(), nerr.clone());
        let (wc, wout, werr) = (l.wasm.0, l.wasm.1.clone(), l.wasm.2.clone());

        let native_wasm_agree = nc == wc && nout == wout && nerr == werr;
        let interp_matches_native = ic == nc && iout == nout && ierr == nerr;

        if native_wasm_agree {
            if interp_matches_native {
                agreed += 1;
            } else {
                // The load-bearing case. Native and WASM agree, the interp — an
                // independent spec sharing no codegen pass with them — disagrees.
                // Either both backends are wrong the same way, or the interp is.
                both_backends_wrong.push(format!(
                    "{name}:\n  interp:  exit={ic} stdout={iout:?} stderr={ierr:?}\n  \
                     native:  exit={nc} stdout={nout:?} stderr={nerr:?}\n  \
                     wasm:    exit={wc} stdout={wout:?} stderr={werr:?}\n  \
                     → native==wasm consensus, interp dissents. Diagnose: is the \
                     interpreter wrong (fix it) or is this a BOTH-BACKENDS-WRONG bug?"
                ));
            }
        } else {
            // native != wasm: owned by the @xt-allow gate. The interp breaks the
            // tie; we report which backend it sides with.
            let sides_with = if interp_matches_native {
                "native"
            } else if ic == wc && iout == wout && ierr == werr {
                "wasm"
            } else {
                "neither"
            };
            backend_split.push(format!(
                "{name}: native!=wasm (owned by wasm_cross gate); interp sides with {sides_with}\n  \
                 interp:  exit={ic} stdout={iout:?} stderr={ierr:?}\n  \
                 native:  exit={nc} stdout={nout:?} stderr={nerr:?}\n  \
                 wasm:    exit={wc} stdout={wout:?} stderr={werr:?}"
            ));
        }
    }

    // ── Honest, loud reporting ──
    eprintln!(
        "\ninterp_cross_target_spec (3-way oracle): {total} fixtures | \
         {agreed} interp==native==wasm | {} skipped | {} backend-split | {} interp-dissent",
        skipped.len(),
        backend_split.len(),
        both_backends_wrong.len()
    );
    eprintln!("\n  Skips (interpreter self-reported out-of-scope — never silent):");
    for (n, r) in &skipped {
        eprintln!("    - {n}: {r}");
    }
    if !backend_split.is_empty() {
        eprintln!(
            "\n  Backend splits (native!=wasm; owned by wasm_cross gate, interp tie-break logged):"
        );
        for s in &backend_split {
            eprintln!("    ~ {s}");
        }
    }

    if !both_backends_wrong.is_empty() {
        panic!(
            "\n\n========================================================================\n\
             BOTH-BACKENDS-WRONG SUSPECT(S): the interpreter (an independent spec\n\
             sharing no codegen pass with either backend) dissents from a\n\
             native==wasm consensus on {} fixture(s). The 2-way gate is blind to\n\
             this. For EACH: decide is the interpreter wrong (fix it) or did we\n\
             just find a bug both backends share (report + fix the backends)?\n\
             ========================================================================\n\n{}\n",
            both_backends_wrong.len(),
            both_backends_wrong.join("\n\n")
        );
    }
}
