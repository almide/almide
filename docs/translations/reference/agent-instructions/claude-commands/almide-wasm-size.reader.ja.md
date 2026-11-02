<a id="wasm-binary-size-measurement"></a>

# WASM バイナリサイズの測定

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-wasm-size.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](almide-wasm-size.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-wasm-size.reader.zh-CN.md)

> [正本の英語コマンド資料](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-wasm-size.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範的な履歴参照用スナップショット。この不活性な参考資料はエージェントコマンドではなく、記載された指示の実行権限を与えず、自動同期もされない。翻訳のために以下のコマンドは実行しておらず、独立した測定や現時点の検証を行ったとは主張しない。

これらのサンプルの WASM バイナリサイズを測定する。測定値は、生成された Hello, world の基準値とは区別して扱う。

<a id="steps"></a>

## 手順

1. 測定スクリプトを実行する：
   ```bash
   bash research/benchmark/perf/wasm-size/measure.sh
   ```

2. `docs/benchmarks/README.md` とそこからリンクされた台帳の、対応する日付付きの記録と比較する。以前のトップ README の該当節は削除されている。

3. 公開する Hello, world の基準値には `scripts/gen-readme-stats.sh --measure` を使う。他のサンプルの結果は個別に日付を付けて保管し、生成ブロックを上書きしない。

4. コミット：`Update WASM binary sizes: {brief summary}`

<a id="source-files"></a>

## ソースファイル

`research/benchmark/perf/wasm-size/` に配置：
- `hello.almd` — Hello World
- `fizzbuzz.almd` — FizzBuzz
- `fibonacci.almd` — フィボナッチ
- `closure.almd` — クロージャ（高階関数）
- `variant.almd` — バリアント（代数的データ型＋パターンマッチ）

<a id="notes"></a>

## 注記

- すべてのバイナリは自己完結型（アロケータ、文字列処理、ランタイムを含む）
- ゲスト側の所有権管理に外部 GC は不要。ただし、ホスト操作の実行には、互換性のある Wasm ランタイムと成果物のインポートが必要。
- `almide build --target wasm` でビルド
