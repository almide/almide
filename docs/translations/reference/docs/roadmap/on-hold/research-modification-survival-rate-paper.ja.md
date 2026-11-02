# 研究：変更生存率の論文

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](research-modification-survival-rate-paper.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](research-modification-survival-rate-paper.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](research-modification-survival-rate-paper.zh-CN.md)



> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/research-modification-survival-rate-paper.md)の非規範的な読者向け翻訳です。自動同期されない歴史的な固定スナップショットです。状態・計画・件数は原文時点の記述であり、現在の実装や完了を立証しません。この翻訳では主張を独立に実行・検証していません。

> 原文の説明：言語間で LLM によるコード変更の生存率を測定する学術論文。

目標：arXiv プレプリント — *「LLM によるコード変更のためのプログラミング言語設計：反復編集を通じた生存率の測定」*（原題：*"Designing Programming Languages for LLM Code Modification: Measuring Survival Rate Across Iterative Edits"*）

### 過去のデータ収集計画

これは以前の論文構成案であり、リポジトリ内で稼働する収集ジョブではありません。[担当範囲の境界](../../../../../../CLAUDE.md)に記載されているとおり、MSR の全作業は現在 [Almide Dojo](https://github.com/almide/almide-dojo) に属します。ローカルの `research/benchmark/msr/` と `research/benchmark/framework/` のハーネスは2026-08-08に削除されました。以下の観察は歴史的な主張として保持します。新しい論文には、Dojo が管理するデータ、正確な実行／モデルの版、来歴が必要です。この文書は、外部で収集や公開が行われたことを立証しません。
- [x] 元のベンチマーク基盤（benchmark.rb、SPEC-v1/v2、テストスクリプト、複数言語。歴史的なもので、現在のローカル実行器ではない）
- [x] ベースラインデータ：16言語 × minigit タスク
- [ ] UFCS 導入後の Almide データ（v0.3.0以降、10回以上の試行）
- [ ] 前後比較：同じタスク、同じ LLM、UFCS 導入前と導入後
- [ ] 統計的有意性のために各言語10回以上の試行（理想は30回）

### アブレーション研究
- [ ] UFCS の有効／無効
- [ ] LLM 向けエラーメッセージの有効／無効
- [ ] CLAUDE.md テンプレートの提供あり／なし
- [ ] どの設計要因が生存率に最も寄与するかを測定

### 論文の構成
- [ ] 「変更生存率」の形式的定義（タスク仕様、変更手順、合否基準）
- [ ] 複数言語の比較（Almide、Python、Go、TS、Rust。同じ LLM、同じ試行）
- [ ] 分析：言語設計が変更の成功に影響するのはなぜか？
- [ ] 再現可能性：すべてのベンチマーク、仕様、テストスクリプトをオープンソースにする
