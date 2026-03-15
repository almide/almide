# B. 実用化に必要なもの

## 1. 11 stdlib ランタイム未実装

**状態:** 以下のモジュールが `runtime/rust/src/` に存在しない

| モジュール | TOML 定義数 | 難易度 | 依存 |
|-----------|------------|--------|------|
| io | 3 | Low | stdin/stdout |
| log | 8 | Low | eprintln |
| random | 4 | Low | rand crate or std |
| uuid | 6 | Low | format |
| json | 36 | Medium | serde_json or 手書き parser (value.rs に既存) |
| regex | 8 | Medium | regex crate |
| datetime | 21 | Medium | chrono crate or std::time |
| fs | 24 | Medium | std::fs |
| http | 26 | Hard | reqwest or ureq |
| crypto | 4 | Hard | sha2/hmac crate |

**修正方針:** TOML 定義は全てある。`runtime/rust/src/<module>.rs` を作成し、`src/emit_rust/lower_rust.rs` の include_str に追加
**見積り:** io/log/random/uuid = 1日、json/regex/datetime/fs = 3日、http/crypto = 2日

## 2. let-polymorphism の Rust codegen

**状態:** チェッカーは `let f = (x) => x; f(1); f("hello")` を通す。Rust closure は monomorphic なのでコンパイルエラー
**修正方針:** polymorphic な let binding を Rust のジェネリック関数に変換

```rust
// Almide: let f = (x) => x
// 現在: let f = move |x| x;  (monomorphic)
// 目標: fn f<T>(x: T) -> T { x }  (generic)
```

**条件:** let binding の型に未解決 TypeVar がある場合のみ。monomorphic binding はそのまま closure
**見積り:** 2-3日

## 3. WASM target

**状態:** CLI に `--target wasm` があるが動作未確認
**修正方針:** `almide build app.almd --target wasm` で .wasm を生成。wasm-pack or rustc --target wasm32
**見積り:** 1-2日（Rust codegen が正しければ rustc に渡すだけ）
