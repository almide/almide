<!-- description: Unify grammar definitions into a single source of truth -->
<!-- done: 2026-03-18 -->
# Grammar Codegen: Single Source of Truth [P1]

## Problem

Almide's grammar is scattered across 3 locations:

| Location | Format | Purpose |
|----------|--------|---------|
| `src/parser/` + `src/lexer.rs` | Hand-written Rust | Compiler core |
| `tree-sitter-almide/grammar.js` | Hand-written JS | Editor parsing, syntax highlighting |
| (not yet implemented) vscode-almide TextMate | JSON | VSCode syntax highlighting |

Every keyword addition or operator change requires manual synchronization across all locations. Since the `stdlib/defs/*.toml` → `build.rs` → `src/generated/` pattern has been successful for stdlib, we apply the same approach to grammar.

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

### Phase 1: tokens.toml — Centralized Keyword/Operator Management

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

**Generated artifacts:**
- `src/generated/token_table.rs` — lexer keyword HashMap, TokenType enum
- `tree-sitter-almide/` keywords section
- TextMate grammar keyword/operator scopes

**Effect:** adding a keyword requires editing only one file

### Phase 2: precedence.toml — Operator Precedence

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

**Generated artifacts:**
- tree-sitter `prec.left()` / `prec.right()` configuration
- Parser precedence table (for verification — consistency checks against hand-written parser)

### Phase 3: rules.toml — Grammar Rules (future)

Declarative description of grammar rules. A PEG/BNF-style DSL. Since the design becomes large at this point, we decide after evaluating Phase 1-2 results.

## Implementation Order

1. Create `grammar/tokens.toml`
2. Add token table generation to `build.rs` (coexisting with existing stdlib generation)
3. Change lexer.rs keyword HashMap to read from `src/generated/token_table.rs`
4. Output tree-sitter grammar.js keyword section via generation script
5. TextMate grammar generation
6. Phase 2 (precedence.toml)

## Priority

**P1** — tree-sitter と vscode-almide が動き始めた今、同期コストが現実の問題になる。Phase 1 だけでも大きな効果。

## Reference

| Project | Approach |
|---------|----------|
| **Almide stdlib** | `stdlib/defs/*.toml` → `build.rs` → `src/generated/` — applying the same pattern to grammar |
| **Rust (rustc)** | Keyword list centralized in `rustc_span::symbol`, generated via macros |
| **Swift** | `gyb` (Generate Your Boilerplate) generates from token definitions |
| **TypeScript** | Keyword table in `src/compiler/scanner.ts`, TextMate hand-written |
