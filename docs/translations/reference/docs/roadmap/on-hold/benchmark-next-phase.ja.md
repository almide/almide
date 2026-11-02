<a id="llm-benchmark-next-phase"></a>

# LLM ベンチマーク：次のフェーズ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](benchmark-next-phase.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](benchmark-next-phase.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](benchmark-next-phase.zh-CN.md)

> 非規範の固定読者用スナップショット。原文を正本とし、自動同期せず、実装・実測・安全性を独立に再検証したものではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/benchmark-next-phase.md)
> 過去の観察は実行／モデル版をこの記録に持たない。旧 n≥10 目標、検定案、工数見積もり、期待する優位を、統計的十分性・独立性・現在の約束・実証された優位と区別する。再開実験は Dojo が管理し、旧ローカルハーネスを復活させるものではない。


> 原文の説明：LLM ベンチマーク Phase 2-3：言語間比較、難しい問題、公開

これは以前のベンチマーク／公開計画と、当時報告された Phase 1 の観察を記録するものであり、現在実行可能なベンチマーク手引きではない。[CLAUDE.md](../../../../../../CLAUDE.md) に従い、MSR の収集は [Almide Dojo](https://github.com/almide/almide-dojo) が担当する。ローカルハーネスは 2026-08-08 に削除された。この研究計画を再利用するには、競合するローカル MSR 値を再作成するのではなく、Dojo 管理の実行と来歴が必要である。

---

<a id="historical-reported-data-runmodel-versions-not-recorded-here"></a>

## 過去の報告データ（実行／モデルの版はここに記録されていない）

| 言語 | モデル | n | 平均 MSR | 範囲 |
|----------|-------|---|----------|-------|
| Almide | Haiku | 14 | 92.1% | 80–100% |
| Almide | Sonnet | 9 | 92.2% | 70–100% |
| Python | Haiku | 3 | 88.9% | 77–100% |

**不足：** Python データは統計比較には不足（n=3）。n≥10 が必要。

---

<a id="phase-2a-python-n10-cross-language-comparison"></a>

## Phase 2A：Python n=10（言語間比較）

**目標：** Python に対する Almide の MSR 優位が統計的に有意か判断する。

**タスク：**
1. Haiku で Python MSR ベンチマークを n=10 実行
   - 元のスクリプト位置：`research/benchmark/msr/python/modifications/run.sh`（削除済み、現行コマンドではない）。新しい実行は Dojo で定義する。
   - 約 30 分（10 回 × 9 タスク × 1 タスク約 20 秒）
2. Sonnet で Python MSR ベンチマークを n=10 実行
3. タスク単位および集約で Fisher の正確確率検定を適用
4. 報告：信頼区間、p 値、効果量

**当初の仮説：** 約 8–12% の差。これは実測結果でも検出力の保証でもない。収集前に標本単位と分析を選ぶ。繰り返されたタスク／実行が自動的に独立観測になるわけではない。

**リスク：** Python も 90%+ に達すれば、この難度で差は有意にならないかもしれない。それも有効な知見であり、区別するにはより難しい問題が必要ということになる。

---

<a id="phase-2b-harder-problems"></a>

## Phase 2B：より難しい問題

**目標：** Almide の型システムがより大きな測定可能な優位をもたらす変更タスクを作る。

**型システムに負荷をかける問題分類：**

| 分類 | Almide が勝つと考える理由 | 例 |
|----------|----------------------|---------|
| **複数モジュールのリファクタリング** | コンパイラが全呼び出し箇所を検出。Python は grep に依存 | 3 モジュールで使う型を改名 |
| **エラー伝播チェーン** | 明示的な `Result` 処理／`!` 伝播と手動 try/except の比較 | 4 関数を伝播する新たなエラーケースを追加 |
| **ジェネリック型の変更** | `List[A]` → `Map[String, A]`。全利用側に更新が必要 | 関数の返り値型を List から Map に変更 |
| **網羅性の強制** | variant 追加で影響する非網羅的 match が露呈する（ワイルドカード match は網羅的なままの場合がある） | 4 関数で使う 5 ケースの enum に 3 variant を追加 |
| **必須レコードフィールド追加** | 型検査器が欠けたフィールドを検出 | 10 箇所で使うレコードに必須フィールドを追加 |
| **effect 境界を越える変更** | pure fn が誤って effect fn を呼ぶとコンパイルエラー | IO を専用 effect fn に移すリファクタリング |

**難度区分：**
- Tier 1（元の基準）：単一ファイル、50-80 行、1-2 個の型変更 — MSR 約 92%
- Tier 2（目標）：単一ファイル、100-200 行、3-5 個の連鎖変更
- Tier 3（挑戦）：複数ファイル、200-500 行、アーキテクチャ変更

**実装：**
1. Tier 2 タスクを 10 件作る（v1 解＋変更指示＋v2 テスト）
2. Dojo 管理のタスク集に追加する。`research/benchmark/msr/modifications/prompts/` は削除されたローカル位置
3. Almide と Python の両方で n=10 を実行
4. 難度別 MSR を比較

---

<a id="phase-3-publication"></a>

## Phase 3：公開

**目標：** 測定したタスク、モデル、言語の版に範囲を限定した再現可能な比較を公開する。否定的または結論の出ない結果も含める。

**成果物：**
1. **README 節** — 主要数値を含む 3 文の要約
2. **ブログ記事／技術報告** — 方法、生データ、統計分析
3. **再現パッケージ** — スクリプト＋プロンプト＋生出力を Dojo 管理の実験とともに公開

**統計手法：**
- タスク別：Fisher の正確確率検定（2×2 表：合否 × 言語）
- 集約：対応のある比率検定（McNemar または層別 Fisher）
- 報告：p 値、差の 95% CI、効果量（Cohen's h）
- 透明性：全 LLM 生出力をコミットし、完全な再現性を確保

**信頼性チェックリスト：**
- [ ] 全条件で n ≥ 10
- [ ] 複数モデル（最低 Haiku＋Sonnet）
- [ ] 複数の難度
- [ ] 同一タスク構造での Python 比較
- [ ] 生データを公開
- [ ] p 値を報告した統計検定

---

<a id="execution-order"></a>

## 実行順序

1. **Phase 2A** が先 — Dojo に実験を再構築する。元のローカル基盤は削除済み
2. **Phase 2B** — 問題設計が必要（1-2 日）
3. **Phase 3** — データが揃った後

**当初の工数見積もり：** 3–5 日。現在の約束ではない。
**再開前に：** Dojo の担当を合意し、過去の生データを探し、新しい実験を定義する。ここでは外部の実行が現在どの状態かを主張しない。
