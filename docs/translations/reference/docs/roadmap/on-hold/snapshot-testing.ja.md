# ファイルベースのスナップショットテスト — 保留

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](snapshot-testing.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](snapshot-testing.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](snapshot-testing.zh-CN.md)



> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/snapshot-testing.md)の非規範的な読者向け翻訳です。自動同期されない歴史的な固定スナップショットです。状態・計画・件数は原文時点の記述であり、現在の実装や完了を立証しません。この翻訳では主張を独立に実行・検証していません。

> 原文の説明：提供済みのソースリテラルによるスナップショットアサーションを超える、任意のファイルベーススナップショット。

## すでに利用可能なもの

`testing.assert_snapshot(actual, expected)` は出力を期待する**ソースリテラル**と比較します。`almide test --update-snapshots <file>` はそのリテラルを書き換えることで、新しい値や変化した値を受け入れます。通常のテスト実行は書き込まず、CI モードは受け入れを拒否します。付随するスナップショットファイルはありません。

現在の API と受け入れ規則は [CLI リファレンス](../../../../../specs/cli.md#スナップショット-testingassert_snapshot) にあります。実装：`stdlib/testing.almd` と `src/cli/commands.rs::cmd_test_update_snapshots`。根拠：`spec/stdlib/testing_snapshot_test.almd` と `tests/snapshot_update_test.rs`（正確な書き換え、受け入れの反復、通常の失敗、CI の書き込み拒否）。

## 残る設計上の問い

元の提案は期待する出力を別ファイルに保存するものでした。これは提供済みのリテラルベースのアサーションとは異なる機能で、保留のままです。外部スナップショットファイルを必要とする具体的な用途がある場合にのみ再検討します。その際は、既存のリテラルベースのテストを変えずに、ファイル命名、レビュー可能な受け入れ、古いファイルの扱い、CI の書き込み拒否を定義します。

以前の `snapshot(result)` という案は提案であり、実装済み API ではありません。設計上の参考は引き続き MoonBit の `inspect` の期待値と Gleam のファイルベースのテストスナップショットです。どちらも Almide が付随ファイルの契約を持つことを意味しません。
