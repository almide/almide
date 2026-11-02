Historical description: DO-178C flight-grade gap analysis: status of six pillars + two engineering keystones.
<a id="flight-profile--航空品質への接地分析と2つのキーストーン"></a>

# Flight Profile — grounded analysis of aviation quality and two keystones

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](flight-profile.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-profile.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flight-profile.zh-CN.md)

> Non-normative fixed-source historical reader snapshot. The unchanged pinned original remains the source of this translation; current bounded-profile authority is ALS §B, not the historical @flight design. No autosync or independent verification of proofs, measurements, implementation status or standards mappings was performed. Flight/aviation-grade, certified, globally unusual, O(1), WCET and comparative wording below records the original analysis/proposal, not aviation certification, tool qualification, authority acceptance or a new guarantee. Counts, old source line numbers, proposed gates and later amendments retain their separate dates/status. Allocation counts do not alone establish byte bounds or physical WCET; technical proof assets do not establish flight applicability. The source’s MISRA/Ravenscar/DO-178C comparisons and claims about what those standards mandate were not independently validated here. Commands are reproduction references, not execution instructions. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md)

> **Goal**: ground in actual code **which flight-grade pillars (DO-178C Level A / DO-333 / DO-330)
> the PCC spine in [v1-proof-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md) already addresses, and what is missing**,
> and define **two keystones that engineering can close**. Aviation is the north star of
> [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md), not a near-term product (see the GTM ladder in
> [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md)), but the flight profile defined here **shares the same certified MIR witness**
> as wasm sovereignty for the agent market. This is therefore not an either/or choice, but the problem of
> “adding a second consumer of the same witness.”
> **Related**: [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) (breakdown of five standards mechanisms) /
> [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md) / [v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) (§1 wasm sovereignty, §9 rejected alternatives).

---

<a id="1-結論一行"></a>

## 1. Conclusion (one line)

> **Of the six flight-quality pillars, the PCC spine addresses “pillar① artifact safety” to a globally unusual depth.
> Of the remaining five, two can be closed by engineering: (a) lift loops into Coq to bound WCET, and
> (b) productionize MIR→Rust to bind the certificate to a Ferrocene-qualified flight target.
> They are independent, both extensions rather than rebuilds, and converge in a single flight stack.**

---

<a id="2-当初の6本柱分析--現行の認証状態ではない"></a>

## 2. Original six-pillar analysis — not current certification status

The theorem counts, coverage and not-started statuses in this table are snapshots of the original design.
For current proof/measurement scope, see `proofs/receipt.sh`; for the normative bounded profile,
[ALS §B](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md); for the native renderer,
`crates/almide-mir/src/render_native.rs`.
The existence of technical assets does not imply qualification, flight applicability or authority acceptance.

DO-178C requires not a “correct artifact” but a **development process that produces a system of evidence**
(consistent with the breakdown of five standards mechanisms in [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md)).

| Pillar | What DO-178C requires | v1 status |
|---|---|---|
| **① Artifact safety** | Memory safety, bounded resources, type soundness, bounded capabilities | 🟢 **Strong** — PCC checker + 43 theorems, rederived each build. Globally unusual |
| **② Object-code fidelity** | Machine code actually flying = source semantics | 🟡 **Halfway** — byte binding remains “trusted” (§3 contract), unproven. Moreover, the proof target is **wasm** (aircraft do not fly on wasm) |
| **③ Functional-requirements traceability** | Bidirectional requirements ↔ code ↔ verification + MC/DC structural coverage | 🔴 **Zero** — safety is proven, but no tracking mechanism for “does it compute the correct control law?” |
| **④ Execution-environment qualification** | RTOS / HW / engine also DAL-A, WCET analysis | 🟡 **Halfway** — #865 (2026-09-18, PR #2304) removed wasmtime from the trusted base: `crates/almide-wasm-vm` executes Critical-profile `to_wasi` artifacts with a closed instruction set, load-time verification, fixed-capacity stack and fuel; requirements bind to REQ-VM-N in `REQUIREMENTS.md`. WCET uses #569's counted-loop design ([flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)). Remaining: qualification of the VM itself and RTOS / HW (outside this document's scope) |
| **⑤ Certification dossier + tool qualification** | PSAC–SAS, 71 objectives, DO-330 TQL | 🔴 A certificate is a technical artifact, not a certification artifact. No mapping to objectives |
| **⑥ Authority acceptance + track record** | DER / SOI audits, service history | 🔴 This repo does not show authority acceptance or flight experience for Almide. It does not claim the worldwide absence of precedent |

**This document covers the portions of ② and ④ that engineering can close** (pillars③⑤⑥ involve standards
processes and years of authority work, in the domain of [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) and GTM).

---

<a id="3-フロントあ--wcet--静的確保"></a>

## 3. Front (a) — WCET / static allocation

<a id="31-驚き朗報アロケータは既に-wcet-フレンドリー"></a>

### 3.1 Surprise (good news): the allocator is already WCET-friendly

The original assumption of fixed 64KiB pages and a single probe belonged to the old renderer.
The current structural allocator in `crates/almide-wasm/src/alloc_inline.rs` has size classes/free lists;
heap caps and growth handling also depend on actual build/host conditions.
Abstract allocation counts or O(1) asymptotics do not trivially yield physical WCET on the target machine.
Evaluation must specify the allocator, generated-code and host scope.

<a id="32-本当の-wcet-ギャップはアロケータではなくループ"></a>

### 3.2 The real WCET gap is **loops, not the allocator**

WCET is fundamentally about **loop iteration counts**. There is a structural hole here:

- **The Coq model is still a “loop-free” fragment**. `proofs/Termination.v:16-18` says so explicitly,
  and `OwnershipChecker.Op` has no loop constructor. Rust-side MIR has loop markers
  (`crates/almide-mir/src/lib.rs:240` `LoopStart`/`LoopBreakUnless`/`LoopEnd`), yet **the proof spine
  knows nothing of loops** (certificates treat loop markers as no-ops).
- `LoopBreakUnless` is an **arbitrary runtime condition** (`lib.rs:242`): no static iteration upper bound
  is expressed anywhere.
- RC letters i/a/d/m **abstract away byte quantities** (`Op::Alloc` = one `'i'` regardless of contents,
  `lib.rs:129`). Therefore **byte-based WCET is inexpressible in the current model**, although count-based is possible.

<a id="33-キーストーンループを-coq-op-型に持ち上げる"></a>

### 3.3 Keystone: lift loops into the Coq `Op` type

This one change unlocks multiple flight properties **simultaneously**:

1. **Loop-termination proof** (currently only the loop-free fragment in `Termination.v`)
2. **no-alloc-in-loop** (provable with a structural predicate analogous to `proofs/Subset.v`; currently the
   `scalar_loop_depth` lowering gate at `crates/almide-mir/src/lower/control.rs:446` can enforce it only on
   the **compiler side = unproven**)
3. **Counted-loop iteration bound → total allocation-count bound** (the desugaring of `try_lower_scalar_for_range`
   at `control.rs:589` already knows the iteration count — capture it there)

<a id="34-スロットイン箇所自然な順"></a>

### 3.4 Slot-in points (natural order)

| # | Location | Property | Grade |
|---|---|---|---|
| 1 | Lowering loop-subset gate (`control.rs:426,589`, existing `scalar_loop_depth`) | Enforce no-alloc-in-loop **at rejection time** | Compiler-side (unproven), cheapest |
| 2 | New certificate properties + Coq proofs (`NoAllocInLoop.v` / `AllocBound.v`, modeled on `Subset.v`) | **Recheck every build** with the checker | Proper flight-grade; **prerequisite: loops in the Coq Op type** |
| 3 | New MIR pass (counted loops only + derive iteration bounds) | Per-loop / whole-program allocation-count bounds | The substance of the WCET bound |

`Repr`/`LayoutId` already carry size information (`lib.rs`), so static pools / arena-reset can be added by
replacing `$alloc` in `render_wasm.rs`. **Fragmentation warning**: an exact-fit-only free list degenerates
into bump growth under mixed-size churn → the flight profile requires size-class pools or arena-reset.

<a id="35-飛行プロファイル--サブセット弱みではない"></a>

### 3.5 Flight profile = subset (not a weakness)

Counted ranges only, no-alloc-in-loop, static pools: **the very discipline C in DO-178C practice already uses
(no recursion, no dynamic allocation, bounded loops)**. The difference is replacing “maintained by review”
with “mechanically enforced by certificates.” **Profile restrictions are not a weakness; enforcing with proof
the shape flight code already uses becomes a selling point.**

→ **Concrete implementation design (Op extensions, Coq theorem statements, connection points, de-risking order)**:
[flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)

---

<a id="4-フロントい--rust--ferrocene-忠実性"></a>

## 4. Front (b) — Rust → Ferrocene fidelity

<a id="41-原設計の再利用見積もり割合工数は未評価"></a>

### 4.1 Original reuse estimate (proportion and effort unevaluated)

The spine splits cleanly at the wasm boundary:

| Class | Files | Implications for flight |
|---|---|---|
| **Target-independent (reuse as-is)** | `OwnershipChecker.v` (`check_sound`) / certificate i/a/d/m/r/b (= **MIR ownership facts**, not wasm, `OwnershipChecker.v:155-168`) / `certificate.rs` (zero target awareness) / `RuntimeModel.v` (abstract `Mem = Z→Z`) / `NameTotality` / `CapabilityBound` / `TypeConcretization` / `Subset` / `Termination` / `FreeList` / `CowSafety` / `ALS` | Lemmas about the abstract event model are reuse candidates. Semantic correspondence with generated Rust is a separate obligation |
| **wasm-specific (rebuild)** | `Translation.v` (op→wasm table) / `WasmEncode.v` / `WasmExec.v` / `WasmRcDec.v` + V validator in `translation_validation.rs` | 4/16 files, **the thinnest and most deferred layer** |

<a id="42-native-経路で残る証明境界"></a>

### 4.2 Proof boundaries remaining on the native path

- Theorems about `OwnershipChecker` or eager event sequences do not directly state semantic preservation
  of generated Rust. A `rust_pattern` string table alone does not prove all Almide→Rust translation fidelity.
- A policy of adopting Ferrocene is different from having actually verified with a qualified toolchain.
  The current `.github/workflows/ferrocene-lane.yml` explicitly uses stock rustc.
- Qualification of a downstream tool does not automatically remove upstream translation obligations.
  Even choosing a path other than Wasm requires evidence connecting its model/generated code/machine code;
  low cost has not been established.

<a id="43-原設計の-renderer-工数評価現在は-native-renderer-が実在"></a>

### 4.3 Original renderer-effort estimate (a native renderer now exists)

The following compares the original demo and walker; it does not mean the current renderer is unstarted:

- **Clean ownership-preserving mapping** = `crates/almide-mir/src/render_native.rs` (368-line demo).
  But uniform Vec<i64>, no CLI connection, and numbered locals `v0`/`v1` made it **not review-grade**.
- **Production Rust** = `crates/almide-codegen/src/walker/` + `crates/almide-codegen/templates/rust.toml`
  (v0/legacy). Idiomatic, **reviewable and meets DO-178C “source must be readable,”** but
  **no correspondence object between MIR ops and Rust fragments, not driven by v1 MIR, and unproven**.

→ The real pivot cost = **building a production-grade v1 MIR→Rust renderer** (a full-language version of
`render_wasm.rs`, about 5× the scale). Arrange for **the same certified MIR witness to drive both targets**.

<a id="44-v1-mir-architecture-9-との和解"></a>

### 4.4 Reconciliation with v1-mir-architecture §9

[v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) §1 says “wasm = sovereign verification path, Rust = an almost
trivially lowered stepping stone”; §9 rejects “make Rust the single source of truth = build on Rust semantics
without a formal specification.” **The flight profile does not contradict that rejection**:

> Flight **does not build on Rust semantics**. It binds to certified **MIR facts**; Rust remains just a renderer,
> and Ferrocene takes over trust for Rust→machine code. MIR stays the single source of truth.
> The only change is promoting the Rust renderer to a **proof-bound second production target**:
> sovereignty stays with MIR.

§9 rejected making Rust the **truth**. This profile makes Rust a **proof-bound renderer**, a different proposal.

→ **Concrete implementation design (rust_pattern table, fidelity theorem, trusted-base ledger, Ferrocene demonstration path)**:
[flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)

---

<a id="5-合流--一貫した飛行スタック"></a>

## 5. Convergence — a coherent flight stack

```
Almide 飛行サブセット            counted loop / no-alloc-in-loop / static pool  ← (あ)§3.5
   ↓  lower + checker
認証済み MIR(唯一の真)          既存4性質 + 新: bounded-alloc / counted-loop    ← (あ)§3.3
   ↓  render_rust(本番版を新設)                                                 ← (い)§4.3
可読 Rust                        rust_pattern 忠実性(eager instance 既証明)     ← (い)§4.2
   ↓  Ferrocene(資格化済み)      Rust→機械語の信頼を肩代わり = Gap 1 迂回         ← (い)§4.2
飛行 HW で走る機械語 + WCET 有界
```

Trust at each layer is accounted for. **Strategic freedom**: because the soundness core is target-independent,
**wasm sovereignty (agent market) and Rust flight need not be an either/or choice**: both become
**different consumers of the same certified MIR witness**. The decision is not “which to abandon,” but
“**whether to add a Rust flight renderer as the second consumer of the same witness**.”

---

<a id="6-2つのキーストーン"></a>

## 6. Two keystones

| | Keystone | Kind | Unlocks | Prerequisite |
|---|---|---|---|---|
| **(a) WCET** | Lift loops into Coq `Op` type + counted-loop bounds | Proof extension | Termination / no-alloc-in-loop / allocation-count bounds — three flight properties simultaneously | None (independent of Rust decision) |
| **(b) Ferrocene** | Production v1 MIR→Rust renderer + `rust_pattern` fidelity layer | Evidence of translation/runtime correspondence and engineering (effort undetermined) | Bypass Gap 1 / flight artifacts / Rust market | Establish §4.4 policy |

**Order**: they are independent. If ordering them, **lift loops in (a) first**: settle the flight subset's
“shape” and unlock a termination proof that is valuable on its own. **For (b), first record the §4.4 strategic
decision (reconciling wasm sovereignty and Rust flight), then start the production renderer.**

---

<a id="7-flight-readiness-ladder--近い-の-exit-criteria"></a>

## 7. Flight-readiness ladder — exit criteria from 🔴 to near

The A/B-tier status in [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) / GTM is currently 🔴 (not feasible yet).
Define ordered gates to flip this to **near**. Before “flight-grade achieved” (all pillars①–⑥ satisfied and
authority acceptance), place an intermediate goal: **the technical prerequisites for viable sales are in place**.

<a id="71-近いの定義誇張しない線"></a>

### 7.1 Definition of “near” (a non-exaggerated boundary)

> **Near** = admissible to A/B-tier certification and safety engineers as a “qualifiable implementation substrate,”
> with technical prerequisites in place to make **pilots/partnerships realistic**.
> **≠** Full aircraft certification, authority (FAA/EASA) acceptance or production flight.
> Those are multiyear customer-side processes outside this ladder (§8).

<a id="72-gate近い順序つき"></a>

### 7.2 Gates (🔴→near, ordered)

| Gate | Content | Why it gates credibility | Linked design |
|---|---|---|---|
| **G-F0 Execution parity** | Real programs run end-to-end and match v0/oracle byte-for-byte (complete Gap 3) | A language that cannot run is rejected at the door. The floor for everything | Self-host runtime (in progress) |
| **G-F1 Flight-profile definition + enforcement** | Specify flight subset (counted loops / no-alloc-in-loop / bounded static allocation); checker enforces boundary as a wall | Without a “shape” to show safety engineers, the conversation cannot begin | §3 keystone (a), first half |
| **G-F2 Prove bounded WCET** | Lift loops into Coq + counted-loop bounds + total allocation bounds as proven properties | “WCET by construction” is table stakes for A/B tiers | §3.3 keystone (a), core |
| **G-F3 Decide flight target + bind fidelity** | Decide Rust→Ferrocene as flight target; prove production MIR→Rust + `rust_pattern` fidelity over flight subset; demonstrate Ferrocene compilation | Without a concrete grounding for object-code trust (pillars②④), it stays theoretical | §4 keystone (b) |
| **G-F4 Reference app passes make verify end-to-end** | Small safety-critical-shaped module (control-law kernel/state machine/watchdog) in flight profile: runs→oracle match→certificate issuance→readable Rust→Ferrocene | **Material** that draws certification engineers in. Turn slides into artifacts | `make verify` killer demo ([v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md)) |
| **G-F5 Certificate↔certification-objective mapping (dossier seed)** | Draft which DO-178C objectives / DO-330 tool-qualification arguments / DO-333 formal-method credit the certificate/receipt covers, in a form a DER can review | “Certification later” cannot support “qualifiable” | CG domain of [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) |
| **G-F6 Qualification-kit product definition (AbsInt model)** | Scope an evaluation-kit proposal (checker evidence, verification harness, traceability, dossier templates). A definition does not mean qualification or acceptance achieved | Without something to sell, “near” does not become sales | GTM ([v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md)) |

**Once G-F0..G-F6 are all in place, flip A/B-tier status from 🔴 to near.**

<a id="73-当初の進捗マーカー現行-verdict-は-receipt-を参照"></a>

### 7.3 Original progress markers (see receipt for current verdict)

- **G-F0**: 🔄 In progress — climbing execution parity through self-host stdlib (moving string/list/math to Almide)
- **G-F1**: 📐 Normative spec exists, unenforced — [flight-subset-spec](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md) (feature classification, open-question decisions, `@flight` enforcement design)
- **G-F2/G-F3**: 📐 Designed, unimplemented — the two keystones in §3/§4 are **the largest technical mountains** ([flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md) / [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md))
- **G-F4**: 📐 Designed, unimplemented — [flight-reference-app](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-reference-app.md) (PID control-law kernel + seven make verify stages + receipt. Slice 0 is currently green)
- **G-F5/G-F6**: 📐 Designed, not started — [flight-qualification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md) (DO-178C/330/333 mapping + qualification kit)

→ On the ladder, **still partway through G-F0** (every gate now has a design). Critical path = finish G-F0 →
keystones (a)(b) → reference app → dossier seed + kit definition. **All gates have advanced from “not started”
to “designed, unimplemented”** (all seven gates have concrete design documents).

<a id="74-近いに不要誇張防止後回し"></a>

### 7.4 **Not required** for “near” (avoid exaggeration, defer)

This overlaps §8, but makes the ladder boundary explicit. Do **not** add the following to “near” (that would overbuild):

- Full DO-178C certification of an actual aircraft (customer-side, years)
- Authority acceptance of a “PCC = qualification substitute” argument (zero precedent, regulatory)
- MC/DC structural coverage of the **entire language** (substitute DO-333 formal-method credit for the flight subset)
- WCET of the **entire language** (only the flight subset needs it)
- Service history / flight experience

---

<a id="8-この文書が変えないもの正直な範囲"></a>

## 8. What this document does not change (honest scope)

Engineering can close only part of pillars②④. **The following remain far from flight-grade and outside this profile**:

- **Pillar③ Functional-requirements traceability + MC/DC** — safety proofs cannot substitute (application responsibility +
  tracking/coverage mechanisms needed). CG domain of [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md).
- **Pillar⑤ Certification dossier + DO-330 objective mapping** — certification-format artifacts not yet generated.
- **Pillar⑥ Authority acceptance + service history** — “replace qualification with PCC” has zero precedent and requires
  years of regulatory work. Zero flight experience.

→ Thus it is correct, as [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) / GTM says, that **aviation is a north star and credibility
anchor, not a near-term product**. The agent and CRA markets unlock **without closing** this profile's keystones.
This profile defines “two shortest engineering paths to **seriously approach aviation**,” not the entire process
of “winning aviation.”

---

<a id="9-関連"></a>

## 9. Related

- [flight-subset-spec](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md) — **G-F1 normative spec** (flight-subset features IN/OUT, open-question decisions, `@flight` enforcement)
- [flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md) — **Concrete implementation design for keystone (a), G-F2** (counted loops + Coq lifting + allocation bounds)
- [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md) — **Concrete implementation design for keystone (b), G-F3** (production MIR→Rust + rust_pattern fidelity)
- [flight-reference-app](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-reference-app.md) — **G-F4** (PID control-law kernel + seven make verify stages + receipt)
- [flight-qualification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md) — **G-F5/G-F6** (DO-178C/330/333 mapping + qualification kit)
- [v1-proof-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md) — Concrete PCC spine, Gap 1/2
- [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md) — i/a/d/m/r/b ownership alphabet
- [v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) — §1 wasm sovereignty, §9 rejected alternatives (reconciled in §4.4)
- [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) — Five standards mechanisms, pillars③⑤ domain
- [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) — Property coverage 4/8, gap ledger, GTM ladder
- [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) — L0-L4, aviation = north star

Reader explanation of the preserved Japanese diagram: it depicts the proposed chain from the Almide flight subset (counted loops / no allocation in loops / static pools, front (a) §3.5), through lowering and checking, to certified MIR as the single source of truth (four existing properties plus bounded allocation / counted loops, (a) §3.3). A newly built production render_rust ((b) §4.3) yields readable Rust with rust_pattern fidelity (the diagram calls the eager instance already proven, (b) §4.2); Ferrocene, labeled qualified in the diagram, takes over Rust-to-machine-code trust to bypass Gap 1 ((b) §4.2), ending in machine code running on flight hardware with bounded WCET. This is the original diagram's design claim, not an independently verified implemented or qualified stack.

Historical consistency note: §2 explicitly declines to claim that no precedent exists worldwide, while later §7.4/§8 retain stronger “zero precedent” wording. Both are preserved rather than reconciled into a new factual claim. The old diagram's qualification/proof wording and §4.2's stock-rustc/unproven-translation boundaries likewise remain distinct. §7.3 and §9 call the old subset document normative; the current bounded-profile authority remains ALS §B, as stated above.
