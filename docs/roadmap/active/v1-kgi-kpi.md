<!-- description: v1 KGI/KPI scoreboard — the terminal goal indicators (trust + writability), the guard invariants that must never degrade (checker size, TB purity, axiom cleanliness, zero claim-drift), and the progress KPIs toward each gap. Weekly fill-in. -->
# v1 KGI / KPI スコアボード

> **これは何か**: v1 の終局指標(KGI)と、それを守る/攻める指標(KPI)を週次で
> 埋めるスコアボード。このプロジェクトは構造が特殊で ―― **KPI が「攻める系
> (伸ばす)」と「守る系(絶対に劣化させない不変条件)」の2種に割れ、守る系を破った
> 瞬間に攻める系の成果が無効化される**。だから守る系を先に見る。
> **関連**: [v1-system-map](v1-system-map.md) / [receipt-logic](receipt-logic.md) /
> [trust-layer](trust-layer.md)(L0-L4)/ [v1-proof-architecture](v1-proof-architecture.md)。

## 守るものの本質(一行)

> **信じる対象を、小さく・純粋に保つこと。** 検査器は数百行のまま、信頼基底は
> 宣言した底のまま。これが崩れたら、どれだけ性質を増やしても flight-grade は
> 届かない。

---

## KGI(終局指標 ―― これが真なら「勝った」)

勝利は **連言**。片方だけは既知の失敗モード(信頼だけ=CompCert ニッチ /
書けるだけ=ただの AI 言語)。

| KGI | 達成状態(これが真であるべき) | 現在 |
|---|---|---|
| **KGI-T(信頼)** | 第三者が **irreducible floor(Coq カーネル / wasm 意味論 / wasmtime 忠実 / HW / ALS 妥当性)だけ**を信じて、**実プロダクションで使う全プログラム**について[安全性束]を、**数百行の資格化済み検査器**で**毎ビルド再導出**できる。外部資格化(隣接市場 → 航空) | ⬜ 切片のみ |
| **KGI-W(書ける)** | 同一条件・対照群つきで、機械が Almide を**修正して他言語より正確に**書ける(統制 MSR で勝つ) | ⬜ 未測定 |
| **連言(真の勝利)** | 上記 2 つが**同時に**真 ―― 機械が最も正確に書け、かつその出力が flight-grade で信頼できる | ⬜ |

**[安全性束]** = メモリ安全 / leak なし / capability 有界 / 型確定 / call 規約適合 /
バイトがソース意味論を refine(C-FAITHFUL)/ byte 再現(C-REPRO)。

---

## 守る系 KPI(不変条件 ―― 1つでも破れたら攻める成果は無効)

**目標は「常に = この値 / ≤ この上限」。伸ばすのではなく、劣化させない。**

| 守る KPI | 守るべき値 | 現在 |
|---|---|---|
| **検査器規模** | ≤ 数百行・**プログラム / 言語 / コンパイラ複雑度に非依存**(規模 ∝ #イベント文字 + #subset 性質 + #op→パターン表) | ✅ |
| **信頼基底(TB)** | = 宣言した floor。**creep ゼロ**(import が増えても台帳で境界づけ) | ✅ TRUSTED_BASE.md 維持 |
| **公理純度** | 全定理 `Print Assumptions` ⊆ 標準・信頼拡張(native_decide 等)ゼロ | ✅ "Closed under the global context" |
| **証明の独立検査** | kernel-checked + coqchk + クロスバージョン(0 sorry / 0 Axiom) | ✅ 9.1.1 + 9.2 |
| **主張ドリフト** | = 0(公開主張 ⊆ 機械照合) | ⚠️ 機械照合は未(手動で 1 件検出・修正) |
| **silent 通過** | = 0(受理 ⟹ 安全 or `[COMPILER BUG]` 停止。shortcut 全機械ゲート) | 🔄 hole-hunt 未乾 |

**この表の意味**: 攻める成果を「守る KPI を破って」買ったら、それは前進ではなく
**純損失**(検査器が数千行になれば、全言語を覆っても資格化対象が消える)。
これが「整える勝負」の KPI 表現 ―― **不変条件を破る前進は、無い方がマシ**。

---

## 攻める系 KPI(進捗 ―― KGI へ向けて伸ばす)

| 攻める KPI | 0 ──────────→ 完了 | 現在 | gap / C-* |
|---|---|---|---|
| **性質被覆** | 安全性束 の 何 / 8 | **4 / 8**(mem✅ name✅ cap✅ type✅ / leak・reuse・call-mode・byte 残) | C-SAFE / C-PROVEN |
| **証明書形式の表現力** | eager → perceus → full | **eager**(i/a/d/m 合法、r/b 拒否) | 横断(形式 v1) |
| **実プログラム被覆** | PCC 鎖を端まで通るコーパス数 | **1**(3 行 fixture) | Gap 3 |
| **言語面被覆** | subset → call → 制御フロー → closure → nested heap → 全言語 | **subset(call なし)** | Gap 3 |
| **バイト束縛** | §3 契約(信頼) → 証明済み | **信頼のまま** | **Gap 1(本丸)** |
| **frees / leak-freedom** | eager(leak 既知の負) → frees(leak 証明済み) | **eager** | Gap 2 |
| **統制 MSR** | 未測定 → 対照群との差 | **未測定** | 柱 A |
| **外部** | ローカル → CI 強制 → 多様性 2 実装 → CertiCoq 機械語 → 隣接資格化 → 航空 | **CI 強制 ✅** | KGI-T |

### gap ↔ 攻める KPI 対応

- **Gap 1**(最重要・最深): バイト束縛 ―― witness ⟹ wasm バイトを §3 契約から
  証明済みへ。WasmCert-Coq import + ランタイム heap refinement 証明 + per-build V'。
- **Gap 2**: frees / leak-freedom / reuse ―― 精密所有権モデル(Perceus per-edge)を
  MIR に。証明書形式の a/m/r 文字がその ground-fact 化の入口。
- **Gap 3**: 言語面被覆 + 実プログラム被覆 ―― call(即時) → 制御フロー → closure
  → nested heap。二重オラクル(v0 コーパス)が被覆テスト。

---

## KGI と KPI の関係(運用ルール = この 1 行)

> **KGI = max(攻める系) s.t. 守る系を全てピン留め。** 守る系を破って買った
> 攻める系の値は、加算ではなく**減算**(信頼の核を侵食するから)。

だから週次の見方は「攻める系がどれだけ伸びたか」**の前に**「守る系が全部 green の
ままか」を見る。守るが 1 つでも落ちたら、その週は前進ではなく**後退**。

---

## 週次記録(最新を上に追記)

### 2026-06-13 — 基準点
- **守る系**: 全 green(検査器小 / TB 宣言済み / 公理純 / CI green・クロスバージョン
  9.1.1+9.2)。主張ドリフトの機械照合と silent 通過(hole-hunt 乾き)は未完。
  **形は無傷** ―― 「整える」は現サブセットに対して達成。
- **攻める系**: 4/8 性質・eager のみ・実 1 プログラム・subset(call なし)・
  バイト束縛は信頼のまま・frees 未着手・**MSR ゼロ**。
- **読み**: 守るべきものは守れている。満たすべき中身が薄い。KGI の両輪
  (KGI-T 切片のみ / KGI-W 未測定)はどちらもこれから。次の勝負は、守る系を
  1 つも落とさずに攻める系(とりわけ **MSR** と **Gap 1 バイト束縛**)を伸ばせるか。

<!-- 週次テンプレ:
### YYYY-MM-DD
- 守る系: [全green か / 落ちた不変条件があれば即記録 → これは後退]
- 攻める系: [動いた攻めKPIと現在値]
- 読み: [守りを落とさず攻めたか。次の一手]
-->

---

## 一言

守るのは「**信じる対象の小ささと純度**」(= 数百行の検査器 + 底まで潰れた TB)。
KGI は「**実プロダクション全体を、その小さな信頼で毎ビルド証明でき、かつ機械が
最も正確に書ける**」の連言。**今は守りが完璧で攻めが初期** ―― 次の勝負は、守る系を
1 つも落とさずに攻める系(MSR と Gap 1)を伸ばせるか。
