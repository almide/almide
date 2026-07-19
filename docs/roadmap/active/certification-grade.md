<!-- description: Certification-grade hardening — adopt the mechanisms of DO-178C / ISO 26262 / IEC 61508 (spec, traceability, coverage, tool qualification, dossier) for the machine-written-software trust layer -->
# Certification-Grade Hardening — 認証級への硬化

> **Goal**: 宇宙・航空・自動車基準 (DO-178C / ISO 26262 / ECSS / IEC 61508) を構成する
> 5 メカニズムのうち工学で取れるものを全て取り込み、[trust-layer](trust-layer.md) の
> L2/L3 を「認証級」の定義に引き上げる。証明書そのもの (TÜV 監査・DO-330 TQL) は
> 商業オプションとして分離 — ここで作るのはその前提となる証拠体系。
> **実地監査所見**: [flight-evidence-gaps](flight-evidence-gaps.md) — 2026-07-03 の
> ハンズオン改修で観測した証拠体系の穴 7 件（F1 oracle 循環は CG-1 の、F2 カバレッジ錯覚は
> evidence ladder の、それぞれ実測された裏付け）。
> **Active scope: CG-1** — ALS (Almide Language Specification) + reference interpreter
> の規範化 + 契約の spec-keying。
> **Exit criteria (CG-1)**: 全 active 契約が ALS の節を `spec` フィールドで参照し、
> `check-contracts.sh` が spec ↔ contract ↔ fixture の三層トレーサビリティを強制する。
> `almide-interp` が「3rd judge」から規範意味論に昇格し、native / wasm は
> 「仕様に一致すべき実装」になる。

## 規格の分解 — 認証基準は 5 つのメカニズムでできている

DO-178C (航空, Level A-E)、ISO 26262 (自動車, ASIL A-D)、ECSS-E-ST-40C / NPR 7150.2
(宇宙)、IEC 61508 (親規格, SIL 1-4) は、剥がすと同じ部品に分解できる:

| # | メカニズム | 規格での名前 | Almide 現状 |
|---|---|---|---|
| ① | 実装から独立した規範仕様 | requirements / language specification | ❌ native = oracle の循環 (最深ギャップ) |
| ② | 双方向トレーサビリティ | bidirectional traceability | ✅ C-NNN ↔ `@contract:` 双方向ゲート |
| ③ | 検証の格付けと下限 | MC/DC coverage, DO-333 formal credit | ◑ evidence ladder rank 0-5 + floor rule。欠け = コンパイラ自体のカバレッジ |
| ④ | ツール資格 | DO-330 TQL-1, ISO 26262 TCL3 | ◑ `Verified<T>` belt + byte-identity = per-build 自己検証。欠け = 資格化パッケージ |
| ⑤ | プロセスと責任 | audits, LTS, Known Problems, liability | ◑ flagged ratchet = Known Problems ledger 相当。欠け = 組織側 (スコープ外) |

## 現有資産の規格対応表

既に持っているものを規格の語彙で言い直す。CG の各 Phase はこの表の「欠け」を埋める。

| Almide の資産 | 規格での対応物 |
|---|---|
| 契約台帳の双方向ゲート ([contracts.toml](../../contracts/contracts.toml) ↔ `spec/wasm_cross/*.almd`) | DO-178C bidirectional traceability (reqs ↔ tests) |
| evidence ladder doc-only→lean ([contract-classes.txt](../../../scripts/lib/contract-classes.txt) rank 0-5) | SPARK assurance ladder (Stone→Platinum) / DO-333 formal credit |
| `flagged-for-revision` ラチェット (減る方向のみ) | 認証コンパイラ必須の Known Problems ledger |
| [Perceus-belt](almide-perceus-belt.md) `Verified<T>` 型状態 | tool self-verification (検証なき IR は emit 不能) |
| [capability system](effect-system-capability.md) | ISO 26262 freedom-from-interference / ARINC 653 partitioning |
| byte-identity gate + host-arch deterministic codegen | configuration management (規格要求を超過) |
| nightly fuzz (`n =` を evidence に記録) | 定量化された検証証拠 |
| reference interpreter (`crates/almide-interp`, 3rd judge) | **規範意味論の候補** (CG-1 で昇格) |

## 最深のギャップ: 「実装が仕様」は認証では通らない

現在の台帳は native = oracle で成立している。工学的には正しいブートストラップだが、
認証の世界では循環 — 「要求はコードから独立に存在し、コードは要求に対して検証される」
が絶対条件。

先行者がこの道を踏んでいる: **Ferrocene は Rust を ISO 26262 ASIL D / IEC 61508 に
資格化する前提として FLS (Ferrocene Language Specification) を書いた**
(Rust に規範仕様が無かったため。後に Rust Project 公式仕様の基盤に採用)。

Almide には Ferrocene より良い出発点がある: 実行可能な reference interpreter が既に
存在する。CG-1 で oracle を反転させると、**spec (interp) / native / wasm の三つ巴 =
本物の N-version 検証** (2-of-3 多数決) になり、循環が消えるだけでなく検証力が上がる。

## Phases

### CG-1: ALS + 規範意味論の昇格 (M-L) — Active

1. **`almide-interp` を規範意味論に昇格** — 「3rd judge」から「THE spec」へ。
   まず gap audit: interp が評価できない言語機能・stdlib 関数の棚卸し
   (評価不能領域 = 仕様の穴として台帳化、ratchet で縮める)
   - ✅ **台帳 + 両方向ゲート着地** (issue #564): `crates/almide-interp/interp-abstain-ledger.txt`
     + `interp_cross_target_test.rs::interp_abstain_ledger` (バックエンド不要 = CI で
     self-skip しない)。新規 abstain は台帳更新を強制、解消済みエントリの放置も fail —
     台帳は縮む方向のみ。初回監査: 121 fixtures 中 評価可能 49 (40%) / 評価不能 72
     （内訳: 設計上の除外 ~18 (transcendental 6 / in-place 5 / fan 非決定 7) + glue 未実装 ~54）。
     **再計測 (2026-07-19)**: コーパスは `spec/wasm_cross/` **270 fixtures** に拡大、
     `interp-abstain-ledger.txt` の non-comment エントリは **133** = 評価可能 **137 (≈51%)**。
     評価可能率は 40%→51% に改善（内訳の再分類は別途要作業）
2. **ALS の起草** — interp を executable semantics として参照する散文仕様。
   構成: 構文 / 静的意味論 (型・capability) / 動的意味論 (interp 参照) /
   観測等価性の定義 (stdout, stderr, exit code)。**53 契約の statement が節の種** —
   契約は既に規範的文体で書かれている
3. ✅ **契約の spec-keying — DONE** (issue #565, closed): `[[contract]]` に
   `spec = "ALS §x.y"` フィールドを追加 (active 契約は REQUIRED)、`check-contracts.sh` が
   spec ↔ contract ↔ fixture の三層対称性を強制（`docs/specs/als/` の `## ALS-xx` 節を
   実在チェック、未参照節も検出）。**CG-1 の残作業は item 1 のみ** — issue #564
   (interp を「3rd judge」から規範意味論へ完全昇格) は依然 **OPEN**

### CG-2: コンパイラ自体の structural coverage (M)

- spec / fixture 実行下でコンパイラの branch coverage を計測 (cargo llvm-cov)、
  CI ratchet 化 (下がらない)
- 安全パス (`PerceusVerifyPass`, capability check, `StackBalancePass`) は
  MC/DC 相当まで引き上げ
- 規格対応: DO-178C Level A structural coverage。
  発見した未到達コード = 死んだ分岐 (削除) か 欠けた fixture (追加) の二択

### CG-3: Almide/Critical profile (M)

MISRA C / Ada Ravenscar / SPARK subset の analog。`almide check --profile critical`:

- **全域性必須** — C-001/C-002/C-047 (div/mod/pow は trap せず abort) の主題を
  言語全体に一般化
- 有界再帰・確保上限 (静的に検査可能な形)、RC drop カスケード長の上限
  → WCET 解析可能性
- capability は deny-all 出発 (`Rand` / `Time` も明示 grant)
- flagged 契約の機能は使用不可 (現状 flagged はゼロ — C-006 fan.timeout は 0.29.0 で言語ごと削除済み)
- **Critical は subset であって方言ではない** — 全 Critical コードは通常モードでも
  そのまま有効 (SPARK ⊂ Ada と同じ関係)

### CG-4: Translation validation (L)

belt の思想をビルド単位の証明に拡張: 「この .wasm はこの検証済み IR の正しい翻訳」
という証拠を成果物ごとに添付する。

- DO-330 の**上位互換** — プロセスの資格化ではなく per-artifact proof
  (seL4 / CompCert の系譜)。紙の規格には構造的にできない芸当
- 段階: `Verified<T>` の適用範囲拡大 → emit invariant の機械検査 →
  (遠期) Lean での emit 規則証明
- CompCert 級のフル証明は [completeness roadmap](correctness-guarantee-gaps.md)
  で棄却済み — translation validation は同じ保証クラスをビルド単位で取る代替

### CG-5: Qualification bundle (S-M)

`make verify` ([trust-layer](trust-layer.md) receipts harness) の出力を
署名付き・版付き dossier に:

- 契約台帳 + spec トレーサビリティ + カバレッジ + Lean 0-sorry + fuzz `n =` +
  byte-identity + Known Problems (flagged 一覧)
- リリースごとに自動生成・添付。**将来の商業資格化 (Ferrocene playbook) の入力**
  であり、dossier が先にあれば資格化コストは劇的に下がる

## 何をしないか

- **証明書の取得それ自体** (TÜV / DO-330 監査) — 監査対応・LTS 保証・賠償責任は
  組織の問題で、工学では買えない。Ferrocene も AdaCore もこれを売る会社として存在する。
  カテゴリ確立後の商業オプションとして分離
- **フライトソフトウェア市場への参入** — 買い手はエージェント基盤。規制産業向けは
  dossier が揃ってからの選択肢
- **CompCert 級フルコンパイラ証明** — データ付きで棄却済み。CG-4 で代替

## 先行者の地図 — 何を借りるか

| 先行者 | 実績 | 借りるもの |
|---|---|---|
| SCADE KCG | DO-178C Level A 資格済みコード生成器 | 「資格化されたコンパイラの出力はソースレビュー不要」という **claim の形** — trust layer と完全に同型 |
| Ferrocene | rustc を ISO 26262 ASIL D / IEC 61508 資格化 | **spec を先に書く** playbook (FLS)。資格化資料の公開スタイル |
| SPARK | DO-333 で証明をテスト代替として認めさせた | assurance ladder の設計 (evidence ladder は既に同型) |
| CompCert / seL4 | 検証済みコンパイラ / binary までの証明 | translation validation の発想 (CG-4) |
| MISRA / Ravenscar | 認証可能サブセット | profile は subset であって方言ではない原則 (CG-3) |

## リスク

| リスク | 対処 |
|---|---|
| ALS が実装の逆写経になり循環が形を変えて残る | interp を**先に**昇格し、ALS は interp に対して書く。native との不一致は ALS 側でなく実装側のバグとして扱う |
| カバレッジ工数の沼 | 安全パス限定の MC/DC から始め、全体は branch ratchet のみ |
| Critical profile が言語を二つに割る | subset 原則 (CG-3) を gate で強制 — Critical で valid なら通常モードでも valid |
| 認証ごっこ (誰も使わない dossier) | CG-5 は trust-layer の `make verify` と同一物の整形。独立の成果物を作らない |
