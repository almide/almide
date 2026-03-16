# Security Model

Almide のセキュリティは 5 層で構成される。各層が独立に機能し、全層が揃うと supply chain 含めた構造的安全性が成立する。

## Layer 1: Effect Isolation — pure fn は I/O 不可能

```
fn parse(s: String) -> Value = ...          // pure。I/O 不可能
effect fn load(path: String) -> String = ... // I/O 可能
```

- `fn` は `effect fn` を呼べない。コンパイラが検証
- pure fn は外界に一切アクセスできない。データ窃取も外部通信も型エラー
- **セキュリティ上の意味**: パッケージが pure fn しか export してなければ、そのパッケージは原理的に無害

### 実装状況

- [x] チェッカーで pure fn → effect fn 呼び出しをエラーにする (`src/check/calls.rs`)
- [x] Rust unit テスト追加 (`tests/checker_test.rs` — 5 tests)
- [ ] stdlib effect fn も検証済み（`fs.read_text` 等が pure fn から呼ぶとエラー）
- [ ] 高階関数経由の effect 漏れ防止（将来課題）

## Layer 2–5: TBD
