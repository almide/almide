# Mission-critical attack list

Written 2026-08-18, at the close of the deep-wash campaign (7 compiler-bug
families found and fixed, 501-fixture corpus, walls pinned at 16, CI green at
`8be514bb4`). Goal state: **Almide is defensibly usable in mission-critical
domains.** Every item carries a measurable exit criterion — an item without
one is not on this list.

Sizes: S (≤1 day) / M (≤1 week) / L (≤1 month) / XL (a quarter+).

## Tier 0 — Semantics must stop moving (everything else builds on this)

- [x] **A0-1 (#530 + #1485 + #1483) (L) Spec freeze + compatibility contract.**
  DECLARED 2026-08-20 (4997c7535, after the owner-sequenced ADR-0012 D2/D3
  legs): docs/STABILITY.md — frozen surface (LLM-facing = stable), ALS
  conformance clause ("the implementation is wrong"), breaking-change policy
  (major + @dialect bump + migration note); proofs/stability-closure.toml —
  the six-criterion stability bar with Ns fixed before the numbers
  (#1485 CLOSED), reported on every push. Residual burn-downs stay with
  their carriers: #530 (prose contradictions to zero), #1483 (the
  bidirectional surface sweep + experimental marker). 0.57.2 itself carries
  DECIDED 2026-08-20 (owner): freeze is GO, sequenced AFTER the ADR-0012
  error-surface end state lands — #1193 (D2: `-> T!E` carries the typed error
  in the fallibility marker) then #1194 (D3: fmt normalizes return-position
  Result to `T!E`/`T!`) — because the fmt canonical form changes the surface
  the freeze would pin. Order: #1193 -> #1194 -> ALS freeze declaration +
  breaking-change policy (change = explicit major) + the #530/#1485/#1483
  closure drafts.
  two behavior changes; a patch-level semantic move disqualifies the language
  for this market. Exit: an ALS freeze document, a breaking-change policy
  (deprecation cycle, never silent), and a 1.0 criteria checklist the release
  gate enforces.

## Tier 1 — Close the verification holes (compiler correctness)

- [x] **A1-1 (#1226) (L) Interp tagged-heap slice (#1226 slice 2).** HEAP_BOUNDARY
  abstains = 115/501 fixtures; every abstain is a place where a shared
  frontend bug passes on a 2-0 vote (the E0004 family was exactly this
  shape). Exit: abstain rate < 5% (voting ≥ 476/501).
  DONE 2026-08-19 (#1226 closed): abstains 135→27 rows = 4.6% of the corpus,
  real 3-way votes 566/589 (96.1%), zero dissent held across every increment;
  the 23 residual abstains are classified in interp-abstain-classes.toml.
- [ ] **A1-2 (XL, #1527) Wall burn-down, fuzz-frequency order.** The subset walls
  sit on everyday code. Burn down by observed frequency (per 30-min fuzz run):
  List-argument materialization (~98×), unresolvable-`if` with call-bearing
  arms (~59×), registry gaps `result.flatten/to_list/filter`, `list.zip_with`
  (~78×), scalar-binding subset (~25×), then the 16 pinned specimens
  (3-level nesting, Map-valued maps, tuple-list equality, heap-acc folds).
  Exit: walls < 100 per 30-min fuzz run AND every graduation lands its
  specimen as a voting fixture under contract in the same PR (the shrink-only
  rule already in place).
- [x] **A1-3 (L, #1528) Negative-test 10× (test-surface-25x tier 1).** 99 diagnostic
  pairs vs rust's ~15k is the widest maturity gap. Exit: every E-code has a
  fixture family covering each hint variant and each fix-it verdict; ≥1,000
  pairs; the coverage gate enumerates E-codes with < 3 fixtures.
  PROGRESS 2026-08-18: **749 pairs** (wave-2's 730 + the #1518/#1521/#1486/
  #1515 families); three new E-codes (E058/E059/E060) landed with families.
  CLOSED 2026-09-02 (#1784): **1,000 pairs exactly**; multi-file fixtures
  (`almide.toml` + `src/*.almd` siblings) retired the E033/E420 exemptions,
  so every code is on the ≥3-family bar the coverage gate enforces.
- [x] **A1-4 (M, #1529) RC-placement snapshots (koka parc model).** Commit the
  post-RC-insertion drop/dup placement as expected output beside the runtime
  result for an RC-critical corpus (the koka_parc* family is the seed), so a
  benign-today placement move is loud. Exit: snapshot gate in CI over ≥ 20
  shapes.
  DONE 2026-08-19 (#1529 closed): 21-shape snapshot gate
  (tests/rc_placement_snapshot_test.rs), drop-order mutant kill-evidence.
- [x] **A1-5 (M, #1530) Heap-cap leak harness (grain makeGcProgram model).** A hard
  heap budget knob on both targets; run each RC fixture at N units and N-1 —
  silent leaks become deterministic OOM. Exit: churn corpus runs under the
  cap in CI; one deliberately-leaking control proves the harness bites.
  DONE 2026-08-20 (#1530): `almide build --heap-cap` on both targets (wasm
  frontier ceiling in $alloc, native live-bytes global allocator, both the
  defined C-197 abort shape); harness tests/heap_cap_test.rs — churn corpus
  unperturbed under the cap, rc_dec-removal mutants 4/9 deterministic OOM,
  native cap=1 enforcement live.
- [x] **A1-6 (M, #1531) Allocation-count assertions (roc alloc-count model).** An
  allocation counter surface + exact loop-body counts with a control program
  per assertion. Exit: gate over ≥ 10 loop shapes asserting zero per-iteration
  allocation.
  DONE 2026-08-19 (#1531 closed): static-zero gate over the shipped WAT
  (tests/alloc_count_gate_test.rs), 10 shapes + 2 allocating controls.
- [x] **A1-7 (S) Filed diagnostics trio.** #1509 (`not (expr)` guard parse),
  #1510 (`t.0.1` float lexing hint), #1511 (fmt Option-canonicalization E054
  — fmt must be total over legal programs). Exit: all three closed with
  fixtures. DONE 2026-08-18, shipped in v0.57.2.

## Tier 2 — Prove the findings have dried up (time is the ingredient)

- [ ] **A2-1 (M, #1532 + #924) Fuzz nightly hardening + 90-day green streak.** Fix the
  drain wedge first (a stuck native cargo build survives the per-case
  timeout — runner needs a kill on the BUILD phase; it wedged two campaigns
  this week). Then: nightly runs with rotating seeds, findings auto-filed,
  streak meter. Exit: 90 consecutive green nights with zero new
  correctness findings.
- [x] **A2-2 (M, #1533) Real-code acceptance tier.** Compile-and-test the real
  downstream projects (dfa, parsegen, and the other consumers) in CI as an
  acceptance ring — the E0004 and #1501 classes were both found by real code,
  not generated code. Exit: ≥ 3 real projects green in CI on every develop
  push.
  DONE 2026-08-20 (#1533 closed): Acceptance Ring workflow — dfa/parsegen/svg
  at pinned refs against the develop-built compiler on every push/PR, with a
  test-file floor and a native-fallback ceiling; kill-evidence for both; green
  from its first run. teastia (private) joins when a read token exists.
- [x] **A2-3 (S) New-angle cadence.** Each quarter adds one new detection
  angle (this campaign added: reference-suite ports, panic-wash, nested-type
  matrix). The #1508 backlog (or-patterns, string patterns, NaN-bits) feeds
  it as features land. Exit: a standing roadmap row per quarter; a quarter
  with a new angle and zero findings is the drying-up evidence this market
  asks for. 2026-Q3 QUOTA EXCEEDED (2026-08-18): pass-isolated
  semantic-preservation gate (#1487, mutant-kill verified), perf ablation
  leg (#1466), cap-effect consistency gate (#1515), seeded regex
  differential (its first CI kill confirmed the same day). Recurs 2026-Q4.

## Tier 3 — Outside the compiler (no issue-fixing moves these)

- [ ] **A3-1 (#1534) (S) Security posture floor.** SECURITY.md with a disclosure
  channel, dependency lock audit in CI (the MVS+lock work is the seed), and
  a release-signing story. Exit: documented, linked from README.
- [ ] **A3-2 (#1535) (M) Support contract.** LTS policy (which versions get fixes,
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
