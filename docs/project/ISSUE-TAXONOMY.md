# Issue severity taxonomy (#1482)

Before this file existed the tracker had category labels (`bug`,
`wasm-codegen`, `fuzz`, …) but **no severity class**, so "are there open
release blockers?" was not a queryable question — every release answered it
by memory. This file defines the closed set, the admission criterion for
each class, and the one rule that consumes them.

## The closed set

| label | admits exactly | examples |
|---|---|---|
| `I-unsound` | The guarantee spine is violated: something `almide check` + the v1 certificate accepted breaks memory safety or the verified-ownership contract (use-after-free, double-free, a cert that proves the wrong thing). | a drop route that frees a live handle |
| `I-miscompile` | An accepted program computes a **wrong value** on some target — including both-targets-agree-and-both-wrong (the interp third-oracle class). Walls are NOT miscompiles: an honest refusal is the absence of this class. | `5 \|> half ?? -1` printing `0` |
| `I-divergence` | native / wasm / interp disagree on **observable output** (stdout, stderr, exit code) for an accepted program. The contract ledger's byte-identity promise is broken. | `list.unique` keeping `0.0` after `-0.0` on one leg only |
| `regression` | Behaviour that worked in a **released** version is broken at HEAD. Pair it with an `I-*` class when one applies. | a fixture green at v0.57.0, red on develop |
| `P-critical` / `P-high` / `P-low` | Scheduling priority. Orthogonal to the `I-*` axis; carries **no blocking power** by itself. | — |

Rules of use:

- An `I-*` label asserts the class is **confirmed** (reproduced, or pinned by
  a failing fixture) — suspicion stays unlabeled until verified.
- One issue can carry several classes (`I-divergence` + `regression`).
- Walls, missing features, perf gaps and doc drift are **not** `I-*` classes;
  they take `enhancement` / `wasm-codegen` / `documentation` as before.

## The blocking rule

**A FINAL release tag must not ship while an issue labeled `I-unsound`,
`I-miscompile`, `I-divergence`, or `regression` is open.**

- Measured by [`scripts/count-release-blockers.sh`](../../scripts/count-release-blockers.sh);
  the release workflow runs it with `--gate` before creating a final release,
  so the rule is enforced, not remembered.
- An `-rc` prerelease tag is exempt (an RC exists precisely to soak a tree —
  see the RC procedure in `CLAUDE.md`); the count is still printed.
- The escape hatch is **demotion, not waiver**: if a blocker is judged
  acceptable for a release, the judgment is a mob decision that removes or
  changes the label with the reasoning on the issue, and the release seal's
  `known_problems` field records the disposition. The gate itself takes no
  arguments that skip it.

## Amending the set

The set is closed on purpose — a taxonomy that grows ad hoc stops meaning
anything. Adding a class, changing an admission criterion, or changing which
classes block is a mob decision recorded by editing this file and
`scripts/count-release-blockers.sh` in the same commit.
