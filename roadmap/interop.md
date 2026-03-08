# 外部連携・インターオペラビリティ

## FFI（Foreign Function Interface）
Rustライブラリを直接呼び出す仕組み。

```almide
// 提案: extern宣言でRust関数をバインド
extern "rust" fn crypto_hash(data: String) -> String

// Cargo.tomlベースの依存追加と連携
```

## JavaScript interop（TSターゲット）
TS/JSの既存ライブラリを使えるようにする。

```almide
// 提案
extern "js" fn fetch(url: String) -> String
```

## WASM拡張
- WASI preview 2 対応
- ホストバインディング（Cloudflare Workers, Fastly Compute等）
- コンポーネントモデル対応

## C ABI
低レベルライブラリ（SQLite, OpenSSL等）を使うための仕組み。
Rustのunsafe FFI経由で実現可能。

## Priority
FFI（Rust） > JS interop > WASM拡張 > C ABI
