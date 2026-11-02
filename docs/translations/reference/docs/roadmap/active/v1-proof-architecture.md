Source description: Proposed terminal-state proof architecture; current implementation and trusted base remain separately scoped
<a id="v1-proof-architecture--構想着地形"></a>

# v1 Proof Architecture — envisioned final form

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-proof-architecture.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-proof-architecture.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-proof-architecture.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md)

> Source caveat: terminal-state design prose is not evidence that K/V, verified extraction, independent implementations or qualification have been delivered. Broad trust and safety rhetoric is preserved as the source position, bounded by its later corrections: checker soundness only covers stated properties and model/artifact premises, while ALS intent, execution, extraction, tooling, OS and hardware remain trusted. Certificate issuance is not third-party certification. The original table’s untrusted checker means the compiler-side checker, distinct from the trusted property/translation checkers. Cross-target byte equality is restricted by observable contracts and host boundaries; refinement alone does not erase those conditions. Describing known limits does not independently establish all other properties. Diagram text is retained verbatim with a full explanation outside it. No regulatory applicability, implementation, proof or measurement was independently validated.


> **What this is**: a concept document showing, in one picture, the **completed form** of the proof/trust architecture for v1, which rebuilds Almide from zero. It describes **what the result looks like**, not a procedure. Every project-specific term is defined where introduced so someone unfamiliar with the conversation or existing documents can read from the beginning.
> **Sibling documents**: [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) (L0–L4 category strategy), [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) (receipt format), [completeness-by-construction](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/completeness-by-construction.md) (semantic ledger), [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) (certification-grade hardening). This document **folds their intended final forms into one picture**.

---

<a id="1-なぜこの設計が要るのか"></a>

## 1. Why this design is needed

In an era when AI writes programs at massive scale and speed, the software bottleneck has shifted **from “can we write it?” to “can we trust it?”** Code appears almost for free, but it is no longer possible for humans to read all of it and establish safety.

Software trust has traditionally rested on **who wrote it** (author reputation + human review). But when the author is a machine, reputation does not accumulate, reading cannot keep up with writing, and there is nobody to hold responsible when it breaks. **Person-based trust does not scale to machine authors.**

The only remaining path is **to verify the artifact itself, not its author**: not “trust me,” but “check for yourself.” Almide v1 makes this the architecture of the language and toolchain itself.

---

<a id="2-設計の背骨これ一つ"></a>

## 2. The design's single backbone

> **Do not prove the compiler correct. Prove only a small checker's soundness; have the compiler emit a “certificate” with each build, and have the checker check it every time.**

The basis is a simple asymmetry: **making is hard, but checking is orders of magnitude easier** (solving Sudoku is hard; checking a solved grid takes an instant).

Consequences:

- The compiler, of any size—even 100,000 lines—**may contain bugs**. If it produces bad output, the accompanying certificate will not hold and the checker rejects it.
- The **only thing that must be proved is checker soundness**: “**If the checker accepts, the artifact has the property.**” This theorem never mentions the compiler's internals, so those 100,000 lines become **untrusted**.

This structurally removes the need to build a perfect compiler for the sake of proof.

---

<a id="3-用語初出で定義"></a>

## 3. Terms (defined on introduction)

| Term | Meaning |
|---|---|
| **ALS** (Almide Language Specification) | The language's **normative semantics**, the sole canon formally defining what each program means. Both compiler and reference executor follow/implement it rather than deciding meanings themselves. |
| **Certificate / witness** | **Machine-checkable evidence** the compiler attaches to an artifact: statements such as “all memory was cleaned up” or “nothing was written to an invalid location,” in a form the checker can compare. |
| **Certificate format** | A language for writing witnesses, **deliberately made maximally simple**. Simplicity reduces ambiguity and lets anyone write an independent reader. |
| **Checker** | A small program (hundreds to a thousand lines) that reads the certificate and compares it with the artifact to confirm validity. Trust boundaries beyond the checker remain, including formalization, execution, extraction and hardware. |
| **Refine (refinement)** | The formal version of “faithfully implements”: “actual artifact a refines model M” means that a's observable behaviors are only those M permits. |
| **Trusted Base (TB)** | The set of things **we decide to trust without questioning** when accepting the claim. Smaller is better. |
| **Coq / Rocq** | A tool for writing proofs whose **tiny core (kernel) rigorously rechecks for holes**. However large the automation, the kernel alone is ultimately trusted in that mechanism (a way to narrow trust). |
| **CompCert** | A **C compiler whose correctness is mathematically proved**, one example of a verified compilation path considered here. |
| **CertiCoq** | A candidate verified compilation route from Gallina. Its connection in this concept includes unimplemented parts; assessment must state the relevant proof assumptions, extraction/execution environment and machine-model-to-hardware boundary. |
| **DO-178C / DO-330** | Aviation software certification rules (178C) and provisions for **officially recognizing tools** within them (330). |
| **Thompson hole** | The classic problem that “a betrayal built into the compiler itself cannot be seen by reading source.” Verified compilation is one countermeasure component, but does not automatically close the provenance of the binary run, build environment or lower layers. |

---

<a id="4-着地形--完成したアーキテクチャ"></a>

## 4. Final form — the completed architecture

The goal is to concentrate the formal-proof checking core in the **Coq kernel**. That does not mean it becomes the entire system's sole trusted base. ALS expressing intent, model/artifact correspondence, extraction/execution routes and hardware/OS follow §5/§8 and the current scopes of `TRUSTED_BASE.md` / `proofs/receipt.sh`. The diagram below is a final-form concept containing unmet items, not a currently accepted qualified configuration.

```
        ┌────────────── 規範意味論 ALS(Coq 内の形式意味論)──────────────┐
        │   すべてのプログラムの意味は、ここだけが定める                   │
        └───────────────────────────┬───────────────────────────────────┘
                       refine        │        refine
        ┌──────────────┘             │             └──────────────┐
   非信頼コンパイラ          性質 P {メモリ安全・stack 均衡・        参照実行器
   (規模不問・バグ可)        name 全域・capability 上界・型        (ALS の実行像)
        │                    concretize・終端挙動} = ALS に対して定義
        ▼
  ( wasm バイト列 a , 証明書束 c ) ── 継ぎ目は「証明書形式」ただ一つ
        │                            (極限まで単純・意味論を完全文書化)
        ├──► 性質検査器 K   K(c, a) = 受理 ⟹ a は P を全部満たす
        └──► 翻訳検査器 V   V(a, ALS(s)) = 受理 ⟹ a の挙動は ALS(s) を refine
                  │
   K・V は Gallina 関数として書かれ、Coq で健全性を証明され、
   CertiCoq + CompCert で機械語化され、独立 2 実装(うち 1 つは機械語まで検証)が
   一致して初めて通り、DO-330 で資格化される唯一の対象。
```

Full explanation of the preserved Japanese diagram: normative ALS semantics formalized in Coq alone defines every program's meaning. Both the untrusted compiler (arbitrary size, bugs allowed) and reference executor (ALS's executable image) refine it. Property set P—memory safety, stack balance, total names, capability upper bounds, type concretization and termination behavior—is defined against ALS. The compiler emits wasm bytes a and certificate bundle c; their sole seam is a deliberately simple, fully documented certificate format. Property checker K accepts K(c, a) only if a meets every property in P. Translation checker V accepts V(a, ALS(s)) only if a's behavior refines ALS(s). The envisioned K and V are Gallina functions with Coq soundness proofs, compiled to machine code by CertiCoq + CompCert. Acceptance requires agreement of two independent implementations (one verified down to machine code); these are the sole intended DO-330 qualification subjects. This is the proposed terminal architecture, not proof that these connections or qualification already exist.

**Compilation and certificate issuance are the same act.** Each analysis pass's output is its witness. The compiler speaks only the certificate format; the checker listens only to that format. They do not know each other's internals—the architecture's single seam.

**ALS is the sole norm.** Only ALS defines meaning. The compiler, reference executor and both backends are all refinements of ALS. Thus two-target byte identity is not an after-the-fact check, but **a consequence naturally arising from the same system**.

**Acknowledge two checkers from the outset.**
- Property checker K checks, through the certificate, that artifact a has properties P.
- Translation checker V checks **on every build** that emitted bytes a refine ALS(s), answering the reviewer's decisive question: “**You proved the model, but what about the actual thing that flies?**”

Both are small, both Coq-proved, both verified down to machine code, both qualification subjects. They share an “artifact semantics library” to reduce duplicated qualification.

**WASM is the canonical trusted artifact.** rustc participates only in the one act of building the compiler, never in the artifact path (= the smallest foundation to trust). Readable Rust → Ferrocene is a second qualification route, likewise a refinement of ALS.

**Diversity and audit.** Acceptance requires two independent checker implementations, from different teams and languages, to agree. A gate checks every time that all theorems' used axioms fit the fixed standard set (Coq `Print Assumptions` ⊆ standard). There are zero tricks that expand trust silently, such as trusting compiler execution within proofs.

---

<a id="5-信頼境界--何を信じ何を信じないか"></a>

## 5. Trust boundaries — what is and is not trusted

| Category | Contents | Correctness |
|---|---|---|
| **Untrusted** | parser, checker, all codegen passes, optimizer, wasm emitter (= almost all code volume) | Not required; bugs allowed. Checkers verify output every time. |
| **Trusted (= qualification subjects)** | Property checker K, translation checker V, certificate-format semantics | Coq soundness proof + verification to machine code + agreement of two independent implementations |
| **Trusted Base (TB, not questioned)** | Coq kernel / CompCert and CertiCoq TB / hardware / **whether ALS captures intent** | The bottom that proofs cannot remove (§8) |

**Design intent**: organize evaluation subjects around checker-centered evidence. However, neither DO-330 qualification scope nor exemption of the whole compiler from assessment is determined by this technical design alone. It does not claim a small checker is already qualified. Scope and acceptance require separate evaluation.

---

<a id="6-受領書receipt"></a>

## 6. Receipt

The original concept organizes C-SAFE / C-REPRO / C-FAITHFUL / C-PROVEN through receipts. For the fixtures, models, declines and trusted boundaries of current claims, see the [receipt generator](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/receipt.sh) and [proven versus trusted](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md). It does not claim byte identity on arbitrary hosts or refinement of all ALS semantics has been achieved.

Current `make verify-trust` / `make receipt` rederive the listed checks/evidence within their declared scope. They are not an all-purpose `make verify` that verifies external measurements or unimplemented concepts. Local rechecking reduces reliance on CI alone but does not remove trust boundaries in models, execution, toolchains and so on.

---

<a id="7-完全性の定義使用目的に相対化して閉じる"></a>

## 7. Defining completeness (closed relative to intended use)

“Complete coverage of absolute semantics” expands to CompCert scale, so it is not the goal. Instead:

> **Completeness(U)** = the claims C(U) needed for intended use U are enumerated, every claim has a checker, and every TB fits within the axiom set.

- **U₁ = sandboxed agent execution**: C(U₁) = { capability upper bounds, termination behavior on errors, determinism, reproducibility }, mostly composed of C-SAFE/C-REPRO.
- **U₂ = unattended production operation**: C(U₁) + coverage of C-FAITHFUL.

The receipt states **which use has been proved**. Do not say “Almide is complete”; make it countable: “**complete for U₁, n/n for U₂**.”

---

<a id="8-消えない底ラベルに刻む"></a>

## 8. The irreducible bottom (engraved on the label)

The trusted base that remains under any verification regime and cannot be removed by proof:

- **Coq kernel** (small, scrutinized worldwide; trusting it is standard).
- **CompCert / CertiCoq's own TB**.
- **Hardware / OS**.
- **Whether ALS expresses our intent**—this alone is unprovable and can be checked **only empirically**, through the reference executor, daily MSR measurements and real use.

**Explicitly listing these four in known limitations is what licenses calling the rest “proved.”** Drawing the line—“proved up to here; trust us beyond here”—is stronger in review than saying “everything is perfect.”

---

<a id="9-何をしないか"></a>

## 9. What we do not do

- **Prove the entire compiler** (CompCert-scale full proof). Instead, reduce the task to checker soundness + per-build certificates.
- **Cover absolute semantics completely**. Completeness is relative to intended use (§7).
- **Invent a new logic**. Accumulated scrutiny is itself a requirement for logic; borrow it (Coq). The certificate format, checker and independent checker implementation may be custom; the logic may not.
- **Maintain two logics for routine use**. The original concept favors Coq for the routine proof system. Using one proof system is distinct from reducing the whole-system TB to the kernel alone.
- **Hide axioms**. Record TB in the ledger rather than concealing it (§8).
- **Let checkers bloat**. Checking must remain easier than generating. An overgrown checker becomes the next trust problem; protect against this with simplicity and diversity.

---

<a id="10-v0-から-v1-へ運ぶもの--捨てるもの"></a>

## 10. What moves from v0 to v1, and what is discarded

- **Discard**: compiler implementation code—the v0 body with proof retrofitted onto its phase structure.
- **Carry forward**: **knowledge**. The spec corpus, oracle registry and known edge cases discovered over 12 releases (of #484's kind) are reused from day one as **differential-test / fuzz oracles** for the v1 compiler. **Rebuild the compiler, not the knowledge.**

---

<a id="一文で"></a>

## In one sentence

This concept aims to expand the scope in which artifacts and evidence can be independently checked, reducing the compiler area that must be trusted. Unimplemented connections among K/V/ALS, model validity, extraction/execution environments, hardware and other trust remain. It is not an achieved claim that “one checker is the entire trusted base” or “all code automatically becomes untrusted.”
