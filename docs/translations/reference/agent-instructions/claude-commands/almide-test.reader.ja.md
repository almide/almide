<a id="tiered-test-suite"></a>

# 段階別テストスイート

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-test.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](almide-test.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-test.reader.zh-CN.md)

> [正本の英語コマンド資料](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-test.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範的な履歴参照用スナップショット。この不活性な参考資料はエージェントコマンドではなく、記載された指示の実行権限を与えず、自動同期もされない。翻訳のために以下のコマンドは実行しておらず、独立した測定や現時点の検証を行ったとは主張しない。

Almide の全テストスイートを階層順に実行し、最初に失敗した段階で停止する。

<a id="steps"></a>

## 手順

1. **cargo test** — Rust コンパイラの単体テスト
   ```bash
   cargo test
   ```
   いずれかのテストが失敗したら停止する。

2. **lang テスト** — 言語機能のテスト（デフォルトのテスト経路）
   ```bash
   almide test spec/lang/
   ```
   いずれかのテストが失敗したら停止する。

3. **stdlib テスト** — 標準ライブラリのテスト（デフォルトのテスト経路）
   ```bash
   almide test spec/stdlib/
   ```
   いずれかのテストが失敗したら停止する。

4. **integration テスト** — 複数モジュール／統合テスト（デフォルトのテスト経路）
   ```bash
   almide test spec/integration/
   ```
   いずれかのテストが失敗したら停止する。

5. **WASM テスト** — WASM ターゲットでの全テスト
   ```bash
   almide test --target wasm
   ```
   成功／失敗／スキップを報告する。

6. **要約** — 各段階の結果を報告する：
   - cargo test: 成功／失敗
   - lang: N ファイル成功
   - stdlib: N ファイル成功
   - integration: N ファイル成功
   - WASM: N 成功、N 失敗、N スキップ

<a id="notes"></a>

## 注記

- コンパイラのソースを変更した場合は、実行前に必ず `make install` を行う
- ある段階で失敗した場合は、次の段階に進む前に原因を調べて修正する
- 報告された操作と `proofs/target-availability.toml` を使って Wasm の壁／スキップを分類する。fs/io を一括して未対応のカテゴリと扱わない。デフォルトのテスト経路では Wasm の壁に対して native にフォールバックする場合がある。バックエンドを切り分ける際は、明示的なターゲットオプションを使う。
