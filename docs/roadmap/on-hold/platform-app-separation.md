<!-- description: Platform/application separation for dual-target compilation -->
# Platform/Application Separation

プラットフォーム（ランタイム層）とアプリケーションを分離し、WASM + native のデュアルターゲットを効率化する。

## 参考

- **Roc**: `parse/header.rs` — `PlatformHeader` で requires/provides/exposes を宣言
  - プラットフォームは一度コンパイルすれば複数アプリで再利用
  - アプリはプラットフォームの型にだけ依存

## ゴール

- プラットフォームが WASM/native 両方のバイナリを提供
- アプリはターゲットを意識せず書ける
- プラットフォームの切り替えでデプロイ先を変更
