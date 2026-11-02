# Issue Ledger Burn-down — the completeness campaign's remaining-work gauge

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](issue-ledger-burndown.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](issue-ledger-burndown.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](issue-ledger-burndown.zh-CN.md)



> Source: [docs/roadmap/active/issue-ledger-burndown.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-ledger-burndown.md), pinned to `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader translation of a historical roadmap snapshot; it is not automatically synchronized. Dates, status, estimates, measurements and plans refer to the source record, not a new implementation or independent verification. Historical approval requests and recommended commands are not authorization to execute them now. The original is authoritative.

> Source description metadata: The issue-ledger burn-down gauge: keeping the open-issue count an honest measure of remaining work.

> As of 2026-06-11. The ledger → tracker injection is complete: **the number of open GitHub issues = known remaining completeness work**. Closure discipline = eliminate the entire class (each issue already states structural closure conditions; a patch does not close it). Stopping condition = #535's loop-until-dry.

## Results (the two days 2026-06-10–11)

Releases: v0.26.20 → **v0.27.3** (6 releases). Closed: the #484-490 group + #500/501/502/505 + #522/523/524/525/526/527/517 (all by eliminating the class).
PRs: #487–#543. Matrix gates 0→29 cells; churn gates 0→3;
contracts 58→67 (including C-066); wasm memory model “everything leaks” → “O(1) by default”.

## Landscape of the remaining 15 issues

### Week-scale — 8 issues (1 issue = 1 focused session)

| issue | Work | What changes when closed |
|---|---|---|
| #514 | Resolve fan race/any/map-err divergence (align or contract) | The silent-wrong monitoring slot is cleared |
| #515 | §3 metamorphic binding gate | let⟺assign acceptance equivalence becomes machine-checked |
| #516 | §9 wire interp as a third adjudicator into the fuzzer (Oracle trait) | Bugs where both targets are equally wrong become visible |
| #521 | §8 stdlib @semantics manifest | doc↔impl agreement becomes a regen-and-diff gate |
| #531 | §4 2c canonicalize CopyClass (with behavior review) | 4 Copy predicates → 1 |
| #532 | §7 fmt multi-module re-typechecking + §10 release promotion | Complete gate coverage |
| #533 | §13+PR-3 divergence queue + []-typing | Zero recorded cross-target divergences |
| #534 | Specification-contradiction burn-down (??/Never/fan order/empty pages) | prose↔behavior agreement |

### Weeks-scale (backbone) — 6 issues

| issue | Work | Scale |
|---|---|---|
| #528 | QualifiedRef newtype (mechanize #433, apply the §4 cycle) | 1–3 weeks |
| #529 | WasmIR intermediate layer (make emitter structural invariants hold by construction) | 2–3 weeks |
| #530 | Promote the ALS normative specification (CG-1) | 1–2 weeks |
| #518 | construct-from-temp overcounting (Koka-style precise dup/drop) | Research-oriented |
| #519 | Allocator policy (monotonically growing acc × first-fit) | Research-oriented |
| #520 | Extend runtime frees in Lean (proof of the allocator state machine) | Research-oriented |

### Process — 1 issue

- **#535 loop-until-dry**: rotate the lens used for hole-hunting; only after 2–3 consecutive empty rounds is it “complete to the best of current knowledge”. Round 1 (multi-truth / silent-fallback / rule-bypass) found 6 classes → all handled. **Round 2 has not been run**. Candidate next lenses: pass-order assumptions, checker-accepts-lowering-reinterprets, interp semantic differences, diagnostic divergence, and host-environment dependencies other than hash order.

## Estimates

- Zero known holes (all week-scale issues cleared): **4–7 days** (at the observed pace).
- A state that can be called “complete” (3 backbone items + a dry hunt): **another 3–6 weeks**.

## Recommended order

#515/#516 (designed, ready to start) → #514 (clear the silent-wrong slot) → #531/#532/#533/#534
→ hunt round 2 → backbone (#528 → #529 → #530).
