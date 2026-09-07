//! The cross-target observable-equivalence GATE: every `spec/wasm_cross/*.almd`
//! fixture must produce byte-identical (stdout, stderr, exit code) on the
//! native and wasm targets. Native is the reference; a `// @xt-allow:` line
//! marks a tracked divergence (logged, never silently ignored, flagged once it
//! goes stale).
//!
//! One binary per corpus gate (wasm_runtime_test_parts/corpus.rs says why):
//! this one, `wasm_runtime_opt_parity` and `wasm_runtime_interp_oracle` share
//! the corpus SOURCE and each build their own table, so the CI shard packer
//! can place each corpus build on its own runner.
//!
//! Requires: the `almide` binary (`ALMIDE_BIN`, else target/release/almide)
//! and wasmtime; the gate self-skips without them (CI sets
//! `ALMIDE_EXPECT_TOOLS` so a missing tool fails the tripwire instead).

// common.rs and corpus.rs serve every wasm_runtime_* binary; this gate reads
// only the native and wasm legs.
#![allow(dead_code)]

include!("wasm_runtime_test_parts/common.rs");
include!("wasm_runtime_test_parts/interp_leg.rs");
include!("wasm_runtime_test_parts/corpus.rs");

// ── Data-driven discovery test ──
// Scans spec/wasm_cross/*.almd, runs each on both targets, compares output.

#[test]
fn wasm_cross_target_spec() {
    // The cross-target observable-equivalence GATE (ratchet). Every program in
    // spec/wasm_cross/ must produce byte-identical (stdout, stderr, exit code) on
    // native and wasm. A `// @xt-allow: <reason + tracking ref>` line marks a
    // KNOWN / intentional divergence: it is exempt from the equality assertion but
    // LOGGED, so a divergence is never silently ignored — and once it is fixed the
    // gate flags the now-stale allow so the entry gets removed. Native is the
    // reference; native==wasm is a hard invariant, not a "target difference".
    // The legs come from the corpus table (corpus.rs), built once per binary.
    // The classification below is unchanged.
    let Some(legs) = corpus() else { return };

    let mut passed = 0;
    let mut allowed: Vec<String> = Vec::new();
    let mut stale: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();

    for l in legs {
        let name = &l.name;
        let (rc, rout, rerr) = (l.native.0, &l.native.1, &l.native.2);
        // The corpus records a wasm build/run panic — or a mid-run wasmtime
        // spawn failure — as a sentinel leg so each gate reports it in its own
        // words. Never a whole-gate return: that discarded the rest of the
        // corpus and every accumulated failure as a green pass (#991).
        if l.wasm.0 == i32::MIN && l.wasm.1 == "<panicked>" {
            failed.push(format!("{name}: WASM build/run panicked"));
            continue;
        }
        if l.wasm.0 == i32::MIN && l.wasm.1 == "<wasmtime-spawn-failed>" {
            failed.push(format!("{name}: wasmtime could not be spawned mid-run"));
            continue;
        }
        let (wc, wout, werr) = (l.wasm.0, &l.wasm.1, &l.wasm.2);
        let equal = rc == wc && rout == wout && rerr == werr;

        match (equal, l.allow.as_ref()) {
            (true, None) => passed += 1,
            (true, Some(r)) => stale.push(format!("{name}: @xt-allow now MATCHES (was: {r}) — remove the directive")),
            (false, Some(r)) => allowed.push(format!("{name}: {r}")),
            (false, None) => failed.push(format!(
                "{name}: cross-target divergence\n  native: exit={rc} stdout={rout:?} stderr={rerr:?}\n  wasm:   exit={wc} stdout={wout:?} stderr={werr:?}")),
        }
    }

    eprintln!(
        "\nwasm_cross_target_spec (gate): {passed} equal, {} tracked-divergence(s), {} stale-allow(s), {} unexpected",
        allowed.len(), stale.len(), failed.len()
    );
    for a in &allowed { eprintln!("  ~ tracked: {a}"); }

    let mut problems = failed;
    problems.extend(stale);
    if !problems.is_empty() {
        panic!("\n{} cross-target gate problem(s):\n\n{}", problems.len(), problems.join("\n\n"));
    }
}
