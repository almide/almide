# Unit 0.50 — Plan: the decade gate release

- **Aim**: 0.4x arc — the exit audit. The decade's claims become public numbers under a
  ratchet, or they are not claims.
- **Issues**: [#999](https://github.com/almide/almide/issues/999)

## In three lines

#999 measured build-speed, runtime-perf and safety numbers on 2026-07-30 and observed that
**none of them are in the README** — the only benchmark row is LLM-writability.
A decade gate that publishes nothing measurable is not a gate.
Done means the numbers are public, regenerated from a source rather than hand-typed, and
under a ratchet that fails when they regress.

## Background — the numbers are stale and must be re-measured, not copied

#999's table is from **0.39.0**. This decade shipped v0.42.0 with a single driver, a
concurrency stance, three new contracts and a checker soundness fix. Copying 0.39.0's numbers
into the README would be publishing a claim about a compiler that no longer exists.

Re-measured today (0.42.0, Apple Silicon, `examples/lisp.almd` = 268 lines):

| scenario | time |
|---|---|
| `almide check` | 30 ms |
| build, warm (content-cache hit) | 0.26 s |
| build, cold | 0.61 s |
| build, cold `--target wasm` | 0.28 s |

**Runtime numbers are NOT re-measured, and are therefore not published.** An attempt today
read 0.77s then 0.01s for the same binary — the first was page-in, and `fib(32)` completes
under `time -p`'s resolution. A benchmark that cannot be measured repeatably is not evidence,
and publishing it would be worse than publishing nothing. The proper harness (`lang-bench`)
exists; using it is this Unit's work, not a five-minute re-run.

## Scope

- S1 Publish the build-speed numbers with their method (done — README).
- S2 Re-measure runtime perf with `lang-bench`, at a size the harness can resolve, against
  `rustc -O` on the same machine.
- S3 Publish the safety numbers (contract count, flagged count, proof counts) from the
  generator that already produces them, so they cannot drift.
- S4 Put all three under a ratchet that fails CI on regression, the way the perf-ratio and
  contract gates already do.

## Out of scope

- New optimization work to make a number look better. This Unit publishes what is true.

## Done-criteria

- Every published number states its method and its measurement date.
- No number is hand-typed where a generator could produce it (the claims block already works
  this way — `scripts/gen-claims.sh`).
- A ratchet exists for each, and CI fails on regression.

## Risks

- **R1 — publishing a number measured badly.** Realized once already today (the runtime
  attempt above). Absorption: a number whose repeat runs disagree by 70x does not ship, and
  saying so in the README is better than a footnote.
- **R2 — hand-typed numbers drifting.** Absorption: S3's generator requirement; the claims
  block is the precedent.

## Proposed Bolts

- **B1** — Build-speed numbers in the README with method. **Done.**
- **B2** — Runtime perf via `lang-bench` vs `rustc -O`.
- **B3** — Safety numbers from a generator.
- **B4** — Ratchets for all three.
- **B5** — Gate 0.50 audit: confirm the decade's exit conditions and release.
