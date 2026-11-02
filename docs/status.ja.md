# プロジェクトの現状

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./status.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](./status.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](./status.zh-CN.md)

Almide の現在地をまとめています。記録時点が付いた集計値（標準ライブラリの規模、テストファイル、契約、ミューテーションスコア）は[計測ページ](./benchmarks/README.md#derived-counts)に表示され、CI が各台帳と照合します。

| 分類 | 状況 |
|----------|--------|
| 成熟度 | 1.0 未満で、`develop` 上で活発に開発中。LLM 向けの表面仕様の変更は、[STABILITY.md](./STABILITY.md) に従い、移行診断を伴う dialect epoch を通してのみ行います（2026-08-20 宣言） |
| サポート | 1.0 未満では最新リリース系列のみ。方針とバージョニングの保証：[SUPPORT.md](../SUPPORT.ja.md) · 脆弱性：[SECURITY.md](../SECURITY.ja.md) |
| コンパイラ | 純 Rust 製コンパイラ。配布物には独立した証明書検査器 `almide-verify` も同梱します |
| ターゲット | Rust（ネイティブ）、WASM（直接出力する structural leg。[仕組み](./how-it-works.ja.md)を参照） |
| 検証付きコード生成 | structural wasm 経路は、ビルドごとに所有権、境界、名前、ケイパビリティの検証用証拠（witness）を出力します。`almide verify` は、配布される wasm ファイルに SHA-256 で結び付けられたバンドルを検査します（#4011）。これは意味的な正しさや成果物全体のメモリ安全性の証明ではありません。対象範囲の限界と残る信頼上の前提は、[証明済みの事項と信頼に依存する事項](./contracts/proven-vs-trusted.md)および[メモリ安全性の不足事項一覧](./design/MEMORY-SAFETY.md#the-gap-table-2026-10-24-4011)に明記しています。旧 MIR→WAT レンダラーは #2761 で廃止されました |
| コード生成 | Rust：Nanopass + TOML テンプレート。wasm：structural engine → 直接出力（v0 エミッターと旧 MIR→WAT レンダラーは廃止済み。未対応の境界はエラーとなり、フォールバックはしません） |
| 標準ライブラリの検査 | 標準ライブラリの公開関数すべてに、[`proofs/stdlib-inspection.toml`](../proofs/stdlib-inspection.toml) の `inspected` 印が付いています。検査内容は、ネイティブと wasm で同一のバイト列、宣言した計算量の範囲内に収まる呼び出しごとのコスト、codopsy A、新たにファジングした組合せ、ダイヤモンド型のモジュール依存を経由した呼び出しです。2026-10-19 時点（almide 0.68.0）で未検査は 0 件。`tests/stdlib_inspection_test.rs` はシグネチャまたは本体の変更時に印を無効にし、未検査件数は減少方向にのみ許容されます |
| 成果物 | `almide compile` による `.almdi` モジュールインターフェースファイル |
| Playground | [公開中](https://almide.github.io/playground/) — ブラウザー内でコンパイラが WASM として動作します |

リリース履歴：[CHANGELOG.md](../CHANGELOG.md) · 計画：[ロードマップ](./roadmap/README.md)。
