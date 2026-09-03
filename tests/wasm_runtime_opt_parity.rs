//! The wasm-opt parity GATE: `--wasm-opt` must not change an observable.
//! Every `spec/wasm_cross/*.almd` fixture is built plain and with `--wasm-opt`,
//! and the two wasm legs must agree on (stdout, stderr, exit code).
//!
//! One binary per corpus gate (wasm_runtime_test_parts/corpus.rs says why):
//! this one, `wasm_runtime_cross_target` and `wasm_runtime_interp_oracle`
//! share the corpus SOURCE and each build their own table, so the CI shard
//! packer can place each corpus build on its own runner.
//!
//! Requires: the `almide` binary, wasmtime and `wasm-opt` (binaryen); without
//! wasm-opt the gate self-skips (CI sets `ALMIDE_EXPECT_TOOLS` so a missing
//! tool fails the tripwire instead).

// common.rs and corpus.rs serve every wasm_runtime_* binary; this gate reads
// only the plain-wasm and wasm-opt legs.
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
