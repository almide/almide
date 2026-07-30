# Unit 0.41 — Plan: Make fuzz-nightly a Usable Nightly Instrument Again

> 日本語版: [inception.ja.md](./inception.ja.md)

- **Aim**: The first principle of the 0.4x arc ("instruments and the edit loop"):
  instruments before surgery. Until the silent-wrong-code detector runs every night,
  the codegen surgery ahead (0.5x wasm optimizers, 0.6x cranelift) happens without a
  safety net. Prerequisite for Gate 0.50 ("continuously green fuzz is the normal state").
- **Issues**: [#924](https://github.com/almide/almide/issues/924) (primary),
  [#917](https://github.com/almide/almide/issues/917) (residual check and close)

## In three lines

- Nightly fuzz completed only 1 of the last 20 nights. The causes — a build tax and
  mid-run runner reclamation — are understood, and the fix is visible
- Prebuild the fuzzer, split the campaign into short shards, restore the budget, and
  record throughput every night
- Release after 3 consecutive complete nights. Closing #924 itself (14 consecutive nights)
  carries over into 0.42

## Background

From the 2026-07-27 audit (#924): the last 20 nights break down as 1 success, 17 failures,
3 cancelled. Three root causes: (a) the runner reclaims long jobs mid-run; (b) of the
10-minute budget, 4m48s went to building the fuzzer, leaving 2m39s of actual fuzzing;
(c) the budget was then cut to 5 minutes, dropping throughput to ~1,000 programs/night.

Why this weighs more than it looks: 49 of 258 closed issues are in the
silent/miscompile/diverge class (including #727, where one run clustered 478 divergences).
Today's "0 open silent bugs" reflects a stalled instrument, not a clean compiler.
#796 (two consecutive green nights) has never been satisfied.

## Scope

- S1 **Remove the build tax** — cache the fuzzer binary from the release build so the whole
  budget goes to fuzzing
- S2 **Survive runner reclamation** — split the campaign into N short shards (reclamation
  kills long jobs). If still unstable, consider a self-hosted / larger runner
- S3 **Restore the budget and make throughput visible** — return the budget to its pre-cut
  level and record programs/night in the run summary so regressions show
- S4 **Check #917's residuals** — the perf scoreboard and two-sided ratchet landed for the
  native leg after 0.40.2. Verify what remains (whether the wasm leg is in the suite;
  the dated-results practice), add Bolts if anything remains, otherwise close with evidence

## Out of scope

- Fixing the findings fuzz surfaces — that is 0.42 (#796 true green)
- SIMD revival / wasm optimizers — 0.53–0.54 (#929)
- Extending the fuzz lenses (pass-ordering checks etc.) — 0.52 (#912)

## Done-criteria

- fuzz-nightly completes its full budget **3 nights in a row** (= the repair is judged done.
  #924's own closure condition is 14 consecutive nights, so the issue stays open and is
  watched across the 0.42 period. **Separating the release judgment from the issue closure
  is the main thing M0 approves**)
- The run summary shows programs/night, and the numbers confirm the build tax is ~zero
- #917 is closed, or its residuals are Bolts in this Unit's ledger

## Risks

- R1 GitHub-hosted runner reclamation is outside our control → absorb it with sharding.
  If the completion rate does not improve after 3 observed nights, call a human (M6) to
  decide on self-hosted migration
- R2 "3 nights" and "14 nights" cost real time → while waiting, the loop may run ahead on
  independent work such as drafting the next Unit's (0.42) plan — never its ledger,
  per the approval rule

## Proposed Bolts

- B1 Fuzzer prebuild — cache the release-build artifact; remove the build tax from the nightly job
- B2 Shard split — restructure the campaign into N short jobs, eliminating the
  reclamation failure class
- B3 Budget restoration + programs/night in the run summary
- B4 #917 residual check → close or add Bolts
- B5 Observe 3 consecutive complete nights, record the evidence → release

## Approval (M0)

- Status: **approved**
- Approver / date / notes: O6lvl4 / 2026-07-31 / approved in session. The separation of the
  release judgment (3 consecutive complete nights) from #924's closure (14 nights) is approved.
