# Unit 0.41 — Ledger

> 日本語版: [construction.ja.md](./construction.ja.md) — the loop updates only this English
> ledger; the translation may lag.

> Paired plan: [inception.md](./inception.md) (must be approved)
> Rule: a checkbox without evidence (commit SHA / CI run URL) is invalid.
> Bolt N's evidence is recorded at the start of the next iteration, while checking the
> previous run's CI.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Fuzzer prebuild removes the build tax | The nightly job skips building; fuzz time ≈ the whole budget | pending | — |
| B2 | Split the campaign into N short shards | The restructured workflow completes one full night | pending | — |
| B3 | Restore the budget + record programs/night | The run summary shows programs/night; budget at or above the pre-cut level | pending | — |
| B4 | Check #917's residuals | #917 closed, or its remaining work added to this ledger as Bolts | pending | — |
| B5 | Observe 3 consecutive full-budget nights | 3 run URLs recorded as evidence | pending | — |

## Notes

(not started)

## Unit completion

- [ ] Every Bolt done with evidence
- [ ] The evidence satisfies the plan's done-criteria (state which evidence maps to which criterion)
- [ ] Release v0.41.0 (ordinary minor — automatic). #924 stays open until its 14-night
      condition and carries over into 0.42
