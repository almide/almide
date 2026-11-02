Source description metadata: Certification-grade hardening — adopt the mechanisms of DO-178C / ISO 26262 / IEC 61508 (spec, traceability, coverage, tool qualification, dossier) for the machine-written-software trust layer
<a id="certification-grade-hardening--認証級への硬化"></a>

# Certification-Grade Hardening

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](certification-grade.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](certification-grade.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](certification-grade.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md)

> Source caveat: certification-grade is an engineering goal, not an obtained certificate, tool qualification, regulatory approval or safety guarantee. Standards mappings, competitor qualifications and claimed cost benefits are source claims, not independently audited findings. The initial native-oracle/spec gap and drafting plan coexist with the later ALS/spec-keying landing record; the initial 121-fixture counts are historical. N-version majority voting and interpreter promotion do not by themselves prove independence or all semantics. Critical applies to the stated entry-program scope with trusted stdlib; fan admissibility and RC cascade bounds remain open. Translation validation requires artifact correspondence and checker-property obligations, and is not an automatic replacement for DO-330 process requirements.

> **Goal**: Incorporate all engineering-accessible parts of the 5 mechanisms constituting space, aviation and automotive standards (DO-178C / ISO 26262 / ECSS / IEC 61508), raising L2/L3 of the [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) to the definition of “certification-grade.” Separate the certificates themselves (TÜV audits / DO-330 TQL) as a commercial option — what we build here is their prerequisite evidence system.
> **Hands-on audit findings**: [flight-evidence-gaps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-evidence-gaps.md) — 7 evidence-system gaps observed during hands-on changes on 2026-07-03 (F1 oracle circularity provides measured support for CG-1, and F2 the coverage illusion for the evidence ladder).
> **Active scope: CG-1** — Make ALS (Almide Language Specification) + the reference interpreter normative, and spec-key contracts.
> **Exit criteria (CG-1)**: Every active contract references an ALS section through its `spec` field, and `check-contracts.sh` enforces three-layer spec ↔ contract ↔ fixture traceability. Promote `almide-interp` from “3rd judge” to normative semantics, with native / wasm becoming “implementations required to match the specification.”

<a id="規格の分解--認証基準は-5-つのメカニズムでできている"></a>

## Decomposing the standards — certification standards consist of 5 mechanisms

DO-178C (aviation, Level A-E), ISO 26262 (automotive, ASIL A-D), ECSS-E-ST-40C / NPR 7150.2 (space), and IEC 61508 (parent standard, SIL 1-4) can, when stripped down, be decomposed into the same parts:

| # | Mechanism | Name in the standards | Almide status |
|---|---|---|---|
| ① | Normative specification independent of implementation | requirements / language specification | ❌ Circular native = oracle (deepest gap) |
| ② | Bidirectional traceability | bidirectional traceability | ✅ C-NNN ↔ `@contract:` bidirectional gate |
| ③ | Verification grading and floor | MC/DC coverage, DO-333 formal credit | ◑ evidence ladder rank 0-5 + floor rule. Missing: coverage of the compiler itself |
| ④ | Tool qualification | DO-330 TQL-1, ISO 26262 TCL3 | ◑ `Verified<T>` belt + byte-identity = per-build self-verification. Missing: qualification package |
| ⑤ | Process and responsibility | audits, LTS, Known Problems, liability | ◑ flagged ratchet corresponds to a Known Problems ledger. Missing: organizational side (out of scope) |

<a id="現有資産の規格対応表"></a>

## Mapping existing assets to the standards

Restate what we already have in the standards' vocabulary. Each CG phase fills a “missing” item in this table.

| Almide asset | Counterpart in the standards |
|---|---|
| Contract-ledger bidirectional gate ([contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) ↔ `spec/wasm_cross/*.almd`) | DO-178C bidirectional traceability (reqs ↔ tests) |
| evidence ladder doc-only→lean ([contract-classes.txt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/lib/contract-classes.txt) rank 0-5) | SPARK assurance ladder (Stone→Platinum) / DO-333 formal credit |
| `flagged-for-revision` ratchet (decreasing only) | Known Problems ledger required for a qualified compiler |
| [Perceus-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md) `Verified<T>` typestate | tool self-verification (unverified IR cannot be emitted) |
| [capability system](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md) | ISO 26262 freedom-from-interference / ARINC 653 partitioning |
| byte-identity gate + host-arch deterministic codegen | configuration management (exceeds standard requirements) |
| nightly fuzz (record `n =` in evidence) | Quantified verification evidence |
| reference interpreter (`crates/almide-interp`, 3rd judge) | **Candidate normative semantics** (promoted in CG-1) |

<a id="最深のギャップ-実装が仕様は認証では通らない"></a>

## Deepest gap: “implementation is the specification” does not pass certification

The current ledger works with native = oracle. This is sound engineering bootstrapping, but circular in certification: “requirements exist independently of code, and code is verified against requirements” is an absolute condition.

A predecessor has taken this path: **Ferrocene wrote FLS (Ferrocene Language Specification) as a prerequisite for qualifying Rust to ISO 26262 ASIL D / IEC 61508** (because Rust had no normative specification; it was later adopted as the foundation for the Rust Project's official specification).

Almide has a better starting point than Ferrocene: an executable reference interpreter already exists. Reversing the oracle in CG-1 yields **spec (interp) / native / wasm as a three-way contest = genuine N-version verification** (2-of-3 majority voting), eliminating circularity and improving verification power.

<a id="phases"></a>

## Phases

<a id="cg-1-als--規範意味論--当初の計画と着地記録"></a>

### CG-1: ALS + normative semantics — original plan and landing record

ALS now exists at [docs/specs/als/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/README.md), and contract spec-keying is also covered by the current ledger/gate. The initial 121-fixture audit and drafting steps below are historical plans and measurements; they do not mean ALS remains undrafted today.

1. **Promote `almide-interp` to normative semantics** — from “3rd judge” to “THE spec.” First, a gap audit: inventory language features / stdlib functions the interpreter cannot evaluate (record unevaluable regions as specification gaps, shrinking them with a ratchet).
   - ✅ **Ledger + bidirectional gate landed** (issue #564): `crates/almide-interp/interp-abstain-ledger.txt` + `wasm_runtime_interp_ledger.rs::interp_abstain_ledger` (no backend required = does not self-skip in CI). New abstentions force a ledger update; leaving resolved entries also fails — the ledger only shrinks. Initial audit: **49 evaluable (40%) / 72 unevaluable out of 121 fixtures**. Breakdown: ~18 design exclusions (transcendental 6 / in-place 5 / nondeterministic fan 7) + **~54 unimplemented glue (map.new, set.from_list, json.parse, string.take/slice, list.get_or, bytes.* …)** — the latter are potential conversions from “skip” into a third vote.
2. **Draft ALS** — a prose specification referencing interp as executable semantics. Structure: syntax / static semantics (types, capabilities) / dynamic semantics (interp reference) / definition of observational equivalence (stdout, stderr, exit code). **Statements of 53 contracts seed the sections** — contracts already use normative prose.
3. **Spec-key contracts** — add a `spec = "ALS §x.y"` field to `[[contract]]` (REQUIRED for active contracts), extending `check-contracts.sh` into a three-layer symmetry gate.

<a id="cg-2-コンパイラ自体の-structural-coverage-m"></a>

### CG-2: Structural coverage of the compiler itself (M)

- Measure compiler branch coverage under spec / fixture execution (cargo llvm-cov), with a CI ratchet (never decreases).
- Inventory current safety checks to define coverage scope. The original `PerceusVerifyPass` / `StackBalancePass` are former-emitter names, so build the current inventory from `proofs/receipt.sh` / codegen passes.
- Standards counterpart: DO-178C Level A structural coverage. Discovered unreachable code is either a dead branch (remove it) or a missing fixture (add one).

<a id="cg-3-almidecritical-profile-m"></a>

### CG-3: Almide/Critical profile (M)

An analog of MISRA C / Ada Ravenscar / SPARK subset. `almide check --profile critical`:

- **Totality required** — generalize the theme of C-001/C-002/C-047 (div/mod/pow abort rather than trap) to the entire language.
- Bounded recursion and allocation limits (statically checkable forms), and a bound on RC drop cascade length → feasibility of WCET analysis.
- Capabilities start deny-all (explicit grants also for `Rand` / `Time`).
- Admissibility follows current ALS §B and critical-profile checks. Do not use removal of `fan.timeout` in 0.29.0 as a current prohibition rationale (it was later reintroduced as an oracle-body form).
- **Critical is a subset, not a dialect** — all Critical code remains valid unchanged in normal mode (the same relationship as SPARK ⊂ Ada).

**Status (#567 landed)**: `almide check --profile critical` is operational — applies the bounded profile (ALS §B, E070–E078) to all functions; capabilities start deny-all, with `--allow IO|Net|Env|Time|Rand|Process` providing grants. Scope is the entry program (stdlib is trusted runtime beneath the profile). `tests/critical_profile_test.rs` enforces the subset property in CI by cross-checking normal-mode PASS for every negative example. Static memory discipline (#568) also landed: the reference application is critical-clean + kernel emission is allocation-free + it completes within a fixed `--heap-cap` budget (exceeding the budget causes deterministic OOM abort); partitioned-host requirements are in docs/project/PARTITIONED-RUNTIME.md. Remaining: `fan.*` admissibility (awaiting #1628 stage 2 cost boundaries), RC drop cascade bound.

<a id="cg-4-translation-validation-l"></a>

### CG-4: Translation validation (L)

Extend the belt concept to per-build proof: attach evidence to each artifact that “this .wasm is a correct translation of this verified IR.”

- Consider per-artifact verification as evidence for qualification assessment. Do not claim it is a superset of DO-330 or an automatic substitute for process requirements.
- Original `Verified<T>` belongs to the former emitter's design. Inventory current targets / trusted seams, and proceed from machine-checking emit invariants to required semantic-preservation obligations → (long term) proofs of emit rules in Lean.
- For CompCert-grade full proof, see the historical discussion in the [completeness roadmap](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md). For translation validation to give equivalent guarantees, the checker's target properties and correspondence to the actual artifact must be established separately.

<a id="cg-5-qualification-bundle-s-m"></a>

### CG-5: Qualification bundle (S-M)

Turn output of `make verify` ([trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) receipts harness) into a signed, versioned dossier:

- Contract ledger + spec traceability + coverage + Lean 0-sorry + fuzz `n =` + byte-identity + Known Problems (flagged list).
- Automatically generate and attach for each release. **Input to future commercial qualification (Ferrocene playbook)**; having the dossier first dramatically reduces qualification cost.

<a id="何をしないか"></a>

## What we will not do

- **Obtain the certificates themselves** (TÜV / DO-330 audits) — audit handling, LTS guarantees and liability are organizational matters, not something engineering can buy. Ferrocene and AdaCore exist as companies selling these. Separate as a commercial option after establishing the category.
- **Enter the flight-software market** — buyers are agent platforms. Regulated industries are an option once the dossier is ready.
- **CompCert-grade full compiler proof** — rejected with data. Substitute CG-4.

<a id="先行者の地図--何を借りるか"></a>

## Map of predecessors — what to borrow

| Predecessor | Track record | What to borrow |
|---|---|---|
| SCADE KCG | DO-178C Level A qualified code generator | Refer to qualification-material structure. Do not conclude from this repo that output review becomes unnecessary |
| Ferrocene | Qualifies rustc to ISO 26262 ASIL D / IEC 61508 | **Write the spec first** playbook (FLS); public style of qualification materials |
| SPARK | Got proof accepted as a substitute for testing under DO-333 | Assurance-ladder design (evidence ladder is already isomorphic) |
| CompCert / seL4 | Verified compiler / proofs down to binary | Translation-validation idea (CG-4) |
| MISRA / Ravenscar | Certifiable subsets | Principle that a profile is a subset, not a dialect (CG-3) |

<a id="リスク"></a>

## Risks

| Risk | Response |
|---|---|
| ALS becomes reverse transcription of implementation, retaining circularity in another form | Promote interp **first**, write ALS against interp. Treat native disagreement as an implementation bug, not an ALS bug |
| Coverage-effort quagmire | Start with MC/DC limited to safety passes; only a branch ratchet for the whole compiler |
| Critical profile splits the language in two | Enforce subset principle (CG-3) by gate — valid in Critical means valid in normal mode |
| Certification playacting (a dossier nobody uses) | CG-5 formats the same thing as trust-layer `make verify`; do not create an independent artifact |

<a id="組織実績のはしごbc-セクション"></a>

## Organizational / track-record ladder (sections B/C)

The **non-engineering** parts of the flight-grade program — a legal entity able to sign, DER negotiations, LTS policy, first production deployment, service-history records, and a position statement on certification of machine-written code — are collected in [flight-organization.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md). They were previously queued as issues #577–#585, but each describes itself as “not an engineering item”; mixing them into the queue of actionable work makes that queue misleading. The umbrella issue #586 checklist continues to track progress.
