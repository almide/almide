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

// ── Stage 4 (#1411): the failure-side floor ──
//
// Measured 2026-08-14: 41 of 418 wasm_cross fixtures reach an error path.
// Nothing said that number should be higher, and the week's three runtime
// bugs all lived on the side no fixture ran. This gate fixes the measured
// ratio as a FLOOR: the failure side may only grow its share.
//
// A RATIO, not a count, deliberately: a count floor is satisfied forever by
// the 41 that already exist, while the corpus around them grows — the share
// of the corpus that exercises failure would decay right back toward where
// it started. The ratio floor means adding ~10 success-side fixtures obliges
// one failure-side fixture, which is the pressure the category exists to
// apply. And deliberately NO TARGET above the floor: a target invites
// fixtures written to move a percentage (the RESEARCH doc's argument).
//
// Failure-side = wasm_cross fixtures whose NATIVE leg fails (nonzero exit or
// an `Error:` line — same predicate as the 2026-08-14 measurement) plus every
// wasm_fail fixture (failing is their admission criterion, enforced above).
// Compared as a cross-multiplication so no float threshold can drift.
const FAILURE_FLOOR_NUM: usize = 41; // of
const FAILURE_FLOOR_DEN: usize = 418; // — the 2026-08-14 measurement

#[test]
fn failure_side_floor() {
    let Some(legs) = corpus() else { return };
    let cross_total = legs.len();
    let cross_failing = legs
        .iter()
        .filter(|l| l.native.0 != 0 || l.native.2.contains("Error:"))
        .count();

    let fail_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_fail");
    let fail_count = std::fs::read_dir(&fail_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "almd").unwrap_or(false))
                .count()
        })
        .unwrap_or(0);

    let num = cross_failing + fail_count;
    let den = cross_total + fail_count;
    eprintln!(
        "failure_side_floor: {num}/{den} failure-side (wasm_cross {cross_failing}/{cross_total} + wasm_fail {fail_count}); floor {FAILURE_FLOOR_NUM}/{FAILURE_FLOOR_DEN}"
    );
    assert!(
        num * FAILURE_FLOOR_DEN >= FAILURE_FLOOR_NUM * den,
        "the failure-side share of the corpus fell below the floor: \
         {num}/{den} < {FAILURE_FLOOR_NUM}/{FAILURE_FLOOR_DEN}. The corpus grew \
         without its failure side growing — add the error-path fixture the new \
         work implies (spec/wasm_fail/, or an erring wasm_cross cell), or this \
         decays back to the 9.8% that hid #1400/#1408/#1410."
    );
}

// ── Stage 3 (#1411): the declared failure ledger for wasm_cross ──
//
// The 2026-08-14 audit's finding was the OPPOSITE of the plan's premise: the
// 41 error-reaching wasm_cross fixtures are not incidental — every one is a
// deliberate failure test by name and by assertion (assert_abort_*,
// int_div_by_zero, index_bounds*, option_none_unwrap_term, ...), and the
// wasm_cross equality gate already holds them to "both legs break alike".
// They were NOT moved to wasm_fail/: 41 files of contract-evidence path churn
// to re-gain a property they already have.
//
// What they lacked is VACUITY detection — the C-216 class. wasm_cross passes
// when both legs agree, including agreeing to SUCCEED: a change that makes
// int_div_by_zero stop erroring on both legs would go green silently, and the
// fixture would keep its name while testing nothing. This ledger closes that:
// every fixture listed here must still FAIL on the native reference. One that
// stops failing is flagged — either the intent changed (remove the row, a
// reviewed shrink, same discipline as GENUINE_SKIPS in the wasm:skip ledger)
// or a vacuity crept in (fix the fixture).
//
// A NEW error-path fixture in wasm_cross does not need a row (growth is free;
// the floor above applies the pressure); this list only pins the ones whose
// failure is load-bearing today.
const WASM_CROSS_DECLARED_FAILING: &[&str] = &[
    "assert_abort_eq",
    "assert_abort_msg",
    "assert_abort_multiline",
    "assert_abort_ne",
    "bytes_new_exhaustion",
    "effect_fn_value",
    "fan_any_allfail",
    "fan_block_err_list_order",
    "fan_map_err",
    "fan_map_inline_err",
    "fan_race_mapper",
    "fan_sibling_trap",
    "float_clamp_invalid",
    "fuel_trap_window",
    "guard_else_exit_code",
    "index_bounds",
    "index_bounds_i64",
    "index_bounds_write_heap",
    "index_bounds_write_only_var",
    "int8_div_overflow",
    "int_clamp_inverted",
    "int_div_by_zero",
    "int_div_by_zero_literal",
    "int_div_overflow",
    "int_mod_by_zero",
    "int_mod_overflow",
    "int_pow_negative_exponent",
    "int_rotate_nonpositive_width",
    "list_chunk_zero",
    "list_window_zero",
    "list_windows_zero",
    "matrix_dims_guard_overflow",
    "matrix_dims_guard_rows",
    "option_none_unwrap_term",
    "repeat_size_ceiling",
    "time_negative_scale",
    "time_negative_trap",
    "to_fixed_domain_abort",
    "to_fixed_domain_abort_hi",
    "top_let_div_eager",
    "top_let_div_used",
];

#[test]
fn declared_failing_fixtures_still_fail() {
    let Some(legs) = corpus() else { return };
    let by_name: std::collections::HashMap<&str, &FixtureLegs> =
        legs.iter().map(|l| (l.name.as_str(), l)).collect();
    let mut problems: Vec<String> = Vec::new();
    for name in WASM_CROSS_DECLARED_FAILING {
        match by_name.get(name) {
            None => problems.push(format!(
                "{name}: listed as a declared-failing fixture but absent from spec/wasm_cross \
                 — remove the row with the file, the ledger only shrinks deliberately"
            )),
            Some(l) => {
                let fails = l.native.0 != 0 || l.native.2.contains("Error:");
                if !fails {
                    problems.push(format!(
                        "{name}: declared failing but the native leg now SUCCEEDS \
                         (exit={} stderr={:?}) — the fixture went vacuous (the C-216 class). \
                         Restore the failing input, or remove the row as a reviewed change of intent.",
                        l.native.0, l.native.2
                    ));
                }
            }
        }
    }
    eprintln!(
        "declared_failing_fixtures_still_fail: {}/{} still failing",
        WASM_CROSS_DECLARED_FAILING.len() - problems.len(),
        WASM_CROSS_DECLARED_FAILING.len()
    );
    assert!(problems.is_empty(), "\n{}", problems.join("\n\n"));
}
