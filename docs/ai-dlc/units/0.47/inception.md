# Unit 0.47 — Plan: query/incremental foundation, phase 1

- **Aim**: 0.4x arc — the edit loop should do work proportional to what changed, not to the
  project. This is the row #1003 (module cache) is built on.
- **Issues**: [#928](https://github.com/almide/almide/issues/928)

## In three lines

#928 describes an arc, not a patch, and names a hard prerequisite: "a query layer cannot sit
on six parallel drivers."
**That prerequisite is now met** — Unit 0.42 shipped the single driver in v0.42.0, with the
migration ratchet at 0 and the stage order pinned by a gate.
Done means either the incremental layer exists, or the measurement says it is not yet needed
and the trigger is re-armed with numbers — the same discipline Unit 0.45 applied to #1002.

## Background — the prerequisite is met, and the metric may already be

**Prerequisite (met).** #928: "This is an arc, not a patch, and it has a hard prerequisite:
driver unification (separate issue) — a query layer cannot sit on six parallel drivers."
`almide-driver` is now the only place the post-typecheck stage order is spelled;
`tests/one_driver_test.rs` fails if a second appears, and the order flip was verified
byte-identical across all 329 `spec/wasm_cross` fixtures.

**Success metric, measured today** (develop @ b5d530fa, warm):

| target | lines | `almide check` |
|---|---|---|
| `spec/integration/clone_opt_test.almd` | — | **10ms** |
| `examples/lisp.almd` | 268 | **20–30ms** |
| `spec/stdlib/stdlib-test.almd` | 612 | **10ms** |

#928's stated metric is "LSP hover latency on a 20-module project < 50ms warm" and
"`almide check` warm rebuild after a 1-file edit does O(changed module) work". The first
half appears **already satisfied at the sizes that exist** — check is an order of magnitude
under the budget. The second half is a statement about asymptotics, not latency, and cannot
be falsified by a corpus whose largest file is 612 lines.

**So this Unit has the same shape as 0.45's.** The honest first deliverable is not the
implementation; it is resolving whether the trigger has fired. And unlike 0.45, the answer
is genuinely unknown, because **nothing at the relevant scale exists yet** — which is
precisely what Unit 0.46 is building.

## Scope

- S1 Measure the metric at the sizes that exist, and state which half of it is already met.
- S2 Re-measure once 0.46's program passes ~5k lines, which is the first point where
  "O(changed module)" is distinguishable from "O(project)".
- S3 Build phase 1 (per-module fingerprint → skip typecheck/lower for unchanged modules) ONLY
  if S2 shows the budget broken.
- S4 Whatever the answer, leave #928 with numbers and a sharper re-arm condition.

## Out of scope

- Salsa-style memoization. #928 itself sequences it last ("only if (2)+(3) prove
  insufficient"), and nothing has yet shown (2) is needed.
- The LSP work (#928's step 3). It is a separate consumer with its own latency budget and
  its own measurement; folding it in here would make one Unit answer two questions.

## Done-criteria

- The metric is measured at the largest available project, with the method stated.
- #928 carries the answer with numbers.
- If S3 fires: a 1-file edit in a multi-module project does measurably less work than a cold
  build, shown by a before/after table rather than by asserting the design.

## Risks

- **R1 — building the layer because the ladder has a row for it.** The measurement so far
  points the other way. Absorption: S3 is conditional, and the Unit may close having built
  nothing, exactly as 0.45 did.
- **R2 — measuring at a scale that cannot show the difference.** A 612-line corpus cannot
  distinguish O(module) from O(project). Absorption: S2 gates on 0.46 reaching ~5k lines;
  until then any "it is fast enough" claim is about the wrong sizes.
- **R3 — the two halves of the metric get conflated.** Latency (already met) and asymptotics
  (unknown) are different claims. Absorption: report them separately, always.

## Proposed Bolts

- **B1** — Measure the metric at every size that exists; state which half is met. (Mostly
  done in this plan; formalize with the method and land the table.)
- **B2** — Wait on 0.46 reaching ~5k lines, then re-measure. This Bolt is a dependency, not
  work.
- **B3** — Resolve the trigger in #928 with the numbers.
- **B4** — Conditional: per-module fingerprinting, if B3 says fired.
