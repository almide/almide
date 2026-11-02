<a id="road-to-10--065--099-バージョンラダー"></a>

# Road to 1.0 — 0.65 → 0.99 version ladder

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](ROAD_TO_1_0.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ROAD_TO_1_0.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ROAD_TO_1_0.zh-CN.md)

> Non-normative fixed-source historical roadmap reader; the pinned original remains authoritative and unchanged. No autosync or independent rerun. Current version, issue counts, shipped status and measurements belong to the dated source, not today. Planned gates and qualification work are requirements or historical reports, not present fulfillment, independent proof or certification. Closed not planned does not mean completed. The August 1 branch-protection commands and release procedures are preserved incident history, not authorization to execute or change settings; cancelled CI is not successful CI. The source’s transitive-verification claim retains its stated empty-code-diff and green-version limits. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md)

> **Current v0.64.0 (2026-09-27)**. A ledger dividing every minor from 0.65 to 1.0 into decade-sized
> arcs (remaining 0.6x / 0.7x / …), with one headline feature per minor. **All 88 open issues are allocated
> somewhere in this ledger** (see “Ledger completeness” below).
>
> **This is the timeline document.** Winning paths and conditions for the same work by evaluation axis
> (speed / performance / correctness / LLM writability / DX / ecosystem) are in [ALL_AXES.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ALL_AXES.md).
> Work-unit (Bolt) granularity is in [v1 Bolt Backlog](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-bolt-backlog.md); rows here bundle those Bolts into versions.

<a id="2026-09-27-に引き直した理由"></a>

## Why redraw it on 2026-09-27?

The old ledger (drafted 2026-07-30, 59 rows from 0.41 → 0.99) ordered 0.6x = cranelift,
0.7x = proofs and Critical profile, 0.8x = qualification kit, 0.9x = normative specification.
What actually happened:

- **0.7x–0.9x contents closed early.** The flight-grade arc (2026-08-27–28) closed #566 / #567 / #568 / #569 /
  #570 / #572 / #573 / #574 / #575 / #576 / #776, then #571 (08-30) / #865 (09-18) / #586 (09-21).
  ALS normativity #530 and interp normative semantics #564 also closed 08-27. Of about 80 issues cited in
  the old ledger, only seven remained open (#924 / #1003 / #1005 / #1315 / #1331 / #1388 / #2146).
- **0.6x went to the structural leg, not cranelift.** The greenfield wasm engine became default in v0.60.0;
  0.61–0.64 finished it (unified borrow analysis, leak fixes, http layer, `almide survive`).
  #1005 (cranelift) has had no comments since filing.
- **The program track (#577–#585) closed on 2026-08-13 (not planned).** These legal/commercial/organizational
  items are not engineering work this repo can take on, so moved to #586 and
  [flight-organization](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md). This is not a claim that anything was completed.

Therefore, rather than editing individual rows, **rebuild 0.65 onward from currently open issues**.
Keep the 0.41–0.64 record under “Shipping record” below.

<a id="読み方と運用ルール"></a>

## How to read and operate it

- **Decade = arc**, with a theme and exit gate. Decade boundaries (0.70, 0.80, 0.90) are gate releases,
  one release fixing that decade's exit audit.
- **Each minor = one row.** Patches (0.65.x) are unrestricted. Numbers may shift within a decade:
  the invariant is the decade gate, not the version number. When findings or dogfooding interrupt,
  move existing rows later within the decade and **leave a note** (no quiet renumbering).
- **Allocate every new issue somewhere in this ledger in the same PR.** Do not create unlisted issues.
- **A Unit closes when its tag exists** (see the 2026-08-01 record below).
- **Execute through AI-DLC**: Intent = decade / Unit = row / Bolt = work cycle; humans only at Mob points
  (`mob` label). Bolt list and state: [v1 Bolt Backlog](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-bolt-backlog.md).

<a id="順序の根拠"></a>

### Rationale for order

1. **bug > chore > feature, with regressions and release blockers first**: first row of each decade.
2. **Put trust on shipped artifacts first.** Certificates currently exist only on the incumbent leg, not the
   default structural leg. Remove incumbent **after** moving proofs to structural (reverse it and proofs disappear).
3. **Performance before attempt-count work** (2026-09-08 policy): while representation and speed move,
   measured MSR numbers immediately age. MSR measurement/publication is 0.8x.
4. **Instruments before surgery**: perf instrumentation (0.77) is prerequisite for reading native optimization
   results. Survive A/B (0.84) reads the numbers afterward.
5. **Bundle breaking surface changes immediately before specification freeze** (0.95 = edition row).
6. **Freeze last**: freeze declares completion, not aspiration.

---

<a id="06x-残り--出荷物に信頼を載せる"></a>

## Remaining 0.6x — put trust on shipped artifacts

Make the structural leg shipped by default the only certificate-bearing leg. Retire the incumbent wasm emitter
(about 26,000 lines). **The bottleneck is proof coverage of the default leg** (certified wasm_cross currently
one of 789, `d9945cd57`).

| Version | Major feature | Issue |
|---|---|---|
| 0.65 | **Burn down release blockers and bugs** — `process.exit` 126..255 (regression), missing upper-bound checks in wasm `bytes.repeat` and native `matrix.linear_f32_row_no_bias` (two I-divergences), `panic` in value-producing branches, misleading diagnostics, static musl binaries | [#2780](https://github.com/almide/almide/issues/2780), [#2782](https://github.com/almide/almide/issues/2782), [#2783](https://github.com/almide/almide/issues/2783), [#2769](https://github.com/almide/almide/issues/2769), [#2771](https://github.com/almide/almide/issues/2771), [#2772](https://github.com/almide/almide/issues/2772), [#2777](https://github.com/almide/almide/issues/2777) |
| 0.66 | **HTTP serving first half** (ADR-0020 steps 0–4) — effect fn values through declared fn types, router walls, generalize E008 (epoch 6), instance-closed serve arguments, `kv` module, native concurrent serve | [#2705](https://github.com/almide/almide/issues/2705), [#2696](https://github.com/almide/almide/issues/2696), [#2697](https://github.com/almide/almide/issues/2697), [#2698](https://github.com/almide/almide/issues/2698), [#2699](https://github.com/almide/almide/issues/2699), [#2665](https://github.com/almide/almide/issues/2665) |
| 0.67 | **HTTP serving second half** (ADR-0020 steps 5–8) — SIGTERM drain/flush, HTTP-semantic serve comparison, stock `incoming-handler@0.2` export, LLM docs (last). Close #2705 | [#2692](https://github.com/almide/almide/issues/2692), [#2700](https://github.com/almide/almide/issues/2700), [#2659](https://github.com/almide/almide/issues/2659), [#2701](https://github.com/almide/almide/issues/2701) |
| 0.68 | **Zero remaining structural lowering work** — empty the route census (91 rows, shrink-only #2741), remove direct incumbent routes | [#2743](https://github.com/almide/almide/issues/2743)–[#2751](https://github.com/almide/almide/issues/2751), [#2752](https://github.com/almide/almide/issues/2752) |
| 0.69 | **Structural witness ladder** — witnesses for call arguments, branch/loop frames, closure/effect fn, call-modes, name totality and capability; regenerate receipts and SHA-256-pin each `.wasm`; move incumbent-only CI gates | [#2755](https://github.com/almide/almide/issues/2755), [#2756](https://github.com/almide/almide/issues/2756), [#2757](https://github.com/almide/almide/issues/2757), [#2758](https://github.com/almide/almide/issues/2758), [#2759](https://github.com/almide/almide/issues/2759), [#2760](https://github.com/almide/almide/issues/2760), [#2753](https://github.com/almide/almide/issues/2753) |
| 0.70 | **Gate release — delete incumbent wasm emitter** (one-leg router, remove `ALMIDE_WASM_INCUMBENT`, delete `render_wasm*`) | [#2761](https://github.com/almide/almide/issues/2761), [#2739](https://github.com/almide/almide/issues/2739), [#1696](https://github.com/almide/almide/issues/1696), [#1584](https://github.com/almide/almide/issues/1584) |

**Gate 0.70**: one wasm leg / C-SAFE and C-PROVEN receipts from default-leg builds / certified wasm_cross reaches
all cases under a grow-only ratchet / ADR-0020 serving contracts hold on both legs.

<a id="07x--native-の性能と表現"></a>

## 0.7x — native performance and representation

Eliminate areas where wasm beats native (Map / JSON / Value by 1.5–2.3×, #2157), and align the native leg's
“numbers story.” Representation ruling (ADR-0018) comes first because every subsequent optimization targets it.

| Version | Major feature | Issue |
|---|---|---|
| 0.71 | **`Value` representation** — implement ADR-0018 (`Value::Str` = `AlmideStr`, ruled 2026-09-08). Remove string copies accounting for 68% of decode | [#1679](https://github.com/almide/almide/issues/1679) |
| 0.72 | **Native Map / Value at least as good as wasm twins** — including only native becoming quadratic when accumulating into Maps in loops | [#2157](https://github.com/almide/almide/issues/2157), [#2393](https://github.com/almide/almide/issues/2393) |
| 0.73 | **Parallel native `fan`** — `fan.map` is sequential, effect callbacks run one at a time | [#2044](https://github.com/almide/almide/issues/2044), [#2594](https://github.com/almide/almide/issues/2594) |
| 0.74 | **Unify fusion** — rebuild fusion as specialization of self-hosted HOF bodies; fuse `|>` chains on Rust target too. Make recommended spelling the fastest spelling | [#2289](https://github.com/almide/almide/issues/2289), [#2045](https://github.com/almide/almide/issues/2045), [#2331](https://github.com/almide/almide/issues/2331) |
| 0.75 | **Bytes / numerical workloads** — `string.to_bytes` uses `Vec<i64>` (84% of lexer in malloc/free), dead BLAS path | [#2078](https://github.com/almide/almide/issues/2078), [#2512](https://github.com/almide/almide/issues/2512) |
| 0.76 | **Size and startup** — comparative measurement of native hello 468 KB, 0.166 ms before main, most of small wasm not being the program | [#2092](https://github.com/almide/almide/issues/2092), [#2162](https://github.com/almide/almide/issues/2162), [#2312](https://github.com/almide/almide/issues/2312) |
| 0.77 | **Full perf instrumentation** — runtime ratchet for 13/13 programs, `almide profile`, fuzz perf-class findings and downstream 4–5% regressions | [#2142](https://github.com/almide/almide/issues/2142), [#2598](https://github.com/almide/almide/issues/2598), [#2302](https://github.com/almide/almide/issues/2302), [#2415](https://github.com/almide/almide/issues/2415) |
| 0.78 | **One native Rust producer** (`codegen` and `mir/render_native`) — Critical profile native path also uses it. **Mob** | [#1480](https://github.com/almide/almide/issues/1480) |
| 0.79 | **Debug-path decision row** — measure whether rustc still bottlenecks editing, decide to start/not start cranelift (#1005) and wasm DWARF (#1315). Same “measure and decide” row as 0.45; writing no code may be correct | [#1005](https://github.com/almide/almide/issues/1005), [#1315](https://github.com/almide/almide/issues/1315) |
| 0.80 | Gate release — fix the native performance audit | — |

**Gate 0.80**: runtime ratchet monitors 13/13 programs / native does not lose to its wasm twin on the same program /
victory claim against handwritten Rust (#1330) maintained under ratchet.

<a id="08x--エージェント編集ループと-msr-の公開"></a>

## 0.8x — agent editing loop and public MSR

Expand `almide survive` / `repair` diagnostics shipped in v0.64.0 into the complete agent toolkit,
and show the effect **in numbers**. MSR measurement belongs here (ordering rationale 3).

| Version | Major feature | Issue |
|---|---|---|
| 0.81 | **`almide query`** — hover / definition / references / inlay hints as CLI subcommands | [#2148](https://github.com/almide/almide/issues/2148) |
| 0.82 | **Agent-connect and generative code actions** — bundle language guide in binary; missing patterns, label completion, decoder generation, pipe conversion in MSR order | [#2153](https://github.com/almide/almide/issues/2153), [#2640](https://github.com/almide/almide/issues/2640) |
| 0.83 | **Incremental foundation** — consume `.almdi` interfaces and early cutoff, module-level cache, determine entry point from IR | [#2151](https://github.com/almide/almide/issues/2151), [#1003](https://github.com/almide/almide/issues/1003), [#2372](https://github.com/almide/almide/issues/2372) |
| 0.84 | **Measure survive A/B** — dojo#58/#59; winning condition: mean attempts ≤ 0.8×control. Close #2147 once numbers exist | [#2147](https://github.com/almide/almide/issues/2147) |
| 0.85 | **Complete contract-relative survival arc** — saturation verdict for contract-preserving modification bank (dojo#3), live-allocation counter for resource oracle | [#1998](https://github.com/almide/almide/issues/1998), [#2581](https://github.com/almide/almide/issues/2581) |
| 0.86 | **Protocols as DDD ports** — generic protocols, protocol-as-type (end of #1998 arc). Decide whether to collapse 119 stdlib `_str` twins with protocols (**Mob**) | [#1589](https://github.com/almide/almide/issues/1589), [#1460](https://github.com/almide/almide/issues/1460) |
| 0.87 | **Current MSR measurement** — update 147-day-old thesis numbers. Frontier-model rows manually measured (no API keys in CI) | [#1963](https://github.com/almide/almide/issues/1963) |
| 0.88 | **Public MSR harness** — other languages runnable under identical conditions, third-party reproducible. README freshness ratchet | [#2146](https://github.com/almide/almide/issues/2146) |
| 0.89 | **Fill developer-experience gaps** — standard paths for fuzzing / coverage / benchmarks, REPL without recompilation | [#1490](https://github.com/almide/almide/issues/1490) |
| 0.90 | Gate release — fix the agent editing-loop audit | — |

**Gate 0.90**: MSR independently reproducible and at most 90 days old / survive effect measured in controlled A/B
(publish wins and losses) / agents use query, survive and repair on the same CLI/MCP/LSP surface.

<a id="09x--エコシステム表面凍結10-エンドゲーム"></a>

## 0.9x — ecosystem, surface freeze, 1.0 endgame

Build a path for externally authored packages to publication, bundle breaking surface changes in one place, then freeze.

| Version | Major feature | Issue |
|---|---|---|
| 0.91 | **Package publication path** — publish path, write MVS-selected version into lock, generate API docs from interface artifacts | [#2332](https://github.com/almide/almide/issues/2332), [#2532](https://github.com/almide/almide/issues/2532), [#2597](https://github.com/almide/almide/issues/2597) |
| 0.92 | **Processes and cryptography** — handles spawning children in own group, nonblocking poll, tree-wide stop; `process` as WIT-resource host capability; pure Almide AEAD / Ed25519 / HMAC | [#2587](https://github.com/almide/almide/issues/2587), [#2589](https://github.com/almide/almide/issues/2589), [#2596](https://github.com/almide/almide/issues/2596) |
| 0.93 | **Component Model remainder** — host-boundary determinism, env/process p3, wasi:http@0.3 client, P3 handler export and stock `kv` binding | [#1628](https://github.com/almide/almide/issues/1628), [#1710](https://github.com/almide/almide/issues/1710), [#2702](https://github.com/almide/almide/issues/2702) |
| 0.94 | **Remaining accepted ADRs** — ADR-0005 (operator desugaring, remove `unwrap_or`, make `?.` official), ADR-0021 docs | [#1107](https://github.com/almide/almide/issues/1107), [#2728](https://github.com/almide/almide/issues/2728) |
| 0.95 | **Edition row (last window for breaking changes)** — `fan {}` as parallel record literal, non-ASCII identifiers (replace type/value case split), source-declarable ownership guarantees (fip/fbip shape). Each **Mob** | [#1388](https://github.com/almide/almide/issues/1388), [#1456](https://github.com/almide/almide/issues/1456), [#1479](https://github.com/almide/almide/issues/1479) |
| 0.96 | **Specification freeze** — final syntax/operator/stdlib-boundary audit. ALS is already normative (#530); this declares freeze and activates edition policy | — |
| 0.97 | **Version/dialect commitment** — document and activate `proofs/dialect-epochs.toml` and interface diff (`scripts/check-interface-diff.sh`) as 1.x compatibility promises | — |
| 0.98 | **Audit zero known divergence** — contract ledger `flagged-for-revision` 0 (currently 0 / 369), wall 0, claim-drift 0, full-budget consecutive green fuzz nightlies | [#924](https://github.com/almide/almide/issues/924) |
| 0.99 | RC hardening — freeze while all gates stay green. Only 1.0 release remains | — |

**Gate 1.0**: every item in “Definition of 1.0” below holds.

<a id="10-以降"></a>

## After 1.0

| Issue | Description | Why it is outside 1.0 |
|---|---|---|
| [#1331](https://github.com/almide/almide/issues/1331) | `fan` → WGSL: CPU / GPU legs from one source, identical output | Adds a new backend. It serves none of the items in the definition of 1.0, and adding it after the freeze does not break compatibility |

<a id="パッチ枠行を持たない-chore"></a>

## Patch slot (chores without a row)

| Issue | Description |
|---|---|
| [#2618](https://github.com/almide/almide/issues/2618) | Drift in stamped ledger counts — resolved by `gen-ledger-counts.sh` in release procedure step 7 |

---

<a id="10-の定義"></a>

## Definition of 1.0

Include achievement status as of 2026-09-27.

| Item | Condition | Status |
|---|---|---|
| **Specification** | ALS is normative (#530), almide-interp supplies normative semantics (#564), and the specification is frozen | Normative status ✅ (08-27); freeze at 0.96 |
| **Correctness** | Fuzz always green, 0 hole-hunt findings, 0 flagged contract-ledger rows, translation-validation certificates on every build of **the default leg** | hole-hunt ✅ (#912), flagged 0 ✅; certificates only on incumbent → 0.69–0.70; consecutive green fuzz days: 0 → 0.98 |
| **Speed** | Edit loop independent of scale; gap against Rust measured and governed by ratchets | Scale independence ✅ (#1334); ratchet at 7/13 programs → 0.77. Whether rustc must be removed from the debug path is decided at 0.79 |
| **Trust** | Critical profile + qualification dossier can be delivered as a product; reference app supplies evidence | ✅ (#567 / #571 / #574 / #776). Organizational work (such as the signing legal entity) is outside the repo ([flight-organization](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md)) |
| **Numbers** | Measurements of build-speed / runtime-perf / safety are published in README | ✅ (#999), with a 90-day freshness gate |
| **Metric** | MSR reproducible by third parties — public dojo harness, allowing measurement of other languages under the same conditions | 0.88 (#2146) |

---

<a id="出荷記録041064"></a>

## Shipping record (0.41–0.64)

Correspondence between old ledger rows and actual tags. **Rows and versions are not 1:1**
(see the 2026-08-01 record below).

| Version | Date | Headline |
|---|---|---|
| v0.41–v0.44 | 07-31〜08-01 | Instruments (fuzz nightly, perf scoreboard), single driver (#925), concurrency stance and fan family (#1000) |
| v0.45–v0.49 | 08-01 | rtlib conversion, query infrastructure, and module cache were measured and deliberately not started; 10k-line dogfood completed (#1001) |
| v0.50 | 08-01 | **Gate** — three sets of numbers published, derived, and ratcheted (#999) |
| v0.51–v0.53 | 08-02〜03 | Unified effect surface rules, JSON interop decision (#1062), platform conformance (Windows / Wasm 3.0) |
| v0.54–v0.56 | 08-06 | Error surface decided and shipped — `-> T!`, auto-`?` removed, `T?`, try_ family removed |
| v0.57–v0.59 | 08-11〜27 | Integer-domain sweep (218 cells), 3-way oracle 87%, static DI through protocols, `@bounded` profile |
| v0.60 | 08-30 | **Structural leg becomes the default** (greenfield Stage 2) |
| v0.61–v0.62 | 08-30〜09-08 | Patch train, rest-binding patterns, executable docs, wasm-leg leak fixes |
| v0.63 | 09-24 | One analysis decides borrowing and cloning, checked against the output IR (#2186 / #2231) |
| v0.64 | 09-27 | Check edits before applying them (`almide survive`, #2147), repair in diagnostics, http layer |

Of the old ledger's 23 “research-origin issue allocations (2026-08-13)” and “axis-roadmap issue allocations,”
all closed except #1315 / #1331 / #1388. Those three are allocated above to 0.79 / after 1.0 / 0.95.
The old full ledger is in git history (`git show 8de6d7473:docs/roadmap/ROAD_TO_1_0.md`).

<a id="台帳の完全性"></a>

## Ledger completeness

- **Version rows 35 / 35**: every minor 0.65–0.99 has a row (32 headline features + three gate releases).
- **Open issues 88 / 88 allocated** (2026-09-27): 86 in version rows, one after 1.0, one patch slot.
- For rows whose issue column is “—” (gate releases 0.80 / 0.90, 0.96 / 0.97 / 0.99), file an issue when
  starting and add its link in the same PR.
- Divergence between ledger and issue list is debt: allocate new issues here in the same PR; closed issue links
  make progress visible.

<a id="release-order-deviation-2026-08-01--recorded-so-the-ladder-can-be-read-honestly"></a>

## Release-order deviation, 2026-08-01 — recorded so the ladder can be read honestly

**The ladder's rule is one Unit, one release, in order.** It was broken during the 0.4x decade
and this note is the correction rather than a quiet renumber.

What happened: v0.45.0 shipped, `Cargo.toml` was bumped to 0.46.0, and then the work and
records for Units **0.47, 0.48, 0.49 and 0.50 all landed on `develop` before v0.46.0 was
tagged**. The tree therefore carried five Units' worth of change under one unreleased version
number.

Why it happened, plainly: Units 0.45, 0.47 and 0.48 resolved by *measurement* rather than by
implementation — each concluded "the trigger does not fire, here are the numbers" — so they
produced documents rather than artifacts, and a document feels like it does not need a release.
It does. A release is what makes a row's conclusion citable and dated, and a measurement whose
conclusion is "we are not building this" is exactly the kind of decision that needs a fixed
point someone can point at later.

**The correction**: releases resume in order from **v0.46.0**, one per row, and no row is
described as shipped before its tag exists. Where a release carries records that landed early,
its notes say so instead of pretending the ordering held.

**The rule, sharpened for next time**: a Unit is not done when its `construction.md` is
written. It is done when the tag exists. Starting the next Unit before that is what produced
this, and the ladder is only auditable if the two stay coupled.

<a id="merged-past-unverified-ci-2026-08-01--the-mechanism-that-should-have-stopped-it"></a>

## Merged past unverified CI, 2026-08-01 — the mechanism that should have stopped it

**v0.47.0, v0.48.0 and v0.49.0 were tagged on commits whose CI never completed.** Not failed —
**CANCELLED**, each superseded by the next merge while still in flight. Recorded here because
the cause is a missing mechanism, not a missing intention.

**What happened.** The first two release PRs were merged after explicitly polling their checks
to green. Polling took ~50 minutes per release, so the remaining three switched to
`gh pr merge --merge --auto`, expecting auto-merge to hold until checks passed. `main` requires
a pull request but has **no required status checks**, so `--merge` executed immediately and
`--auto` was a no-op. The command returned `MERGED` and the release proceeded.

**A flag was substituted for a verification.** That is the whole failure. The intention was
identical in all five releases; only the enforcement differed.

**The damage, measured rather than assumed**: `git diff v0.49.0 v0.50.0` restricted to
`crates/ src/ stdlib/ runtime/ spec/ tests/ tools/ .github/` is **empty** — every difference is
documentation — and v0.50.0 (`c75f2ee8`) is green on `main`. So the three tags are verified
transitively and nothing shipped is unverified in substance. Each release note now says so
rather than leaving the gap to be discovered.

**Not retracted, and the reason matters.** The release-deletion procedure in CLAUDE.md is for a
BROKEN release. These are not broken; they lack a completed run on their own commit, which is a
process defect. Deleting them would break anyone who pinned a tag, and removing 0.49 alone
would restore the 0.48 → 0.50 gap that this ladder exists to prevent.

<a id="the-fix-is-a-mechanism-not-a-resolution"></a>

### The fix is a mechanism, not a resolution

**Required status checks on `main` are NOT configured and should be.** With them, `--merge`
would have been refused by GitHub regardless of what the operator intended:

```bash
gh api -X PUT repos/almide/almide/branches/main/protection --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "Test Rust", "Test WASM", "Emit & Format",
      "Cross-Target (Rust vs WASM)",
      "Coq proofs + axiom audit + PCC gate",
      "WASM host-arch determinism"
    ]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null,
  "required_linear_history": false,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
```

`"strict": true` also forces the branch to be up to date before merging, which would have
caught the second half of this: `develop` moved under two of these PRs while they were open, so
the checks that were cancelled were cancelled for a reason worth surfacing.

This is the same discipline the repository already applies to everything else — the contract
ledger, the ratchets, the down-only counts. **A rule that depends on the operator remembering
is not a rule.** Until it is configured, the release procedure below is the fallback, and it is
strictly weaker.

<a id="until-then-the-release-procedure-has-one-added-step"></a>

### Until then: the release procedure has one added step

Between "merge" and "tag": **confirm every check on the PR reached `SUCCESS`**, by reading the
conclusions, not by trusting a merge flag.

```bash
gh pr view <N> --json statusCheckRollup \
  --jq '[.statusCheckRollup[]|select(.conclusion!="SUCCESS" and .conclusion!="SKIPPED")|{name,conclusion,status}]'
```

Empty output, and only empty output, is permission to tag. `CANCELLED` counts as not-verified.
