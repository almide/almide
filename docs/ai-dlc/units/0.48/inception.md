# Unit 0.48 — Plan: query foundation, phase 2 (the build pipeline on queries)

- **Aim**: 0.4x arc — phase 2 of the same arc as 0.47.
- **Issues**: [#928](https://github.com/almide/almide/issues/928)

## In three lines

#928 sequences its own work: (1) unified driver, (2) per-module fingerprint, (3) LSP holds
the workspace, (4) memoization "only if (2)+(3) prove insufficient".
0.42 shipped (1); 0.47 owns (2) and is gated on a measurement that needs 0.46's program.
This Unit is (3) and (4), and it **cannot be planned in detail before 0.47 answers whether
(2) was even needed** — so its first deliverable is that dependency, stated honestly.

## Background

Splitting #928 across two ladder rows was a sizing decision, not a design one. The issue is
one arc with four steps and an explicit "only if" between them.

What is known today (measured, develop @ b5d530fa, warm): `almide check` is 10–30ms on every
file that exists, against a 50ms budget. See Unit 0.47's plan for the table and for why a
612-line corpus cannot distinguish O(module) from O(project).

## Scope

- S1 Wait on 0.47's measurement. If (2) proves unnecessary, this Unit's scope collapses to
  the LSP half — and that is a real outcome, not a deferral.
- S2 The LSP half (#928 step 3): hold the analyzed workspace, re-analyze the dirty module +
  reverse deps, add cancellation. This has its OWN latency budget and is worth doing even if
  the compiler-side fingerprinting is not.
- S3 Memoization (#928 step 4) only if S1+S2 prove insufficient, per the issue's own rule.

## Out of scope

- Anything 0.47 owns. If this Unit finds itself implementing per-module fingerprinting, the
  rows were mis-split and the ladder should be corrected rather than the boundary blurred.

## Done-criteria

- 0.47's measurement is in hand and this plan is rewritten against it (a plan written before
  its input arrives is a guess; this one says so).
- If S2 fires: LSP hover latency measured before and after on a project with ≥20 modules.

## Risks

- **R1 — planning phase 2 before phase 1 has an answer.** That is the state today, and the
  honest response is a short plan that names the dependency instead of a long one that
  invents requirements. Absorption: this plan is deliberately thin and will be rewritten.

## Proposed Bolts

- **B1** — Rewrite this plan once 0.47's B3 resolves the trigger.
- **B2** — LSP: hold the workspace, dirty-module re-analysis, cancellation.
- **B3** — Measure hover latency before/after on a ≥20-module project.
- **B4** — Conditional: memoization, only if B2's numbers demand it.
