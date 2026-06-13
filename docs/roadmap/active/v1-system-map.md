<!-- description: v1 system map — mermaid diagrams of the whole trust architecture: each component's what / why / which area it secures, the PCC trust flow, the trust base, the three pillars, the threat model, and the maturity ladder. -->
# v1 System Map — 全体像(mermaid)

> **これは何か**: Almide v1 の信頼アーキテクチャを **一枚ずつの図**で見る地図。
> 各部品について「**何か / なぜ使うか / Almide のどの領域を担保するか**」を示す。
> 詳細な根拠は兄弟文書へ:
> [v1-proof-architecture](v1-proof-architecture.md)(着地形)、
> [receipt-logic](receipt-logic.md)(受領書の形式)、
> [trust-layer](trust-layer.md)(カテゴリ戦略 L0-L4)、
> [completeness-by-construction](completeness-by-construction.md)(意味論台帳)。

---

## 0. 背骨(全図に共通する一文)

> **コンパイラの正しさは証明しない。小さな検査器の健全性だけを証明し、
> コンパイラには毎ビルド「証明書」を吐かせ、検査器が毎回照合する。**
> 確かめるのは作るより桁安い ―― だから信頼が「10 万行」から「数百行」に潰れる。

---

## 1. 信頼の流れ(PCC 鎖)

未信頼のコンパイラが成果物と証明書を吐き、**信頼すべき小さな検査器だけ**が
それを再検証する。信頼境界(赤=信頼不要 / 緑=信頼)が読みどころ。

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

**担保する領域**: コンパイラが何を吐こうと、証明書が成立しなければ検査器が弾く
=**コンパイラのバグが成果物の信頼を壊せない**。

---

## 2. 信頼の土台(Trusted Base のスタック)

下ほど「無検証で信じるもの(TB)」、上ほど「証明・検査されるもの」。
**信じるべきは最下層だけ**に絞る、というのがこの積み方の主張。

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

**担保する領域**: 「Coq 証明済み」が嘘にならないこと ―― `Print Assumptions` ⊆
標準 + coqchk 独立再検査で、TB に余計なものが紛れ込まないことを毎回機械確認。

---

## 3. コンポーネント早見表(何 / なぜ / 担保)

| 部品 | 何か | なぜ使う | Almide のどの領域を担保 |
|---|---|---|---|
| **ALS**(規範意味論) | 言語の意味の唯一の正典(Coq 内) | 全バックエンドの基準点。意味の二重定義を防ぐ | 意味論の一貫性(**C-FAITHFUL**) |
| **almide-mir** | 所有権 + レイアウト明示の唯一の真 | レンダラに再決定させない単一決定点 | 所有権決定の一貫性(`is_heap`/last-use/Repr の #643 クラス) |
| **二レンダラ**(wasm 主権 / Rust=Ferrocene) | MIR の**翻訳のみ**・再決定しない | wasm=最小 TCB の正典成果物、Rust=Ferrocene 踏み台 | byte 再現性(**C-REPRO**)・二経路の同値 |
| **証明書形式** | 極小 witness 言語(Metamath 級) | 未信頼コンパイラと信頼検査器の**唯一の継ぎ目** | 検証の独立性・意味の無曖昧さ |
| **性質検査器 K** | 証明書を照合する小プログラム(Coq 証明済) | `accept ⟹ P`。信頼を数百行に絞る | メモリ安全・name 全域・capability 上界・型 concretize・stack 均衡・終端(**C-SAFE/C-PROVEN**) |
| **翻訳検査器 V** | emit wasm が ALS を refine するか毎ビルド確認 | 審査の必殺質問「証明したのはモデルでは?」への答え | モデル↔実物の対応(**C-FAITHFUL**) |
| **Coq/Rocq** | 証明を書きカーネルが再検査 | 信頼を極小カーネルに絞る + 精査の蓄積 | 全証明の健全性(**C-PROVEN**) |
| **CompCert/CertiCoq** | 検証済みコンパイラ | 検査器を機械語まで・抽出穴と Thompson 穴を閉じる | 検査器**自身**の正しさ(脅威 T4) |
| **差分オラクル**(v0 corpus/contracts/interp) | parity 到達まで温存する上位独立 oracle | blind rewrite 却下・退役前に消さない | 回帰検出・**v0 の知識の保存** |
| **dojo**(MSR) | 日次の LLM 筆記性測定 | mission 指標(唯一の指標) | 「書ける」=柱 A の需要側 |
| **make verify-trust** | 第三者が全主張を再導出する単一入口 | CI を TB の外に出す | 受領書の**再現可能性** |

---

## 4. 三本柱と指標(全指標はここに転がる)

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

**担保する領域**: 柱 A=需要(機械が正確に書けるか)、柱 B=成果物(信頼を渡せるか)、
柱 C=認証(審査に乗るか)。三つが揃って初めて「信頼層」を名乗れる。

---

## 5. 脅威モデル(どの部品がどの脅威を止めるか)

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

**読みどころ(T2)**: capability の健全性は「コンパイル時チェック」ではなく
**wasm import セクション=成果物自体が運ぶ構造的上界**が担う。だから
**著者がチェックを騙しても無駄** ―― import に無いものは呼べない(wasm 仕様が担保)。

---

## 6. 成熟度ラダー(航空品質への登攀・現在地つき)

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

**経路の現実**: G4 の航空は最難関・最後。**換金と資格化は隣接市場を先に通る**
―― CRA / 暗号 / AI エージェント基盤(1〜3 年)→ 自動車・産業 ISO 26262 /
IEC 61508(要・会社化)→ **航空は最後**。航空は製品の近期目標ではなく、規律を
生む北極星。

---

## 7. 一枚にまとめると

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

**全体の担保構造(一言)**: 赤(未信頼)で大量に作り、緑(数百行・資格化対象)で
毎ビルド検証し、青(受領書)で第三者が手元再導出する ―― **信じる対象を数百行に
絞りきり、それ以外は誰も信用しなくてよい状態**。これが Almide v1 が担保する全体像。
