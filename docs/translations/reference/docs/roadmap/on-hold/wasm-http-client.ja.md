# WASM HTTP クライアント

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-http-client.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](wasm-http-client.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-http-client.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-http-client.md)の非規範的な読者用翻訳です。原文を正本とし、自動同期は行いません。




> 非規範の読者向け翻訳。原文コミット `35297a612fb29e3f78f6587aba253aa91d51f466` の固定スナップショットであり、自動同期されない。状態・実装・計測の記述は原文時点の記録で、翻訳時の独立検証や現行保証ではない。計画を実装済みとは扱わない。

説明: 提供済みの wasm HTTP クライアント経路と、残る既定コンポーネントへの移行。

## 提供済みの機能範囲

関数ごとの正本は [target-availability.toml](../../../../../../proofs/target-availability.toml) である。現在の API は [HTTP リファレンス](../../../../../stdlib/http.md) にある。古い機能群の名前や wasm ターゲットが存在することから利用可否を推測してはならない。

- 完全なレスポンスを扱うクライアント `http.get_response` と `http.send` は、WASI 0.3 コンポーネント経路と `almide run --target wasm` の組み込みホストで動作する。以前公開されていた `http.request_response` という名前は `http.send(http.new_request(...))` に置き換えられている。
- 既定の stock-p1 成果物にはまだ HTTP インポートがなく、これらの呼び出しを E081 で拒否する。p3 成果物は `ALMIDE_COMPONENT_P3=1` と `--component` でビルドし、`wasmtime run -S http=y` で実行する。
- 利用可否は操作ごとに異なる。完全レスポンスに関する主張をすべての HTTP API に広げず、呼び出しハンドルとサーバー側の提供機能はマトリクスで個別に確認する。

根拠: [http_response_cross_test.rs](../../../../../../tests/http_response_cross_test.rs) は完全レスポンスのステータス、ヘッダー、本文、失敗をネイティブと比較する。[target-availability.toml](../../../../../../proofs/target-availability.toml) は `http.get_response` と `http.send` を `stock-p1` でのみ利用不可と記録している。

## 残る計画

クライアントは未実装機能ではない。残るデプロイ移行は [ADR-0023 の実装計画](../../../../../adr/0023-wasm-http-over-wasi-http.md#7-implementation-plan) に属し、既定のコンポーネントへの切り替えとプライベートホスト経路の廃止を含む。このページは ADR のドキュメント作業が参照するパスに残っているものであり、2 つ目の実装チェックリストではない。個々の段階を判断する際は、この計画の歴史的な API 名ではなく、マトリクスと現在のテストに従う。
