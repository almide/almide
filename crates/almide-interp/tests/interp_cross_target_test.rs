//! The 3-way cross-target oracle.
//!
//! `spec/wasm_cross/*.almd` is the cross-target observable-equivalence corpus:
//! every program must produce a byte-identical `(stdout, stderr, exit)` on the
//! native (Rust) and WASM backends. That 2-way vote — gated by
//! `tests/wasm_runtime_test.rs::wasm_cross_target_spec` — is structurally blind
//! to a *both-wrong-the-same-way* bug: if codegen and the WASM emitter share a
//! lowering pass that is wrong, both agree and the gate stays green while the
//! observed behaviour is wrong.
//!
//! This test adds the interpreter as a third, independent judge. The interp
//! runs the IR at the pre-codegen cut point (`lower → optimize → mono →
//! ir_link`), so it shares *none* of `almide-codegen`'s target-lowering passes
//! with either backend. When all three agree, the result is corroborated by an
//! executable spec that cannot share a codegen bug with the backends. When the
//! interp disagrees with a native==wasm consensus, exactly one of two things is
//! true and the test says which:
//!
//!   (a) the interpreter is wrong  → fix the interpreter, or
//!   (b) the interpreter is right  → we just found a both-backends-wrong bug
//!       that the 2-way gate is blind to. This is the entire point of the
//!       third judge, so it is REPORTED LOUDLY (the test fails with a
//!       `BOTH-BACKENDS-WRONG` banner).
//!
//! Skips are data-driven, never silent. A fixture is skipped only when the
//! interpreter itself reports it cannot run that program — either it fails to
//! lower at the interp's lightweight cut point (no stdlib bodies / unresolved
//! `import json|regex|…`), or evaluation reaches an out-of-interp-scope
//! capability (`RunStatus::Unsupported`) or the fuel budget
//! (`RunStatus::FuelExhausted`). Every skip is logged with its concrete reason
//! and the skip count is printed; there is no hardcoded skip-list to drift out
//! of date.
//!
//! The skip SET is additionally audited by `interp_abstain_ledger` (below)
//! against the committed `interp-abstain-ledger.txt`. The ledger does NOT
//! drive skipping (skips stay self-reported, per the rule above); it makes
//! coverage shrinkage a reviewed decision instead of a silent drift: a fixture
//! abstaining without a ledger entry fails, and a ledger entry whose fixture
//! no longer abstains fails (the ledger may only shrink toward zero). This is
//! the CG-1 gap-audit ratchet — the evaluable boundary of the executable spec,
//! as a file.
//!
//! Requires the workspace `almide` binary (built `--release`) for the native /
//! WASM legs and `wasmtime` for the WASM run. If either is absent the test
//! self-skips cleanly (matching the existing wasm_runtime harness behaviour),
//! so it never spuriously fails a machine without the toolchain.

use std::path::{Path, PathBuf};
use std::process::Command;

use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_interp::{Interpreter, RunStatus};
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_optimize::{mono, optimize};

// ── Toolchain location (mirrors tests/wasm_runtime_test.rs::almide_bin) ──

/// Path to the `almide` binary used for the native + WASM legs.
/// Order: `ALMIDE_BIN` env → workspace `target/release/almide` → `almide` on PATH.
fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    // CARGO_MANIFEST_DIR here is crates/almide-interp; the workspace target dir
    // is two levels up.
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/wasm_cross")
}

// ── Leg 1: native backend (compile to a binary, then run it) ──
// Identical shape to wasm_runtime_test.rs::run_native_capture: build to a binary
// FIRST (compiler diagnostics discarded), then run it, so the captured stderr is
// the PROGRAM's runtime stderr — not the compiler's warnings — matching the wasm
// path (build then wasmtime). `almide run` would mix compile-time warnings into
// stderr and spuriously diverge.
fn run_native_capture(source: &str) -> (i32, String, String) {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("test.almd");
    let bin_path = dir.path().join("test_native_bin");
    std::fs::write(&src_path, source).unwrap();
    let build = Command::new(almide_bin())
        // ISOLATE the native build scratch dir. `almide build` (native) compiles
        // generated Rust inside `std::env::temp_dir().join("almide-build")` — a
        // SHARED dir guarded only by a cross-process flock held through
        // build+copy. When other test processes (e.g. the 2-way
        // `wasm_cross_target_spec` gate) run `almide build` in parallel, the
        // shared `src/main.rs`+`target/` can get cross-contaminated and a fixture
        // spuriously fails to compile with another fixture's types (observed:
        // `inff64`, `Box<Tree>` leaking across fixtures). Pointing `TMPDIR` at a
        // unique per-invocation dir gives this build its own scratch, removing
        // the race so the third judge is deterministic.
        .env("TMPDIR", dir.path())
        .args([
            "build",
            src_path.to_str().unwrap(),
            "-o",
            bin_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to build native");
    if !build.status.success() {
        return (
            build.status.code().unwrap_or(-1),
            String::new(),
            String::from_utf8_lossy(&build.stderr).trim().to_string(),
        );
    }
    let out = Command::new(&bin_path)
        .output()
        .expect("failed to run native binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

// ── Leg 2: WASM backend (compile to wasm, run via wasmtime) ──
// `None` if wasmtime is unavailable (the whole gate then self-skips). Mirrors
// wasm_runtime_test.rs::run_wasm_capture.
fn run_wasm_capture(source: &str) -> Option<(i32, String, String)> {
    let dir = tempfile::tempdir().unwrap();
    let src_path = dir.path().join("test.almd");
    let wasm_path = dir.path().join("test.wasm");
    std::fs::write(&src_path, source).unwrap();
    let build = Command::new(almide_bin())
        // Isolate scratch (see run_native_capture). The wasm path uses bare
        // rustc into the output dir, but pinning TMPDIR keeps any incidental
        // temp use off the shared dir too.
        .env("TMPDIR", dir.path())
        .args([
            "build",
            src_path.to_str().unwrap(),
            "--target",
            "wasm",
            "-o",
            wasm_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to build wasm");
    assert!(
        build.status.success(),
        "wasm build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    match Command::new("wasmtime")
        .arg("--dir=/")
        .arg(wasm_path.to_str().unwrap())
        .output()
    {
        Ok(o) if o.status.code() != Some(127) => Some((
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::from_utf8_lossy(&o.stderr).trim().to_string(),
        )),
        _ => None,
    }
}

// ── Leg 3: the interpreter (in-process, no codegen) ──

/// The interp leg's outcome. Either an observable `(exit, stdout, stderr)`
/// 3-tuple to vote with, or a `Skip(reason)` meaning the interpreter cannot run
/// this fixture — NOT a divergence.
enum InterpLeg {
    Ran(i32, String, String),
    Skip(String),
}

/// Lower `source` to a linked `IrProgram` at the interpreter's cut point
/// (`lower → optimize → mono → ir_link`) with NO stdlib bodies loaded — the same
/// lightweight `canonicalize(.., iter::empty())` recipe the interp's eval_test
/// uses. Returns `Err(reason)` (rather than panicking) when the program does not
/// parse / typecheck at this cut point, so the harness can record it as a clean,
/// reasoned skip (e.g. `import json` is unresolved without the json module
/// source). All of parse / check / lower run under `catch_unwind` so an internal
/// `assert`/`unwrap` in the frontend on an out-of-scope construct degrades to a
/// skip instead of crashing the whole gate.
fn lower_for_interp(source: &str) -> Result<almide_ir::IrProgram, String> {
    let src = source.to_string();
    let result = std::panic::catch_unwind(move || {
        let tokens = Lexer::tokenize(&src);
        let mut parser = Parser::new(tokens);
        let mut prog = match parser.parse() {
            Ok(p) => p,
            Err(e) => return Err(format!("parse error: {:?}", e)),
        };
        if !parser.errors.is_empty() {
            return Err(format!("parse errors: {:?}", parser.errors));
        }

        let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
        let mut checker = Checker::from_env(canon.env);
        let diags = checker.infer_program(&mut prog);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
            .map(|d| d.message.clone())
            .collect();
        if !errors.is_empty() {
            // The common case: an `import json|regex|…` fixture whose module
            // source is not loaded by the empty-stdlib recipe. A reasoned skip,
            // not a divergence.
            return Err(format!("type errors at interp cut point: {:?}", errors));
        }

        let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
        optimize::optimize_program(&mut ir);
        mono::monomorphize(&mut ir);
        ir_link::ir_link(&mut ir);
        Ok(ir)
    });
    match result {
        Ok(r) => r,
        Err(_) => Err("interp lowering panicked (out-of-scope construct)".to_string()),
    }
}

/// Run the fixture through the interpreter. Maps the interpreter's own
/// self-reported scope limits to a `Skip`; everything else is a real third vote.
/// stdout/stderr are `.trim()`-ed to match the native/wasm legs' comparison.
fn run_interp_capture(source: &str) -> InterpLeg {
    let ir = match lower_for_interp(source) {
        Ok(ir) => ir,
        Err(reason) => return InterpLeg::Skip(reason),
    };
    // The interpreter is single-shot per program; catch a defensive panic so an
    // evaluator bug surfaces as a loud skip rather than poisoning the gate.
    let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Interpreter::new(&ir).run_main()
    })) {
        Ok(o) => o,
        Err(_) => return InterpLeg::Skip("interp evaluation panicked".to_string()),
    };
    match &outcome.status {
        RunStatus::Ok | RunStatus::Aborted => InterpLeg::Ran(
            outcome.exit_code(),
            outcome.stdout.trim().to_string(),
            outcome.stderr.trim().to_string(),
        ),
        RunStatus::Unsupported(what) => {
            InterpLeg::Skip(format!("out-of-interp-scope capability: {what}"))
        }
        RunStatus::FuelExhausted => {
            InterpLeg::Skip("interp fuel/recursion budget exhausted".to_string())
        }
    }
}

// ── The 3-way gate ──

#[test]
fn interp_cross_target_spec() {
    // Self-skip on a toolchain without the binary or wasmtime (CI without
    // `make install`, or a machine lacking wasmtime) — same posture as the
    // existing wasm cross gate. The interp leg alone is meaningless without the
    // two backend legs to judge against.
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        eprintln!("interp_cross_target_spec: almide binary unavailable — skipping");
        return;
    }
    if Command::new("wasmtime").arg("--version").output().is_err() {
        eprintln!("interp_cross_target_spec: wasmtime unavailable — skipping");
        return;
    }

    let dir = spec_dir();
    if !dir.exists() {
        eprintln!(
            "interp_cross_target_spec: {} missing — skipping",
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
        eprintln!("interp_cross_target_spec: corpus empty — skipping");
        return;
    }

    let total = entries.len();
    let mut agreed = 0usize; // interp == native == wasm
    let mut skipped: Vec<(String, String)> = Vec::new(); // (fixture, reason)
    // interp disagrees with a native==wasm consensus → a both-backends-wrong
    // suspect (or an interp bug). The headline of this whole test.
    let mut both_backends_wrong: Vec<String> = Vec::new();
    // native != wasm: a 2-way divergence already owned by wasm_runtime_test's
    // gate. We do NOT fail here (that would double-report and fight the
    // @xt-allow ratchet); instead the interp casts a tie-breaking vote and we
    // log which backend it corroborates, as a diagnostic aid for that gate.
    let mut backend_split: Vec<String> = Vec::new();

    for entry in &entries {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let source = std::fs::read_to_string(&path).unwrap();

        let interp = run_interp_capture(&source);
        let (ic, iout, ierr) = match interp {
            InterpLeg::Ran(c, o, e) => (c, o, e),
            InterpLeg::Skip(reason) => {
                skipped.push((name, reason));
                continue;
            }
        };

        let (nc, nout, nerr) = run_native_capture(&source);
        let wasm = match std::panic::catch_unwind(|| run_wasm_capture(&source)) {
            Ok(Some(w)) => w,
            Ok(None) => {
                // wasmtime vanished mid-run; abandon the gate cleanly rather
                // than half-reporting.
                eprintln!("interp_cross_target_spec: wasmtime unavailable mid-run — skipping");
                return;
            }
            Err(_) => {
                // A WASM build/run panic is a backend bug, not an interp skip.
                // Surface it as a backend split (native is the reference) so it
                // is never swallowed.
                backend_split.push(format!(
                    "{name}: WASM build/run panicked; interp={ic}/{iout:?} native={nc}/{nout:?}"
                ));
                continue;
            }
        };
        let (wc, wout, werr) = wasm;

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
            // native != wasm: owned by wasm_runtime_test's @xt-allow gate. The
            // interp breaks the tie; we report which backend it sides with.
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

/// The committed inventory of fixtures the interpreter cannot evaluate.
fn ledger_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("interp-abstain-ledger.txt")
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
             # Gate:   interp_cross_target_test.rs::interp_abstain_ledger — fails on a\n\
             #         new abstain missing here AND on a stale entry that now evaluates.\n\
             # Regenerate (then review the diff!):\n\
             #   ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test -p almide-interp interp_abstain_ledger\n\
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
             ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test -p almide-interp interp_abstain_ledger",
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
