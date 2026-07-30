# Unit 0.41 — Ledger

> 日本語版: [construction.ja.md](./construction.ja.md) — the loop updates only this English
> ledger; the translation may lag.

> Paired plan: [inception.md](./inception.md) (approved 2026-07-31)
> Rule: a checkbox without evidence (commit SHA / CI run URL) is invalid.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Fuzzer prebuild removes the build tax | The nightly job skips building; fuzz time ≈ the whole budget | done (landed before construction) | Build split into its own job in `.github/workflows/fuzz-nightly.yml` (e043ddc0 → 6e898989 chain); the fuzz job's overhead is an artifact download. 2026-07-30 run: `budget_completed=true elapsed=305.9s` of a 5-min budget ([run 30516939965](https://github.com/almide/almide/actions/runs/30516939965)) |
| B2 | Split the campaign into N short shards | The restructured workflow completes one full night | dropped — M6 confirmed 2026-07-31 | The failure class (runner reclamation killing long jobs) was eliminated by the job split + budget-fits-lifetime design instead; 3 consecutive completed nights prove it. Raising nightly coverage (sharding or budget raise) is deferred to after 0.42 — recorded on [#924](https://github.com/almide/almide/issues/924) |
| B3 | Restore the budget + record programs/night | The run summary shows programs/night; budget at or above the pre-cut level | throughput done / budget-raise dropped (same M6) | `fuzz-night:` record line via `scripts/fuzz-night-verdict.sh` (b0e24675): `minutes=5 generated=459 throughput=90.0prog/min findings=1` on 2026-07-30. The 5-min budget is a documented deliberate choice (fits the shortest observed runner lifetime, 7m44s); raising coverage deferred with B2 |
| B4 | Check #917's residuals | #917 closed, or its remaining work added to this ledger as Bolts | done | [#917 closed 2026-07-31](https://github.com/almide/almide/issues/917) with evidence: dated results `research/benchmark/perf/results/2026-07-30-m4pro.json` (8 benchmarks, `native` + `wasm` variants, Rust references), two-sided ratchet `scripts/check-perf-ratio.sh` (1ae35a5f), publication (17ffd666, 1ead38c8) |
| B5 | Observe 3 consecutive full-budget nights | 3 run URLs recorded as evidence | done (met before construction) | `scripts/fuzz-track-record.sh` scores full-budget streak **3/14**: [7/28 run 30332337014](https://github.com/almide/almide/actions/runs/30332337014), [7/29 run 30425978731](https://github.com/almide/almide/actions/runs/30425978731), [7/30 run 30516939965](https://github.com/almide/almide/actions/runs/30516939965) — all `FINDINGS (full budget, red on findings)`: the instrument completed; the red is a real finding, which is 0.42's work |

## Notes

Reality diverged from the plan at first contact, in the good direction: the mechanism repair
(B1, B5) had already landed on develop via the audit-sweep commits (e043ddc0, b0e24675,
71fc6053, 6e898989) before this Unit's construction started — the plan was drafted from
#924's text without re-reading the workflow's current state. Lesson recorded: an inception
must verify the present state of its target, not just the issue that described it.

The nightly red is now a REAL finding (findings=1 on each of the 3 completed nights),
filed by the workflow onto the open fuzz-labeled issue. Triaging it is 0.42 (#796), not 0.41.

M6 decision (2026-07-31, confirmed in session by O6lvl4): drop B2 and B3's budget-raise from this Unit —
the failure class they targeted no longer exists, and nightly-coverage growth
(sharding or a budget raise, currently ~459 programs/night vs ~2,000 pre-cut) is deferred
until after 0.42 clears the findings. Deferred-work record kept on #924.

## Unit completion

- [x] Every Bolt done with evidence (B2/B3-budget dropped, M6 confirmed 2026-07-31)
- [x] The evidence satisfies the plan's done-criteria:
  - "3 consecutive full-budget nights" → B5 evidence (streak 3/14, three run URLs)
  - "programs/night in the run summary, build tax ~zero" → B3 evidence (`fuzz-night:` line) + B1 evidence (job split)
  - "#917 closed or residuals as Bolts" → B4 evidence (closed with evidence comment)
- [ ] Release v0.41.0 (ordinary minor — automatic). #924 stays open until its 14-night
      condition and carries over into 0.42
