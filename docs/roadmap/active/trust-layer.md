<!-- description: Category strategy — winning "the trust layer for machine-written software": MWS Trust Levels, receipts, critical path -->
# Trust Layer — 機械が書くソフトウェアの信頼層

> **Goal**: 「エージェントが書いたコードを、人間レビューなしに近い信頼度で運用できる」
> というカテゴリの測定基準 (MWS Trust Levels) を定義し、Almide をその reference
> implementation にする。
> **Active scope**: Levels の定義公開 + L1 ([capability Phase 1-2](effect-system-capability.md)) + receipts harness。
> **Exit criteria**: 第三者が `git clone && make verify` で L1-L2 の全 claim を再現でき、
> MCP クライアントから capability-bounded Almide agent が設定 3 行で動く。

## なぜこのカテゴリが立つか

コードの書き手がエージェントに移ると、言語の選定者は開発者個人から
**エージェント運用者**に変わり、選定基準が変わる:

| 人間が書く時代の基準 | 機械が書く時代の基準 |
|---|---|
| 慣れ・求人・表現力 | 修正生存率 (MSR) |
| ライブラリの広さ | blast radius の証明可能性 |
| ビルドの速さ | ビルド・実行の再現性 |
| ピーク性能 | レビューなしで運用できるか |

右列は Almide の設計メトリクスそのもの。設計目的関数と購買基準が一致している
言語は他にない。このカテゴリでの勝ち方は言語間比較ではなく、
**測定基準を定義する側に回る**こと — SLSA が supply chain security でやった構図。
基準を他者が自己評価に使い始めた時点でカテゴリは確立し、その基準の
reference implementation がデフォルトになる。

## MWS Trust Levels (L0–L4)

各レベルは「運用者が*検証なしに*何を信頼できるか」で定義する。
**gerrymander しない** — 各レベルは Almide 抜きでも業界にとって意味を持つこと。
恣意的な基準は採用されず、採用されない基準はカテゴリを作らない。

| Level | 目標 | 現行の根拠と限界 |
|---|---|---|
| **L0 Contained** | 実行時の隔離 | Wasm host の import/設定/許可に依存。native build や外部コードを自動的に sandbox 化する主張ではない |
| **L1 Bounded** | 宣言した能力の制限 | capability checker と artifact/host 境界を組み合わせる。共有 import の操作や host の実装まで、import の有無だけで証明しない |
| **L2 Reproducible** | 指定条件での再現・観測比較 | `proofs/receipt.sh` の C-REPRO/C-SAFE と fixture/decline 範囲を参照。任意ホスト・任意入力の不変性ではない |
| **L3 Verified** | 明示した安全性モデルの機械証明 | 現行 Rocq/structural RC 証拠と trusted boundary は receipt が正本。全コンパイラパスや全 stdlib の証明ではない |
| **L4 Measured** | LLM 編集品質の測定 | Dojo 所有。公開結果のモデル/課題/版に限定し、この repo だけから外部測定の最新状態を断定しない |

> **この L0-L4 は trust の梯子で、`docs/specs/edit-locality.md` の L1-L3
> （編集フレーム / クロスターゲット一致 / 診断局所性）とは別物**。番号は共有して
> いるが主張は無関係で、どちらかがどちらかの前提になってもいない。文脈で判らない
> ところでは、どちらの梯子かを書くこと。

設計上の要点: **L1 が trust の技術的心臓**。L0 は「壁の外に出られない」だが、
L1 は declared capability と実際の artifact/host 設定を照合する目標である。
manifest の比較は運用者の判断を助けるが、ホスト・native dependency・FFI の
意味を含む全レビューを置き換えない。L2-L4 も各証拠の scope と trusted base を伴う。

## 既存 roadmap とのマッピング

このカテゴリ戦略は新規の実装 roadmap を**増やさない**。既存 roadmap の
完遂順序と公開形式を定義するメタ層である。

| Level | 担当 roadmap | 残作業 |
|---|---|---|
| L1 | [effect-system-capability](effect-system-capability.md) | 既存 compile check と、artifact/host 境界の残提案を分ける |
| L2 | [wasm-platform-frontier](wasm-platform-frontier.md) | deterministic profile 明文化 (NaN 監査 + relaxed SIMD 不使用宣言) |
| L3 | `proofs/receipt.sh` / `TRUSTED_BASE.md` | 現行 Rocq/structural の証明範囲と未証明の接続を明示 |
| L4 | almide-dojo (別リポジトリ) | MSR の公開形式・第三者再現手順 |
| L2/L3 の認証級硬化 | [certification-grade](certification-grade.md) | CG-1〜CG-5 (ALS + 規範意味論 / coverage / Critical profile / translation validation / dossier) |
| 性能測定（保証ではない） | [wasm-optimization-roadmap](../done/wasm-optimization-roadmap.md) | 4 losses burndown → 11/11 |

性能比較は測定した workload・版・設定の範囲に限る。元の 11/11 の結果から、
安全性機構のコストが常にゼロ、または全レベルの主張が無条件とは言えない。

## Critical Path

依存関係順。perf burndown と Perceus Phase A は独立に並行可能。

1. **capability Phase 1+2** — L1 本体。`manifest.json` = 運用者が load 前に
   検証できる初めての「受領書」。Phase 1 単独でも
   「宣言能力の compile-time 検査」を説明できるが、実行・build の sandbox 全体を保証しない
2. **deterministic profile 明文化** — frontier doc の TODO そのまま。小さく、
   L2 の仕様語彙が手に入る
3. **receipts harness (`make verify-trust` / `make receipt`、既存)** — receipt に列挙された検査・証拠を、それぞれの宣言スコープで再導出:
   receipt の claim ごとの根拠・対象 fixture・decline・trusted boundary を再導出する。
   元の 62 files / 11 bench / oracle registry の数値は現在の網羅性一覧ではない。
   **検証可能性を売る言語は、claim 自体が検証可能でなければならない**
4. **Trust Levels の外部公開** — このファイルを基準文書化して公開。
   他のツールチェーンが自己評価に使える形式にする (基準を使わせて勝つ)
5. **MCP stdlib module** ([capability Phase 5](effect-system-capability.md)) —
   配布のくさび。「Claude Code 設定 3 行で capability-bounded agent が動く」が
   カテゴリの最初のデモであり、買い手 (エージェント基盤開発者) への導線
6. (並行) **perf burndown** → workload ごとの測定で改善を確認
7. (並行) **証明範囲の拡張** → L3 の対象と未証明境界を更新（全体完了とはしない）

## 何をしないか

- **一般用途エコシステム競争** — ライブラリの広さだけで競わない。現行 API は
  [stdlib index](../../stdlib/README.md) を参照し、固定の旧関数数を使わない。
- **FFI/native の境界** — `ffi` や native dependency は実在するため、
  「ユーザー FFI が存在しない」を L1-L3 の前提にしない。利用時の許可・信頼範囲は
  [package system](../../specs/package-system.md) と現行 CLI 仕様に従う。
- **基準の gerrymandering** — 各レベルは競合が部分達成できる正直な定義を保つ。
  Deno が L1 の半分を満たす事実を表に書くことが、基準の信頼性そのもの

## リスク (正直に)

| リスク | 性質 | 対処 |
|---|---|---|
| データ逆風 (LLM は Python/TS の海で訓練) | 本プロジェクト全体の賭け | dojo で賭けの成否が数字で見える。in-context (CHEATSHEET + skill) が事前学習の慣れに勝つかを測定し続ける |
| カテゴリ不成立 (無監督運用の市場が来ない) | タイミングリスク | エージェント基盤の現在の軌道では「いつ」の問題。受領書を先に積む時間競争と捉える |
| ラボ製 agent 言語の登場 | 最大の競合シナリオ | 機能は模倣可能だが方法論 (oracle pairing / ratchet / daily MSR / Lean) の retrofit は高くつく。先行の受領書がそのまま防御 |
