
// ── The three corpus gates ──
//
// Each keeps its own `#[test]`, its own name and its own assertions; only the
// source of the legs changed (see p4_corpus.rs). The comparison logic below is
// the same as when each gate walked the corpus itself — a refactor that quietly
// weakened one of these would be far worse than the builds it saved, so the
// classification arms were moved verbatim.

#[test]
fn wasm_opt_parity_spec() {
    // `--wasm-opt` must not change an observable. Same corpus as the
    // equivalence gate, and it reuses that gate's plain-wasm leg as its
    // baseline instead of rebuilding it.
    let Some(legs) = corpus() else { return };
    // wasm-opt absent → nothing to compare; the other gates still ran.
    if legs.iter().all(|l| l.wasm_opt.is_none()) {
        eprintln!("wasm_opt_parity_spec: wasm-opt unavailable — skipping");
        return;
    }

    let mut passed = 0;
    let mut failed: Vec<String> = Vec::new();
    for l in legs {
        let Some(opt) = &l.wasm_opt else { continue };
        if &l.wasm == opt {
            passed += 1;
        } else {
            let (pc, pout, perr) = &l.wasm;
            let (oc, oout, oerr) = opt;
            let name = &l.name;
            failed.push(format!(
                "{name}: wasm-opt changed observable behavior\n  plain:     exit={pc} stdout={pout:?} stderr={perr:?}\n  wasm-opt:  exit={oc} stdout={oout:?} stderr={oerr:?}"
            ));
        }
    }

    eprintln!("\nwasm_opt_parity_spec (gate): {passed} equal, {} mismatch(es)", failed.len());
    if !failed.is_empty() {
        panic!("\n{} wasm-opt parity gate problem(s):\n\n{}", failed.len(), failed.join("\n\n"));
    }
}

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

// ── The abstain ledger gate (CG-1 gap audit) ──
//
// Backend-free by design: it evaluates only the interp leg, so it NEVER
// self-skips on a missing almide binary or wasmtime. It deliberately does NOT
// read `corpus()` — touching that table would trigger 318 native+wasm builds
// and destroy exactly the property that makes this gate trustworthy.

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross")
}

/// The committed inventory of fixtures the interpreter cannot evaluate.
fn ledger_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/almide-interp/interp-abstain-ledger.txt")
}

/// Coverage audit for the executable spec: runs ONLY the interp leg over the
/// cross-target corpus (no almide binary, no wasmtime — this gate never
/// self-skips on CI) and holds the observed abstain set equal to the committed
/// ledger, in both directions:
///
///   - a fixture the interp cannot evaluate but absent from the ledger FAILS —
///     coverage shrinkage must be a reviewed ledger edit in the same PR, never
///     a silent drift (the documented weakness this gate exists to close);
///   - a ledger entry whose fixture now evaluates (or was renamed/removed)
///     FAILS — stale entries hide progress; the ledger may only shrink.
///
/// The ledger never decides WHAT is skipped (skips stay interp-self-reported);
/// it only audits the set. Regenerate after a deliberate change with
/// `ALMIDE_UPDATE_INTERP_LEDGER=1` and review the diff.
#[test]
fn interp_abstain_ledger() {
    let dir = spec_dir();
    if !dir.exists() {
        eprintln!(
            "interp_abstain_ledger: {} missing — skipping",
            dir.display()
        );
        return;
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "almd").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());
    if entries.is_empty() {
        eprintln!("interp_abstain_ledger: corpus empty — skipping");
        return;
    }

    let total = entries.len();
    // fixture stem → first-line reason, in corpus order
    let mut observed: Vec<(String, String)> = Vec::new();
    for entry in &entries {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let source = std::fs::read_to_string(&path).unwrap();
        if let InterpLeg::Skip(reason) = run_interp_capture(&source) {
            observed.push((name, reason.replace('\n', " ")));
        }
    }

    if std::env::var("ALMIDE_UPDATE_INTERP_LEDGER").is_ok() {
        let mut out = String::from(
            "# interp-abstain-ledger — fixtures of spec/wasm_cross/ the reference\n\
             # interpreter cannot evaluate (its self-reported coverage gaps), i.e. the\n\
             # current boundary of the executable spec. CG-1 gap audit; shrink to zero.\n\
             #\n\
             # Format: <fixture-stem>  <reason as last observed>  (first token is the key)\n\
             # Gate:   wasm_runtime_test.rs::interp_abstain_ledger — fails on a\n\
             #         new abstain missing here AND on a stale entry that now evaluates.\n\
             # Regenerate (then review the diff!):\n\
             #   ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test --test wasm_runtime_test interp_abstain_ledger\n\
             # Preferred alternative to adding an entry: widen the interp glue\n\
             # (bridge.rs / hofs.rs / dispatch.rs — see crates/almide-interp/CLAUDE.md).\n\n",
        );
        for (n, r) in &observed {
            out.push_str(&format!("{n}  {r}\n"));
        }
        std::fs::write(ledger_path(), out).unwrap();
        eprintln!(
            "interp_abstain_ledger: regenerated with {} abstain(s) of {} fixtures — review the diff",
            observed.len(),
            total
        );
        return;
    }

    let ledger_text = std::fs::read_to_string(ledger_path()).unwrap_or_else(|_| {
        panic!(
            "interp-abstain-ledger.txt missing at {} — seed it with \
             ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test --test wasm_runtime_test interp_abstain_ledger",
            ledger_path().display()
        )
    });
    let ledger: std::collections::BTreeSet<String> = ledger_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    let observed_set: std::collections::BTreeSet<String> =
        observed.iter().map(|(n, _)| n.clone()).collect();

    let new_abstains: Vec<&(String, String)> = observed
        .iter()
        .filter(|(n, _)| !ledger.contains(n))
        .collect();
    let stale: Vec<&String> = ledger
        .iter()
        .filter(|n| !observed_set.contains(*n))
        .collect();

    eprintln!(
        "\ninterp_abstain_ledger (executable-spec coverage): {} fixtures | {} evaluated | {} abstained (ledgered)",
        total,
        total - observed.len(),
        observed.len()
    );

    let mut failures = String::new();
    if !new_abstains.is_empty() {
        failures.push_str(&format!(
            "\nUNLEDGERED ABSTAIN(S) — the interpreter cannot evaluate {} fixture(s) not \
             recorded in interp-abstain-ledger.txt:\n",
            new_abstains.len()
        ));
        for (n, r) in &new_abstains {
            failures.push_str(&format!("    - {n}: {r}\n"));
        }
        failures.push_str(
            "  Preferred fix: widen the interp glue so the fixture evaluates \
             (bridge.rs / hofs.rs / dispatch.rs — see crates/almide-interp/CLAUDE.md).\n  \
             Otherwise: record the abstention in the ledger IN THIS SAME PR \
             (ALMIDE_UPDATE_INTERP_LEDGER=1 regenerates) — shrinking the executable \
             spec's coverage is a reviewed decision, never a silent drift.\n",
        );
    }
    if !stale.is_empty() {
        failures.push_str(&format!(
            "\nSTALE LEDGER ENTRY(IES) — {} ledgered fixture(s) no longer abstain \
             (now evaluated, renamed, or removed):\n",
            stale.len()
        ));
        for n in &stale {
            failures.push_str(&format!("    - {n}\n"));
        }
        failures.push_str(
            "  Remove the entries (ALMIDE_UPDATE_INTERP_LEDGER=1 regenerates) — \
             the ledger may only shrink toward zero.\n",
        );
    }
    if !failures.is_empty() {
        panic!("{failures}");
    }
}
