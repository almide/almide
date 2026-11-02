# LSP コードアクション — 残るリファクタリング

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](lsp-code-actions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](lsp-code-actions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](lsp-code-actions.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/lsp-code-actions.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットであり、自動同期されない。状態・実装・計測の記述は原文時点の記録で、翻訳時の独立検証や現行保証ではない。計画を実装済みとは扱わない。

説明: 提供済みの診断クイックフィックスを超える、生成型リファクタリングとインポート管理の残る対応範囲。

## 提供済みの基盤

LSP は `src/cli/lsp.rs` で `textDocument/codeAction` の対応を通知し、処理する。`src/cli/lsp_code_actions.rs` はすでに以下を提供する。

- 既知モジュールの限定された一覧に対する E003 インポート挿入。すべてのモジュールに対する一般的な自動インポート解決ではない。
- E006 による、囲んでいる関数の `effect fn` への変換。
- 診断の主要な修正案と代替案からのクイックフィックス編集。機械適用可能な修正には優先マークが付く。

`tests/lsp_test.rs::lsp_try_fix_reaches_client_as_code_action` と `lsp_machine_applicable_repair_is_the_preferred_quickfix` が修正の伝達を固定して検証する。新たな診断修正を追加するために、同じ編集を LSP 専用にもう一度実装する必要はない。

## 残る提案 — 保留中

- 現在の限定的な E003 一覧ではなく、コンパイラのモジュール情報から導く、より広いインポート管理。
- 初期化式のエフェクトを保持する、安全な未使用束縛リファクタリング。
- 網羅性チェッカーのケース集合からの、不足した match アームの生成。
- 検査済みの型からの、関数シグネチャへの型注釈挿入。

これらは対応範囲と生成型リファクタリングの目標であり、すべてのコードアクションが存在しないという主張ではない。各アクションを利用可能と説明する前に、エディタープロトコルの回帰テストと、編集の受理・拒否テストが必要である。優先度と作業再開の基準は [arena 突破計画](../../../../../roadmap/active/arena-breakthroughs.md) にある。

## 設計の参考資料

Gleam の `code_action.rs` はインポート管理、パターン抽出、注釈挿入の例を提供する。Nickel の `analysis.rs` は、型と契約を認識したエディター支援を比較検討する材料である。これらは設計の参考資料であり、Almide の現在の対応範囲を示す主張ではない。
