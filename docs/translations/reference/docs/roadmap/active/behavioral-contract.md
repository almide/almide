Source description metadata: Behavioral evidence proposal: distinguish prior-version preservation and human-approved properties from scoped compiler/runtime claims.

<a id="behavioral-contract--改変保存と批准性質を区別する設計案"></a>

# Behavioral Contract — a design proposal distinguishing modification preservation from ratified properties

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](behavioral-contract.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](behavioral-contract.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](behavioral-contract.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md)

> Source caveat: these are proposed receipts and a proposed harness, not current guarantees of complete semantic preservation or functional correctness. Prior-version quality, ratification quality, property coverage, checker/artifact correspondence and trusted boundaries remain assumptions. Narrow RC claims must not be promoted into arbitrary whole-program refinement. The risk table asks that minimal-declaration discipline be stated in section 5, while the listed constraints there do not explicitly use that phrase; this proposal-level mismatch is retained. No exclusive advantage over other verification approaches is established.

> **What this is**: a design proposal for handling evidence of modification preservation and ratified properties in machine-generated/modified code, distinguishing the source of the specification from the object of verification. It addresses the **intent axis** (does the program itself do the right thing?), orthogonal to the **trust axis** (can the compiler/toolchain be trusted?) pursued by [v1-proof-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md).
> **Active scope**: turn receipt **C-PRESERVED** (differential preservation) into a harness, and establish the decidable fragment for **C-ASSERTED** (ratified properties).
> **Exit criteria**: for an agent-modified `.almd` module, the proposed harness emits “preserved the observational contract” in a form reproducible by a third party. The original `make verify` is a conceptual name, not a guarantee that current receipts preserve all semantics.
> **Sibling documents**: [v1-proof-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md) (destination), [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) (receipt format), [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) (L0–L4 trust axis).
> **Measurement**: [law-blocks-experiment](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md) (a record of one experiment testing C-ASSERTED with MSR and automated Lean proofs, 2026-09-27).

---

<a id="1-なぜ別の層が要るか--c-faithful-はプログラムの正しさではない"></a>

## 1. Why another layer is needed — C-FAITHFUL is not “program correctness”

Existing evidence has different scopes for different claims. C-FAITHFUL in `proofs/receipt.sh` concerns the correspondence of explicitly specified RC routines/models; it does not claim that every arbitrary whole program has been proved to refine ALS. C-SAFE/C-REPRO also come with target fixtures, declines and trusted boundaries. Conformance to a functional specification is yet another question.

> “The sort implementation preserves the source semantics” and “that sort satisfies the specification of sorting” are separate obligations. This does not mean the former has already been proved for arbitrary sorts.

The axis marked `負け` in the comparison table — `機能仕様`, proved by Dafny/F*/SPARK/Lean (“the output is a sorted permutation,” for example) — is the latter: **the behavior of the program itself**. It is a separate question one level above the trust axis, not filled by existing layers. Hence a new layer is added.

The preserved comparison labels mean “loss” and “functional specification,” respectively.

---

<a id="2-この提案で重視するもの"></a>

## 2. What this proposal values

Competing with existing formal-verification systems in expressiveness or market ranking is not this proposal's goal. It investigates whether operations involving machine generation/modification can make the specification's source, the properties being verified and the unverified scope explicit at low cost. Other languages and proof systems may support similar operations; it does not presume assertions about others such as “only experts can use them,” “adoption is not growing,” or “this approach is unusable in principle.”

---

<a id="3-再定義--機能正しさは証明問題でなく信頼できる仕様の出所問題"></a>

## 3. Reframing — functional correctness is a “trusted specification provenance problem,” not a “proof problem”

Even if `code ⊨ spec` is proved, trust merely moves to **“is that spec correct?”**

- **A world where humans write specifications**: that human is the trust oracle. That is why it works.
- **A world where machines write them** (= Almide's world): a machine-written specification is **only as trustworthy** as machine-written code. The same-author problem (§v1-arch 1).

In addition to proof obligations, this makes explicit **where the specification comes from and on what basis it is adopted**. This is neither unique to Almide nor a problem other verification systems cannot handle. This proposal considers two candidate sources of specifications:

1. **The immediately preceding trusted version** — the pre-modification code has a history of “having been operated as correct.” Use it as a differential oracle.
2. **The asymmetry of co-generation + ratification** — machines propose many short properties; humans review and ratify their meaning, coverage and fit to the use. Short wording alone guarantees neither validity nor completeness.

---

<a id="4-着地形--c-faithful-の上に二つの受領書を積む"></a>

## 4. Destination — add two receipts above C-FAITHFUL

Consider reusing the design that separates a witness from a small checker. Existing RC theorems do not automatically become proofs of functional specifications: new grounds are required for each property's semantics, checker soundness and correspondence with artifacts. The choice of proof system is separate from the system-wide TB; the latter retains assumptions about models, execution, extraction and so on. The design policy of not inventing a new logic remains, but there is no claim that Coq alone closes all trust.

<a id="c-preserved--差分保存近未来の本丸"></a>

### C-PRESERVED — differential preservation (the near-term main objective)

> **Claim**: this modification preserves the **observational contract** (declared input/output and effect surface) of the immediately preceding trusted version. **Specification source = preceding version.**

This is a proposal for preserving observational contracts relative to the pre-modification version, not the MSR measurement itself. The previous version's operational history helps choose the comparison source, but does not remove its bugs, unobserved inputs or environmental differences. Suitability for modifying existing code is to be verified with the harness and usage conditions fixed.

- Different from C-REPRO: C-REPRO is “same source → same bytes” (determinism). C-PRESERVED is “**different source** (after modification) → contractually equivalent.”
- Also different from a compiler regression oracle (new compiler ≈ old compiler): that concerns toolchain stability; this concerns **preservation under modification of the user's program**. The layers differ.

<a id="c-asserted--批准性質グリーンフィールド用"></a>

### C-ASSERTED — ratified properties (for greenfield code)

> **Claim**: the program satisfies the ratified property set Π. Π belongs to a **decidable fragment**; a machine discovers satisfaction, a checker rechecks it, and **humans ratified only the text of Π**.

New code has no previous version, so this proposes using a ratified property set as a separate source of evidence. Review effort is not necessarily limited to three lines, and unspecified requirements are not guaranteed.

| Proposed receipt | Specification source | Main use | Remaining assumptions |
|---|---|---|---|
| **C-PRESERVED** | Immediately preceding trusted version | Modifying/refactoring existing code | Previous-version quality, observational-contract coverage, identification of the environment |
| **C-ASSERTED** | Short properties from co-generation + human ratification | Correctness of new code (partial) | Validity of ratified properties, correspondence between checker and artifact |

---

<a id="5-設計上の絶対制約自分を縛る"></a>

## 5. Absolute design constraints (self-imposed)

- **Restrict the property language to a decidable/automatable fragment.** The moment expressiveness is pursued too far, properties that cannot be automated must be handled. Display unproved properties as unclaimed, and measure the effectiveness and burden of the restriction.
- **C-PRESERVED is “preservation,” not “correctness.”** It is worthless if the preceding version is wrong, and does not help greenfield code. **A necessary condition, not a sufficient one.**
- **C-ASSERTED can never reach completeness.** Even proving `順列 ∧ 整列` does not catch “a stable sort is required.” The endpoint is the tier **“machine-verified against ratified Π.”**
- **Do not mix the three layers into one “correctness” number** (strictly retain the 3 tiers of done). A receipt must state **which claim from which source** it carries.
- **Do not bloat the checkers.** These checkers must also remain easier than generation — an overgrown checker becomes the next trust problem (the same discipline as §v1-arch 9).

The preserved property expression means “permutation ∧ sorted.”

---

<a id="6-完全性用途相対で数える"></a>

## 6. Completeness (count relative to the use)

Extend the U-relative completeness of [v1-proof-architecture §7](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md) one step:

> **U₃ = unsupervised *modification* operations**: C(U₂) + **C-PRESERVED coverage**. In operations where agents continually rewrite production code, each modification preserves the observational contract.

Do **not** say “Almide proves program correctness.” Report only countable forms: “**n/n modifications received preservation receipts for U₃**” and “this function is **verified against ratified Π**.”

---

<a id="7-何をしないか"></a>

## 7. What we will not do

- **Functional verification against human-written specifications** (the Dafny/SPARK approach). Instead, shift the specification's source to the machine-generation regime (§3).
- **Beat Lean in benchmarks.** That is vanity. The winning condition is not “the most expressive proof system,” but **“the cheapest trustworthy behavioral guarantee for machine-written code.”**
- **An expressive property logic.** Stay within a decidable fragment (§5).
- **Blend C-PRESERVED with C-ASSERTED.** Different provenance means different quality of trust.

---

<a id="リスク-正直に"></a>

## Risks (honestly)

| Risk | Nature | Response |
|---|---|---|
| SMT incompleteness erodes writability | This layer's greatest engineering risk | Restrict the property language to a decidable fragment. LLMs propose Π; humans only ratify. If automated satisfaction fails, honestly display the gap as “unclaimed” |
| C-PRESERVED cold-start | An initial version has no preceding oracle | Greenfield uses C-ASSERTED + C-FAITHFUL only. Honestly label the tier as **new code is weaker**. Accept that modification operations are its strength |
| Ratification fatigue (humans rubber-stamp Π) | Trust rests on ratification discipline | Measure coverage and quality of ratified Π in dojo. State the “discipline of minimal declarations” explicitly among §5 constraints |
| “Winning on an axis” becomes vanity | Strategic risk | The aim is to neutralize a weakness (provide functional correctness one level up at low cost). Keep the primary win in the moat (LLM × machine-verified contracts) |

---

<a id="一文で"></a>

## In one sentence

A design proposal for verifying machine-generated code by presenting **preservation of the observational contract relative to the previous version** and **conformance to human-ratified properties** as separate evidence. Both make specification provenance and verification scope explicit; neither claims unspecified requirements, correctness of all programs, or exclusive superiority over other approaches.
