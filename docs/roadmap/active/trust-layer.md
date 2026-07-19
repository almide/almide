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

| Level | 定義 | 証明手段 | Almide 現状 | 最近接の他者 |
|---|---|---|---|---|
| **L0 Contained** | サンドボックス内で実行される | ランタイム隔離 (wasmtime `--dir` 等) | ✅ | 全 wasm 言語 (Rust/Go/MoonBit) |
| **L1 Bounded** | バイナリが manifest を超える操作を**構造的に**できない | compile-time capability check + WASI import pruning + machine-readable manifest | 🔄 [capability Phase 1-2](effect-system-capability.md) | Deno (runtime flag のみ。コンパイル時証明・import pruning なし) |
| **L2 Reproducible** | 同一ソース → 任意ホストで byte-identical バイナリ。実行は決定的 | host-arch deterministic codegen gate (済) + Wasm 3.0 deterministic profile 宣言 ([frontier](wasm-platform-frontier.md)) + native↔wasm xtarget gate (済, 270 files / 0 exceptions) | 🔄 profile 宣言のみ未 | なし (cargo は既定で非再現、他言語に equivalence claim なし) |
| **L3 Verified** | ツールチェーンの安全性パス自体が機械検証済み。stdlib は oracle-paired | [Perceus-belt](almide-perceus-belt.md) Phase A (Lean) + oracle registry + grandfathered count 0 | 🔄 Phase B 済 / Phase A 大幅進捗（perceus_all_heap_freed 等の主要定理は証明済み、残項目は要確認） | なし (RustBelt は学術成果でツールチェーン gate ではない) |
| **L4 Measured** | 生成プロセス (LLM が書く工程) の品質が継続測定・公開されている | dojo daily MSR、モデル横断、公開ダッシュボード | 🔄 測定は稼働 / 公開形式未 | なし |

設計上の要点: **L1 が trust の技術的心臓**。L0 は「壁の外に出られない」だが、
L1 は「壁の中で何ができるかをバイナリ自体が証明する」。エージェント運用者が
load 前に manifest を policy と突き合わせられる — これが「レビューなしの信頼」の
工学的代替物。L2-L4 はその信頼を「誰の言葉も信じずに再導出できる」形にする。

## 既存 roadmap とのマッピング

このカテゴリ戦略は新規の実装 roadmap を**増やさない**。既存 roadmap の
完遂順序と公開形式を定義するメタ層である。

| Level | 担当 roadmap | 残作業 |
|---|---|---|
| L1 | [effect-system-capability](effect-system-capability.md) | Phase 1 (compile check) + Phase 2 (import pruning + manifest.json) |
| L2 | [wasm-platform-frontier](wasm-platform-frontier.md) | deterministic profile 明文化 (NaN 監査 + relaxed SIMD 不使用宣言) |
| L3 | [almide-perceus-belt](almide-perceus-belt.md) | Phase A (Lean 証明) |
| L4 | almide-dojo (別リポジトリ) | MSR の公開形式・第三者再現手順 |
| L2/L3 の認証級硬化 | [certification-grade](certification-grade.md) | CG-1〜CG-5 (ALS + 規範意味論 / coverage / Critical profile / translation validation / dossier) |
| 性能税ゼロ | [wasm-optimization-roadmap](wasm-optimization-roadmap.md) | 4 losses burndown → 11/11 |

性能の位置づけ: 性能はレベルではなく**前提条件の除去**。「信頼を取ると遅くなる」
というトレードオフが存在しない (11/11 vs Rust+LLVM) ことで、レベル表の claim が
条件付きでなくなる。

## Critical Path

依存関係順。perf burndown と Perceus Phase A は独立に並行可能。

1. **capability Phase 1+2** — L1 本体。`manifest.json` = 運用者が load 前に
   検証できる初めての「受領書」。Phase 1 単独でも
   "compile-time sandboxed AI agent containers" を claim できる
2. **deterministic profile 明文化** — frontier doc の TODO そのまま。小さく、
   L2 の仕様語彙が手に入る
3. **receipts harness (`make verify`)** — 全 claim を第三者が一コマンドで再現:
   xtarget byte-identity (270 files)、11 bench 比較、capability pruning
   (pruned import の不在を wasm-objdump で検証)、oracle divergence 0。
   **検証可能性を売る言語は、claim 自体が検証可能でなければならない**
4. **Trust Levels の外部公開** — このファイルを基準文書化して公開。
   他のツールチェーンが自己評価に使える形式にする (基準を使わせて勝つ)
5. **MCP stdlib module** ([capability Phase 5](effect-system-capability.md)) —
   配布のくさび。「Claude Code 設定 3 行で capability-bounded agent が動く」が
   カテゴリの最初のデモであり、買い手 (エージェント基盤開発者) への導線
6. (並行) **perf burndown** → 性能税ゼロ
7. (並行) **Perceus Phase A** → L3 完成

## 何をしないか

- **一般用途エコシステム競争** — ライブラリの広さで戦わない。必要表面積は
  stdlib 381 関数 + vetted `@inline_rust` packages (FFI はベンダー責務、
  ユーザー空間に unsafe は存在しない)
- **unsafe / ユーザー FFI / 手動メモリ制御** — 欠落ではなく L1-L3 の前提条件。
  「堀」として contract に明記する
- **基準の gerrymandering** — 各レベルは競合が部分達成できる正直な定義を保つ。
  Deno が L1 の半分を満たす事実を表に書くことが、基準の信頼性そのもの

## リスク (正直に)

| リスク | 性質 | 対処 |
|---|---|---|
| データ逆風 (LLM は Python/TS の海で訓練) | 本プロジェクト全体の賭け | dojo で賭けの成否が数字で見える。in-context (CHEATSHEET + skill) が事前学習の慣れに勝つかを測定し続ける |
| カテゴリ不成立 (無監督運用の市場が来ない) | タイミングリスク | エージェント基盤の現在の軌道では「いつ」の問題。受領書を先に積む時間競争と捉える |
| ラボ製 agent 言語の登場 | 最大の競合シナリオ | 機能は模倣可能だが方法論 (oracle pairing / ratchet / daily MSR / Lean) の retrofit は高くつく。先行の受領書がそのまま防御 |
