<a id="trust-layer--機械が書くソフトウェアの信頼層"></a>

# Trust Layer — the trust layer for machine-written software

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](trust-layer.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](trust-layer.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](trust-layer.zh-CN.md)

> Non-normative source-pinned reader snapshot. The unchanged original remains authoritative for its consumers. No automatic synchronization or independent execution. Historical claims, accepted proposals, implementation status and measured evidence retain their original scope; this translation does not independently verify them. Inline code remains verbatim, including original-language string literals. MWS L0-L4 is a proposed trust ladder, not edit-locality L1-L3 and not a certification. Three-line setup and reproducible claims are exit goals, not verified deliveries here. Preserve all host/native/FFI, fixture, decline, trusted-base and external-Dojo limits. Exclusivity, competitor assessments and market inevitability statements are the historical author’s strategic judgments, not independently established facts. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md)

> Original metadata, inactive: Category strategy—winning “the trust layer for machine-written software”: MWS Trust Levels, receipts, critical path.

> **Goal**: Define the measurement criteria (MWS Trust Levels) for the category “operating agent-written code at a level of trust approaching no human review,” and make Almide its reference implementation.
> **Active scope**: Publish the Levels' definition + L1 ([capability Phase 1-2](../../../../../roadmap/active/effect-system-capability.md)) + receipts harness.
> **Exit criteria**: A third party can reproduce every L1-L2 claim with `git clone && make verify`, and a capability-bounded Almide agent runs from an MCP client with 3 lines of configuration.

<a id="なぜこのカテゴリが立つか"></a>

## Why this category can emerge

As agents become the writers of code, the language selector changes from the individual developer to the **agent operator**, changing the selection criteria:

| Criteria when humans write | Criteria when machines write |
|---|---|
| Familiarity, hiring, expressiveness | Modification survival rate (MSR) |
| Breadth of libraries | Provability of the blast radius |
| Build speed | Reproducibility of builds and execution |
| Peak performance | Whether operation without review is possible |

The right column is Almide's design metrics themselves. No other language aligns its design objective function with the buying criteria. Winning this category is not about comparing languages, but about **becoming the party that defines the measurement criteria**—the pattern SLSA used in supply-chain security. Once others start using the criteria to assess themselves, the category is established, and its reference implementation becomes the default.

<a id="mws-trust-levels-l0l4"></a>

## MWS Trust Levels (L0–L4)

Each level is defined by “what an operator can trust *without verification*.” **Do not gerrymander**: each level must matter to the industry even without Almide. Arbitrary criteria will not be adopted, and criteria that are not adopted do not create a category.

| Level | Goal | Current evidence and limits |
|---|---|---|
| **L0 Contained** | Runtime isolation | Depends on Wasm host imports/configuration/permissions. Not a claim that native builds or external code are automatically sandboxed |
| **L1 Bounded** | Restriction of declared capabilities | Combines capability checking with artifact/host boundaries. Import presence alone does not prove the operations of shared imports or the host implementation |
| **L2 Reproducible** | Reproduction and observational comparison under specified conditions | Refer to C-REPRO/C-SAFE in `proofs/receipt.sh` and the fixture/decline scope. Not invariance for arbitrary hosts and inputs |
| **L3 Verified** | Mechanical proof of an explicit safety model | The receipt is authoritative for current Rocq/structural RC evidence and trusted boundaries. Not a proof of all compiler paths or all stdlib |
| **L4 Measured** | Measurement of LLM editing quality | Owned by Dojo. Limited to the models/tasks/versions in public results; this repo alone does not establish the latest status of external measurements |

> **This L0-L4 is a trust ladder, distinct from L1-L3 in `docs/specs/edit-locality.md` (edit frame / cross-target agreement / diagnostic locality).** They share numbers, but their claims are unrelated, and neither is a prerequisite of the other. Where context does not make it clear, state which ladder is meant.

Design point: **L1 is the technical heart of trust**. L0 says “cannot get outside the wall,” whereas L1 aims to check declared capabilities against actual artifact/host settings. Manifest comparison helps operator judgment but does not replace every review, including the semantics of hosts, native dependencies, and FFI. L2-L4 likewise carry the scope and trusted base of each piece of evidence.

<a id="既存-roadmap-とのマッピング"></a>

## Mapping to the existing roadmap

This category strategy **adds no new implementation roadmap**. It is a meta-layer defining the completion order and publication form of existing roadmaps.

| Level | Responsible roadmap | Remaining work |
|---|---|---|
| L1 | [effect-system-capability](../../../../../roadmap/active/effect-system-capability.md) | Distinguish existing compile checks from remaining artifact/host-boundary proposals |
| L2 | [wasm-platform-frontier](../../../../../roadmap/active/wasm-platform-frontier.md) | Document the deterministic profile (NaN audit + declaration of no relaxed SIMD) |
| L3 | `proofs/receipt.sh` / `TRUSTED_BASE.md` | State current Rocq/structural proof scopes and unproven connections |
| L4 | almide-dojo (separate repository) | MSR publication format and third-party reproduction steps |
| Certification-grade hardening of L2/L3 | [certification-grade](../../../../../roadmap/active/certification-grade.md) | CG-1–CG-5 (ALS + normative semantics / coverage / Critical profile / translation validation / dossier) |
| Performance measurement (not a guarantee) | [wasm-optimization-roadmap](../../../../../roadmap/done/wasm-optimization-roadmap.md) | 4 losses burndown → 11/11 |

Performance comparisons are limited to the workloads, versions, and settings measured. The original 11/11 result does not establish that safety mechanisms always cost zero, or that all levels' claims are unconditional.

<a id="critical-path"></a>

## Critical path

In dependency order. Performance burndown and Perceus Phase A can proceed independently in parallel.

1. **Capability Phase 1+2**—the body of L1. `manifest.json` is the first “receipt” an operator can verify before loading. Phase 1 alone can explain “compile-time checking of declared capabilities,” but does not guarantee a complete execution/build sandbox.
2. **Document the deterministic profile**—exactly the frontier document's TODO. Small, and supplies L2's specification vocabulary.
3. **Receipts harness (`make verify-trust` / `make receipt`, existing)**—rederive the checks/evidence listed in the receipt within each declared scope: the basis, target fixtures, declines, and trusted boundaries for each receipt claim. The original counts of 62 files / 11 benches / oracle registry are not a current coverage inventory. **A language selling verifiability must make the claims themselves verifiable.**
4. **Publish Trust Levels externally**—turn this file into a criteria document and publish it in a form other toolchains can use for self-assessment (win by having others use the criteria).
5. **MCP stdlib module** ([capability Phase 5](../../../../../roadmap/active/effect-system-capability.md))—the distribution wedge. “A capability-bounded agent runs with 3 lines of Claude Code configuration” is the category's first demo and an entry point for buyers (agent-platform developers).
6. (Parallel) **Performance burndown** → verify improvements with workload-specific measurements.
7. (Parallel) **Expand proof coverage** → update L3's scope and unproven boundaries, without declaring overall completion.

<a id="何をしないか"></a>

## What not to do

- **Compete as a general-purpose ecosystem**—do not compete on library breadth alone. For the current API, consult the [stdlib index](../../../../../stdlib/README.md), not a fixed old function count.
- **FFI/native boundaries**—`ffi` and native dependencies exist, so “there is no user FFI” must not be a premise of L1-L3. Follow the [package system](../../../../../specs/package-system.md) and current CLI spec for permissions and trust boundaries when using them.
- **Gerrymander the criteria**—keep an honest definition at every level that competitors can partially meet. Writing in the table that Deno meets half of L1 is the very basis of the criteria's credibility.

<a id="リスク-正直に"></a>

## Risks, honestly

| Risk | Nature | Response |
|---|---|---|
| Data headwind (LLMs are trained on an ocean of Python/TS) | The whole project's bet | Dojo makes the bet's success or failure visible in numbers. Keep measuring whether in-context material (CHEATSHEET + skill) beats pretraining familiarity |
| Category fails to emerge (no market for unsupervised operation) | Timing risk | With the current trajectory of agent platforms, it is a question of “when.” Treat this as a race to accumulate receipts first |
| An agent language from a lab appears | Largest competitive scenario | Features can be imitated, but retrofitting the methodology (oracle pairing / ratchet / daily MSR / Lean) is expensive. Earlier receipts themselves are the defense |
