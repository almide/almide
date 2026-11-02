Original description metadata: Proposed qualification-evidence mapping and kit design; no accepted regulatory credit or qualified tool established by this document
<a id="flight-qualification--do-178cdo-330do-333-マッピング--資格化キットg-f5--g-f6"></a>

# Flight Qualification — DO-178C/DO-330/DO-333 Mapping + Qualification Kit (G-F5 + G-F6)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](flight-qualification.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-qualification.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flight-qualification.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md)

> Source caveat: all regulatory objective mappings and kit/customer workflows are historical proposals requiring expert acceptance, not legal or certification guidance or evidence of an obtained qualification, DAL/TQL credit, flyable product or cost reduction. The source itself separates existing technical assets from unproved renderer/extraction seams and authority acceptance; retain this boundary when reading its stronger original tables and final sales/use narrative. Fixed counts and versions are historical snapshots, not current inventory. Stdout-only capability modeling does not prove general bounded resources or all host effects; modeled RC/wasm results do not establish application correctness, full translation fidelity or WCET. The customer workflow names a Ferrocene route while the source explicitly reports a stock-rustc lane pending contract; it is not evidence that that qualified route was used. Original command names are preserved without asserting equivalence or executing them. No standards interpretation, external product qualification, proof or measurement was independently validated.

> **Goal**: Concrete design for [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) ladder **G-F5** (mapping PCC certificates/receipts to certification objectives) and **G-F6** (qualification-kit product definition, AbsInt/SCADE-KCG model). Ground it in what the spine **actually produces now**.
> **Related**: [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2 / [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) (5 standard mechanisms, ALS, evidence ladder) / [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) (claim classification) / [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md) / [flight-evidence-gaps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-evidence-gaps.md) (field-audit findings — current limits of qualification claims).

---

<a id="0-元の設計時点の技術資産スナップショット"></a>

## 0. Technical-asset snapshot at the original design date

The fixed counts and versions below are original planning values, not a current inventory. For current proofs, measurements and trusted boundaries, see `proofs/receipt.sh` and generated `docs/TRUST-SPINE.md`; for ALS, see the [normative specification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/README.md). Existence of technical assets and authority-accepted qualification are separate; this document does not establish the latter.

- Coq **16 files / 45 theorems**, all `Print Assumptions` = "Closed under the global context", `coqchk` De-Bruijn rechecking, cross-version 9.1.1 + 9.2 (`proofs/check.sh`, `TRUSTED_BASE.md`).
- Qualification target = `proofs/checker.ml` (currently 1,144 lines of extracted OCaml — `scripts/gen-claims.sh` regenerates the actual count into docs/TRUST-SPINE.md) ← `OwnershipChecker.v` (309) + `Subset.v` (88) + `Extract.v`. Core = signed Δ left fold + `subset_check` inclusion.
- `make verify-trust` = `check.sh` + `gate.sh` + `corpus-wall.sh` + `cargo test -p almide-mir`. `make receipt` = `receipt.sh`. CI executes both (`.github/workflows/trust-spine.yml`).
- Property coverage **4/8** (mem/name/cap/type ✅; leak/reuse/call-mode/byte remain). In-profile 4083/4195.
- Receipt claims = C-PROVEN / C-SAFE / C-FAITHFUL / C-WALL / C-REPRO (`receipt.sh`, [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md)).

---

<a id="1-g-f51--do-178c-table-a-目標マッピング"></a>

## 1. G-F5.1 — DO-178C Table A objective mapping

The following is an **unaccepted mapping proposal** considered for the G-F1 subset, not a table establishing DAL-A credit or the correctness of standards interpretation. Certificates/receipts are verification-process evidence for the **source-coding & integration + verification-of-verification rows**, not evidence of requirements/functional correctness.

<a id="専門レビューに付す対応候補"></a>

### Candidate mappings for expert review

| Table | Objective (paraphrased) | Almide evidence | Strength |
|---|---|---|---|
| **A-5 obj 6** | Source code accurate and consistent (stack/memory management, no unbounded use) | `check_all_sound` (RC balance → no double-free/leak), `balanced_cert_frees_in_memory`, `StackBalance`, `FreeList.alloc_not_live`, `CowSafety`. Receipt **C-SAFE/C-PROVEN** | Strong, formal, every build |
| **A-5 obj 1-2** | Source conforms to LLR (partial: memory/ownership LLR fixed by ALS) | `TypeConcretization`, `NameTotality`. ALS (`certification-grade.md` CG-1) supplies LLR | Partial (language-safety LLR only, not application-functional LLR) |
| **A-2 obj 5** | Source-code development (certificate = MIR-bound development artifact) | i/a/d/m/r/b witness + `Translation.v` op→wasm table | Supporting |
| **A-6 (integration) partial** | EOC robust / no runtime error on target | `WasmExec` (actual bytes execute `rt_inc/dec`, double-free trap) + sentinel. **Wasm target only** | Partial, target-annotated (Ferrocene pivot moves it to a flyable target) |
| **A-7 obj 3-4** | **Verification of verification** — procedures/results correct and verification itself sound | **Strongest claim**: soundness theorem = verification of verification method. Independent `coqchk` recheck + axiom audit + claim-drift gate | Strong; Almide's home ground |
| **A-7 obj 1-2** | HLR/LLR test coverage | Proposal to map formal evidence (credit/substitution undecided; functional LLR excluded) | Only via DO-333 |

<a id="触れない目標正直なギャップder-に明言必須"></a>

### Objectives not addressed (honest gaps, must state explicitly to DER)

| Objective | Reason | Whose work |
|---|---|---|
| A-3/A-4 HLR&LLR correctness, completeness, traceability | Spine proves safety properties; “computes the correct control law” is separate. Pillar ③ = zero | Customer domain process |
| A-7 obj 5-8 MC/DC, structural coverage | Coverage is a property *of the checker*, not application decision logic | Customer (or DO-333 credit for the subset) |
| A-4 obj 13 WCET | `Termination.v` is loop-free; loops are no-ops. WCET = keystone (あ), pending | Open keystone |
| A-6 complete EOC/target testing | Wasm is proven, but “airplanes do not fly on wasm” | Ferrocene pivot (G-F3) |
| A-1 PSAC/plan conformance | No certification plan yet | G-F6 dossier template |

**Boundary of external explanation**: “Almide supplies checker theorems over explicit models and fixture revalidation results. Which certification objectives may accept these as evidence requires expert evaluation including intended use, configuration and unproved portions; no acceptance is claimed here.”

---

<a id="2-g-f52--do-330-ツール資格化論証checker-を証明する"></a>

## 2. G-F5.2 — DO-330 tool-qualification argument (“prove the checker”)

**Proposal**: organize evidence for a configuration that independently checks compiler output, and submit what should be the qualification target for evaluation. Checker theorems alone cannot downgrade the compiler to TQL-5 or make the checker “qualified by formal proof.” This repo does not show such authority acceptance. Treat specific TQL/Criteria/credit decisions as unresolved.

**Qualification-evidence package (all exist, `proofs/`)**:
1. The checker is small and auditable. Core = signed Δ left fold + `subset_check`. Size ∝ #event characters + #subset + #table entries, **independent of program/language/compiler scale** (tripwire in `certificate-format-v1.md`: forbid checker from opening callees / walking CFG / inference).
2. Checker soundness is machine-proved (`check=accept ⟹ P`): 45 theorems, `Qed`, 0 `Admitted`.
3. **Independent rechecking** with `coqchk` (De Bruijn criterion, second small kernel).
4. **Axiom audit** `Print Assumptions` = closed; claim-drift gate prevents the public ledger from drifting beyond `coqchk` verification.
5. **Rederive every build** with `make verify-trust` on the customer's machine. **CI is not in the trusted base** (not “trust our CI,” but “customers can rederive”).
6. **Cross-version rederivation** (9.1.1 + 9.2) = robustness against toolchain versions.

**Points DER will still challenge (state proactively)**:
- **The witness ⟹ emitted-bytes seam is trusted, unproved**. The checker validates MIR-level witnesses; emitted wasm/Rust bytes realizing them is the §3 renderer contract (`Translation.v` is a presence-table match). Neither the checker's qualification status nor the renderer's TQL follows from this. The Rust/Ferrocene approach does not automatically prove Almide→Rust fidelity. `.github/workflows/ferrocene-lane.yml` currently explicitly says it uses stock rustc pending a contract.
- **Extraction is trusted**: proofs are over Coq terms; the running checker is OCaml via `ocamlopt` (Thompson hole). Close with CertiCoq+CompCert (pending).
- **Checker implementation configuration**: include independence, common-cause failures and version management in evaluation materials. Do not decide sufficiency from a single general statement about standards.
- **No evidence of acceptance**: this repository does not establish authority acceptance of Almide. Do not infer the worldwide absence of precedent or the years required for review from this material alone.
- **ALS validity is empirical, never proven**: whether formal semantics captures intent is the irreducible floor.

---

<a id="3-g-f53--do-333-形式手法-credit-マッピング"></a>

## 3. G-F5.3 — DO-333 formal-methods credit mapping

The following is the original working table for considering soundness, correspondence to implementation, and correspondence to requirements. Acceptance of DO-333 credit, substitutability for verification activities, and satisfaction of standard clauses are unresolved; a theorem in each row does not make these automatic.
| Proposed DO-178C objective to consider mapping | Proven property (file) | J1 soundness | J2 tool correctness | J3 property captures requirements (weakness) |
|---|---|---|---|---|
| A-5 no double-free/leak | `check_all_sound`/`balanced_cert_frees_in_memory` | `accept ⟹ no_double_free ∧ no_leak`, axiom-clean | `coqchk`+`Print Assumptions`+drift gate | RC balance = memory-safety LLR. ✓ Over the subset |
| A-5 no dangling reference | `check_names_cert_sound` | Proven subset law | Same as above | name-totality = all used names defined |
| A-5 bounded resource/no undeclared effect | `check_caps_cert_sound` (`reachable⊆declared`, transitive) | Proven | Same as above | **Honest scope: Stdout only**. Other host effects unmodeled — disclosure mandatory |
| A-5 type soundness | `TypeConcretization` | Proven | Same as above | LLR = Unknown types do not reach codegen |
| A-6 EOC robustness (wasm) | `WasmExec`/`rc_dec_bytes_trap_on_zero` | Proved over actual bytes (wat2wasm-grounded) | wat2wasm + wasmtime differential | RC-safe ops only; functional list-ops unproved in bootstrap runtime |

Formal evidence is a submission candidate within the stated model scope of **memory-safety/name/type/capability**. Acceptance as certification credit is a separate decision. **Not covered**: functional correctness (pillar ③, no formal control-law specification), WCET (`Termination.v` treats loops as no-ops; needs keystone (あ)), outside the value-semantics subset (C-WALL is the boundary: explicitly `Unsupported` outside in-profile, no silent false credit). DO-333's J3 discipline is the receipt's per-claim “scope(honest)” column itself — a receipt is a table of technical claims, grounds and scope, not automatic DO-333 conformity.

---

<a id="4-g-f6--資格化キット製品内容exist-vs-build"></a>

## 4. G-F6 — Qualification kit (product contents: EXIST vs BUILD)

AbsInt/SCADE-KCG model: **sell a kit, not a box.** Kit = packaged, versioned and signed inputs that eliminate the customer's tool-qualification cost.

| # | Kit artifact | Status | Location / gap |
|---|---|---|---|
| 1 | Checker technical assets (does not mean qualified) | Technical assets exist | `checker.ml` + `OwnershipChecker.v`/`Subset.v`/`Extract.v` + `build-checker.sh` |
| 2 | Proof spine (16 `.v`/45 theorems) + axiom audit + reproducible coqchk | ✅ EXISTS | `proofs/*.v`, `check.sh` |
| 3 | Verification harness (rederive all with 1 command) | ✅ EXISTS | `make verify-trust`, CI mirror |
| 4 | Receipt (schema + sample) = typed claim bundle | ✅ EXISTS | `make receipt`→`receipt.sh`, [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) |
| 5 | Trusted-base ledger (toolchain pin + irreducible floor + known limits) | ✅ EXISTS | `TRUSTED_BASE.md` |
| 6 | Certificate-format specification (i/a/d/m/r/b + side-table + size invariant) | ✅ EXISTS | `certificate-format-v1.md` |
| 7 | Corpus-wall evidence (totality + accept⟹safe over real corpus) | ✅ EXISTS | `corpus-wall.sh`, C-WALL |
| 8 | ALS normative specification | Specification exists; qualification acceptance separate | [ALS](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/README.md); implementation correspondence and scope in each chapter |
| 9 | Traceability matrix (ALS § ↔ C-NNN ↔ fixture) | ◑ PARTIAL | `contracts.toml` bidirectional gate exists; `spec="ALS §"` keying is CG-1 |
| 10 | Flight-profile specification (counted-loop/no-alloc/bounded, enforced by checker) | 📐 DESIGNED | [flight-subset-spec](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md). Keystone (あ) pending |
| 11 | WCET-bound evidence (lift loops into Coq, alloc-count ceiling) | ⬜ BUILD | Keystone (あ) G-F2 |
| 12 | Native translation/toolchain evidence | Renderer exists; full translation fidelity and qualification not established | `render_native.rs`, stock-rustc Ferrocene lane; separate from G-F3 issues |
| 13 | Dossier template (PSAC/SVP/SAS skeleton → G-F5 objective table) | ⬜ BUILD | §1 mapping is the seed; `certification-grade.md` CG-5 |

**Summary**: the technical core (1-7) **EXISTS now**, enforced by CI. Certification packaging (8-13) is build-out — in particular, **13, the dossier template, is a direct product of the G-F5 mapping in §1**. G-F6 can flip from 🔴 to close on “definition only” (full build unnecessary).

---

<a id="5-キット境界--顧客統合"></a>

## 5. Kit boundary + customer integration

<a id="境界absintscade-kcg-アナロジー"></a>

### Boundary (AbsInt/SCADE-KCG analogy)

This product boundary is the original design's analogy; this document has not confirmed SCADE/AbsInt qualification conditions or source-review exemptions. Do not infer Almide is entitled to the same treatment.

| Proposed technical assets for evaluation | Scope requiring separate judgment/verification |
|---|---|
| Checker soundness proofs and explanation of target witnesses/models | Tool qualification, applicable TQL and acceptance for intended use unresolved |
| Scoped mem/name/caps/type receipts and declared subset | Source→artifact correspondence, unproved runtime/host, application-functional requirements |
| Verification harness, traceability, proposed dossier templates | Sufficiency of actual target configuration/evidence and customer/expert evaluation |
| WCET models/measurement proposals (no completion claim) | Actual hardware/environment assumptions, system-level timing/safety case |

The kit is a proposal to organize evidence and aid evaluation, not to guarantee generated-code safety for free, qualify the checker through proof alone, or automatically change the compiler to TQL-5. This document establishes neither authority acceptance nor whether worldwide precedents exist.

<a id="顧客の使い方"></a>

### Customer usage

1. **Write flight-profile Almide** — control-law kernels/state machines/watchdogs in the flight subset.
2. **`make verify` rederives certificates every build** — on the customer's machine (CI outside trusted base). An explicit `Unsupported` wall outside in-profile, not silent.
3. **Render to a flight target** — MIR → readable Rust (`rust_pattern` faithful) → **Ferrocene** (ASIL-D/IEC 61508 qualified) for Rust→machine code, bypassing wasm-byte-binding Gap 1.
4. **Fold receipts + kit dossier into the customer's certification package** — PSAC cites the DO-330 argument (§2) and DO-333 justification (§3); receipts become A-5/A-7 verification-result evidence, and the ledger the customer's tool-qualification trusted-base disclosure. Customers perform A-3/A-4, MC/DC and SSA in their own process.

**Net**: Almide is a **qualification-kit vendor** (the Ferrocene/AdaCore/AbsInt seat), not an avionics integrator. Value = **the dossier exists before the customer's qualification work begins, lowering qualification cost** — applying the Ferrocene FLS approach to per-build PCC certificates rather than one-off compiler qualification.
