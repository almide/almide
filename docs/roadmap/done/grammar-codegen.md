<!-- description: Unify grammar definitions into a single source of truth -->
# Grammar Codegen: Single Source of Truth [P1]

## Problem

Almideの文法が3箇所に分散している:

| 場所 | 形式 | 用途 |
|------|------|------|
| `src/parser/` + `src/lexer.rs` | Rust手書き | コンパイラ本体 |
| `tree-sitter-almide/grammar.js` | JS手書き | エディタパース、構文ハイライト |
| (未実装) vscode-almide TextMate | JSON | VSCode シンタックスハイライト |

キーワード追加・演算子変更のたびに全箇所を手動で同期する必要がある。stdlibで `stdlib/defs/*.toml` → `build.rs` → `src/generated/` のパターンが成功しているので、文法にも同じアプローチを適用する。

## Design

```
grammar/
├── tokens.toml         # キーワード、演算子、デリミタの定義
├── precedence.toml     # 演算子優先順位テーブル
└── rules.toml          # 文法規則 (宣言、式、パターン等)

build.rs (or standalone tool) が生成:
├── tree-sitter-almide/grammar.js
├── vscode-almide/syntaxes/almide.tmLanguage.json
└── src/generated/token_table.rs
```

### Phase 1: tokens.toml — キーワード・演算子の一元管理

```toml
# grammar/tokens.toml

[keywords]
control = ["if", "then", "else", "match", "for", "in", "while", "do", "guard"]
declaration = ["fn", "type", "trait", "impl", "let", "var", "test", "import", "module"]
modifier = ["pub", "local", "mod", "effect", "async", "strict", "deriving"]
value = ["true", "false", "none", "some", "ok", "err", "todo", "not", "and", "or"]
flow = ["try", "await", "break", "continue"]

[operators]
arithmetic = ["+", "-", "*", "/", "%", "^"]
comparison = ["==", "!=", "<", ">", "<=", ">="]
assignment = ["="]
other = ["++", "|>", "..", "..=", "=>", "->", "@", "_"]

[delimiters]
open  = ["(", "[", "{"]
close = [")", "]", "}"]
separator = [",", ":", ";", "."]
```

**生成物:**
- `src/generated/token_table.rs` — lexerのキーワードHashMap、TokenType enum
- `tree-sitter-almide/` の keywords セクション
- TextMate grammar の keyword/operator スコープ

**効果:** キーワード追加が1ファイルの編集で完結

### Phase 2: precedence.toml — 演算子優先順位

```toml
# grammar/precedence.toml

[[level]]
name = "pipe"
operators = ["|>"]
associativity = "left"

[[level]]
name = "or"
operators = ["or"]
associativity = "left"

[[level]]
name = "and"
operators = ["and"]
associativity = "left"

[[level]]
name = "comparison"
operators = ["==", "!=", "<", ">", "<=", ">="]
associativity = "left"

[[level]]
name = "range"
operators = ["..", "..="]
associativity = "none"

[[level]]
name = "additive"
operators = ["+", "-", "++"]
associativity = "left"

[[level]]
name = "multiplicative"
operators = ["*", "/", "%", "^"]
associativity = "left"

[[level]]
name = "unary"
operators = ["-", "not"]
associativity = "right"
```

**生成物:**
- tree-sitter の `prec.left()` / `prec.right()` 設定
- パーサーの優先順位テーブル（検証用 — 手書きパーサーとの整合性チェック）

### Phase 3: rules.toml — 文法規則 (将来)

文法規則の宣言的記述。PEG/BNF的なDSL。ここまで来ると設計が大きくなるので、Phase 1-2 の成果を見てから判断。

## Implementation Order

1. `grammar/tokens.toml` 作成
2. `build.rs` にトークンテーブル生成を追加 (既存のstdlib生成と共存)
3. lexer.rs のキーワードHashMap を `src/generated/token_table.rs` から読むように変更
4. tree-sitter grammar.js のキーワード部分を生成スクリプトで出力
5. TextMate grammar 生成
6. Phase 2 (precedence.toml)

## Priority

**P1** — tree-sitter と vscode-almide が動き始めた今、同期コストが現実の問題になる。Phase 1 だけでも大きな効果。

## Reference

| Project | Approach |
|---------|----------|
| **Almide stdlib** | `stdlib/defs/*.toml` → `build.rs` → `src/generated/` — 同じパターンを文法に適用 |
| **Rust (rustc)** | キーワードリストは `rustc_span::symbol` に一元管理、マクロで生成 |
| **Swift** | `gyb` (Generate Your Boilerplate) でトークン定義から生成 |
| **TypeScript** | `src/compiler/scanner.ts` にキーワードテーブル、TextMate は手書き |
