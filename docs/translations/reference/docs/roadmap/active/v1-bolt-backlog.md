Description: AI-DLC Bolt backlog for the v1 climb — the camps/steps roadmap expressed as intent-driven, time-boxed Bolts (each with intent / Definition-of-Done / gate / deps / status). The construction guardrails are the goal-prompt discipline; each Bolt's exit gate is independent review (reviewer agent + Trust Spine CI + unbiased dual-oracle corpus); humans (Mob) decide at the marked forks. Tracks “how many Bolts remain” to each summit.
<a id="v1-bolt-backlogai-dlc-管理"></a>

# v1 Bolt Backlog (AI-DLC management)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-bolt-backlog.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-bolt-backlog.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-bolt-backlog.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-bolt-backlog.md)

> Source caveat: this is the dated 2026-09-27 planning/measurement snapshot, not today's release status, market ranking, execution authorization or qualification evidence. Preserve the historical incumbent/structural distinction, partial witness coverage and byte-binding gap; certificates here are scoped proof-checking artifacts, not external certification. wasm test pass rate is not separately measured native-byte equality; the fake-model A/B wiring result is not effectiveness evidence, and the listed Llama comparisons do not establish every-model or causal market advantage. Source totals are not reconciled: the 919 build cases have listed categories 881+24+5, the 462 test cases have 392+61, and 55 downstream files have 37+0+17+3; the document does not clarify missing or overlapping categories. Approximate ratios and all labels/counts remain as recorded. Plans for the default leg, kit readiness, physical target calibration and actual Ferrocene use/qualification are separate obligations. Historic self-running, dispatch, budget, push and CI instructions are inert documentation, not authorization or execution.

> **What this is**: the camp/step roadmap to the summit structured as **AI-DLC Bolts** (intent-driven, time-boxed work units on the scale of a few hours). **1 Bolt ≒ 1 commit/brick**. Measure progress in “how many Bolts remain” and work through them in an autonomous loop.
> **Related**: [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) / [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) / [effectful-27-blueprint](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effectful-27-blueprint.md) / [interp-is-desugar-to-tostring](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/interp-is-desugar-to-tostring.md) / [arena-breakthroughs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md).
>
> **Last updated: 2026-09-27 (v0.64.0 shipping day, origin/develop `d027bbb77`)**. Changes since the previous 2026-06-21 version are summarized under “What changed in three months” below. Statuses have been backed up by issues/PRs/commits.

<a id="3-か月で何が変わったか2026-06-21--09-27"></a>

## What changed in three months (2026-06-21 → 09-27)

- **The name changed**: June's “v1” is now the **incumbent leg** (`crates/almide-mir`, the side carrying certificates). The default shipped leg is the newly built greenfield **structural leg** (`crates/almide-wasm`). Stage 2 landed in #1599-#1601 (2026-08-26), becoming the default in v0.60.0. Sources: `docs/wasm/README.md`, `docs/ARCHITECTURE.md`.
- **The oracle changed**: no longer “byte equality with v0,” but native ⇄ wasm byte equality (`spec/wasm_cross`, contract ledger C-NNN) + interp for 3-way comparison.
- **① (run rate) cleared the floor**: the unbiased corpus went from June's 0/15 to 666+15 / 684 (about 99.6%; C1-B5 below).
- **The structure inverted**: **the default shipped leg has no proof** (trusted, not proven). Certificates exist only on the incumbent side. The structural witness covers **1** of 789 wasm_cross cases (`d9945cd57`). → Camp 4's main route changed from “add proofs” to “**move the proofs onto the default leg**.”
- **The tools for ② shipped**: v0.64.0 shipped `almide survive` / `apply --if-survives` (#2147) and structured `repair` diagnostics (#2149). Actual A/B numbers are still absent.

<a id="ai-dlc-マッピング"></a>

## AI-DLC mapping

| AI-DLC | Its realization in this project |
|---|---|
| **Intent** (what to build) | Camp (execution / MSR / WASI / proof / flight) |
| **Bolt** (time-boxed work unit) | 1 commit = 1 brick (with intent + DoD + gate) |
| **Construction discipline** | goal-prompt (walls not holes, push+CI, dual-oracle, false-green, root-fix-not-revert) |
| **Bolt done-gate** | Independent CI (Coq+axioms+PCC) ∧ native⇄wasm byte equality + 3-way oracle ∧ contract ledger ∧ discipline. **A self-run is not completion** |
| **Mob checkpoint** (human) | Direction / priority / scope / “is this the right goal?” / MSR results → GTM axis. The `mob` label on GitHub |

<a id="bolt-テンプレ"></a>

## Bolt template

```
Bolt <ID>: <一行 intent>
  DoD  : 機械でチェックできる完了条件
  gate : 検証手段(CI / dual-oracle / unbiased corpus)
  deps : 依存 Bolt
  状態 : ⬜未 / 🔄進行 / ✅完了 / 🧱壁(部分) / 📐設計済 / 🪦前提が変わり置換
```

Translation of the template: Bolt ID and one-line intent; DoD is a machine-checkable completion condition; gate is the verification method (CI / dual-oracle / unbiased corpus); deps lists prerequisite Bolts; status is ⬜ not started / 🔄 in progress / ✅ complete / 🧱 wall (partial) / 📐 designed / 🪦 replaced because its premises changed.

---

<a id="-camp-1--実行パリティほぼ完了残りは-incumbent-退役へ合流"></a>

## 🏕 Camp 1 — Execution parity [nearly complete; the remainder converges on incumbent retirement]

- **C1-B1: Execute heap-result variant match** — ✅
  The #1492 arc (2026-08-17) passed **through lowering alone**. Coq and the checker were unchanged; June's “Camp 4 gated” diagnosis was wrong. Wall corpus: 7/10. The remaining 3 are Map/Set heap-key typed twins (→ N-B4). Source: [camp4-general-heap-match](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/camp4-general-heap-match.md). The structural leg can lower arbitrary shapes.
- **C1-B2: Remaining heap-value plumbing** — ✅ (🪦 replaced: the old histogram assumed the incumbent)
  Current measurements (#2739): 919 spec build-lane cases → structural 881 / incumbent-only 24 / rejected by both legs 5. Test lane: 462 cases → structural 392 / incumbent 61.
- **C1-B3: Closures C2 (first-class)** — ✅
  Closure Architecture v2 P0-P6 complete ([closure-architecture-v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)). Structural env drop-glue (`3f60174d3`), route handler closures (`cf90ab9d7`). “Closures have no ABI” series: #2287 ✅ / #2288 ✅ / **#2289 🔄** (→ N-B3).
- **C1-B4: stdlib breadth** — 🔄 (nearly complete)
  Of 1,003 public functions: structural 942, incumbent-only 19, both-leg walls 42. Function-level reachability frontier is 0 (PR #1224). Host capabilities remain. `proofs/wasm-reachability-baseline.txt`, `proofs/target-availability.toml`.
- **C1-B5: ⭐ Measure honest distance using an *unbiased* corpus** — ✅ **remeasured (2026-09)**
  892 almide-dojo LLM solutions (generated in a separate process) → structural 666 / incumbent 15 / both-leg rejection 3 / old syntax 208 (excluded). **About 99.6% of the 684 valid cases pass on wasm** (June: 0/15). #2739, `dc832c1b9`.
  ⚠ This is the rate of “tests passing on wasm,” not a separately calculated native byte-equality rate.
- **C1-B6: Run real repos (cross-repo conquest)** — 🔄
  csv ✅ / svg ✅ (June) / almide/dfa has 50 tests byte-identical on wasm. Four downstream repos (almai / comide / golemide / gramide-cli), 55 test files → structural 37 / incumbent-dependent 0 / both-leg rejection 17 / E081 (http/process) 3.
- **Camp 1 Exit**: real agent programs run and are byte-identical → **substantially achieved**. The remaining tail is “both-leg rejection” and incumbent retirement (N-B1).

<a id="-camp-2--msr存亡の賭け道具は揃い実測待ち"></a>

## 🏕 Camp 2 — MSR (the existential bet) [tools ready; measurements awaited]

MSR work moved to [almide/almide-dojo](https://github.com/almide/almide-dojo) (repo boundaries are in CLAUDE.md). The axis moved from “binary survival rate” to “**a contract-preserving change bank + recovery A/B using survive**.”

- **C2-B1: Set up a controlled MSR experiment** — ✅ (🪦 June's `msr-v2` workflow was replaced)
  Dojo's daily lane (`src/main.almd`, the harness itself written in Almide) and cross-language lane (`src/msr/`, 7 languages, 21 tasks). Each run stamps a verdict of `comparable` / `inconclusive-saturated`. To counter saturation, a contract-preserving change bank has been introduced (dojo#3, 400 tasks, 8 families, hidden oracle, wasm leg, resource oracle).
- **C2-B2: Run it and produce numbers** — 🔄
  - Daily (2026-09-27, v0.63.1, 38 tasks, comparable): Llama 3.3 70B 28/38 (73%), Llama 3.1 8B 13/38 (34%).
  - Cross-language (2026-09-22, Llama 70B, 21 tasks): **Almide 80%** / Rust 95% / Go 100% / TS 100% / Zig 61% / MoonBit 47% / Gleam 14%. **Almide loses to Rust/TS/Go**.
  - No Sonnet 5 row (#1617 closed. The decision “do not put API keys in CI” requires manual measurement).
  - The bank pilot (400×3×3) was canceled after 6h43m on 2026-09-25. No saturation verdict obtained.
  - #1963 (P-critical), “the thesis has no current measurements,” is OPEN.
- **C2-B3: ⭐ Remeasure with MSR redefined (failure clarity + recoverability)** — 🔄 (tools shipped; A/B awaited)
  v0.64.0 shipped `almide survive` / `apply --if-survives` (#2147, CLI/MCP/LSP), the structured `repair` field + bidirectional ratchet + `explain --list` (#2149 ✅). dojo#58 implemented survive A/B (control vs survive, equal budget of 4 attempts, winning condition = mean attempts ≤ 0.8×control). Wiring demonstrated with a fake model gives 3.00→2.00, but this is not evidence of effectiveness.
  Remaining: dojo#59 (pin to v0.64.0) → dispatch with Llama 70B (about 2,200-2,500 Workers AI calls). #2147 remains OPEN until numbers arrive. June's inject-mistake catch rate was absorbed into survive A/B (inferred).
- **Contract-relative survival arc #1998** (ratified 2026-09-07): #1994 / #1995 / #1996 / #1991 / #1997 / #2009 (L2/L3) ✅; dojo#3 (bank) and #1589 (generic protocol) remain. The edit-locality L1 prediction loop (`proofs/l1-verdicts.toml` + dojo `src/l1_loop.almd`) is running ([edit-locality-theory](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/edit-locality-theory.md)).
- **Camp 2 Exit (Mob decision)**: determine the main GTM axis. **Cross-language Almide 80% < Rust 95% backs June's decision: “binary MSR alone cannot win; ② verification + recovery is robust.”** The survive A/B numbers will inform the final decision.

<a id="-camp-3--検証済み-wasmwasi-component-producer大半は済検証部分が残る"></a>

## 🏕 Camp 3 — Verified WASM/WASI component producer [mostly done; “verified” remains]

- **C3-B1: Effectful WASI floor** — 🔄 (mostly done)
  Clock reaches embedded host / p1 shim / p2 and p3 components through host op 60 (`124d80cfb`). The p1 shim provides fs and env.os/temp_dir/cwd (`7782e66d0`). p3 has fs read / write (#1662 / #1666). `declared ⊇ used` is proven in Coq (`proofs/CapabilityBound.v` per function, `CapabilityReach.v` for transitive closure). `--profile critical` is deny-all + `--allow` (#567 ✅).
  Remaining: env/process p3 host surface (remaining #1628 work, #2589).
  ⚠ The capability vocabulary is a generic nat-indexed allowlist, not individual Coq types for Fs/Net/Clock/Random/Env. Whether this meets June's DoD is a Mob decision.
- **C3-B2: Verify/minimize the Component Model boundary** — 🔄 (production done; verification ⬜)
  Stage 0 adapter (#1631) → stage 1 direct p2 (#1632, 1/3.2 the adapter-form size) → stage 2 direct p3 (#1646) + fan.any cancel (#1664) + fs → stage 4 WASI pin ledger (`proofs/wasi-pin-policy.toml`). There is **still no** canonical ABI (lift/lower) proof or minimization argument. Tracking #1628 is OPEN.
- **Related landed work**: wasm VM #865 ✅ (PR #2304, parity 681 matches / 0 mismatches). http layer (v0.64.0: `http.serve` works on the embedded wasm lane, C-367; call handle C-366). Follow-ups: #1710 (wasi:http@0.3 client) and #2702 (P3 handler export).
- **Camp 3 Exit**: produce verified components → **production achieved; “verified” awaits Camp 4's transfer (N-B2)**.

<a id="-camp-4--証明の完全性incumbent-上は概ね達成--既定-leg-への載せ替えが本線"></a>

## 🏕 Camp 4 — Proof completeness [largely achieved on incumbent → transfer to the default leg is the main route]

- **C4-B1: leak-freedom / reuse / call-mode** — 🔄 (**incumbent ✅, structural ⬜**)
  Leak-freedom: `OwnershipChecker.check_sound` + OwnershipLoop. Reuse: FreeList.v / FreeListRc.v (#909 ✅) / CowSafety.v. Call-mode: `CallModes.v` (`c485bef9a`). Runtime: StructuralRuntime/Alloc/Decode/Run.v (#576 ✅). Native ownership certifier #2231 ✅ (ledger 752→0, remaining #2239 ✅).
  ⚠ On the default structural leg, only **1 of 789** wasm_cross cases is certified (grow-only ratchet, `d9945cd57`). Remaining: #2755-#2759 (branch / loop / closure / call-modes / names / caps).
  ⚠ The “4/8” in [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) is stale, still dated 07-30.
- **C4-B2: Byte binding (Gap 1, the hardest)** — 🔄 (partial)
  The RC primitives (`$rc_inc` / `$rc_dec`) have bytes→ISA theorems inside Coq (WasmEncode/Exec/Isa/Decode.v, StructuralDecode.v). Arbitrary-program witness→bytes remains unachieved. The near-term step is **#2760**: regenerate receipts from structural builds and pin each `.wasm` SHA-256.
- **Camp 4 Exit**: safety bundle **on the default leg**, 8/8 → trust claims advance to flight grade.

<a id="-camp-5--飛行級cert-スパイン③④市場--kit-はほぼ揃った"></a>

## 🏕 Camp 5 — Flight grade (cert spine, markets ③④) [📐 → 🔄; kit nearly ready]

- **C5-B1: WCET / counted-loop into Coq** — 🔄
  #569 ✅ (three-layer WCET story + gated PID kernel, PR #1642, `docs/project/WCET-STORY.md`). Loop ownership: `proofs/OwnershipLoop.v` / `CoownLoop.v`. G-F2 (lift loops into Coq and prove an allocation-count bound) is ⬜. Structural witness loop frames: #2757. Target calibration remains external.
- **C5-B2: Production MIR→Rust + Ferrocene** — 🔄
  The Ferrocene migration lane record is all 633 green on Rust 1.95.0 (#573 ✅), trace-map #572 ✅. But the [lane definition](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.github/workflows/ferrocene-lane.yml) explicitly says it uses **stock rustc** until a subscription is introduced. Do not read this record as evidence of execution with Ferrocene's qualified binaries or as a semantics-preservation proof of Almide→Rust lowering. **#1480, the two native Rust producers problem, awaits Mob**.
- **C5-B3: Reference app + qualification kit** — 🔄 (close to ✅)
  Reference PID app: C-230; G-F4: #776 ✅. Kit: #566 / #567 / #568 / #574 / #575 / #576 (45 Coq theorems) / #571 dossier / #1534 signing are all ✅; tracking #586 closed on 2026-09-21. Remaining: ALS (CG-1) PARTIAL (reference evaluator progressing on the als side).
  ⚠ The Gaps table in `proofs/TOOL-QUALIFICATION.md` and kit table 10-13 in [flight-qualification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md) are stale.
- **Camp 5 Exit**: flight ladder G-F0..G-F6 → safety-critical/aviation (partnership). **The kit side is ready; N-B2, putting proofs on the default leg, is the bottleneck**.

<a id="-新-bolt2026-09-に発生した本線"></a>

## 🆕 New Bolts (main route arising in 2026-09)

- **N-B1: Retire the incumbent wasm emitter** (#1696 steps 4-5 → tracking #2739) — 🔄
  DoD: both-leg route census (91 rows, shrink-only, #2741) reaches 0 → remove about 26,000 WAT-emitter lines (#2761). The largest cause is missing fs operations in p1 `to_wasi` (25 cases). Individual items: #2742-#2753, #2703. Deps: N-B2 (unless certificates move to structural first, retirement removes the proofs).
- **N-B2: ⭐ Put certificates on the default leg (structural witness ladder)** — 🔄
  #2754 ✅ → #2755-#2759 (branch / loop / closure / call-modes / names / caps) → #2760 (receipt regeneration + SHA pin). DoD: C-SAFE / C-PROVEN receipts come from structural builds. Grow certified wasm_cross from 1/789. **Camp 3 / 4 / 5 exits all pass through here**.
- **N-B3: Make fusion specialization of self-hosted HOF bodies** (#2289) — 🔄
- **N-B4: Map/Set heap-key typed twins** (remaining 3 in C1-B1 wall corpus, route split `4d75df7ce`) — 🔄
- **N-B5: Measure survive A/B** (dojo#59 → dispatch → evaluate #2147's winning condition) — ⬜ **awaiting Mob approval (Workers AI budget)**
- **N-B6: Rerun the bank pilot for a saturation verdict** (dojo#3, 9/25 canceled) — ⬜
- **N-B7: Rebuild make-verify as a survive / repair version** (restart S-B1) — ⬜
- **N-B8: Stage 4 durability — fuzz true-green streak** (**0 days** in `proofs/STAGE-STATUS.md`) — 🔄
- **N-B9: Remaining #1628 items** (C rows for host-boundary determinism, env/process p3) and moving http to p3 (#1710 / #2702) — 🔄

<a id="-頂上-bolt--gtm"></a>

## 🏔 Summit Bolt — GTM

- **S-B1: make-verify killer demo** — 🔄 (**stalled**)
  `demo/make-verify/` has not changed since 2026-06-18 (`cd3c4e248`). With v0.64.0's `survive` / `repair`, it is now possible to demonstrate “stop a wrong edit before applying it and show how to repair it” for real (→ N-B7).

---

<a id="mob-checkpoint人間あなたが決める分岐"></a>

## Mob checkpoints (forks decided by the human—you)

**Awaiting decisions as of 2026-09-27** (8 GitHub `mob` labels):

- **Approve survive A/B dispatch** (N-B5): Workers AI budget. The cross-language lane is manual-only (dojo#55).
- **Frontier-model rows (Sonnet 5 etc.)**: under the no-keys-in-CI policy, who measures manually, and when?
- **How to treat cross-language 80% < Rust/TS/Go**: use it as the rationale for shifting GTM toward verification?
- **#1480: unify the two native Rust producers**: C5-B2's bottleneck.
- **C3-B1 capability vocabulary**: count the nat allowlist as meeting DoD, or split into Coq types (soundness-critical)?
- **C4-B1 completion definition**: “proofs for which leg?” This document defines it as “on the default leg” (N-B2).

**Standing forks** (unchanged since June): next Camp/Bolt / level of direction (start work, change scope) / expanding the proof/Coq trust vocabulary (always human decisions).

<a id="自走ループmob-以外"></a>

## Autonomous loop (outside Mob decisions)

```
Bolt を1つ取る(優先 = 既定 leg の証明被覆を最も上げる / release blocker が先)
  → goal-prompt 規律で construct
  → done-gate(独立 CI ∧ native⇄wasm byte一致 ∧ 契約台帳 ∧ false-green ∧ 壁)
  → 通れば次 Bolt / 落ちれば root-fix
  → self-run で完了にするな・危険1タスク延期≠セッション終了
```

Translation of the loop: take one Bolt (prioritize the greatest increase in default-leg proof coverage / release blockers first) → construct under goal-prompt discipline → done-gate (independent CI ∧ native⇄wasm byte equality ∧ contract ledger ∧ false-green ∧ walls) → if it passes, next Bolt; if it fails, root-fix → do not declare completion from a self-run; postponing one risky task ≠ ending the session.

<a id="進捗ビューあと何-bolt"></a>

## Progress view (how many Bolts remain)

- **🚩 First commercial summit (① agent market)** = Camp 1 **substantially done** + C3-B1 (remaining env/process) + S-B1 (N-B7) — **close**. The technology is there; the missing pieces are the demo and recovery numbers (N-B5).
- **🏔 True summit (all markets, flight)** = N-B2 (default-leg proof) → N-B1 (retirement) + C4-B2 (byte binding) + Camp 5's G-F2 — **N-B2 is the bottleneck**. June's bottleneck was “Camp 4 escalation”; now it is “the proofs exist, but are not attached to what ships.”

<a id="リリース--issue-スナップショット2026-09-27"></a>

## Release / issue snapshot (2026-09-27)

- Latest: **v0.64.0** (2026-09-27, “check an edit before applying it, repairs in diagnostics, and an http layer”). v0.64.1-rc1 prereleased the same day.
- 88 open issues (enhancement 52 / A-wasm 29 / A-codegen 12 / A-stdlib 12 / proof 10 / mob 8 / bug 5 …).
- **3 release blockers** (`scripts/count-release-blockers.sh`): #2780 (regression: since 0.64, `process.exit` rejects 126..255, preventing wrappers from returning child-process exit status) / #2782 (I-divergence: wasm `bytes.repeat` bypasses C-197's limit check and traps) / #2783 (I-divergence: native `matrix.linear_f32_row_no_bias` skips C-161 and panics).
- Version-ordered plans: [ROAD_TO_1_0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md) (0.65 onward redrawn on 2026-09-27). This document's N-B1 / N-B2 correspond to the remaining 0.6x work, the C2 series to 0.8x.

---

<a id="履歴-mob-判断②-を勝ち筋として-active-昇格2026-06-18"></a>

## History: Mob decision “promote ② to active as the winning route” (2026-06-18)

Against the north star, ② (verification + recovery) was judged the winning route and ① (run rate) the floor; the priority fork was rearranged. Active then: S-B1 / C2-B3; Mob-gated: C4-B1 + C1-B1 cert-precision; fading: C1-B2 / C1-B4.

**2026-09-27 reassessment**: the judgment is **maintained and strengthened**. ① cleared the floor on the structural leg without Camp 4 escalation (C1-B1's gating was a misdiagnosis). ② shipped as tools in v0.64.0, and cross-language numbers (Almide 80% < Rust 95%) also supported “binary MSR alone cannot win.” Two questions remain: quantify ②'s effectiveness (N-B5), and attach verification to what ships (N-B2).
