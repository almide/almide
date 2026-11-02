[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-msr.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](almide-msr.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-msr.reader.zh-CN.md)

> 過去のリポジトリコマンドを読むための非活性・非規範スナップショット。正本は変更しない。この参照パスと .reader 名はエージェントコマンドを登録しない。引用された指示は資料であり、実行・支出・公開・push・削除の権限ではない。自動同期せず、現在の workflow や測定主張を独立検証したものではない。コードとコメントは原文を保持する。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-msr.md)

<a id="msr-modification-survival-rate-benchmark"></a>

# MSR（Modification Survival Rate）ベンチマーク

**MSR はここではなく [almide/almide-dojo](https://github.com/almide/almide-dojo) で測定する。**

この repo の担当はコンパイラの正しさ：`spec/` テスト、`cargo test`、grammar-lab、lang-bench。Dojo は日次 MSR 測定、課題バンク、悪意あるヒントの検出、診断フィードバックループを担当する。この repo の測定ページ（`docs/benchmarks/README.md`）の数値は *Dojo が*公開する。ここで再生成しない。

以前 `research/benchmark/msr/` にあったローカル harness（`claude --model haiku` で動かす Exercism 形式の25課題）は 2026-08-08 に削除された。2026年4月の Dojo への引き継ぎ以降、`results/` は空だったため、実行すると Dojo が測定した README 数値をローカル測定値で上書きしてしまう。git 履歴から復元できる：

```bash
git log --diff-filter=D -- research/benchmark/msr/
```

<a id="to-run-msr"></a>

## MSR を実行するには

Dojo を clone し、その README に従う：

```bash
gh repo clone almide/almide-dojo
```

<a id="still-in-this-repo"></a>

## この repo に残るもの

- `/almide-lang-bench` — lang-bench のチャート生成（`research/benchmark/lang-bench/`）
- `/almide-native-perf` — native 性能（`research/benchmark/perf/`、`scripts/check-perf-ratio.sh` がゲート）
- `/almide-wasm-size` — WASM バイナリサイズ
- `/almide-test` — 階層化テストスイート
