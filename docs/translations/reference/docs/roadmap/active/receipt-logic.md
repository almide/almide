Source description metadata: Formal foundation for the trust layer — receipt logic: claim types, threat model, trust bases, falsification procedures, completeness relative to use-case.

<a id="receipt-logic--受領書の論理"></a>

# Receipt Logic — the logic of receipts

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](receipt-logic.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](receipt-logic.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](receipt-logic.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md)

> Source caveat: this is a dated draft containing broad proposed claim schemas and later scope corrections, not evidence that arbitrary executions or programs have been verified. Artifact hashes identify bytes but do not establish witness fidelity, complete memory safety, capability confinement or sandbox configuration. The universal C-FAITHFUL table formula and native trust-bridge wording must be read with the explicit scope limits, fixtures, declines and trusted boundaries; sampled equality is not a proof for all programs. The 107 fixtures, 5/n and quarter estimate remain dated claims. C-SAFE certificate wording denotes the document’s verification mechanism, not third-party safety qualification. The conceptual local rederivation command is retained, not executed or independently validated.

> **Goal**: formally refine [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md)'s L0-L4 and receipts. Define “trust machine-written software without humans reading it” as a bundle of falsifiable claims, and fix the completion conditions for “make everything except rustc complete.”
> **Position**: the meta-layer of trust-layer.md (the logic of claims). certification-grade.md (CG-4 translation validation) and completeness-by-construction.md (the semantics ledger) sit under §6 of this document.
> **Status**: Draft — drafted 2026-06-11. Numbers are from the verification of origin/develop on that date.
> Revised 2026-09-30 (#2760): the prover is the Rocq (Coq) kernel (not Lean — receipts `proofs/receipt.sh` and `proofs/check.sh` use coqc + coqchk). C-SAFE / C-PROVEN were bound to witnesses from the structural leg (shipped wasm) and the artifact SHA-256.

<a id="1-定義--rustc-以外完全の形式化"></a>

## §1 Definitions — formalizing “complete except for rustc”

- **Receipt R(a)** — a bundle of typed claims attached to artifact a. Each claim is a 4-tuple **(claim statement, verifier, trust base, falsification procedure)**.
- **Trust base TB(claim)** — the set of things a consumer trusts without verification when accepting that claim.
- **Axiom set Axioms** = { rustc/LLVM, Rocq (Coq) kernel + OCaml extraction, wasmtime + wasm specification, OS/hardware, validity of ALS }.
- **“Make everything except rustc complete”** ≝ TB(claim) ⊆ Axioms for every claim in R(a), with the claim ledger closed relative to the use (§6).

Explicit design constraint: **CI/GitHub are not axioms.** All claims must be rederivable on the consumer's machine by `git clone && make verify`. CI is merely a courtesy advance run, not the basis of trust.

The final axiom (validity of ALS — “does the specification reflect our intent?”) is unprovable and can only be checked empirically through interp, dojo and use. **Explicitly naming the irreducible foundation is what qualifies the rest to be called “complete.”**

<a id="2-主張の型システム--論理形式が違うものを混ぜない"></a>

## §2 A type system for claims — do not mix different logical forms

The L0-L4 claims fall into 5 types with different logical forms. Different types have different TBs, verifiers, falsification procedures and consumers. Mixed claims collapse under scrutiny.

| Type | Logical form | Verifier | TB | Corresponding level |
|---|---|---|---|---|
| **C-SAFE** | ∀ executions: a does not do X (artifact a = the bytes named by its SHA-256) | Extracted checker + Rocq kernel + almide-verify judge structural-leg witnesses (ownership, name totality, capability upper bounds, call modes); the bundle pins a's SHA-256 and the verifier recomputes it | Rocq kernel + recording discipline (witness ⟺ instruction sequence) + `to_wasi` + wasm specification | L0/L1 |
| **C-REPRO** | compile(s) = a (byte equality) | Recompile + diff | Entire toolchain | L2 |
| **C-FAITHFUL** | behave(a) ≡ sem_ALS(s) | Three-way interp + a series of translation-validation properties | ALS + interp + axioms | L3 |
| **C-PROVEN** | ∀ programs: pass P has property Q (universal, narrow scope) | Rocq kernel (coqc + coqchk, `Print Assumptions` audit, `proofs/check.sh`) | Rocq kernel | L3 |
| **C-MEASURED** | Statistical claim about the generation process | Reproduced dojo run | Published procedure + complete set of pins | L4 |

C-SAFE is **the strongest type verifiable from the artifact alone, without source**. C-FAITHFUL is the heaviest type, including ALS in its trust base. Assemble receipts starting with lighter types (§7).

<a id="3-脅威モデル--各主張が誰を止めるか"></a>

## §3 Threat model — whom each claim stops

| Threat | Content | Blocking claim | Bearer of soundness |
|---|---|---|---|
| **T1** Well-intentioned but buggy AI author | Mainstream case | Type/effect system + tests + MSR loop | Checker |
| **T2** Adversarial author (prompt-injected agent) | Deliberately crafts a capability escape | C-SAFE | The 3 layers below |
| **T3** Buggy compiler | Acceptance ⟹ wrong code | C-FAITHFUL + property-verification series | Verifier (no need to trust the compiler itself) |
| **T4** Malicious compiler (Thompson attack proper) | Self-propagating contamination | C-REPRO → DDC → selfhost | Diversity |
| **T5** Compromised verification infrastructure | CI / checker itself breached | Local rechecking + checker smallness/diversity | Consumer |

<a id="t2-の精密化--l1-の健全性-3-層"></a>

### Refining T2 — 3 layers of L1 soundness

A separation that permits an immediate answer to “what if the author fools the compile-time checks?”:

1. **Imports are part of the connection to the host.** Unimported functions cannot be called directly, but import names alone cannot restrict internal operations of shared gateways, arguments or permissions granted by the host. Check artifact imports against host implementation/permissions.
2. **Language capability checking is a separate check.** Check containment of reachability within declarations under its model and analysis scope. This does not prove arbitrary execution semantics of host/FFI.
3. **Runtime configuration and native-code trust.** Separate Wasm host configuration from the trust boundaries of native build / dependency / FFI. A manifest/hash alone does not complete a sandbox.

An artifact hash binds “which artifact is referred to.” Faithful representation of that artifact's execution by a witness is a separate obligation; do not claim beyond the receipt's C-FAITHFUL scope.

<a id="t3-の精密化--検査の非対称性"></a>

### Refining T3 — the asymmetry of checking

There is no need to prove a 100,000-line compiler. **Instead of verifying the generator, verify its output every time** (the classic asymmetry of proof-carrying code). Trust shrinks from “compiler + rustc” to “small checker + the rustc that built it.”

The original Perceus/StackBalance table was the old emitter's checking ledger. Do not use it as a list of guarantees for every current compilation route. Current evidence entry points are:

| Subject | Evidence entry point | Limits |
|---|---|---|
| Wasm route selection | `src/wasm_route.rs` | Structural-only. Do not use the old `--emit-unverified` waiver as a current workaround |
| IR pass invariants | `crates/almide-codegen/src/pass.rs` | Fatal postcondition checks on violations, not a proof of full translation fidelity |
| Proof/acceptance/byte correspondence | `proofs/receipt.sh` | Scope and trusted boundaries for each of C-PROVEN/C-SAFE/C-FAITHFUL, with fixture counts and declines |

This table guides readers to evidence; it does not claim that kernel proofs automatically run in every ordinary build. See the receipt for rederivation procedures and the scope of each piece of evidence.

The path to “complete” is extending this ledger (§6 U₂), not proving the whole compiler (CompCert-scale, already rejected with data).

<a id="4-二重-rustc-非対称性--native-の信頼は-wasm-経由で導出される"></a>

## §4 The double-rustc asymmetry — native trust is derived through wasm

- **Native artifact**: rustc enters **2 times** (building the compiler + building the emitted Rust), plus runtime/rs.
- **Wasm artifact**: rustc enters once (only building the compiler). Almide owns the whole route from IR to bytes.

Consequence 1: **Wasm has the smallest artifact TCB.** Attach receipts to wasm artifacts first. The sandbox/agent strategy and trust strategy converge on the same answer here.

Consequence 2: **The xtarget byte-equality gate (spec/wasm_cross, 107 fixtures / 3-way) is a bridge of trust.** The native artifact with the larger TCB imports trust through “observational equivalence to wasm.” Byte equality is both a quality gate and the very route by which native trust is derived.

<a id="5-反証手続き--主張の負空間を公開する"></a>

## §5 Falsification procedures — publish the negative space of claims

Only a claim with a published falsification method becomes an asset (an unfalsifiable claim is a liability in the trust layer). Attach “false if this is observed” to each claim type:

| Claim | Falsifying observation |
|---|---|
| C-SAFE | A PoC of double-free, undefined reference, or execution of an undeclared capability in a certificate-checked artifact (matching SHA-256); 1 case defeats the claim |
| C-REPRO | Byte mismatch across hosts; 1 case defeats the claim |
| C-FAITHFUL | Interp/native/wasm three-way disagreement on an accepted program |
| C-PROVEN | Introduction of `Admitted` / axioms (`Print Assumptions` not Closed), coqc / coqchk failure, or a counterexample program |
| C-MEASURED | Numbers disagree in a reproduced run using the published procedure |

A discovered falsifier becomes the ledger's next row (the same dynamics as a bug bounty).

<a id="6-完全性の定義--使用目的に相対化して閉じる"></a>

## §6 Defining completeness — close it relative to the use

“The property list covers observational semantics” is not precise (covering absolute semantics grows to CompCert scale). More precisely:

> **Completeness(U)** ≝ the state in which the set of claims C(U) needed for use U is enumerated, every claim has a verifier, and every TB is contained in the axiom set.

- **U₁ = agent execution inside a sandbox**:
  C(U₁) = { capability upper bound, termination behavior on error, determinism, reproducibility } — mostly C-SAFE/C-REPRO, potentially closable within a quarter using current assets.
- **U₂ = unsupervised production operations**:
  C(U₂) = C(U₁) + C-FAITHFUL coverage — the long march of §3's property ledger (currently 5/n) + full discharge of [completeness-by-construction](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/completeness-by-construction.md)'s semantics ledger + CG-1 oracle inversion.

**Do not say “Almide is complete.” Say “complete for U₁, 5/n for U₂.”** This relativity makes claims falsifiable and incrementally attainable.

Expected objection: “is relativizing completeness an evasion?” Position: a non-relative completeness claim is unfalsifiable, and unfalsifiable claims are precisely the evasion in the trust layer.

<a id="7-着手順受領書の積み方--軽い型から"></a>

## §7 Order of work (assembling receipts — lighter types first)

1. **Receipt v1** (C-SAFE + C-REPRO + C-PROVEN within current scope): wasm import upper-bound manifest + byte-reproducibility hash + Coq proof references (explicitly state the RC subsystem scope). Capability Phase 1-2 is a prerequisite. 2026-09-30 (#2760): C-SAFE is bound to structural-build witnesses and the SHA-256 of bytes shipped by `almide build --target wasm` (`proofs/structural-artifacts.sha256`, the bundle v2 `artifact` record); the receipt names the digest of that ledger file itself.
2. **Receipt v2** (+ C-FAITHFUL): CG-1 oracle inversion + publication and extension of the property ledger.
3. **Receipt v3** (addresses T4): reproducible builds → DDC-style cross-checking using 2 backends → selfhost (formerly `research/selfhost/`, now `research/spike/v1-mir/`). Attacking rustc itself becomes meaningful only after v1/v2 completion.

<a id="trust-layermd-への反映追補-4-点"></a>

## Incorporating into trust-layer.md (4 additions)

Incorporate these when revising trust-layer.md (retain this document as its formal foundation):

1. Threat-model section (T1-T5)
2. A **TB column** in the level table (what is trusted when accepting each level's claims)
3. Falsification-procedure section (§5)
4. Axiom section (explicitly exclude CI + §4's “bridge of trust”)

<a id="何をしないか"></a>

## What we will not do

- **Complete coverage of absolute semantics** — grows to CompCert scale. Completeness is relative to use (§6).
- **Hide axioms** — rustc/LLVM, the Rocq kernel, wasmtime, and ALS validity do not disappear. Explicitly naming the irreducible foundation qualifies the rest to be called “complete.”
- **Bloat verifiers** — checking must remain simpler than generation. An overgrown verifier itself becomes the next trust problem (T5). Defend through smallness and diversity.
