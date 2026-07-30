# AI-DLC Unit Home — Plan/Ledger Pairs

> 日本語版: [README.ja.md](./README.ja.md)
> The English `.md` files are canonical; `.ja.md` files are courtesy translations and may lag.
> The loop reads and writes only the English files.

This directory holds the documents for each Unit (= one row of
[ROAD_TO_1_0.md](../roadmap/ROAD_TO_1_0.md)). One Unit = one folder, always with exactly
these two files:

```
docs/ai-dlc/units/<version>/
  inception.md      — the plan: what to do / what not to do / done-criteria / risks /
                      proposed Bolts. Carries the human-approval record
  construction.md   — the ledger: per-Bolt done-criteria, status, and evidence
                      (commit SHA and CI run URL)
```

The operating model is [docs/AI_DLC.md](../AI_DLC.md); the per-iteration procedure is
[docs/AI_DLC_BOLT_LOOP.md](../AI_DLC_BOLT_LOOP.md).

## Five rules

1. **Always a pair** — never create a ledger without a plan. A Unit whose evidence does not
   map to the plan's done-criteria is not "done".
2. **Write just-in-time** — do not write all 59 plans up front. The previous Unit's results
   (findings, measurements) feed the next plan; a plan written early is stale by the time
   work starts.
3. **No Bolts before approval (M0)** — one approval per Unit. Approving several in advance
   is fine.
4. **The ledger here is the single source of truth for Bolt plans** — do not duplicate them
   in issues; link from the issue to here. A checkbox without evidence (commit SHA and
   CI run URL) is invalid.
5. **Everything traceable** — ladder row ↔ this folder ↔ issues ↔ commits are linked both ways.

## Templates

- [inception-template.md](./inception-template.md) — the plan
- [construction-template.md](./construction-template.md) — the ledger
