[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-lang-bench.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](almide-lang-bench.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-lang-bench.reader.zh-CN.md)

> 過去のリポジトリコマンドを読むための非活性・非規範スナップショット。正本は変更しない。この参照パスと .reader 名はエージェントコマンドを登録しない。引用された指示は資料であり、実行・支出・公開・push・削除の権限ではない。自動同期せず、現在の workflow や測定主張を独立検証したものではない。コードとコメントは原文を保持する。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-lang-bench.md)

<a id="lang-bench-chart-generation"></a>

# Lang Bench チャートの生成

AI Coding Language Benchmark のチャートを再生成し、必要なら Almide の試行を追加する。

<a id="regenerate-charts-only"></a>

## チャートだけを再生成する

`data.json` が既に最新なら：

```bash
python3 research/benchmark/lang-bench/plot.py
```

`docs/figures/lang-bench-{time,loc,pass-rate}.png` を出力する。スクリプトは対応する README 画像 URL のキャッシュ無効化も試みる。現在の先頭 README にこのチャート欄はないため、この処理は何もしない場合がある。

<a id="run-additional-almide-trials"></a>

## Almide の試行を追加する

```bash
# One-time setup
git submodule update --init research/benchmark/lang-bench/upstream

# Run N trials (each ~20-30 min, appended to raw/almide.jsonl)
ruby research/benchmark/lang-bench/runner.rb --trials 10

# Recompute Almide stats in data.json
ruby research/benchmark/lang-bench/aggregate.rb

# Regenerate charts
python3 research/benchmark/lang-bench/plot.py
```

上の原文コメントは順に「初回設定」「N 回試行（各約20～30分、raw/almide.jsonl に追記）」「data.json の Almide 統計を再計算」「チャートを再生成」を意味する。

詳細は `research/benchmark/lang-bench/README.md`。

<a id="data-source"></a>

## データの出典

- `research/benchmark/lang-bench/data.json` — 全言語の集計統計（plot.py の入力）
- `research/benchmark/lang-bench/raw/almide.jsonl` — 追記専用の Almide 生試行記録
- 他の15言語は [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) が出典（Opus 4.6、各20試行）
- ローカル runner は版を指定しない `sonnet` エイリアスを要求する。解決されたモデルと実行の来歴を記録する。このエイリアスだけでは固定モデル版も、学習データへの包含に関する何らかの事実も確立できない。

<a id="notes"></a>

## 注記

- 現在の公開測定は `docs/benchmarks/README.md` にある。日付付きスナップショットのチャートは、ここで再生成するライブチャートとは別物。来歴を対応付けて記録せずに、過去の比較を新しい実行で置き換えない。
- Almide はオレンジ、他の言語は青で強調する。
- コミットメッセージ：`Update lang-bench charts: {brief summary}`
