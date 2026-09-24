# Issue label taxonomy (#1482)

Before this file existed the tracker had category labels (`bug`, the wasm
category, `fuzz`, …) but **no severity class**, so "are there open release
blockers?" was not a queryable question — every release answered it by memory.
This file defines the closed severity set, the admission criterion for each
class, and the one rule that consumes them — and below it, the area axis that
answers the second un-queryable question: **which crate is this issue in?**

## The closed set (severity)

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
  they take `enhancement` / `documentation` plus an area label (below).

## The blocking rule

**A FINAL release tag must not ship while an issue labeled `I-unsound`,
`I-miscompile`, `I-divergence`, or `regression` is open.**

- Measured by [`scripts/count-release-blockers.sh`](../../scripts/count-release-blockers.sh);
  the release workflow runs it with `--gate` before creating a final release,
  so the rule is enforced, not remembered. The number it prints is the count
  of **distinct** open issues: one issue carrying two classes is one blocker,
  not two (#2400). The per-label breakdown above the total shows the overlap.
- An `-rc` prerelease tag is exempt (an RC exists precisely to soak a tree —
  see the RC procedure in `CLAUDE.md`); the count is still printed.
- The escape hatch is **demotion, not waiver**: if a blocker is judged
  acceptable for a release, the judgment is a mob decision that removes or
  changes the label with the reasoning on the issue, and the release seal's
  `known_problems` field records the disposition. The gate itself takes no
  arguments that skip it.

## The area axis (which crate)

Severity says how much an issue hurts; it never said **where the work is**. A
reader who opens the tracker cold could not ask "what is open in the wasm
emitter?" or "what is open in the frontend?" without reading 77 bodies. The
`A-*` axis answers that, and each label's GitHub description names its crates
so the mapping needs no second document:

| label | crates / paths |
|---|---|
| `A-frontend` | `crates/almide-frontend`, `almide-syntax` — parse, check, diagnostics, import table |
| `A-ir` | `crates/almide-ir`, `almide-mir`, `almide-optimize`, `almide-spine` |
| `A-codegen` | `crates/almide-codegen` — nanopasses, Rust lowering, native borrow/RC inference |
| `A-wasm` | `crates/almide-wasm`, `almide-wasm-run`, `almide-wasm-vm` |
| `A-runtime` | `runtime/rs`, `crates/almide-rt-core` — native intrinsics |
| `A-interp` | `crates/almide-interp` — the third oracle |
| `A-stdlib` | `stdlib/*.almd`, `crates/almide-types` stdlib registry |
| `A-driver` | `crates/almide-driver`, `src/` — CLI: run/build/test/check/fmt/bench |
| `A-perf` | `research/benchmark/perf`, `scripts/check-perf-ratio.sh` |
| `A-ci` | `.github/workflows`, `scripts/*.sh` gates, ledgers and ratchets |
| `A-modules` | module and package system — resolution, submodules, MVS, dialect epochs |

Rules of use:

- **One** `A-*` per issue; two only when the work genuinely spans both crates.
  If no label fits, the issue has not said where it lives — that is a gap in
  the issue, not a missing label.
- The label must agree with the issue's own `触る場所` line (see below). They
  are the same claim written twice, and a disagreement means one of them is
  stale.
- `A-wasm` is the former `wasm-codegen`, renamed in place so its issues carried
  over. No script or workflow consumed that name; `fuzz`, `fuzz-findings` and
  `fuzz-perf` ARE consumed by `.github/workflows/fuzz-nightly.yml` and must
  keep their spelling.
- The area axis is **not** closed the way the severity set is: adding a crate
  to the repo may add a label. Removing or re-scoping one still belongs in this
  file, in the same commit as the `gh label` change.

## The issue header (what to do, in the issue)

Every open issue carries a quoted block at the top of its body, above the
original text, so that a reader who opens it alone knows what to do:

```
> **状態** — <BLOCKER / 実装 / 計測器・ゲート / 設計判断 / 追跡 / 調査>・<着手可 / 前提待ち: #NNNN / 判断待ち>
> **一行で** — one sentence
>
> **やること**
> 1. the first concrete action
>
> **完了条件** — a verifiable condition: a gate green, a measured number, a fixture, a diagnostic
> **根拠** — the measurement / file:line / PR that makes this real, or `未測定`
> **触る場所** — the 2-4 paths a fix would edit
```

`状態` takes one of six values, and each admits something specific — the point
is that a reader scanning the tracker can tell what KIND of work an issue is
before reading it:

| 状態 | admits exactly |
|---|---|
| `BLOCKER` | carries `I-unsound` / `I-miscompile` / `I-divergence` / `regression`: a final release tag is refused while it is open. Agrees with the labels in both directions — a `BLOCKER` without one of those labels, or one of those labels without `BLOCKER`, means one of the two is stale |
| `実装` | a change to compiler, stdlib or runtime code whose target shape is known |
| `計測器・ゲート` | the work IS a gate, a ratchet, a manifest or a measurement harness — including fixing one that lies |
| `設計判断` | needs a ruling before any code can be written (usually also labeled `mob`) |
| `追跡` | an umbrella whose items are other issues, or a ledger a bot maintains |
| `調査` | the subject is not pinned down yet; step 1 is a measurement, not an edit |

The second half of the line is the gate on starting: `着手可`, `前提待ち: #NNNN`
(only a true prerequisite — being *related* is not one), or `判断待ち`.

- `根拠` is the load-bearing field: it says whether the issue rests on a
  measurement or on a plausible sentence. `未測定` is a legitimate value and is
  more useful than a confident guess — it tells the next reader that step 1 is
  a measurement.
- `完了条件` never says "it feels done". If the issue cannot state a verifiable
  finish, its 完了条件 is what it would take to *decide* — and that is the work.
- The block is a wrapper: it adds no claim the body does not already contain.
  New evidence goes in a comment, as before.

## Amending the set

The severity set is closed on purpose — a taxonomy that grows ad hoc stops
meaning anything. Adding a class, changing an admission criterion, or changing
which classes block is a mob decision recorded by editing this file and
`scripts/count-release-blockers.sh` in the same commit.
