# Unit 0.42 — Ledger

> Paired plan: [inception.md](./inception.md) (approved 2026-07-31)
> Rule: a checkbox without evidence (commit SHA / CI run URL) is invalid.
> Bolt N's evidence is recorded at the start of the next iteration, while checking the
> previous run's CI. Detail ledger for the findings themselves: Wave 3 in
> `docs/roadmap/active/fuzz-findings-triage.md`.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Build the fuzzer; replay all 5 live records; classify root causes; open the Wave 3 table | Every record reproduced (or its absence explained); each has a named symptom + suspected root-cause class in Wave 3 | pending | — |
| B2 | Fix the OutputDivergence class (native `r15 = 100` vs wasm `r15 = 0`) | The replay is clean on both targets; root cause named; contract entry if observable behavior was defined | pending | — |
| B3 | Fix the build/run-failure classes (NativeBuildFailure, RunFailureDivergence, WasmBuildFailure ×2) | All four replays clean on both targets; root causes named | pending | — |
| B4 | Local campaign on the fixed compiler | A fresh multi-thousand-program local campaign records 0 findings | pending | — |
| B5 | Observe 2 consecutive green nights | `fuzz-track-record.sh` green streak ≥ 2; #796 closed; release v0.42.0 | pending | — |

## Notes

(not started)

## Unit completion

- [ ] Every Bolt done with evidence
- [ ] The evidence satisfies the plan's done-criteria (state which evidence maps to which criterion)
- [ ] Release v0.42.0 (ordinary minor — automatic)

## Retrospective (Try)

(written when the Unit closes)
