# Compiler structural coverage — 2026-07-04 (proofs/coverage.sh)

Workloads: almide-mir + almide-codegen test suites, render_program over ALL runnable spec (v1 path), `almide test spec/` (v0 production path).

> **Since #2753 (2026-09-29)** the render_program sweep is gone: the gate drives
> every runnable spec program through the shipping `almide build --target wasm`
> (structural leg forced) and the `almide verify --emit` witness producer, and
> the incumbent WAT renderer's files (#2761 deletes them) are excluded from the
> report. The snapshot below predates that and still lists them.

```
crates/almide-frontend/src/lower/mod_p2.rs                                                      735               111    84.90%          21                 4    80.95%         314                41    86.94%           0                 0         -
crates/almide-frontend/src/lower/mod_p3.rs                                                      398               127    68.09%          17                 4    76.47%         157                43    72.61%           0                 0         -
crates/almide-frontend/src/lower/statements.rs                                                  665                90    86.47%          23                 3    86.96%         273                44    83.88%           0                 0         -
crates/almide-frontend/src/lower/types.rs                                                       127                 6    95.28%          13                 1    92.31%          58                 4    93.10%           0                 0         -
crates/almide-frontend/src/stdlib.rs                                                            310               270    12.90%          12                 6    50.00%         148               128    13.51%           0                 0         -
crates/almide-frontend/src/type_env.rs                                                          376                79    78.99%          38                 7    81.58%         219                41    81.28%           0                 0         -
crates/almide-mir/src/certificate.rs                                                           1237               321    74.05%          48                 3    93.75%         688               171    75.15%           0                 0         -
crates/almide-mir/src/certificate_p2.rs                                                         824                 5    99.39%          32                 1    96.88%         347                 3    99.14%           0                 0         -
crates/almide-mir/src/coown_names.rs                                                             39                 0   100.00%           2                 0   100.00%          17                 0   100.00%           0                 0         -
crates/almide-mir/src/lib.rs                                                                    373                61    83.65%          13                 2    84.62%         192                42    78.12%           0                 0         -
crates/almide-mir/src/lib_p2.rs                                                                 186                 0   100.00%          20                 0   100.00%         114                 0   100.00%           0                 0         -
crates/almide-mir/src/lower/binds.rs                                                            515                57    88.93%           5                 0   100.00%         208                17    91.83%           0                 0         -
crates/almide-mir/src/lower/binds_p2.rs                                                        1143               376    67.10%           8                 3    62.50%         569               207    63.62%           0                 0         -
crates/almide-mir/src/lower/binds_p3.rs                                                         998                55    94.49%          26                 1    96.15%         419                21    94.99%           0                 0         -
crates/almide-mir/src/lower/binds_p4.rs                                                        1223               484    60.43%          17                 3    82.35%         502               186    62.95%           0                 0         -
crates/almide-mir/src/lower/calls.rs                                                            575               143    75.13%          18                 9    50.00%         307                64    79.15%           0                 0         -
crates/almide-mir/src/lower/calls_p2.rs                                                         817               212    74.05%          11                 2    81.82%         431               149    65.43%           0                 0         -
crates/almide-mir/src/lower/calls_p3.rs                                                         576               207    64.06%          16                 2    87.50%         273                96    64.84%           0                 0         -
crates/almide-mir/src/lower/calls_p4.rs                                                        1953               583    70.15%          28                16    42.86%         883               285    67.72%           0                 0         -
crates/almide-mir/src/lower/control.rs                                                         1129               352    68.82%          31                 3    90.32%         673               218    67.61%           0                 0         -
crates/almide-mir/src/lower/control_p2.rs                                                      1258               191    84.82%          33                 3    90.91%         635               108    82.99%           0                 0         -
crates/almide-mir/src/lower/control_p3.rs                                                      1108               349    68.50%          25                 5    80.00%         535               162    69.72%           0                 0         -
crates/almide-mir/src/lower/control_p4.rs                                                      2321               551    76.26%          39                 6    84.62%         918               174    81.05%           0                 0         -
crates/almide-mir/src/lower/control_p5.rs                                                      3463              2062    40.46%          50                28    44.00%        1483               838    43.49%           0                 0         -
crates/almide-mir/src/lower/layout.rs                                                            16                 0   100.00%           4                 0   100.00%          12                 0   100.00%           0                 0         -
crates/almide-mir/src/lower/mod.rs                                                             1281               223    82.59%          80                11    86.25%         832               122    85.34%           0                 0         -
crates/almide-mir/src/lower/mod_p2.rs                                                          1101               172    84.38%          77                 3    96.10%         569                69    87.87%           0                 0         -
crates/almide-mir/src/lower/mod_p3.rs                                                           904               201    77.77%          23                 4    82.61%         472               119    74.79%           0                 0         -
crates/almide-mir/src/lower/mod_p4.rs                                                          1189               351    70.48%          39                12    69.23%         629               202    67.89%           0                 0         -
crates/almide-mir/src/lower/mod_p5.rs                                                          1193               138    88.43%          55                 3    94.55%         695                81    88.35%           0                 0         -
crates/almide-mir/src/lower/mod_p6.rs                                                          3111               676    78.27%         104                17    83.65%        1941               464    76.09%           0                 0         -
crates/almide-mir/src/lower/tail.rs                                                            1037               408    60.66%          12                 4    66.67%         530               207    60.94%           0                 0         -
crates/almide-mir/src/purity.rs                                                                  81                 0   100.00%           5                 0   100.00%          46                 0   100.00%           0                 0         -
crates/almide-mir/src/render_rust.rs                                                            457               106    76.81%          20                 2    90.00%         268                57    78.73%           0                 0         -
crates/almide-mir/src/render_wasm.rs                                                            769                54    92.98%          43                 0   100.00%         433                28    93.53%           0                 0         -
crates/almide-mir/src/render_wasm/registry.rs                                                     3                 0   100.00%           1                 0   100.00%         648                 0   100.00%           0                 0         -
crates/almide-mir/src/render_wasm_p2.rs                                                         585               109    81.37%          10                 0   100.00%         304                57    81.25%           0                 0         -
crates/almide-mir/src/render_wasm_p3.rs                                                           3                 0   100.00%           1                 0   100.00%           3                 0   100.00%           0                 0         -
crates/almide-mir/src/translation_validation.rs                                                 256                84    67.19%          15                 0   100.00%         145                41    71.72%           0                 0         -
TOTAL                                                                                        199403             70872    64.46%        6394              2316    63.78%       98363             33553    65.89%           0                 0         -
```

Low-coverage ledger (the next test-writing targets, output-affecting first):
- lower/control_p5.rs 43% — the defunc HOF engine (newest, largest untested surface)
- lower/tail.rs 61%, binds_p4 63%, binds_p2 64% — bind/tail lowering arms
- lower/calls_p3 65%, mod_p4 68% — routing and call-arg materialization


## Re-measurement — 2026-07-04 (after the crush pass)

TOTAL line coverage: **65.89%** on a GROWN denominator (98,573 lines vs 98,363 —
the pass added the zip-fusion / scalar-tuple-fold / beta-reduce / eager-init
machinery WITH their pinned unit tests, so the rate held while the surface grew).
The five graduated probes (tests_part5) cover the newest defunc arms directly.
Next targets unchanged: control_p5 defunc engine, tail.rs, binds_p2/p4.

## Re-measurement — 2026-07-04 (crush-pass close: nn walls 0)

TOTAL line coverage: **66.30%** (99,602 lines, 33,565 missed) — up from 65.89%
on a denominator grown by ~1,240 lines (matrix_core routing, the general
unfaithful-closure wall, defunc `list.find`, cross-module record-name
canonicalization), with only +12 missed lines: the seven new graduated pins
(tests_part5: adt tuple ctor, ctor list-field recursion, matrix floor/walls/
norms+bytes, defunc find, load_weights record return) cover the new surface
almost completely. render_wasm/registry.rs grew 648 → 679 covered lines (the
matrix_core self-host). Next targets unchanged: control_p5 defunc engine
(largest untested surface), tail.rs, binds_p2/p4.

## Rung 2 instruments — 2026-08-26 (#566)

Measured constraints first: rustc's MC/DC instrumentation no longer exists
(added 2024, removed Aug 2025 — rust-lang/rust#144999; 1.96.1 accepts
`block | branch | condition`, probed), so MC/DC-grade evidence is a
per-decision argument, not a compiler flag:

- **proofs/mcdc-ledger.sh / .toml** — every boolean-operator site in the ten
  safety files (perceus_verified, certificate family, translation_validation,
  the effect/capability checks) carries a row: `vectors` naming the
  independence-pair unit tests (each test flips ONE condition and the verdict
  flips with it), or `pending` under a shrink-only ceiling. Landing state:
  51 sites, 6 resolved with 16 tests (perceus_verified 87/207/213, calls.rs
  402/449), 45 pending.
- **Safety-set per-file line floors** — proofs/coverage-safety-baseline.txt,
  enforced by coverage.sh after the TOTAL ratchet: the safety files may not
  rot while the TOTAL holds. Seeded from a full local run (1.96.1/macOS)
  minus a 5-point cross-platform margin (CI 1.89.0/linux measures a few
  points off: translation_validation 70.09 local vs 68.18 CI same night;
  TOTAL 65.50 local vs 67.59 CI).
- **Condition-level backstop** — `ALMIDE_COVERAGE_CONDITION=1` +
  the nightly `coverage-condition` job on a dated-nightly pin
  (nightly-2026-08-01) with a first-step flag probe: per-condition branch
  records including the RHS of lazy operators, same floors, same ratchet.
- **Push-time ratchet** — ci.yml `coverage-ratchet` job runs the same gate at
  push/PR time behind a relevance check (measured 11–14 min in CI; the
  nightly-only cadence surfaced regressions up to 24 h late).

The scanner's own honesty was tuned by its first runs: closure heads
(`.or_else(|| …)`) are excluded by the preceding-token rule, and sites past a
file's first `#[cfg(test)]` are the tests themselves, not ledgered.

## Vector truth check — 2026-08-27 (#566, mutation)

A `vectors` ledger row used to be verified only for EXISTENCE of the named
tests — a vacuous test would have kept the gate green. `proofs/mcdc-mutation.sh`
closes that: for every resolved site it applies the operator-swap mutant
(&& <-> ||) at the site's exact character offset (sources restored and
hash-verified on every exit path) and requires the named vectors to be green
unmutated and red under the mutant. v1 mutant class is the operator swap;
condition-level stuck-at mutants need operand extents (an AST) and are the
next ratchet. Runs nightly (job "mcdc-mutation"); never run cargo
concurrently with it.

## Condition floor re-anchored — 2026-09-29 (#2941)

The nightly per-condition job went red on v0.65.1-rc4: TOTAL 55.14% against the
56.55% floor (fuzz-nightly run 36537776113). The last green night was
36457122588 on 9d80eb004, at 56.94%. #2922 moved this gate's workload off the
incumbent's `render_program` and onto the structural wasm build plus the witness
producer. That move explains the whole drop, in two parts, both structural.

Measured per file from the two nights' tables (branches / missed):

| step | branches | missed | TOTAL |
|---|---|---|---|
| last green night (9d80eb004) | 37886 | 16313 | 56.94% |
| the same night without the INCUMBENT_ONLY files | 36480 | 15888 | 56.45% |
| rc4 (371de8ff5) | 36820 | 16518 | 55.14% |
| develop after #2930 (54349cd6c, probe run 36550517708) | 36745 | 16429 | **55.29%** |

1. **The denominator.** #2922 took the incumbent-only files (`render_wasm*`,
   `region_*`, `pipeline_{b,c,link,global_slots,test_runner}`, …) out of scope.
   Those are the 1406 branches, 425 of them missed (70% covered), that #2935
   deletes. Taking them out moves the same green night from 56.94% to 56.45%,
   which is already below the floor. That floor was seeded on 2026-09-04 with
   those files in scope and driven.
2. **Lowering reachable only through the deleted pipeline.** `almide-mir/src/lower/`
   went from 1146 to 1739 missed branches of about the same total (+593). The
   incumbent pipeline lowered each program's generated `__drop_*` / `__repr_*`
   sources and ran `pipeline_b`'s program passes (`populate_abi_registries`,
   `rewrap_never_err_into_result_targets`, …). Neither product leg calls that
   code any more:
   - the witness producer lowers only the program's own functions;
   - the native leg lowers only the program's own functions.

   On #2935's head, a dead-code probe (`lower` made private) finds 172 items,
   about 7,400 lines, whose only remaining callers are their own unit tests and
   the kept `emit_cert*` / `classify_corpus*` examples. #2950 deletes them. Until
   then no workload can reach them, so adding one would measure nothing the
   product runs.

What is left is noise in both directions: -40 missed in the other reported
crates, and the new structural files' 140 branches, 46 of them missed.

The floor is therefore re-anchored to the CI measurement of develop after #2930:
**55.29%** (`proofs/coverage-baseline-condition.txt` 5655 → 5529). #2935 and
#2950 only take out code that no workload reaches, so each can only raise the
TOTAL. Run `--update` on the first green night after each lands to ratchet the
floor back up.
