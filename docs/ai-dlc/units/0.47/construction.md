<!-- description: Unit 0.47 construction — the incremental-check trigger, measured and re-armed -->
# Unit 0.47 — Construction

The Unit closes having built nothing, which its own Inception named as the likely outcome
(R1) and pre-authorised. What it produces instead is the number #928 was missing: **the exact
project size at which the 50ms budget breaks.**

## S1 / S2 — the metric, measured

**Method**: one binary (`almide 0.45.0`, `sha256:b056d3fe…`), every row measured in ONE run of
one script. Each figure is a **10-run mean** — a single invocation of a ~30ms process is mostly
scheduler noise. Historical sizes of `tools/almide-gates` reconstructed with `git archive` into
separate temp trees; the working tree was never checked out or modified.

A **process-startup baseline** is measured with the same harness (`almide --version`) and
reported separately, because the two consumers of this metric pay it differently: a CLI
`almide check` pays it every time, an LSP server pays it once at boot and never again. Folding
it into one number would answer the wrong question for one of them.

```
startup baseline (almide --version, 10-run mean):  13.3ms
```

| lines | modules | `almide check` (10-run mean) |
|------:|--------:|-----:|
| 382 | 4 | 26.9ms |
| 540 | 5 | 27.3ms |
| 1,095 | 9 | 29.8ms |
| 1,694 | 11 | 30.0ms |
| 2,103 | 15 | 33.7ms |

Least-squares: **3.50 µs/line + 25.5ms fixed**, of which 13.3ms is process startup — so the
compiler's own fixed cost is 12.2ms and its marginal cost is 3.5 µs/line.

## The answer, split in two as R3 demanded

**Latency half — MET, with headroom, and now with an expiry date.**

At the largest project that exists (2,103 lines, 15 modules) a warm `almide check` is **33.7ms
end to end, 20ms of actual work** — against a 50ms budget. Extrapolating the fit:

| consumer | pays startup? | 50ms crossed at |
|---|---|---|
| LSP hover (server resident) | no | **~10,800 lines** |
| `almide check` from a shell | yes | **~7,000 lines** |

| project size | work | CLI total |
|---|---|---|
| 5,000 lines | 30ms | 43ms |
| 10,000 lines | 47ms | 60ms |
| 20,000 lines | 82ms | 96ms |

**Asymptotic half — NOT met, and that is now a measured fact rather than an assumption.**

There is no incrementality at all. Three observations, same run:

- `almide clean` then check, versus the immediate re-run: **128ms vs 124ms** — indistinguishable.
- A second warm re-run: **125ms**. No warm-up effect to find.
- `touch` one leaf module, then check: **131ms**. Touching a module that nothing imports costs
  the same as touching nothing.

Every check is O(project). #928's second half is unmet by construction, not by degree.

## S3 does not fire, and the reason is arithmetic

The layer would buy work proportional to the edit instead of to the project. At 2,103 lines
that saves at most 20ms, of which the fixed 12.2ms is not recoverable by any incrementality
scheme — so the ceiling on the win today is **7.8ms**, against a 50ms budget already met with
33% headroom. Building it now would be building it because the ladder has a row for it, which
is R1.

**What R2 warned about is also now quantified.** The Inception said a 612-line corpus cannot
distinguish O(module) from O(project). That is right, and so is 2,103 lines: at 3.5 µs/line the
whole project-proportional term is 7.4ms, inside the run-to-run spread of a 30ms process. The
scale at which the difference becomes *measurable* and the scale at which it becomes
*expensive* turn out to be roughly the same scale — which is why the re-arm condition below is
stated in lines rather than in milliseconds.

## The re-arm condition, sharpened

#928's original trigger was a latency budget with no size attached, so nothing could tell you
whether you were near it. It is replaced by:

> **Build phase 1 when a single Almide project reaches ~7,000 lines** (the point where a CLI
> `almide check` crosses 50ms) **or ~10,800 lines if only the LSP consumer matters.** Below
> that, the whole project-proportional term is smaller than the compiler's fixed startup cost
> and an incremental layer cannot pay for itself.

Reversible, and cheap to re-test: the fit is five points from `git archive`, and re-running it
against a larger project is one script. What would change the answer is a **super-linear** term
— none is visible across 5.5× of growth — or a fixed-cost reduction that makes the marginal
term dominant sooner.

## Not affected by the 0.46 build-curve retraction

Unit 0.46's build-time table was retracted for clearing the wrong cache. **These numbers are
not.** `almide check` never invokes rustc and produces no artifact, so `$TMPDIR/almide-run` is
not on its path at all — which is also why its cold and warm columns are legitimately equal
where the build's were not. The build figures quoted anywhere in this document refer to the
CORRECTED curve (1.80 ms/line cold, 46 µs/line warm).

The two curves say different things and both are now measured: **checking is cheap and flat;
building is expensive and linear.** That asymmetry is the reason #928's phase 1 can wait while
#1003's cache cannot.

## Done-criteria

- [x] The metric is measured at the largest available project, with the method stated —
      2,103 lines / 15 modules, 10-run means, one binary, one run, startup reported separately
- [x] #928 carries the answer with numbers
- [x] S3's condition evaluated: does not fire; nothing built, as R1 pre-authorised

## Retrospective (Try)

**Keep**: reporting the startup baseline separately. The same 33.7ms is "38% overhead" to an
LSP and "the whole cost" to a shell script, and one number would have quietly answered for the
wrong consumer.

**Keep**: 10-run means. The first single-shot pass read 116–133ms across four targets with no
correlation to size — pure noise dressed as data. It would have supported almost any
conclusion.

**Change**: a trigger phrased as a latency budget with no size attached is not actionable —
you cannot tell whether you are near it without doing the measurement the trigger was supposed
to defer. Phrase deferred work in the units of the thing that will grow.
