# Effect Isolation (Security Layer 1)

pure fn は I/O 不可能。コンパイラが静的に検証。

## 設計

```
fn parse(s: String) -> Value = ...          // pure。I/O 不可能
effect fn load(path: String) -> String = ... // I/O 可能
```

- `fn` は `effect fn` を呼べない。コンパイラが検証
- pure fn は外界に一切アクセスできない。データ窃取も外部通信も型エラー
- **セキュリティ上の意味**: パッケージが pure fn しか export してなければ、そのパッケージは原理的に無害
- stdlib effect fn も同様（`fs.read_text` 等は pure fn から呼ぶとエラー）
- `fan` ブロックも pure fn 内ではエラー

## 実装

- チェッカー: `src/check/calls.rs` — `sig.is_effect && !self.env.in_effect` でエラー
- テスト: `tests/checker_test.rs` — 7 tests (pure→effect, effect→effect, test→effect, stdlib effect, fan in pure/effect)
