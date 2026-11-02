原文の説明メタデータ：認証級への硬化 — 機械筆記ソフトウェアの信頼層に DO-178C / ISO 26262 / IEC 61508 の機構（仕様、トレーサビリティ、カバレッジ、ツール資格、dossier）を採用する
<a id="certification-grade-hardening--認証級への硬化"></a>

# 認証級への硬化

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](certification-grade.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](certification-grade.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](certification-grade.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md)

> 原文注記：「認証級」は工学上の目標であり、証明書取得、ツール資格取得、規制承認、安全保証を意味しない。規格対応、他製品の資格、費用低減は原文の主張であり独立監査した所見ではない。初期の native oracle・仕様欠落と起草計画は後の ALS・spec-keying 着地記録と併存し、初回121-fixture の数値は歴史的なもの。N-version 多数決や interp 昇格だけで独立性・全意味論は証明されない。Critical は信頼する stdlib を伴う記載の入口プログラム範囲に適用し、fan 適格性・RC 連鎖上限は未完。translation validation は成果物対応・checker 対象性質の義務を要し、DO-330 プロセス要求の自動代替ではない。

> **目標**: 宇宙・航空・自動車基準 (DO-178C / ISO 26262 / ECSS / IEC 61508) を構成する
> 5 メカニズムのうち工学で取れるものを全て取り込み、[trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) の
> L2/L3 を「認証級」の定義に引き上げる。証明書そのもの (TÜV 監査・DO-330 TQL) は
> 商業オプションとして分離 — ここで作るのはその前提となる証拠体系。
> **実地監査所見**: [flight-evidence-gaps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-evidence-gaps.md) — 2026-07-03 の
> ハンズオン改修で観測した証拠体系の穴 7 件（F1 oracle 循環は CG-1 の、F2 カバレッジ錯覚は
> evidence ladder の、それぞれ実測された裏付け）。
> **活動範囲：CG-1** — ALS (Almide Language Specification) + 参照インタプリタ
> の規範化 + 契約の spec-keying。
> **終了条件（CG-1）**: 全 active 契約が ALS の節を `spec` フィールドで参照し、
> `check-contracts.sh` が spec ↔ contract ↔ fixture の三層トレーサビリティを強制する。
> `almide-interp` が「3rd judge」から規範意味論に昇格し、native / wasm は
> 「仕様に一致すべき実装」になる。

<a id="規格の分解--認証基準は-5-つのメカニズムでできている"></a>

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

<a id="現有資産の規格対応表"></a>

## 現有資産の規格対応表

既に持っているものを規格の語彙で言い直す。CG の各 Phase はこの表の「欠け」を埋める。

| Almide の資産 | 規格での対応物 |
|---|---|
| 契約台帳の双方向ゲート ([contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) ↔ `spec/wasm_cross/*.almd`) | DO-178C bidirectional traceability (reqs ↔ tests) |
| evidence ladder doc-only→lean ([contract-classes.txt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/lib/contract-classes.txt) rank 0-5) | SPARK assurance ladder (Stone→Platinum) / DO-333 formal credit |
| `flagged-for-revision` ラチェット (減る方向のみ) | 認証コンパイラ必須の Known Problems ledger |
| [Perceus-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md) `Verified<T>` 型状態 | tool self-verification (検証なき IR は emit 不能) |
| [capability system](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md) | ISO 26262 freedom-from-interference / ARINC 653 partitioning |
| byte-identity gate + host-arch deterministic codegen | configuration management (規格要求を超過) |
| nightly fuzz (`n =` を evidence に記録) | 定量化された検証証拠 |
| reference interpreter (`crates/almide-interp`, 3rd judge) | **規範意味論の候補** (CG-1 で昇格) |

<a id="最深のギャップ-実装が仕様は認証では通らない"></a>

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

<a id="phases"></a>

## 段階

<a id="cg-1-als--規範意味論--当初の計画と着地記録"></a>

### CG-1: ALS + 規範意味論 — 当初の計画と着地記録

ALS は現在 [docs/specs/als/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/README.md) に存在し、
契約の spec-keying も現行台帳/ゲートの対象である。以下の初回 121-fixture
監査や起草手順は歴史的な計画・測定であって、ALS が現在未起草という意味ではない。

1. **`almide-interp` を規範意味論に昇格** — 「3rd judge」から「THE spec」へ。
   まず gap audit: interp が評価できない言語機能・stdlib 関数の棚卸し
   (評価不能領域 = 仕様の穴として台帳化、ratchet で縮める)
   - ✅ **台帳 + 両方向ゲート着地** (issue #564): `crates/almide-interp/interp-abstain-ledger.txt`
     + `wasm_runtime_interp_ledger.rs::interp_abstain_ledger` (バックエンド不要 = CI で
     self-skip しない)。新規 abstain は台帳更新を強制、解消済みエントリの放置も fail —
     台帳は縮む方向のみ。初回監査: **121 fixtures 中 評価可能 49 (40%) / 評価不能 72**。
     内訳: 設計上の除外 ~18 (transcendental 6 / in-place 5 / fan 非決定 7) + **glue 未実装
     ~54 (map.new, set.from_list, json.parse, string.take/slice, list.get_or, bytes.* …)** —
     後者が「skip → 第三票」に変換できる伸びしろ
2. **ALS の起草** — interp を executable semantics として参照する散文仕様。
   構成: 構文 / 静的意味論 (型・capability) / 動的意味論 (interp 参照) /
   観測等価性の定義 (stdout, stderr, exit code)。**53 契約の statement が節の種** —
   契約は既に規範的文体で書かれている
3. **契約の spec-keying** — `[[contract]]` に `spec = "ALS §x.y"` フィールドを追加
   (active 契約は REQUIRED)、`check-contracts.sh` を三層対称性ゲートに拡張

<a id="cg-2-コンパイラ自体の-structural-coverage-m"></a>

### CG-2: コンパイラ自体の structural coverage (M)

- spec / fixture 実行下でコンパイラの branch coverage を計測 (cargo llvm-cov)、
  CI ratchet 化 (下がらない)
- 現行の安全性検査を列挙して coverage の範囲を決める。元の
  `PerceusVerifyPass` / `StackBalancePass` は旧 emitter の名前なので、
  現行台帳を `proofs/receipt.sh` / codegen pass から作る
- 規格対応: DO-178C Level A structural coverage。
  発見した未到達コード = 死んだ分岐 (削除) か 欠けた fixture (追加) の二択

<a id="cg-3-almidecritical-profile-m"></a>

### CG-3: Almide/Critical profile (M)

MISRA C / Ada Ravenscar / SPARK subset の analog。`almide check --profile critical`:

- **全域性必須** — C-001/C-002/C-047 (div/mod/pow は trap せず abort) の主題を
  言語全体に一般化
- 有界再帰・確保上限 (静的に検査可能な形)、RC drop カスケード長の上限
  → WCET 解析可能性
- capability は deny-all 出発 (`Rand` / `Time` も明示 grant)
- admissibility は現行 ALS §B と critical-profile 検査に従う。
  `fan.timeout` の 0.29.0 撤去を現在の禁止理由にしない（後に oracle body 形として再導入）
- **Critical は subset であって方言ではない** — 全 Critical コードは通常モードでも
  そのまま有効 (SPARK ⊂ Ada と同じ関係)

**状態 (#567 着地)**: `almide check --profile critical` 稼働 — bounded
プロファイル (ALS §B, E070–E078) を全関数に適用し、capability は deny-all
出発で `--allow IO|Net|Env|Time|Rand|Process` が grant。スコープはエントリ
プログラム (stdlib はプロファイルの下の trusted runtime)。subset 性は
`tests/critical_profile_test.rs` が全負例で通常モード PASS を突き合わせて
CI 強制。静的メモリ規律 (#568) も着地: 参照アプリが critical-clean +
カーネル emit 無割当 + `--heap-cap` 固定予算で完走(予算超過は決定的
OOM abort)、分割ホスト要件は docs/project/PARTITIONED-RUNTIME.md。
残: `fan.*` の admissibility (#1628 stage 2 のコスト境界待ち)、
RC drop カスケード上限。

<a id="cg-4-translation-validation-l"></a>

### CG-4: Translation validation (L)

belt の思想をビルド単位の証明に拡張: 「この .wasm はこの検証済み IR の正しい翻訳」
という証拠を成果物ごとに添付する。

- per-artifact 検証を資格化評価の証拠として検討する。DO-330 の上位互換、
  またはプロセス要求の自動代替とは主張しない
- 元の `Verified<T>` は旧 emitter の設計。現行の対象・trusted seam を列挙し、
  emit invariant の機械検査から必要な意味保存義務へ進む →
  (遠期) Lean での emit 規則証明
- CompCert 級のフル証明は [completeness roadmap](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md)
  の歴史的検討を参照。translation validation が同等の保証を与えるには、
  checker の対象性質と実際の成果物との対応を別途確立する必要がある

<a id="cg-5-qualification-bundle-s-m"></a>

### CG-5: Qualification bundle (S-M)

`make verify` ([trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) receipts harness) の出力を
署名付き・版付き dossier に:

- 契約台帳 + spec トレーサビリティ + カバレッジ + Lean 0-sorry + fuzz `n =` +
  byte-identity + Known Problems (flagged 一覧)
- リリースごとに自動生成・添付。**将来の商業資格化 (Ferrocene playbook) の入力**
  であり、dossier が先にあれば資格化コストは劇的に下がる

<a id="何をしないか"></a>

## 何をしないか

- **証明書の取得それ自体** (TÜV / DO-330 監査) — 監査対応・LTS 保証・賠償責任は
  組織の問題で、工学では買えない。Ferrocene も AdaCore もこれを売る会社として存在する。
  カテゴリ確立後の商業オプションとして分離
- **フライトソフトウェア市場への参入** — 買い手はエージェント基盤。規制産業向けは
  dossier が揃ってからの選択肢
- **CompCert 級フルコンパイラ証明** — データ付きで棄却済み。CG-4 で代替

<a id="先行者の地図--何を借りるか"></a>

## 先行者の地図 — 何を借りるか

| 先行者 | 実績 | 借りるもの |
|---|---|---|
| SCADE KCG | DO-178C Level A 資格済みコード生成器 | 資格化資料の構成を参考にする。出力のレビューが不要になるとはこの repo から断定しない |
| Ferrocene | rustc を ISO 26262 ASIL D / IEC 61508 資格化 | **spec を先に書く** playbook (FLS)。資格化資料の公開スタイル |
| SPARK | DO-333 で証明をテスト代替として認めさせた | assurance ladder の設計 (evidence ladder は既に同型) |
| CompCert / seL4 | 検証済みコンパイラ / binary までの証明 | translation validation の発想 (CG-4) |
| MISRA / Ravenscar | 認証可能サブセット | profile は subset であって方言ではない原則 (CG-3) |

<a id="リスク"></a>

## リスク

| リスク | 対処 |
|---|---|
| ALS が実装の逆写経になり循環が形を変えて残る | interp を**先に**昇格し、ALS は interp に対して書く。native との不一致は ALS 側でなく実装側のバグとして扱う |
| カバレッジ工数の沼 | 安全パス限定の MC/DC から始め、全体は branch ratchet のみ |
| Critical profile が言語を二つに割る | subset 原則 (CG-3) を gate で強制 — Critical で valid なら通常モードでも valid |
| 認証ごっこ (誰も使わない dossier) | CG-5 は trust-layer の `make verify` と同一物の整形。独立の成果物を作らない |

<a id="組織実績のはしごbc-セクション"></a>

## 組織・実績のはしご(B/C セクション)

flight-grade プログラムのうち**エンジニアリングでない**部分 — 署名できる法人、
DER との折衝、LTS ポリシー、最初の実運用配備、サービス履歴の記録、機械筆記
コードの認証に関する立場表明 — は
[flight-organization.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md) に集約されている。
以前は issue #577–#585 として queue に並んでいたが、いずれも本文で
「エンジニアリング項目ではない」と自称しており、着手可能な作業の列に混ざると
queue が嘘をつく。umbrella issue #586 のチェックリストが引き続き進捗を持つ。
