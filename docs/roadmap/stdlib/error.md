<!-- description: Structured error types with wrapping, chaining, and context -->
# stdlib: error [Tier 1]

構造化エラー型。現在 Almide の `Result[T, E]` のエラー型は常に `String`。エラーの分類・チェーン・コンテキスト付加ができない。

## 他言語比較

| 機能 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| エラー型 | `error` interface | `Exception` class hierarchy | `std::error::Error` trait | `Error` class |
| エラーラップ | `fmt.Errorf("%w", err)` | `raise X from Y` | `.context()` (anyhow) | `new Error(msg, {cause})` |
| エラーチェーン | `errors.Unwrap(err)` | `__cause__` chain | `.source()` | `.cause` |
| エラー判定 | `errors.Is(err, target)` | `isinstance(e, Type)` | downcast / match | `instanceof` |
| カスタムエラー | `type MyErr struct` | `class MyError(Exception)` | `#[derive(Error)]` (thiserror) | `class MyError extends Error` |
| スタックトレース | ❌ (pkg/errors) | ✅ built-in | `RUST_BACKTRACE=1` | ✅ built-in |
| エラーメッセージ | `err.Error()` | `str(e)` | `e.to_string()` | `e.message` |

## Almide の現状

```almide
effect fn read_config(path: String) -> Result[Config, String] =
  do {
    let text = fs.read_text(path)  // エラーは "file not found" みたいな文字列
    ok(parse(text))
  }
```

`String` エラーの問題:
- パターンマッチでエラー種別を分岐できない
- エラーチェーン（「X が失敗した、原因は Y」）ができない
- エラーにコンテキスト（ファイルパス等）を付加する統一的な方法がない

## 設計方針

Almide の variant 型を活かし、ユーザー定義エラー型をそのまま `Result[T, E]` の `E` に使う。

```almide
type FileError =
  | NotFound { path: String }
  | PermissionDenied { path: String }
  | IoError { message: String }

effect fn read_config(path: String) -> Result[Config, FileError] =
  do {
    guard fs.exists?(path) else err(NotFound { path })
    let text = fs.read_text(path)
    ok(parse(text))
  }
```

### stdlib で提供するもの

```almide
// エラーにコンテキストを追加
fn error.context(result: Result[T, E], msg: String) -> Result[T, String]

// エラーメッセージ取得（variant → String 変換）
fn error.message(e: E) -> String

// エラーチェーン
fn error.chain(outer: String, inner: String) -> String
// → "outer: inner"
```

## 追加候補 (~10 関数)

### P0
- `error.context(result, msg) -> Result[T, String]` — エラーにコンテキスト追加
- `error.message(e) -> String` — エラー → 文字列
- `error.chain(outer, cause) -> String` — エラーチェーン

### P1
- `error.is?(result, pattern) -> Bool` — エラー種別判定
- `error.map_err(result, f) -> Result[T, E2]` — エラー型変換
- `error.or_else(result, f) -> Result[T, E]` — エラー時のフォールバック

### P2 (将来: union types 前提)
- `Result[T, NotFound | Timeout]` — union error types で型安全なエラー分岐

## 実装戦略

Phase 1 は TOML + runtime で `error.context` 等を追加。Phase 2 は type-system.md の Structured Error Types + Union Types と連動。

## 前提条件

- type-system.md の Union Types（Tier 2 以降で本領発揮）
- `Result[T, E]` の `E` が `String` 以外を許容する codegen 対応
