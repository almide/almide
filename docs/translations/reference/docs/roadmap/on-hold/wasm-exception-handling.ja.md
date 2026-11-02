<a id="wasm-exception-handling"></a>

# WASM の例外処理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-exception-handling.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](wasm-exception-handling.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-exception-handling.zh-CN.md)



> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-exception-handling.md)の非規範的な読者向け翻訳です。自動同期されない歴史的な固定スナップショットです。状態・計画・件数は原文時点の記述であり、現在の実装や完了を立証しません。この翻訳では主張を独立に実行・検証していません。

> 原文の説明：Wasm の例外への変換案。上流の既定値はバージョンとエンジンに依存します。

<a id="proposed-lowering"></a>

## 提案する変換

現在の Result 表現と明示的な `!` 処理の代わりに、ネイティブの `try_table` / `throw` / `exnref` を使うエラー伝播を検討します。元の計画の `?` ごとの用語は古い構文に属します。表現、割り当て、性能上の利点には実際の変換実装と測定が必要です。この記録は Almide に EH が実装されたとは主張しません。

<a id="upstream-prerequisite-update-2026-10-31"></a>

## 上流の前提条件の更新（2026-10-31）

「Wasmtime は既定で EH を無効にする」という一律のブロッカーは古くなりました。Wasmtime 50.0.1 用に固定したソースでは、`Config::wasm_exceptions` は `Strategy::Winch` を除いて既定で有効です。Winch では既定で無効です。この API は `gc` ビルド機能で制御され、設定には GC サポートが必要です。[固定した設定ソース](https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/config.rs#L1438-L1453)を参照してください。これはソースと設定の観察であり、実行テストではありません。

Wasmtime だけから Spin、wasmCloud、Docker/Wasm、その他の埋め込みホストの既定値を推測してはいけません。各ホストのバージョン、エンジン戦略、ビルド機能、設定を確認する必要があります。提供経路を変更する前には、Almide 自身のツール／依存関係の固定版との比較も必要です。

<a id="resume-criteria"></a>

## 再開の条件

必要なホスト対応範囲、エラー／catch の意味、ターゲット間のフィクスチャ、代替経路の必要性を再評価します。上流の既定値が変わっただけでは、この機能は完成せず、新しい言語の意味論の採用も許可されません。[上流の追跡 issue](https://github.com/bytecodealliance/wasmtime/issues/3427) は歴史的背景であり、固定ソースとホストのテストの代わりにはなりません。
