# Mission-critical attack list

Written 2026-08-18, at the close of the deep-wash campaign (7 compiler-bug
families found and fixed, 501-fixture corpus, walls pinned at 16, CI green at
`8be514bb4`). Goal state: **Almide is defensibly usable in mission-critical
domains.** Every item carries a measurable exit criterion — an item without
one is not on this list.

Sizes: S (≤1 day) / M (≤1 week) / L (≤1 month) / XL (a quarter+).

## Tier 0 — Semantics must stop moving (everything else builds on this)

- [ ] **A0-1 (L) Spec freeze + compatibility contract.** 0.57.2 itself carries
  two behavior changes; a patch-level semantic move disqualifies the language
  for this market. Exit: an ALS freeze document, a breaking-change policy
  (deprecation cycle, never silent), and a 1.0 criteria checklist the release
  gate enforces.

## Tier 1 — Close the verification holes (compiler correctness)

- [ ] **A1-1 (L) Interp tagged-heap slice (#1226 slice 2).** HEAP_BOUNDARY
  abstains = 115/501 fixtures; every abstain is a place where a shared
  frontend bug passes on a 2-0 vote (the E0004 family was exactly this
  shape). Exit: abstain rate < 5% (voting ≥ 476/501).
- [ ] **A1-2 (XL) Wall burn-down, fuzz-frequency order.** The subset walls
  sit on everyday code. Burn down by observed frequency (per 30-min fuzz run):
  List-argument materialization (~98×), unresolvable-`if` with call-bearing
  arms (~59×), registry gaps `result.flatten/to_list/filter`, `list.zip_with`
  (~78×), scalar-binding subset (~25×), then the 16 pinned specimens
  (3-level nesting, Map-valued maps, tuple-list equality, heap-acc folds).
  Exit: walls < 100 per 30-min fuzz run AND every graduation lands its
  specimen as a voting fixture under contract in the same PR (the shrink-only
  rule already in place).
- [ ] **A1-3 (L) Negative-test 10× (test-surface-25x tier 1).** 99 diagnostic
  pairs vs rust's ~15k is the widest maturity gap. Exit: every E-code has a
  fixture family covering each hint variant and each fix-it verdict; ≥1,000
  pairs; the coverage gate enumerates E-codes with < 3 fixtures.
- [ ] **A1-4 (M) RC-placement snapshots (koka parc model).** Commit the
  post-RC-insertion drop/dup placement as expected output beside the runtime
  result for an RC-critical corpus (the koka_parc* family is the seed), so a
  benign-today placement move is loud. Exit: snapshot gate in CI over ≥ 20
  shapes.
- [ ] **A1-5 (M) Heap-cap leak harness (grain makeGcProgram model).** A hard
  heap budget knob on both targets; run each RC fixture at N units and N-1 —
  silent leaks become deterministic OOM. Exit: churn corpus runs under the
  cap in CI; one deliberately-leaking control proves the harness bites.
- [ ] **A1-6 (M) Allocation-count assertions (roc alloc-count model).** An
  allocation counter surface + exact loop-body counts with a control program
  per assertion. Exit: gate over ≥ 10 loop shapes asserting zero per-iteration
  allocation.
- [ ] **A1-7 (S) Filed diagnostics trio.** #1509 (`not (expr)` guard parse),
  #1510 (`t.0.1` float lexing hint), #1511 (fmt Option-canonicalization E054
  — fmt must be total over legal programs). Exit: all three closed with
  fixtures.

## Tier 2 — Prove the findings have dried up (time is the ingredient)

- [ ] **A2-1 (M) Fuzz nightly hardening + 90-day green streak.** Fix the
  drain wedge first (a stuck native cargo build survives the per-case
  timeout — runner needs a kill on the BUILD phase; it wedged two campaigns
  this week). Then: nightly runs with rotating seeds, findings auto-filed,
  streak meter. Exit: 90 consecutive green nights with zero new
  correctness findings.
- [ ] **A2-2 (M) Real-code acceptance tier.** Compile-and-test the real
  downstream projects (dfa, parsegen, and the other consumers) in CI as an
  acceptance ring — the E0004 and #1501 classes were both found by real code,
  not generated code. Exit: ≥ 3 real projects green in CI on every develop
  push.
- [ ] **A2-3 (S) New-angle cadence.** Each quarter adds one new detection
  angle (this campaign added: reference-suite ports, panic-wash, nested-type
  matrix). The #1508 backlog (or-patterns, string patterns, NaN-bits) feeds
  it as features land. Exit: a standing roadmap row per quarter; a quarter
  with a new angle and zero findings is the drying-up evidence this market
  asks for.

## Tier 3 — Outside the compiler (no issue-fixing moves these)

- [ ] **A3-1 (S) Security posture floor.** SECURITY.md with a disclosure
  channel, dependency lock audit in CI (the MVS+lock work is the seed), and
  a release-signing story. Exit: documented, linked from README.
- [ ] **A3-2 (M) Support contract.** LTS policy (which versions get fixes,
  for how long), versioning guarantees, and a bus-factor statement honest
  about the maintainer surface. Exit: SUPPORT.md ratified.
- [ ] **A3-3 (XL) Qualification data pack (only if a certified industry is
  targeted).** Map the existing evidence — three-way differential oracle,
  295-contract ledger, gate-verification enumeration, wall honesty — onto a
  tool-confidence argument (IEC 61508 / ISO 26262 TCL vocabulary). The
  infrastructure is unusually strong raw material; the pack is writing and
  audit, not new code. Exit: a reviewed qualification dossier skeleton.
- [ ] **A3-4 (XL) Production ladder.** Non-critical internal tool →
  supervised pilot → mission-critical, each rung with incident SLOs. Exit:
  first external production deployment with a post-mortem-free quarter.

## Sequencing

A0-1, A1-1, A1-2 are the critical path — they gate what "verified" even
means. A1-3..7 and A2-1..2 parallelize freely. Tier 3 starts now (A3-1 is a
day) and runs beside everything. The honest calendar: Tier 0–2 are a
quarter-to-two of focused work; A3-4's ladder is market-driven; A3-3 only if
the target industry demands certification.

## Standing metrics (check each quarter)

| metric | now (2026-08-18) | mission-critical bar |
|---|---|---|
| three-way voting rate | 76.6% | ≥ 95% |
| walls per 30-min fuzz run | ~500 | < 100 |
| diagnostic fixture pairs | 99 | ≥ 1,000 |
| fuzz green streak | 2 rounds | 90 nights |
| real projects in CI | 0 | ≥ 3 |
| open miscompile-class bugs | 0 | 0 (held) |
