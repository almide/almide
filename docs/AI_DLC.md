# AI-DLC Operating Model — Loop Engineering

> 日本語版: [AI_DLC.ja.md](./AI_DLC.ja.md)

The ladder ([ROAD_TO_1_0.md](./roadmap/ROAD_TO_1_0.md)) lines up 59 releases' worth of work,
from 0.41 to 0.99. Driving that by hand would mean hundreds of human decisions. This document
hands most of those decisions to a loop and to CI, and narrows the human's role to **seven
situations**.

The shape is a car navigation system: you pick the destination, the loop drives, and you only
speak at the forks.

> Method source: AWS's AI-Driven Development Lifecycle (AI-DLC). The term mapping is in the appendix.
> The step-by-step procedure is [AI_DLC_BOLT_LOOP.md](./AI_DLC_BOLT_LOOP.md); per-Unit documents
> and their rules live in [docs/ai-dlc/README.md](./ai-dlc/README.md).

## Four terms to remember

- **Unit** — one version's worth of work; one row of the ladder (e.g. 0.41 = fix the fuzz instrument)
- **Inception** — the plan written before a Unit starts: what to do, what not to do, and what "done" means
- **Bolt** — one work cycle of a few hours: implement, test, push
- **Mob** — a situation that needs a human decision. Outside these, the loop never calls a human

The ledger paired with the plan — the file that records each Bolt and its evidence — is called
**Construction**.

## The life of a Unit

1. The loop picks the next Unit from the ladder
2. It writes the plan (`inception.md`) and asks you to approve ← **human here**
3. Once approved, it executes Bolts one at a time from the ledger (`construction.md`).
   Pushes are automatic; CI is the judge
4. When every Bolt is done with evidence, it releases (ordinary minors automatically;
   milestone versions wait for approval)
5. On to the next Unit

Your routine job is the single approval at step 2.

## The seven situations that call you (Mob points)

| # | Situation | Notes |
|---|---|---|
| M0 | Approving a Unit's plan | Once per Unit. Approving several in advance is fine |
| M1 | Approving a milestone release | 0.50 / 0.60 / 0.70 / 0.80 / 0.90, with an audit brief. Ordinary minors release automatically |
| M2 | Decisions that change the language's surface or behavior | Syntax, stdlib boundaries, observable cross-target behavior (= anything touching the contract ledger) |
| M3 | Outward-facing claims | Wording in README or benchmarks. Number refreshes by gated scripts do not call you |
| M4 | Pressure to loosen a standard | When green would require lowering a ratchet or wall. The AI never loosens one on its own; the only direction is forward |
| M5 | Company-side work | Legal entity, certification authorities, funding, deployments (the program track). The loop does not touch these |
| M6 | Distress | Two failures from the same cause, ambiguity in the plan, or an unplanned breaking change |

The call format is fixed: an issue labeled `mob` (body: what happened / evidence / options /
recommendation), plus a notification. Until you answer, the loop continues only with independent
work; under `/loop` it keeps a watch on the mob issue so your reply resumes it, and otherwise
it stops on its own.

## Who catches which mistakes

The seven situations come from one split: mistakes machines can catch versus mistakes only a
human can catch. Only the latter became Mob points.

**Machines (the CI gates) catch:**

- Types, ownership, memory safety — the trust spine and walls ("silently zero" does not exist)
- Cross-target behavior differences — the 3-way oracle, differential fuzz, the byte gate
- Regressions in performance or quality — the ratchets (they can go down, never silently back up)
- Missing contracts — `scripts/check-contracts.sh` (bidirectional fixture ↔ C-NNN links)
- API surface gaps — the matrix gates

**Only a human catches:**

- Work that meets its done-criteria but drifts from the goal
- Claims bigger than the evidence (this actually happened — the 2026-07 audit)
- Taste and judgment in language design
- Business decisions

## Where to see progress

No new dashboards. Everything lands where it already lives:

- Current work — checkboxes and evidence (commit SHA, CI run URL) in
  `docs/ai-dlc/units/<version>/construction.md`
- Finished Units — release notes
- Items waiting on you — issues labeled `mob`
- Overall remaining work — the ladder's issue links closing over time

## How to run it

1. Locally: `/loop follow docs/AI_DLC_BOLT_LOOP.md and execute one iteration`
2. Walking away is fine. When a human is needed you get a notification, and if nothing else
   is actionable the loop stops on its own
3. Cloud residency comes later (`/schedule`). The nightly watch loop opens only after 0.41
   revives the fuzz instrument — calling it "monitoring" while the instrument is dead would be a lie

## The loops

| Name | Job | Status |
|---|---|---|
| L1 Bolt loop | Pick the Unit → plan → approval → Bolts one at a time → release check ([procedure](./AI_DLC_BOLT_LOOP.md)) | Ready |
| L2 Nightly watch loop | fuzz-nightly triage, red-CI detection, ratchet drift watch | Opens after 0.41 |
| L3 Release loop | bump → develop→main PR → tag → verify. Built into L1 | Ready |

## Acceptance criteria for this model itself

- The loop can take 0.41 to release with zero human involvement outside Mob points.
  If a human is needed anywhere else, that is a defect in this model — fix this document
  and the procedure
- At least 80% of human calls turn out to have genuinely needed a human. Noisy calls are
  also a defect
- Zero ratchet loosening and zero drift between claims and reality, continuously

## Appendix: term mapping to AWS AI-DLC

| AWS term | What it is in this repo |
|---|---|
| Intent | A decade arc (e.g. 0.4x "instruments and the edit loop"), with an exit gate |
| Unit | A minor-version row plus the plan/ledger pair in `docs/ai-dlc/units/<version>/` |
| Bolt | One work cycle of a few hours; one row of the ledger |
| Deployment Unit | A released minor (tag + binaries for 5 platforms) |
| Mob Elaboration | Plan approval (M0) and structural changes to the ladder |
| Mob Programming / Testing | Normally replaced by the CI gates; humans only at Mob points |
| Context Memory | Git history + issues + ROAD_TO_1_0 + docs/ai-dlc/units/ |
| Human oversight as a loss function | The "who catches which mistakes" split |

Note: we run AI-DLC's Inception in two levels. Level 1 (decompose Intents into Units and order
them by dependency) is already done for 0.41–0.99 — ROAD_TO_1_0 is its artifact. Level 2 (the
per-Unit plan) is written one Unit at a time, just before work starts: the previous Unit's
results feed the next plan, so we do not write 59 plans up front.
