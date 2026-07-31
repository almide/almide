# Unit 0.49 — Plan: per-module compilation cache

- **Aim**: 0.4x arc — the last piece before the decade gate: an edit recompiles one module,
  not the program.
- **Issues**: [#1003](https://github.com/almide/almide/issues/1003)

## In three lines

#1003 is a full design sketch (typed-IR cache + per-module content-keyed rlib, generalizing
`ensure_runtime_rlib`) with estimates: 4k lines 4.2s → ~1.2s, 20k → ~2.5s, 100k → ~3–4s.
It also carries an explicit stop: "**Do not start until the trigger fires: a real project's
full build exceeds 2-3s (~3,000-5,000 lines single program). No such project exists yet.**"
Done means the trigger is resolved with numbers from 0.46's program, and the cache is built
only if it fired.

## Background

Third trigger-gated row in this decade, and the third with the same discipline: #1002 (0.45,
measured — not fired), #928 (0.47, unmeasurable at current scale), #1003 (here).

The issue is unusually clear about why waiting costs nothing: "This is a pure implementation
change: the module-boundary prerequisite (explicit module interfaces, `almide compile`)
already exists, so deferring accrues no design debt."

Measured today (develop @ b5d530fa, cold `$TMPDIR/almide-run` cleared): `examples/lisp.almd`
(268 lines) builds in **0.61s** cold, **0.26s** warm. Extrapolating #1003's own ~1ms/line
puts the 2–3s trigger at roughly 3,000 lines — which is what 0.46's B3 milestone produces.

**Known risk the issue names**: monomorphization is whole-program, so generics crossing
module boundaries reduce hit rate; worst case (dense generic coupling) degrades the win to
~2x. That is a measurement to make on the real program, not a reason to avoid the design.

## Scope

- S1 Re-measure full build time when 0.46's program passes ~3k and ~5k lines.
- S2 Resolve the trigger in #1003 with those numbers.
- S3 Build the per-module cache ONLY if S2 fired: typed-IR cache keyed by content hash, then
  per-module content-keyed rlib.
- S4 If built: measure the actual speedup against the issue's estimates, including the
  generic-coupling hit rate on real code.

## Out of scope

- Redesigning monomorphization. If dense generic coupling caps the win at ~2x, that is a
  finding to record, not a licence to change mono in this Unit.

## Done-criteria

- #1003 carries the trigger's answer with measured build times at ≥3k lines.
- If S3 fires: a before/after table at 3k / 5k / 10k lines, and the measured hit rate.

## Risks

- **R1 — building on the estimate instead of the measurement.** The issue's numbers are
  extrapolations from a 4,000-fn synthetic file, not from a real module graph. Absorption:
  S1 measures the real program before S3 starts.
- **R2 — the generic-coupling risk turns out to dominate.** Then the cache is worth much less
  than the sketch claims. Absorption: S4 measures hit rate, and a disappointing number is a
  publishable result rather than a failure to hide.

## Proposed Bolts

- **B1** — Measure full build time on 0.46's program at ~3k lines.
- **B2** — Resolve #1003's trigger with those numbers.
- **B3** — Conditional: typed-IR cache keyed by content hash.
- **B4** — Conditional: per-module content-keyed rlib.
- **B5** — Measure the speedup and the generic-coupling hit rate against the estimates.
