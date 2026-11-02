<a id="native-performance-measurement"></a>

# ネイティブ性能の測定

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-native-perf.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](almide-native-perf.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-native-perf.reader.zh-CN.md)

> [正本の英語コマンド資料](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-native-perf.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範的な履歴参照用スナップショット。この不活性な参考資料はエージェントコマンドではなく、記載された指示の実行権限を与えず、自動同期もされない。翻訳のために以下のコマンドは実行しておらず、独立した測定や現時点の検証を行ったとは主張しない。

サンプルのネイティブ CLI を測定する。公開する際は、コンパイラ／ビルド、ホスト、日付の来歴情報を必ず付ける。

<a id="steps"></a>

## 手順

1. 測定スクリプトを実行する：
   ```bash
   bash research/benchmark/perf/native/measure.sh
   ```

2. `docs/benchmarks/README.md` またはそこからリンクされた台帳に、対応する日付付き測定があれば比較する。以前のトップ README の該当節は削除されている。

3. 該当する測定の元データを更新し、必要に応じて派生ブロックを再生成する。生成された表を手作業で編集しない。

4. コミット：`Update native performance stats: {brief summary}`

<a id="what-it-measures"></a>

## 測定対象

- **バイナリサイズ**：実際の CLI アプリ（minigit、約 350 LOC）の strip 済みネイティブバイナリ
- **実行時間**：CLI バイナリに対する 100 回の init+add+commit 操作
- **依存関係**：スクリプトはリテラルのゼロを出力する。リンク状況の測定ではない。静的リンクや依存関係について主張する前に、生成された成果物を調べること。

<a id="source"></a>

## ソース

- CLI アプリ：`research/benchmark/perf/native/cli_app.almd`
- スクリプト：`research/benchmark/perf/native/measure.sh`
