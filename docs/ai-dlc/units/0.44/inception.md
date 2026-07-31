# Unit 0.44 — Plan: Fuzz to True Green

- **Aim**: The 0.4x arc's second step. 0.41 brought the instrument back; 0.42 makes its verdict
  clean. Gate 0.50 needs "continuously green fuzz is the normal state", and #796's
  two-consecutive-green-nights rule became meaningful again the moment the campaign started
  completing its budget.
- **Issues**: [#796](https://github.com/almide/almide/issues/796) (primary),
  [#924](https://github.com/almide/almide/issues/924) (14-night streak continues through this Unit)

## In three lines

- The working instrument is catching real findings: 4 live records across the last 3 nights —
  one silent wrong value (native `r15 = 100` vs wasm `r15 = 0`), one
  check-accepted-but-native-build-failed, one wasm-run-failed-while-native-succeeded,
  and two wasm build failures
- Replay each deterministically, fix root causes forward (worst class first), and record every
  fix in the Wave 3 section of the existing triage ledger
- Done when live findings are 0 and two consecutive nights are green — that closes #796

## Background

Verified present state (not #796's body — its 2026-07-18 seed campaigns are already
12/12 + 8/8 clean per `docs/roadmap/active/fuzz-findings-triage.md`, which stays the
detail ledger for this work). The live findings, from the workflow's reports on #924:

| Kind | Symptom | Replay |
|---|---|---|
| OutputDivergence | stdout differs: native `r15 = 100`, wasm `r15 = 0` | `xtarget-fuzz replay --seed 1785217538023450905 --index 861` |
| NativeBuildFailure | native build failed after check accepted | `xtarget-fuzz replay --seed 1785217538023450905 --index 535` |
| RunFailureDivergence | wasm run failed while native succeeded | `xtarget-fuzz replay --seed 1785304212462799529 --index 57` |
| WasmBuildFailure | wasm build failed | `xtarget-fuzz replay --seed 1785304212462799529 --index 12` |
| WasmBuildFailure | wasm build failed | `xtarget-fuzz replay --seed 1785389912282950207 --index 29` |

Full-budget streak stands at 3/14; the coverage-ratchet job in the same workflow succeeded
on the latest night (no separate diagnosis needed).

## Scope

- S1 Reproduce all live records locally and classify them by root cause
- S2 Fix forward, worst class first — OutputDivergence is the silent-wrong-value class the
  entire trust discipline exists to prevent
- S3 Every fix that touches observable behavior carries its contract entry in the same
  commit (the M2 tripwire stays armed)
- S4 Record the campaign as **Wave 3** in `docs/roadmap/active/fuzz-findings-triage.md`
- S5 Observe two consecutive green nights, then close #796

## Out of scope

- Raising nightly coverage (deferred on #924 until after this Unit)
- The 14-night streak itself — it keeps counting; #924 stays open
- New instrument lenses (pass-ordering etc.) — 0.52 (#912)

## Done-criteria

- Every live finding record is replayed and closed with a named root cause and a fix commit;
  new findings that arrive on subsequent nights join Wave 3 and are held to the same bar
  (the DoD is the streak, not a fixed list)
- `scripts/fuzz-track-record.sh` shows a green streak ≥ 2 and #796 is closed
- The Wave 3 table in fuzz-findings-triage.md is complete — each row has symptom, root
  cause, and fix reference

## Risks

- R1 The instrument keeps finding while we fix — expected and healthy; the streak-based DoD
  absorbs it (loop until dry, not until a list empties)
- R2 A root cause may sit deep in the checker or lowering; if a fix requires changing
  observable behavior or a breaking change, that is M2 (contract decision), and repeated
  failed attempts on the same finding are M6
- R3 Two green nights cost real time — during the wait the loop may draft the next Unit's
  plan (never its ledger)

## Proposed Bolts

- B1 Build the fuzzer, replay all 5 live records, classify root causes, open the Wave 3 table
- B2 Fix the OutputDivergence class (silent wrong value — worst first)
- B3 Fix the build/run-failure classes (NativeBuildFailure, RunFailureDivergence, WasmBuildFailure ×2)
- B4 Re-run a local campaign on the fixed compiler to confirm clean before the nightly does
- B5 Observe 2 consecutive green nights → close #796 → release v0.42.0

## Approval (M0)

- Status: **approved**
- Approver / date / notes: O6lvl4 / 2026-07-31 / approved in session. Streak-based DoD
  (loop until dry) and worst-class-first ordering confirmed.
