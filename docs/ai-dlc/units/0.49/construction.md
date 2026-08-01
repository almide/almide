<!-- description: Unit 0.49 construction — the edit loop measured, two wrong attributions retracted -->
# Unit 0.49 — Construction

The trigger fired. This document records what is measured, what is NOT yet attributed, and two
attributions I published and had to retract — because both were the same mistake, and the
mistake is the transferable part.

## The trigger fired (this is settled)

#1003's stop condition: *"Do not start until the trigger fires: a real project's full build
exceeds 2-3s (~3,000-5,000 lines single program)."*

**Method**: one binary (`almide 0.45.0`, `sha256:b056d3fe…`), all rows in ONE run of one
script. TRUE cold = `rm -rf $TMPDIR/almide-run` **and** `almide clean`, before **every** row.
Historical sizes of `tools/almide-gates` via `git archive` into temp trees.

| lines | cold | warm |
|------:|-----:|-----:|
| 382 | 0.93s | 0.37s |
| 540 | 1.15s | 0.37s |
| 1,095 | 2.48s | 0.40s |
| 1,694 | 2.97s | 0.42s |
| 2,103 | **4.19s** | 0.45s |

**cold: 1.80 ms/line + 0.25s fixed. warm: 46 µs/line + 0.35s. Ratio at 2,103 lines: 9.3×.**

2s is crossed at **~970 lines**, 3s at ~1,530 — both far below the estimated 3,000–5,000.
#1003's own ~1 ms/line estimate was closer to the truth than my first measurement.

## The edit loop, which is what actually matters

A cold build happens once. What a developer pays repeatedly is the rebuild after a one-module
edit. Measured on the same program, with an edit that **changes the program's output** so it
cannot be eliminated:

| action | time |
|---|---|
| `almide run` after an output-changing edit | **3.98s** |
| `almide run` with no change (cache hit) | 0.45s |
| `almide run` after a COMMENT-only edit | 0.44s — a cache HIT |
| `almide test` on a single file | 0.26s |

The comment-only row is informative: the cache is keyed on the **generated Rust**, so an edit
that does not change the emitted code is free. Every semantic edit pays the full ~4s.

`CARGO_INCREMENTAL=1` does not help (4.12s vs 4.33s, inside the noise). Unsurprising: the
generated crate is a **single 7,733-line `src/main.rs`**, so there is nothing for
incrementality to partition.

## Two retractions, same mistake twice in one session

**Retraction 1 — the first build curve.** Reported cold and warm as indistinguishable at every
size and concluded neither #1003 nor #1002 could pay for itself. `almide clean` clears the
DEPENDENCY cache; compiled artifacts live in `$TMPDIR/almide-run` and survived every "cold"
row. Every number was a warm build wearing a cold label. #1003 was closed on it and is
reopened. Corrected table above.

**Retraction 2 — "97.6% of the edit loop is the v1 verified native render."** Measured
`almide run --no-verified` at 0.11s against 3.98s and concluded the verified render was
essentially the whole cost. **`--no-verified` is a REMOVED flag.** It prints an error and
exits. 0.11s was the error message.

Both are the same error: **taking a fast number as evidence of fast work, without checking
that any work happened.** The first measured a cache hit; the second measured an early exit.
The tell in both cases was a number that was too good — a cold build exactly equal to a warm
one, and a 36× speedup from one flag — and in both cases the right move was to ask what the
fast path actually did before reporting what it meant.

## ATTRIBUTED — the trace answered it, and the answer was not a cache

The section below preserved the open number and the hypothesis. Both are now resolved by
instrumenting the pipeline instead of reconstructing it, which is the method the retractions
kept pointing at.

`PhaseTimer` in `src/cli/run.rs`, behind `ALMIDE_TIME_PHASES=1`, on an output-changing edit to
the 2,103-line program:

```
[phase] frontend+emit          323ms   (cumulative     323ms)
[phase] v1-native-render        16ms   (cumulative     340ms)
[phase] cargo                 3092ms   (cumulative    3432ms)
```

**cargo is 90% of the edit loop.** The v1 native render — which retraction 2 wrongly blamed for
97.6% — is 16ms. And the hypothesis in the section below was right: the standalone
`cargo build` that read 0.32s was a fourth cache hit.

### And the 3.1s is opt-level, not the cache design

With a real Almide edit before every row so nothing can hit a cache, reading the cargo phase
from the trace:

| dev `opt-level` | cargo phase |
|---|---|
| 0 | **724ms** |
| 1 (the default until now) | 3,215ms |
| 2 | 3,970ms |

The generated crate is a single 7,733-line `main.rs`, and optimising it is the entire cost.
`CARGO_INCREMENTAL=1` cannot help — there is nothing to partition.

**So the edit loop was paying for optimisation it throws away**, and #1003's cache would have
been an elaborate mechanism for avoiding work that should not be done in that path at all.

### The change, and what was checked before making it

`[profile.dev] opt-level` 1 → 0 in all three generated-Cargo.toml templates.

| | before | after |
|---|---|---|
| `almide run` after a one-module edit | 4.07s | **1.47s** |
| — of which cargo | 3,215ms | **1,127ms** |

Verified before changing, because "it is faster" is not sufficient reason to change a profile:

- **Recursion depth is unaffected.** 200,000-deep NON-tail recursion returns the same answer at
  both levels. This was the most plausible reason for the original `opt-level = 1` — the
  language's idiom guide pushes recursion over loops — and it does not hold.
- **`almide test` is unaffected**: 7.1s for `spec/lang` at both levels, cold or warm, because
  the test path runs on the wasm leg and never reaches cargo.
- **The runtime cost is real and small**: the produced binary is ~1.33× slower (671ms vs 506ms
  on the same program). The trade is ~2.5s of compile against ~0.17s of execution per edit.
- **`--release` and `almide build` are untouched**, so the optimised profile is still one flag
  away and is still what ships.

Reversible: the numbers are in the template's own comment, so raising the level back requires
disagreeing with a measurement that is written next to the value.

## What was NOT attributed — kept for the record

The 3.98s edit loop does not decompose into anything I have measured:

| component, measured separately | time |
|---|---|
| `almide check` (frontend only) | 0.12s |
| `almide … --target rust` (emit Rust source) | 0.42s |
| `cargo build` (debug) after touching `src/main.rs` in the run dir | 0.32s |
| **sum** | **0.86s** |
| **actual `almide run` after a real edit** | **3.98s** |

**~3.1s is unaccounted for.** Also established: the run path uses the **debug** profile
(artifacts land in `target/debug`; 11 there against 2 in `target/release`), so the
`lto = true, codegen-units = 1` release profile — which does cost 5.83s on this crate — is
NOT what the edit loop pays.

The leading hypothesis is that the third row is itself another cache hit: a standalone
`cargo build` after `touch` may reuse artifacts that a genuinely-changed `main.rs` cannot. That
is retraction 1 and 2's mistake a third time, which is precisely why it is written down as a
hypothesis instead of a finding.

**Next step, and it is the method that worked for R3**: stop reconstructing the total from
separately-invoked commands and instrument `almide run` itself — a timing print around the
frontend, the emit, and the cargo invocation, behind an env guard, one build. Reconstruction
has now produced two wrong attributions in one session; a trace has produced none.

## Scope, once the 3.1s is attributed

Deliberately not decided yet. #1003 designs a typed-IR cache plus per-module content-keyed
rlibs, and that is the right answer **if** the cost is rustc compiling one monolithic crate.
If the cost is inside the Almide compiler, the same design would be an elaborate cache around
the wrong thing — which is what building it now, on an unattributed number, would risk.

What is already known and does constrain the design:

- The cache key is the **whole generated program**, so any semantic edit anywhere invalidates
  everything. Per-module keying is the direction regardless of where the 3.1s lives.
- The generated Rust is **one file**. Splitting it into a crate per Almide module is the
  prerequisite for rustc doing less work, and it is also what makes the cost attributable —
  a per-module build is a per-module timing.
- #1003 names the risk itself: monomorphization is whole-program, so generics crossing module
  boundaries reduce hit rate; worst case degrades the win to ~2×. That is a measurement to make
  on the real program once modules are separable.

## Done-criteria

- [x] The trigger is resolved with numbers from 0.46's program — **fired**, at ~970 lines
- [x] #1003 carries the corrected measurement and is reopened
- [x] The cost is attributed: cargo 90%, and within cargo it is `opt-level`
- [x] The edit loop is **2.8× faster** (4.07s → 1.47s) by removing optimisation work from a
      path that discards it — not by caching it
- [ ] #1003's typed-IR + per-module rlib cache: **still open, and now correctly scoped.** With
      opt-level fixed, the remaining cargo phase is 1,127ms on 2,103 lines and still
      whole-program. The cache is the answer to THAT, and the trigger for building it should be
      re-derived from the new curve rather than inherited from the pre-fix one

## Retrospective (Try)

**Change, most important**: a suspiciously good number is a claim about the instrument. Cold
== warm, and 3.98s → 0.11s from one flag, were both reported before asking what the fast path
did. Two retractions in one session from one habit.

**Keep**: retracting in the same place the claim was published. #1003 was reopened, and #1001
and #1002 carry corrections rather than quietly-updated numbers.

**Keep**: refusing to start the implementation on an unattributed number. The trigger firing
says the problem is real; it does not say where the problem is.
