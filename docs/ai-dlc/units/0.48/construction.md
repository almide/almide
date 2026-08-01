<!-- description: Unit 0.48 construction — #928 steps 3 and 4, resolved by 0.47's measurement -->
# Unit 0.48 — Construction

This Unit's Inception said its first deliverable was a dependency, stated honestly: it could
not be planned before 0.47 answered whether #928 step (2) was needed. **0.47 answered.** This
document is what that answer does to steps (3) and (4).

## S1 — the dependency, resolved

Unit 0.47 measured `almide check` across five sizes of `tools/almide-gates`, one binary, one
run, 10-run means, with process startup reported separately:

**3.50 µs/line + 12.2ms compiler fixed cost** (plus 13.3ms process startup for a CLI caller).
At the largest project that exists — 2,103 lines, 15 modules — a check is 33.7ms end to end
against a 50ms budget. The 50ms line is crossed at ~7,000 lines for a CLI and ~10,800 for a
resident LSP.

Step (2) does not fire. So this Unit's scope collapses to the LSP half, exactly as S1
anticipated — "and that is a real outcome, not a deferral."

## S2 — the LSP half also does not fire, and the arithmetic is the same

The Inception argued the LSP half is "worth doing even if the compiler-side fingerprinting is
not," because it carries its own latency budget. That argument is sound in general and does
not survive the numbers here.

An LSP that holds the analyzed workspace and re-analyzes only the dirty module plus its reverse
dependencies buys back the **project-proportional** part of a check. At 2,103 lines that part
is 7.4ms. The remaining 12.2ms is per-invocation compiler fixed cost, which holding the
workspace does not remove — it is setup the server would still do once per analysis pass.

So the ceiling on the LSP win today is **7.4ms against a 50ms budget already met with 33%
headroom**, and the work required is the largest single item in the arc: workspace state,
reverse-dependency tracking, and cancellation, all of which are new machinery with their own
failure modes.

**A caveat worth stating rather than burying**: cancellation is not only a latency feature. An
LSP that cannot cancel an in-flight analysis will queue work behind a keystroke burst, and that
shows up as latency far above the steady-state number no matter how fast a single pass is. This
Unit is NOT claiming the LSP is fine; it is claiming that the *incremental re-analysis* half of
#928 step (3) does not pay for itself yet. If LSP responsiveness becomes a complaint, the first
thing to build is cancellation, and it is independent of everything measured here.

## S3 — memoization stays last, per the issue's own rule

#928 sequences step (4) as "only if (2)+(3) prove insufficient." Neither has been built, so
neither has proven anything. Salsa-style memoization remains out of scope on the issue's own
terms, not on this Unit's judgement.

## The two rows are folded, and why

0.47 and 0.48 were two ladder rows over **one issue with four sequenced steps** — the split was
a sizing decision, which the 0.48 Inception says outright. The measurement collapses both to
the same answer from the same five data points, so keeping them as two shipping events would
put a tag on a document that adds no artifact. They ship together, and #928 carries one
comment rather than two saying the same thing.

Recorded as reversible: if the re-arm condition below fires, the arc resumes at step (2) and
step (3) follows it, in the issue's order.

## The re-arm condition

Inherited from 0.47, unchanged, and it governs both steps:

> **Build phase 1 when a single Almide project reaches ~7,000 lines** (CLI `almide check`
> crosses 50ms) **or ~10,800 lines if only the LSP consumer matters.**

Plus one that belongs to this Unit specifically:

> **Build cancellation when LSP responsiveness is reported as a problem**, independent of the
> steady-state numbers — a queue behind a keystroke burst is not visible in a per-pass
> measurement.

## Done-criteria

- [x] S1's dependency resolved: 0.47's measurement says step (2) does not fire
- [x] S2 evaluated with numbers: the LSP incremental-re-analysis half does not fire either
      (7.4ms ceiling against a 50ms budget met with 33% headroom)
- [x] S3 stays out of scope on #928's own sequencing rule
- [x] #928 carries the answer (one comment, covering both rows)

## Retrospective (Try)

**Keep**: the Inception naming its dependency as its first deliverable instead of guessing.
Planning step (3) in detail before (2) was measured would have produced a design for work that
does not need doing.

**Change**: two ladder rows over one sequenced issue create a false expectation of two
outcomes. Sequenced steps inside one arc want one row with internal gates, not two rows with a
dependency between them.

**Note for whoever resumes this**: the thing most likely to make the arc real is not project
size — it is a second consumer. Every number here is `almide check` invoked once. An editor
typing at 10 keystrokes per second asks a different question, and no measurement in this Unit
answers it.
