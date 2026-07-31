# Unit 0.42 — Ledger

> Paired plan: [inception.md](./inception.md) (approved 2026-07-31)
> Rule: a checkbox without evidence (commit SHA / CI run URL) is invalid.
> Bolt N's evidence is recorded at the start of the next iteration, while checking the
> previous run's CI. Detail ledger for the findings themselves: Wave 3 in
> `docs/roadmap/active/fuzz-findings-triage.md`.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Build the fuzzer; replay all 5 live records; classify root causes; open the wave table | Every record reproduced (or its absence explained); each has a named symptom + suspected root-cause class in the wave table | done | Wave 4 table in `docs/roadmap/active/fuzz-findings-triage.md`: 2 resolved-in-window (861, 535), 3 live (57 trap-divergence, 12 heap-if wall, 29 registry gap). Commit 697c2c91 |
| B2 | Fix the OutputDivergence class (native `r15 = 100` vs wasm `r15 = 0`) | The replay is clean on both targets; root cause named; contract entry if observable behavior was defined | pending | — |
| B3 | Fix the build/run-failure classes (NativeBuildFailure, RunFailureDivergence, WasmBuildFailure ×2) | All four replays clean on both targets; root causes named | in progress — 29 fixed and CI green; 12 root-caused with an extension design (msb drop variant, see Wave 4); 535 resolved-in-window (attribution in B2); 57 waiting on M2 (#1017) | 29: commit 6edfc79d, CI green on develop (Trust Spine success; CI / Cross-Target running at record time). 12: diagnosis in Wave 4, commit follows |
| B4 | Local campaign on the fixed compiler | A fresh multi-thousand-program local campaign records 0 findings | pending | — |
| B5 | Observe 2 consecutive green nights | `fuzz-track-record.sh` green streak ≥ 2; #796 closed; release v0.42.0 | pending | — |

## Notes

- The plan said "Wave 3", but the triage ledger already carries a Wave 3 — this Unit's
  campaign is **Wave 4**. In-scope naming correction.
- B2's original target (the OutputDivergence) turned out to be already resolved on current
  develop — no codegen commits landed between the finding night and now EXCEPT the
  c4d38b1d..52200340 window (7/28–7/29), so B2 is retargeted to: attribute the resolving
  commits for 861 and 535 (bisect the window) and confirm the resolution is real, not
  host-dependent. In-scope retarget; the DoD (streak, not list) is unchanged.
- Fix order for the live three, easiest-first to shrink the nightly surface while the M2
  question on 57 is prepared: 29 (registry gap) → 12 (heap-if wall brick) → 57 (M2 first,
  then implementation per the decision).

## Unit completion

- [ ] Every Bolt done with evidence
- [ ] The evidence satisfies the plan's done-criteria (state which evidence maps to which criterion)
- [ ] Release v0.42.0 (ordinary minor — automatic)

## Retrospective (Try)

(written when the Unit closes)
