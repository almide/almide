# Unit 0.45 — Plan: resolve #1002's trigger, then build only if it fired

- **Aim**: 0.4x arc — the edit loop should not scale with project freshness. But #1002 is a
  CONDITIONAL: it names its own trigger and says "not before".
- **Issues**: [#1002](https://github.com/almide/almide/issues/1002)

## In three lines

#1002 asks for a machine-wide rlib cache for the feature-gated runtimes (http/zlib), and
ends with an explicit trigger: "arm this when http/zlib-using test files make `almide test`
or CI measurably slow — **not before**."
So the Unit's first deliverable is not the cache; it is a measurement that resolves the
trigger either way, recorded so the next person does not re-litigate it.
Done means the trigger is answered with numbers, and the cache is built only if the answer
is yes.

## Background — and an inconclusive first attempt (2026-07-31)

The issue's headline figure is **8.4s wall on a fresh project dir** (Apple Silicon,
2026-07-30), against 0.40s warm.

Attempted to reproduce, and **could not**:

| what was measured | result |
|---|---|
| `almide test spec/stdlib/zlib_test.almd`, `ALMIDE_PROJECT_DIR` to a new dir | 3s |
| same file, warm | 0s |
| 3 http examples vs 3 non-http examples, warm | 0s vs 0s |
| http example after `rm -rf $TMPDIR/almide-run` (1.4G) | 1s vs 0s non-http |
| http example after clearing `almide-run` AND every `almide-rtlib-*` | **0s** |

The last row is the problem. Clearing MORE cache made it FASTER, which cannot be true — so
something else is absorbing the rustls build and I have not identified what. Candidates not
yet ruled out: `~/.cargo` registry state, a target dir inside the repo that `almide test`
reaches, or `almide test`'s per-file `project_dir_override` resolving somewhere unexpected.

**This is recorded as inconclusive rather than as "the trigger has not fired."** A
measurement whose mechanism is not understood is not evidence — the same rule that retracted
the P2 bracket earlier in this cycle. The 0s reading is more likely to be an artifact of my
setup than a real change in the compiler.

CI numbers, which ARE trustworthy (fresh containers, `develop` at 4ab8a5e8):

- `Almide spec tests (Rust target)`: **79s** for 324 files (0.24s/file)
- `Almide examples tests`: **31s** for 11 files (2.8s/file) — 3 of the 11 use http
- `Cargo tests`: **380s** — 4.8× the two spec steps combined, so neither is the CI bottleneck

## Scope

- S1 Identify what actually caches the feature-gated runtime build, so a cold measurement is
  reproducible. Without this nothing else in the Unit is trustworthy.
- S2 Measure the http/zlib premium on a genuinely cold machine state, and separately in a CI
  container.
- S3 Resolve the trigger in writing: fired or not, with the numbers.
- S4 Build the keyed rlib cache ONLY if S3 says fired.

## Out of scope

- The optional follow-up in #1002 (shipping prebuilt rlibs as release assets keyed by rustc
  version). That is a distribution question, not an edit-loop one.

## Done-criteria

- The caching layers are enumerated, so "cold" is a state someone can actually produce.
- The premium is a number with a stated method, on both a local cold state and CI.
- #1002 either carries the implementation or carries a dated "trigger not fired" note with
  those numbers and a sharper re-arm condition.

## Risks

- **R1 — building the cache because the ladder has a row for it.** The issue says not to.
  Absorption: S4 is explicitly conditional, and the Unit is allowed to close having built
  nothing.
- **R2 — trusting a cold measurement that is not cold.** Already realized once, above.
  Absorption: S1 comes first, and no number counts until its mechanism is understood.

## Proposed Bolts

- **B1** — Enumerate the cache layers; produce a reproducible cold state.
- **B2** — Measure the premium locally, cold and warm, with the method written down.
- **B3** — Measure it in a CI container (a workflow-dispatch run is enough).
- **B4** — Resolve the trigger in #1002 and in this ledger.
- **B5** — Conditional: build the keyed rlib cache if B4 says fired.
