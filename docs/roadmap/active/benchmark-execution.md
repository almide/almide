<!-- description: LLM accuracy benchmarks comparing Almide, Python, and MoonBit -->
# LLM Benchmark Execution

**優先度:** 高 — Almide の存在意義「LLM が最も正確に書ける言語」の実証
**前提:** benchmark/ フレームワーク + msr/ ツール構築済み

---

## 初期結果 (2026-03-25, Haiku, n=1)

| Language | Score | Cheatsheet | Training data |
|----------|-------|------------|---------------|
| **Almide** | 24/24 (100%) | Yes (449 lines) | Near zero |
| **Python** | 25/25 (100%) | No | Massive |
| **MoonBit** | 24/24 (100%) | No | Limited |

所要時間: Almide ~11分, Python ~6分, MoonBit ~12分

**結論:** この難度では差がつかない。modification survival rate や より難しい問題が必要。
ただし「Almide は学習データなし + CHEATSHEET だけで Python と同等」は事実として確立。

## ツール

- `research/benchmark/msr/msr.almd` — Almide 用 MSR ランナー（Almide 自身で記述）
- `research/benchmark/msr/python/run.sh` — Python 用（25 問プロンプト付き）
- `research/benchmark/msr/moonbit/run.sh` — MoonBit 用（25 問プロンプト付き）
- `research/benchmark/framework/runner.py` — 汎用フレームワーク（FAR/MSR/FLE 対応、未使用）

## 次のステップ

### Phase 1: n=10 反復実行
- [ ] 同じ 24 問を各言語 10 回実行して安定性を測定
- [ ] 1 回は全部通っても、10 回中の成功率で差が出る可能性

### Phase 2: Modification Survival Rate
- [ ] 模範解答を渡して変更指示を出す（例:「戻り値を Result 型に変更して」）
- [ ] Almide の effect fn / 型システムの強みが活きる変更カテゴリ:
  - 戻り値型変更 (String → Result[String, E])
  - variant case 追加 (exhaustiveness check が全 match を指摘)
  - record field 追加 (コンパイラが全使用箇所を報告)
- [ ] この指標で Almide と Python/MoonBit の差を定量化

### Phase 3: より難しい問題
- [ ] 複数モジュール連携、エラー伝播チェーン、generics の複合使用
- [ ] 実プロジェクト規模（数百行）の問題追加

### Phase 4: 分析・公開
- [ ] 集計、統計的有意性検定 (Fisher exact test)
- [ ] README / サイトへの結果掲載
