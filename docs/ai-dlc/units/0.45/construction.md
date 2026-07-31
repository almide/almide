# Unit 0.45 — Ledger

> Paired plan: [inception.md](./inception.md) — approved 2026-07-31 under the standing
> full-authority directive.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Enumerate the cache layers; produce a reproducible cold state | "Cold" is a state someone can actually produce | **done** | Four layers, below |
| B2 | Measure the premium locally, cold and warm | A number with a stated method | **done** | http 9s vs non-http 0s, all four layers cleared |
| B3 | Measure it in a CI container | — | **done from existing CI data** | See the arithmetic below |
| B4 | Resolve the trigger | #1002 carries the answer with numbers | **done — NOT fired** | Comment on #1002 |
| B5 | Build the keyed rlib cache IF B4 fired | — | **not done, by design** | B4 says the trigger has not fired |

## B1 — the four cache layers

`almide test` does NOT use `almide run`'s project dir, which is why two earlier measurement
attempts read 0s and disagreed with the issue:

| Layer | Path | Used by |
|---|---|---|
| run project dir | `$TMPDIR/almide-run` (or `ALMIDE_RUN_PROJECT_DIR`) | `almide run` |
| **test worker dirs** | `$TMPDIR/almide-test/<worker>` (`src/cli/commands.rs:115`) | **`almide test`** |
| wasm test scratch | `$TMPDIR/almide-wasm-test` | `almide test --target wasm` |
| keyed rlib cache | `$TMPDIR/almide-rtlib-<key>` (`src/cli/cargo_build.rs:525`) | both |

A reproducible cold state clears **all four**. Clearing three and reading 0s was the trap: the
one layer left standing was the one `almide test` actually uses.

Two of my own readings were invalid before this was understood — one used
`ALMIDE_PROJECT_DIR`, which is not a variable the compiler reads (`ALMIDE_RUN_PROJECT_DIR`
is), and one cleared *more* cache and got a *faster* result, which is impossible and was the
signal that the mechanism was not understood. Both are recorded in the plan rather than
quietly dropped.

## B2/B3 — the numbers

All four layers cleared, one binary, one run:

| | cold | warm |
|---|---|---|
| `almide test examples/api-client.almd` (http) | **9s** | 0s |
| `almide test examples/binary-search.almd` (no http) | 0s | 0s |

That reproduces the issue's 8.4s figure and isolates it to the feature-gated runtime build.

CI (`develop` at 4ab8a5e8, fresh containers):

- `Almide spec tests (Rust target)` 79s / 324 files
- `Almide examples tests` 31s / 11 files (3 use http)
- `Cargo tests` 380s

The rlib cache is content-keyed and shared across workers, so a CI container pays the ~9s
**once**, not per file. Against a job whose two spec steps total 110s and whose dominant cost
is 380s of cargo tests, that is roughly **2%**.

## B4 — the trigger has NOT fired

#1002's own condition: "arm this when http/zlib-using test files make `almide test` or CI
measurably slow — not before."

- Edit loop: **zero**. The keyed rlib cache absorbs it after the machine's first build.
- CI: **~9s once per container**, ~2% of the job, and not the bottleneck.

So the Unit closes having built nothing, which is the correct outcome and the one the issue
asks for. Building the cache anyway would add a machine-wide cache layer — more state, more
invalidation surface — to buy 2% of a job that is dominated by something else.

**Sharper re-arm condition** (recorded on #1002): arm when EITHER the http/zlib premium
exceeds 25% of the `Almide spec tests` + `Almide examples tests` wall time in CI, OR a
fresh-dir cold start appears in the interactive edit loop (i.e. something starts clearing
`$TMPDIR/almide-rtlib-*` between edits). The first is measurable from any CI run with the
method above; the second is a design change, not a drift.

## Unit completion

- [x] Every Bolt done with evidence (B5 conditionally not done — the condition was measured)
- [x] The evidence satisfies the plan's done-criteria — S1 → the four-layer table;
      S2 → 9s vs 0s with the method; S3 → the trigger resolved in writing on #1002
- [x] No release: this Unit changed no shipped behaviour. Its output is a decision and a
      measurement, both recorded. A version number for "we measured and correctly built
      nothing" would be noise

## Retrospective (Try)

1. **A cold measurement is only cold if you know every layer.** Two readings were wrong
   before the layers were enumerated, and the second was wrong in a way that LOOKED like
   evidence (clearing more cache made it faster — impossible, and the tell).
2. **When an issue states its own trigger, resolving the trigger IS the work.** The
   temptation was to build the cache because the ladder had a row for it. The row exists to
   ensure the question gets answered, not to guarantee an implementation.
3. **Record the invalid attempts.** Both bad readings are in the plan. A future reader who
   measures 0s will find out why in one page instead of rediscovering it.
