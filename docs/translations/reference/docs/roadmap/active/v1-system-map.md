<!-- description: Intended v1 assurance architecture and historical maturity diagrams, not a completed trust-chain certificate -->
<a id="v1-system-map--全体像mermaid"></a>

# v1 System Map — The Overall Picture (Mermaid)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-system-map.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-system-map.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-system-map.zh-CN.md)

> Non-normative source-pinned historical design map, not a completed security, certification or performance guarantee. The opening scope disclaimer governs every absolute assurance claim and diagram below. Original Mermaid code remains byte-identical; translated diagram labels are supplied outside the code. No new proof, benchmark, deployment or certification verification was run. No automatic synchronization; the pinned original remains authoritative. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-system-map.md)

<a id="design-map-not-a-completed-trust-chain-certificate"></a>

## Design map, not a completed trust-chain certificate

These diagrams combine the intended assurance architecture with the early
v1 maturity snapshot. The labels “current”, zero real programs and completed
trust arrows are not current measurements. In particular, proving a witness
checker does not by itself remove trust in event recording, lowering, runtime
bodies, host behavior or source semantics.

Current scoped claims are in [proven versus trusted](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)
and the [receipt generator](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/receipt.sh). The full K/V/ALS
refinement chain and qualification ladder below are objectives; the diagrams
must not be read as evidence that every shipped artifact has attained them.
The early scoreboard and intended topology are preserved rather than filled
with unmeasured new values.

> **What this is**: a map of Almide v1's trust architecture, **one diagram at a time**.
> For each part: **what it is / why use it / which area of Almide it assures**.
> Detailed rationale is in sibling documents:
> [v1-proof-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md) (destination),
> [receipt-logic](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/receipt-logic.md) (receipt format),
> [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) (L0–L4 category strategy),
> [completeness-by-construction](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/completeness-by-construction.md) (semantic ledger).

---

<a id="0-背骨全図に共通する一文"></a>

## 0. Backbone (One Sentence Shared by Every Diagram)

> **Do not prove the compiler correct. Prove only a small checker's soundness,
> have the compiler emit a “certificate” on every build, and have the checker verify it each time.**
> Checking is orders of magnitude cheaper than constructing: trust shrinks from “100,000 lines” to “a few hundred lines.”

---

<a id="1-信頼の流れpcc-鎖"></a>

## 1. Flow of Trust (PCC Chain)

An untrusted compiler emits the artifact and certificate; **only the small checker that must be trusted** revalidates them. Read the trust boundary: red = no trust needed; green = trusted.

```mermaid
flowchart LR
    SRC["ソース .almd"] --> COMP

    subgraph UNTRUSTED["信頼不要 — バグ可・規模不問（約 10 万行）"]
      COMP["コンパイラ<br/>parse → check → lower → MIR → emit"]
    end

    COMP --> ART["wasm バイト列 a"]
    COMP --> CERT["証明書束 c<br/>(極小 witness 言語)"]
    ALS["ALS 規範意味論<br/>(Coq・唯一の正典)"]

    subgraph TRUSTED["信頼 — 資格化対象・数百行"]
      K["性質検査器 K"]
      V["翻訳検査器 V"]
    end

    CERT --> K
    ART --> K
    ART --> V
    ALS --> V

    K --> OK["accept ⟹ a は性質 P を満たす"]
    V --> OK2["accept ⟹ a は ALS(s) を refine"]

    classDef untrusted fill:#ffebee,stroke:#c62828,color:#000
    classDef trusted fill:#e8f5e9,stroke:#2e7d32,color:#000
    classDef norm fill:#e3f2fd,stroke:#1565c0,color:#000
    class COMP untrusted
    class K,V trusted
    class ALS,OK,OK2 norm
```

Diagram labels: source .almd → compiler (parse → check → lower → MIR → emit), in the untrusted approximately-100,000-line area → wasm bytes a and certificate bundle c (tiny witness language). ALS is the normative Coq semantics, the sole canon. The trusted qualification scope contains property checker K and translation checker V. K acceptance implies property P for a; V acceptance implies that a refines ALS(s).

**Assured area**: whatever the compiler emits, the checker rejects it if the certificate does not hold; **compiler bugs cannot destroy trust in the artifact**.

---

<a id="2-信頼の土台trusted-base-のスタック"></a>

## 2. Foundation of Trust (Trusted-Base Stack)

Lower layers are what is trusted without verification (TB); higher layers are proved or checked. The stack's claim is to narrow **what must be believed to the bottom layer alone**.

```mermaid
flowchart TB
    subgraph L5["未信頼（証明対象でない）"]
      COMP["コンパイラ全体（parse〜emit）"]
    end
    subgraph L4["信頼（資格化対象）"]
      K["性質検査器 K"]
      V["翻訳検査器 V"]
      FMT["証明書形式（意味論を完全文書化）"]
    end
    subgraph L3["証明されるもの"]
      THM["検査器健全性定理<br/>accept ⟹ 性質成立"]
      ALSm["ALS メタ理論 / refinement"]
    end
    subgraph L2["TB — 無検証で信じる"]
      COQ["Coq カーネル<br/>(極小・世界中で精査済み)"]
      CC["CompCert / CertiCoq<br/>(検証済みコンパイラ)"]
      HW["OS / ハードウェア"]
      ALSv["ALS が意図どおりか<br/>(証明不能・経験で確かめる)"]
    end

    COMP -.出力を毎回検証.-> K
    K --> THM
    V --> ALSm
    THM --> COQ
    ALSm --> COQ
    K --> CC
    V --> CC

    classDef untrusted fill:#ffebee,stroke:#c62828,color:#000
    classDef trusted fill:#e8f5e9,stroke:#2e7d32,color:#000
    classDef proven fill:#f3e5f5,stroke:#6a1b9a,color:#000
    classDef tb fill:#fff3e0,stroke:#e65100,color:#000
    class COMP untrusted
    class K,V,FMT trusted
    class THM,ALSm proven
    class COQ,CC,HW,ALSv tb
```

Diagram labels, top to bottom: untrusted whole compiler, not itself the proof subject; trusted qualification scope K/V and fully specified certificate format; checker soundness theorem and ALS metatheory/refinement; unverified trusted base of the Coq kernel (tiny and extensively scrutinized), CompCert/CertiCoq verified compilers, OS/hardware, and whether ALS expresses the intention (not provable, checked through experience). Compiler output is checked each time; soundness and metatheory rely on the kernel, while K/V rely on the verified compilers.

**Assured area**: keeping “Coq-proved” truthful. `Print Assumptions` ⊆ standard assumptions plus independent coqchk rechecking mechanically verify each time that nothing extra has entered the TB.

---

<a id="3-コンポーネント早見表何--なぜ--担保"></a>

## 3. Component Quick Reference (What / Why / Assurance)

| Component | What it is | Why use it | Area of Almide assured |
|---|---|---|---|
| **ALS** (normative semantics) | Sole canon of language meaning, in Coq | Reference for every backend; prevents duplicate semantic definitions | Semantic consistency (**C-FAITHFUL**) |
| **almide-mir** | Single source of truth with explicit ownership and layout | One decision point, not re-decided by renderers | Consistent ownership decisions (`is_heap`/last-use/Repr, #643 class) |
| **Two renderers** (wasm sovereignty / Rust=Ferrocene) | **Translate only** from MIR; no re-decisions | wasm = canonical artifact with minimum TCB; Rust = stepping stone to Ferrocene | Byte reproducibility (**C-REPRO**) and equivalence of both routes |
| **Certificate format** | Tiny witness language, Metamath-scale | **Only seam** between untrusted compiler and trusted checker | Independent verification and unambiguous meaning |
| **Property checker K** | Small certificate-checking program, Coq-proved | `accept ⟹ P`; narrows trust to hundreds of lines | Memory safety, total names, capability upper bound, type concretization, stack balance, termination (**C-SAFE/C-PROVEN**) |
| **Translation checker V** | Checks each build that emitted wasm refines ALS | Answers the review question “Did you prove only the model?” | Model-to-artifact correspondence (**C-FAITHFUL**) |
| **Coq/Rocq** | Write proofs and have the kernel recheck them | Narrows trust to a tiny kernel plus accumulated scrutiny | Soundness of all proofs (**C-PROVEN**) |
| **CompCert/CertiCoq** | Verified compilers | Carry checkers to machine code, closing extraction and Thompson gaps | Correctness of the checker **itself**, threat T4 |
| **Differential oracle** (v0 corpus/contracts/interp) | Higher-level independent oracle retained until parity | Reject blind rewrites; do not delete before retirement | Regression detection and **preservation of v0 knowledge** |
| **dojo** (MSR) | Daily measurement of LLM writability | Mission metric, the sole metric | “Writable”: demand side of pillar A |
| **make verify-trust** | One entry for a third party to rederive all claims | Keeps CI outside the TB | Receipt **reproducibility** |

---

<a id="4-三本柱と指標全指標はここに転がる"></a>

## 4. Three Pillars and Metrics (All Metrics Lead Here)

```mermaid
flowchart TD
    V["Vision: 航空品質<br/>(DO-178C / DO-330)"]

    V --> A["柱A 書ける<br/>Writability"]
    V --> B["柱B 信頼できる<br/>Trust"]
    V --> C["柱C 認証できる<br/>Qualifiable"]

    A --> A1["統制 MSR ≥ Python/TS/MoonBit<br/>(対照群つき・修正タスク)"]
    B --> B1["C-SAFE / C-REPRO / C-FAITHFUL / C-PROVEN"]
    B --> B2["make verify で第三者が全主張再導出<br/>(CI は TB 外)"]
    C --> C1["資格化スコープ = 数百行のみ"]
    C --> C2["TB ⊆ 公理集合・公理清浄"]
    C --> C3["独立 2 実装一致 + 機械語まで検証"]

    classDef vis fill:#e3f2fd,stroke:#1565c0,color:#000
    classDef pillar fill:#ede7f6,stroke:#4527a0,color:#000
    class V vis
    class A,B,C pillar
```

Diagram labels: the vision is aviation quality (DO-178C / DO-330). Pillar A, writability, targets controlled modification-task MSR ≥ Python/TS/MoonBit with controls. Pillar B, trust, comprises C-SAFE/C-REPRO/C-FAITHFUL/C-PROVEN and third-party rederivation via make verify, with CI outside the TB. Pillar C, qualifiability, targets a scope of only hundreds of lines, TB within the axiom set with clean axioms, agreement of two independent implementations, and verification through machine code.

**Assured area**: A = demand (can a machine write correctly?), B = artifacts (can trust be handed over?), C = certification (can it enter assessment?). Only with all three can it call itself a “trust layer.”

---

<a id="5-脅威モデルどの部品がどの脅威を止めるか"></a>

## 5. Threat Model (Which Part Stops Which Threat)

```mermaid
flowchart LR
    T1["T1 善意だがバグる<br/>AI 著者"] --> M1["型/effect 系 + テスト + MSR"]
    T2["T2 敵対的著者<br/>(prompt 注入)"] --> M2["C-SAFE<br/>wasm import 上界（成果物自体が運ぶ）"]
    T3["T3 バグった<br/>コンパイラ"] --> M3["検査器 K<br/>(コンパイラは信頼不要)"]
    T4["T4 悪意ある<br/>コンパイラ Thompson"] --> M4["CompCert/CertiCoq で機械語検証<br/>+ 多様性"]
    T5["T5 汚染された<br/>検査インフラ"] --> M5["ローカル再検証<br/>+ 独立 2 実装"]

    classDef threat fill:#ffebee,stroke:#c62828,color:#000
    classDef mit fill:#e8f5e9,stroke:#2e7d32,color:#000
    class T1,T2,T3,T4,T5 threat
    class M1,M2,M3,M4,M5 mit
```

Diagram labels: T1, a well-intentioned but buggy AI author → type/effect system + tests + MSR. T2, an adversarial author/prompt injection → C-SAFE and the wasm-import upper bound carried by the artifact. T3, a buggy compiler → checker K, with no compiler trust required. T4, a malicious compiler/Thompson → CompCert/CertiCoq verification to machine code plus diversity. T5, contaminated checking infrastructure → local rechecking plus two independent implementations.

**Reading T2**: capability soundness comes not from “compile-time checking” but from the **wasm import section, a structural upper bound carried by the artifact itself**. Thus **tricking the checker does not help the author**: an import absent from it cannot be called, as guaranteed by the wasm specification.

---

<a id="6-成熟度ラダー航空品質への登攀現在地つき"></a>

## 6. Maturity Ladder (Climbing Toward Aviation Quality, with Position)

```mermaid
flowchart LR
    G0["G0 動く<br/>✅ サブセット証明・合成入力"] --> G1
    G1["G1 自己信頼<br/>🔄 実.almd 端まで + make verify + Rocq CI"] --> G2
    G2["G2 再現&測定<br/>⬜ byte 同一 + 統制 MSR 勝利"] --> G3
    G3["G3 資格化級<br/>⬜ 機械語検証・公理清浄・frees の翻訳検証"] --> G4
    G4["G4 認証準備<br/>⬜ DO-330 + dossier → 航空"]

    NOW["現在地: G0 済 / G1 を登攀中<br/>(指標① 実プログラム = 0 が次の山)"]
    NOW -.-> G1

    MOUNT["本丸の山: frees レンダラの翻訳検証<br/>(v0 が出血した所・ここを越えて初めて v0 超え)"]
    MOUNT -.-> G3

    classDef done fill:#e8f5e9,stroke:#2e7d32,color:#000
    classDef now fill:#fff3e0,stroke:#e65100,color:#000
    classDef todo fill:#eceff1,stroke:#546e7a,color:#000
    class G0 done
    class G1 now
    class G2,G3,G4 todo
    class NOW,MOUNT now
```

Diagram labels: G0, works, subset proofs and synthetic inputs complete; G1, self-trust, real .almd end-to-end + make verify + Rocq CI underway; G2, reproduction/measurement, byte identity + controlled MSR victory pending; G3, qualification grade, machine-code verification, axiom cleanliness and frees translation validation pending; G4, certification readiness, DO-330 + dossier → aviation pending. Historical position: G0 complete / climbing G1, with metric 1, real programs = 0, the next climb. The central mountain is translation validation of the frees renderer, where v0 struggled; only crossing it surpasses v0.

**Route reality**: G4 aviation is hardest and last. **Monetization and qualification go through adjacent markets first**: CRA / cryptography / AI-agent infrastructure (1–3 years) → automotive/industrial ISO 26262 / IEC 61508 (requires incorporation) → **aviation last**. Aviation is not a near-term product goal but a north star that supplies discipline.

---

<a id="7-一枚にまとめると"></a>

## 7. Putting It in One Diagram

```mermaid
flowchart TB
    subgraph WRITE["柱A 書ける（需要）"]
      DOJO["dojo / MSR"]
    end
    subgraph BUILD["未信頼の生産（規模不問・バグ可）"]
      SRC2[".almd"] --> MIR["almide-mir（唯一の真）"] --> REND["二レンダラ"] --> WASM["wasm a"]
      MIR --> CERT2["証明書 c"]
    end
    subgraph VERIFY["信頼の検証（数百行・資格化対象）"]
      KK["K 性質検査器"]
      VV["V 翻訳検査器"]
      ALS2["ALS 正典"]
    end
    subgraph RECEIPT["受領書（第三者が make verify）"]
      R["C-SAFE / C-REPRO / C-FAITHFUL / C-PROVEN"]
    end

    CERT2 --> KK
    WASM --> KK
    WASM --> VV
    ALS2 --> VV
    KK --> R
    VV --> R
    DOJO -. mission 指標 .-> SRC2
    R -. 反証手続き公開 .-> AUDIT["第三者が手元で再導出<br/>CI は TB 外"]

    classDef w fill:#ede7f6,stroke:#4527a0,color:#000
    classDef b fill:#ffebee,stroke:#c62828,color:#000
    classDef v fill:#e8f5e9,stroke:#2e7d32,color:#000
    classDef r fill:#e3f2fd,stroke:#1565c0,color:#000
    class DOJO w
    class SRC2,MIR,REND,WASM,CERT2 b
    class KK,VV,ALS2 v
    class R,AUDIT r
```

Diagram labels: demand-side pillar A contains dojo/MSR. Untrusted production, with arbitrary scale and possible bugs, runs .almd → almide-mir (sole truth) → two renderers → wasm a, while MIR also emits certificate c. Trusted verification, hundreds of lines in qualification scope, contains K, V and canonical ALS. Certificates and wasm go to K; wasm and ALS go to V; both yield the C-SAFE/C-REPRO/C-FAITHFUL/C-PROVEN receipt. Third parties rederive it locally via make verify, with public falsification procedures and CI outside the TB. Dojo supplies the mission metric to source production.

**Overall assurance, in one sentence**: produce at scale in red (untrusted), verify every build in green (hundreds of lines, qualification scope), and let third parties locally rederive blue receipts: **narrow trust to hundreds of lines, so nobody needs to trust anything else**. This is the overall assurance architecture for Almide v1.
