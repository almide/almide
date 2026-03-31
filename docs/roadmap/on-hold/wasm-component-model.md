<!-- description: WebAssembly Component Model support with WIT bindings -->
# WASM Component Model

WebAssembly Component Model と WIT (WebAssembly Interface Types) をサポートし、言語間の相互運用を実現する。

## 参考

- **MoonBit**: `wit-bindgen` でバインディング自動生成、`--derive-error` フラグ

## ゴール

- WIT ファイルからバインディング自動生成
- コンポーネント境界での型安全な呼び出し
- 既存の WASI 対応との統合
