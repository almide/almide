# Unit 0.46 — Plan: a real Almide program at scale

- **Aim**: 0.4x arc — this is the row the trigger-gated backlog depends on. #1003 (module
  cache) and #1002 (rtlib) both have conditions phrased in terms of a real project's build
  time, and nothing can currently trip them.
- **Issues**: [#1001](https://github.com/almide/almide/issues/1001)

## In three lines

There is no substantial standalone Almide PROGRAM, so every scale claim and every
scale-triggered optimization is extrapolation.
Building one (~10k lines, something the ecosystem actually needs) turns the trigger-gated
backlog from guesswork into measurement, and produces the first "written in Almide" proof
point.
Done means the program exists, builds, is used, and its build-time profile is recorded.

## Background — one correction to the issue

The issue says "no real Almide program anywhere near that size exists". Measured on develop
(2026-07-31), that is not quite right:

| Corpus | Lines of `.almd` |
|---|---|
| `stdlib/` | **59,548** |
| `spec/` | 41,245 |
| `examples/` | 3,227 (largest single file: `lisp.almd`, 268) |

So a 59k-line real Almide codebase DOES exist and is self-hosted. What it is not, is a
**program**: it is the bundled library, compiled through `bundled_source()` rather than
through the module/package system a user project uses. That distinction is the actual gap —
not the line count. The Unit should say so, because "we have no Almide at scale" is false and
repeating it weakens the honest claim underneath it.

Build-time baseline, same day: `examples/lisp.almd` (268 lines) builds in **2.8s cold**
(cleared `$TMPDIR/almide-run`) and **0.4s warm**. The cold figure is dominated by
cargo/rustc, not by Almide's front half — which is exactly why #1003's trigger ("full build
> 2–3s in a real project") cannot be read off anything that exists today.

## Scope

- S1 Pick the program. It must be something the almide/almide-dojo ecosystem actually needs,
  so the effort is not throwaway — the issue's candidates are task-bank tooling, a docs-site
  generator, or an `.almd`-aware code-search/index tool.
- S2 Build it in Almide, using the module system and package system as a user would (NOT the
  bundled path).
- S3 Record the build-time profile at 1k / 5k / 10k lines, so the curve is measured rather
  than assumed linear.
- S4 Feed the result back into #1003's and #1002's trigger conditions with data.

## Out of scope

- Implementing #1003 or #1002. This Unit produces the signal; those Units consume it.
- Rewriting anything that already works in another language. The point is a NEW program at
  scale, not a port for its own sake.

## Done-criteria

- The program exists in a repo, builds green, and is actually used by something.
- Its line count is ≥ 10k of `.almd`, excluding generated code.
- A build-time table at three sizes, with the method stated (cold vs warm, which cache
  layers were cleared — see Unit 0.45's four-layer table).
- #1003 and #1002 each carry a comment saying whether their trigger fired, with the numbers.

## Risks

- **R1 — this is a multi-session build, not a Bolt.** Absorption: S1 lands first as its own
  Bolt, so the Unit has a reviewable decision before any code. Sizing the program is the
  first real decision, not an afterthought.
- **R2 — picking a throwaway target to hit the line count.** A 10k-line program nobody uses
  measures compilation but not the diagnostics, module system, or ergonomics that scale is
  supposed to stress. Absorption: S1's criterion is "something the ecosystem needs", and the
  Unit does not proceed to S2 until that is answered concretely.
- **R3 — discovering compiler bugs mid-build and losing the thread.** Likely, and desirable —
  that is part of what dogfooding buys. Absorption: findings go to
  `docs/roadmap/active/` with their repro, and the Unit continues rather than forking.

## Proposed Bolts

- **B1** — Choose the program and write down why, with the shape and rough module layout.
- **B2** — Skeleton: module/package layout, builds green, ~1k lines. Record build time.
- **B3** — Grow to ~5k lines of working functionality. Record build time.
- **B4** — Reach ~10k lines. Record build time; plot the curve against the assumed linear.
- **B5** — Resolve #1003's and #1002's triggers with the measured numbers.
