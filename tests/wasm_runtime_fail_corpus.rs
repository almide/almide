//! The failure-side corpus GATE (#1411 stage 1): every `spec/wasm_fail/*.almd`
//! fixture must TERMINATE UNSUCCESSFULLY on both targets, the declared way
//! (`// @expect-fail:`), and break the SAME way on both — the section comment
//! below carries the format.
//!
//! Split out of the former `wasm_runtime_test` binary with the other corpus
//! gates so the CI shard packer (scripts/ci-test-shard.sh) can spread them
//! (wasm_runtime_test_parts/corpus.rs says why).
//!
//! Requires: the `almide` binary and wasmtime; the gate self-skips without
//! them (CI sets `ALMIDE_EXPECT_TOOLS` so a missing tool fails the tripwire).

// common.rs serves every wasm_runtime_* binary; this gate uses only the
// capture legs.
#![allow(dead_code)]

include!("wasm_runtime_test_parts/common.rs");

// ── The failure-side corpus (#1411 stage 1) ──
//
// `spec/wasm_cross/` asks "do the two legs return the same VALUE"; this gate
// asks "do the two legs BREAK the same way". The distinction exists because a
// fixture written alongside a fix exercises the path the author just made work
// — measured 2026-08-14, only 41 of 418 wasm_cross fixtures reach an error
// path at all, and the three runtime bugs the fuzzer found that week (#1400,
// #1408, #1410) all lived on the side no fixture ran. The mature compilers all
// grew this category: Zig's `test/cases/safety/` (104 programs that PASS only
// if the expected panic fires), rustc's `run-fail` mode + `error-pattern`.
// Survey: ../almide-references/RESEARCH-failure-corpus.md.
//
// Format, one judgement per directory (Zig's compile_errors/safety split):
//   // @expect-fail: <substring of stderr>
//       Mandatory. The program must TERMINATE UNSUCCESSFULLY on both legs
//       (nonzero exit), with this substring in stderr, and with the SAME
//       stderr and exit code on both legs. Running to success is a FAILING
//       test — the Zig rule that falling through to the end is itself the
//       failure.
//   // @xf-allow: <reason + tracking ref>
//       A KNOWN cross-leg divergence in how the failure manifests (#1410:
//       wasm swallows the err entirely). Exempt from the leg-equality half,
//       still logged; when the legs re-agree the gate flags the entry as
//       stale so it gets removed. Mirrors wasm_cross's @xt-allow exactly.

#[test]
fn wasm_fail_corpus() {
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        return;
    }
    if Command::new("wasmtime").arg("--version").output().is_err() {
        return;
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_fail");
    if !dir.exists() {
        return;
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "almd").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());
    assert!(
        !entries.is_empty(),
        "spec/wasm_fail exists but holds no fixtures — an empty failure corpus \
         is the 9.8% problem this gate was built to end (#1411)"
    );

    let mut passed = 0;
    let mut allowed: Vec<String> = Vec::new();
    let mut stale: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();

    for e in entries {
        let path = e.path();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(&path).unwrap();

        let Some(expect) = source
            .lines()
            .find_map(|l| l.trim().strip_prefix("// @expect-fail:").map(|r| r.trim().to_string()))
        else {
            failed.push(format!(
                "{name}: missing `// @expect-fail:` header — a wasm_fail fixture must \
                 declare the failure it exists to demonstrate"
            ));
            continue;
        };
        let allow = source
            .lines()
            .find_map(|l| l.trim().strip_prefix("// @xf-allow:").map(|r| r.trim().to_string()));

        let (nc, nout, nerr) = run_native_capture(&source);
        let wasm = match std::panic::catch_unwind(|| run_wasm_capture(&source)) {
            Ok(Some(w)) => w,
            Ok(None) => {
                failed.push(format!("{name}: wasmtime could not be spawned mid-run"));
                continue;
            }
            Err(_) => {
                failed.push(format!("{name}: WASM build/run panicked"));
                continue;
            }
        };
        let (wc, wout, werr) = (wasm.0, &wasm.1, &wasm.2);

        // Half 1 — the NATIVE reference must actually fail, the declared way.
        // A fixture that runs to success is a failing test (the Zig rule): it
        // means the failure it documents no longer fires, and the file must be
        // updated or retired rather than passing vacuously.
        let native_fails_right = nc != 0 && nerr.contains(&expect);
        if !native_fails_right {
            failed.push(format!(
                "{name}: the native leg did not fail as declared\n  \
                 expected: nonzero exit, stderr containing {expect:?}\n  \
                 got: exit={nc} stdout={nout:?} stderr={nerr:?}"
            ));
            continue;
        }

        // Half 2 — both legs break the SAME way (exit + stdout + stderr),
        // unless a tracked @xf-allow names the known divergence.
        let legs_equal = nc == wc && nout == *wout && nerr == *werr;
        match (legs_equal, allow.as_ref()) {
            (true, None) => passed += 1,
            (true, Some(r)) => stale.push(format!(
                "{name}: @xf-allow now MATCHES (was: {r}) — remove the directive"
            )),
            (false, Some(r)) => allowed.push(format!("{name}: {r}")),
            (false, None) => failed.push(format!(
                "{name}: the two legs break DIFFERENTLY\n  \
                 native: exit={nc} stdout={nout:?} stderr={nerr:?}\n  \
                 wasm:   exit={wc} stdout={wout:?} stderr={werr:?}"
            )),
        }
    }

    eprintln!(
        "\nwasm_fail_corpus (gate): {passed} fail-identically, {} tracked-divergence(s), {} stale-allow(s), {} problem(s)",
        allowed.len(),
        stale.len(),
        failed.len()
    );
    for a in &allowed {
        eprintln!("  ~ tracked: {a}");
    }

    let mut problems = failed;
    problems.extend(stale);
    if !problems.is_empty() {
        panic!(
            "\n{} wasm_fail gate problem(s):\n\n{}",
            problems.len(),
            problems.join("\n\n")
        );
    }
}
