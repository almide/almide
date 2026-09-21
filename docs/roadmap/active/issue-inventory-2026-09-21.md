# Open-issue inventory, 2026-09-21 — 78 issues under one header

Snapshot taken the night before the v0.63.0 final tag, after every open issue
received the common header (one `A-*` crate label, a state from a closed set,
a one-line summary and its evidence; the axis and the header are documented
in `docs/project/ISSUE-TAXONOMY.md` by #2417, and the filling spec the seven
agents worked from is `issue-inventory-2026-09-21/issue-header-spec.md`). The numbers
are the counter's own self-check on the 78 open issues at that moment, not a
hand tally: `issue-inventory-2026-09-21/render-issue-index.py` produced
`issue-inventory-2026-09-21/issue-index-2026-09-21.txt`; re-run it rather than editing this page when
they move.

```
open issues: 78   with header: 78   without: 0
  no A-* label:                                0
  state outside the closed set:                0
  state=BLOCKER vs I-*/regression mismatch:    0
  two A-* labels: 7 (#1423 #1479 #1480 #2141 #2143 #2289 #2387 — each really spans two crates)
```

One instrument bug was hit on the way and is recorded here so its false
reading is not quoted later: the state name `計測器・ゲート` contains the `・`
separator, so a split on the first `・` truncated it and the counter reported
**"25 outside the closed set"**. That was the parser, not the data; it now
matches the known set longest-first by prefix.

## Release blockers (4)

The set `count-release-blockers.sh` gates the final tag on. The script prints
**7** because it sums per-label counts and #2400 carries two blocker labels;
the distinct count is 4.

| issue | crate | state on 2026-09-21 |
|---|---|---|
| #2395 | A-wasm | PR #2399 in flight |
| #2397 | A-wasm | PR #2409, draft, held behind #2404 (same ledger rows) |
| #2398 | A-wasm | PR #2404 in flight; rebase after #2399 lands |
| #2419 | A-wasm | fixed on a branch; judge side almide/als#87 merged 2026-09-21, PR #2420 |

Landing order: #2399 → #2404 → #2409; #2420 is independent of the three.

## By state

| state | count | notes |
|---|---|---|
| BLOCKER | 4 | above |
| implementation | 32 | prerequisite-blocked 3 (#1005→#1003, #1584→#1696, #2147→#2149); decision-blocked 3 (#1107, #1696, #2010); **26 startable** |
| instrument / gate | 25 | decision-blocked 3 (#924, #1963, #2382); prerequisite-blocked 1 (#2009→#1997); **21 startable** |
| design decision | 7 | #1460 #1479 #1480 #1490 #1679 #2143 #2162 — every one needs a ○× |
| tracking | 5 | #586 #1514 #1628 #1998 #2302 |
| investigation | 5 | #2078 #2099 #2150 #2156 #2415 — step 1 is a measurement |

## By crate

| label | count | issues |
|---|---|---|
| A-ci | 25 | #586 #924 #1423 #1469 #1508 #1514 #1617 #1998 #2009 #2141 #2143 #2146 #2152 #2163 #2380 #2381 #2382 #2387 #2390 #2391 #2400 #2402 #2403 #2405 #2406 |
| A-wasm | 22 | #1315 #1584 #1628 #1696 #1710 #1997 #2010 #2099 #2141 #2143 #2150 #2156 #2312 #2318 #2387 #2395 #2397 #2398 #2407 #2408 #2411 #2419 |
| A-codegen | 9 | #1331 #1480 #2044 #2045 #2162 #2289 #2293 #2393 #2410 |
| A-driver | 8 | #1003 #1490 #2092 #2147 #2148 #2151 #2153 #2372 |
| A-perf | 6 | #1963 #2142 #2157 #2302 #2331 #2415 |
| A-frontend | 5 | #1388 #1456 #1479 #1589 #2149 |
| A-stdlib | 5 | #1107 #1423 #1460 #2078 #2289 |
| A-ir | 3 | #1005 #1479 #1480 |
| A-runtime | 1 | #1679 |
| A-modules | 1 | #2332 |

A-ci 25 + A-perf 6 = 31 issues are instruments and gates; the language and
compiler proper (wasm 22 + codegen 9 + frontend 5 + ir 3 + stdlib 5) is 44.
Same order of magnitude, which is the stated priority (quality gates above
language features) showing up in the backlog's shape.

## The A-ci 25, sorted by what closing them takes

The question this section answers: can the CI bucket be cleared in one sweep?
Fourteen can. The rest are programmes or need a decision first.

**One-PR gate fixes, no design question open (14).** Each is a script or
workflow change with a measurable before/after; they can run as parallel
single-issue PRs once the improved CI (#2386, #2416, #2418) is on develop.

| issue | what the fix is |
|---|---|
| #2400 | count distinct issues, not per-label sums |
| #2403 | `check-als-pin.sh` compares statements as well as ids |
| #2406 | validate every `C-NNN` citation against the ledger; fix the one dangling citation in stdlib |
| #2405 | manifest generators refuse a stale tree; the gate reads the field it currently skips |
| #2402 | domain-edge matrix covers all 22 json signatures |
| #2382 | fuzz ladder classifies a resource-limit abort on both legs as resource, not semantic |
| #2390 | nightly fuzz report counts preempted shards as unknown, not zero; drop the 20-item body cap |
| #2391 | ledger-counts remediation is delivered (PR), and the nightly names its fix branch |
| #2387 | domain-edges i32_max rows judged on the compiler, not free memory (deterministic bound or declared skip) |
| #2143 | wasm runtime ratchet judges under `GITHUB_ACTIONS` |
| #2141 | wasm size ratchet over a stdlib-linking corpus |
| #2163 | diff the last two almide-gates twins against their `.sh` originals and promote |
| #2380 | source-level gate for the fold-seed-consumes-its-own-source shape |
| #2381 | critical path 55 min → measured after #2386 lands; remaining split is a follow-up with numbers |

**Decided 2026-09-21 (3), so now startable.** #924: the streak counts nights
delivering at least 75% of the planned fuzz-minutes (read from `shards=k/N`),
not "full budget", which the `if: always()` verdict satisfies by construction;
#2390 lands first because the line the streak reads from under-counts. #1998 /
#2009: their two obligations are named **S2 / S3** (the contract-relative
survival ladder); the L2 / L3 of `docs/specs/edit-locality.md:27,33`
(cross-target agreement, diagnostic locality) are untouched. Both rulings are
comments on the issues.

**Programmes, not sweeps (8).** #586 and #1514 (tracking issues), #1508
(reference-suite mining), #1469 (doc generator — premise "no generator" is
false, 43 of 45 stdlib docs already carry a generated index), #1423
(target-availability matrix), #2152 (portable trust / attestation), #2146
(public MSR harness, belongs with Dojo), #1617 (re-run Dojo on Sonnet 5 —
a run, not a code change).

## Premises that had moved — recorded in the headers, not deleted

Over twenty issues had a premise that was no longer true. Each finding is a
cheap tree check by one agent (a grep, an `ls`, a line read) written into the
header's one-liner and evidence with the body intact. **None of them closes
or re-scopes an issue; that is still a human call.**

- #2044 — item 1 is gone (`pass_auto_parallel.rs` retired); `fannkuchredux` already VICTORY; only the `mandelbrot` row remains.
- #2045 — `|>` already fuses; what remains is `list.range` building a `Vec<i64>` head.
- #2318 — direction 1 landed (`region.rs` `forget_window_temps`).
- #2150 — native `AlmideMap` is not a linear scan (indexed above 16 entries); `research/benchmark/perf/README.md:95` ("linear-scan") is rot and is queued as its own PR.
- #1584 — structural is already the default; fallback baseline 25 → 8; the rest waits on #1696's incumbent retirement.
- #1514 — every unchecked child is CLOSED except #924.
- #1469 — the "no generator" premise is false (43/45 stdlib docs have a generated index).
- #2152 / #2151 / #2149 / #2142 / #2163 / #1423 / #1710 / #2312 / #2387 / #2391 — partly landed; the remainder is now explicit in each header.
- #1107 — the `@deprecated(since=3)` window expired at dialect 4, and `unwrap_or(` has 128 remaining sites, not 47: moving the wrong way.
- #1388 — migration cost grew from "2 + 37" to 71 files.

One real design hazard, now closed: #1998 and #2009 proposed "L2 / L3" names
already used with other meanings in `docs/specs/edit-locality.md`; they are
S2 / S3 as of 2026-09-21 (see above).

## Open follow-ups from the inventory pass

- #2398's age: measured on published assets the same night — it does NOT reproduce on v0.62.0; `git bisect run` over v0.62.0..v0.63.0-rc1 (431 commits, 10 release builds) names 092c40d05 (PR #2048, Map/Set entries on typed drop glue, 2026-09-08) as the first bad commit, so it carries the `regression` label for the REPORTED PROGRAM. The defect is older: a second fixture with literal lists prints freed memory on the published 0.62.0 asset (measured the same night, four heap-preserving routes without `list.update` stay clean with the guard disabled, so the map is heap shaping, not a second carrier) — 092c40d05 changed which programs expose the missing retain, not when it arrived. The step script, reproducer, both run logs and the verdict table are in `issue-inventory-2026-09-21/bisect-2398/`; the verdict table is a reconstruction from the run logs, not git's own bisect log (that does not survive `git bisect reset`). The caveat stands: the trigger depends on surrounding allocations, so the claim is "this program does not expose it on 0.62.0", not "0.62.0 is correct".
- #2417 (the header format and the label axis) and #2386 (shard weights) were open at snapshot time.
- `research/benchmark/perf/README.md:95` linear-scan rot — separate docs PR.
