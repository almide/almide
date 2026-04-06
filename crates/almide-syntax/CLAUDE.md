# almide-syntax

Lexer, parser, and AST definition. Produces an untyped AST from source text.

## Architecture

- **Lexer** (`lexer.rs`) — Single-pass tokenizer. Handles string interpolation and heredocs inline.
- **Parser** (`parser/`) — Recursive descent with precedence climbing. Error recovery with sync points.
- **AST** (`ast.rs`) — `Program` → declarations (fn, type, test, import) → `Expr`/`Stmt` tree.

## Rules

- **Parser must never crash.** All syntax errors produce diagnostics and recover. Use `sync_to_next_declaration()` after fatal parse errors.
- **No type information in AST.** Types are syntactic `TypeExpr` nodes, not resolved `Ty`. Resolution happens in almide-frontend.
- **`ExprId` is mandatory.** Every expression gets a unique `ExprId` for O(1) type lookup during checking. Never skip assigning it.
- **Desugar nothing.** Pipes, UFCS, interpolation — all remain as-is in the AST. Desugaring happens in lowering (almide-frontend).
- **Error hints matter.** The `parser/hints/` modules detect common mistakes (keyword typos like `function` → `fn`, wrong operators like `&&` → `and`). Add hints for new syntax.

## Module Layout

```
parser/
├── mod.rs          Parser struct, token management
├── entry.rs        Top-level declarations, module layout
├── declarations.rs fn, type, trait, impl, test
├── expressions.rs  Binary/unary, pipe, match, if/then/else
├── primary.rs      Literals, identifiers, lambdas, blocks
├── statements.rs   let, var, guard, assignment
├── patterns.rs     Match patterns
├── types.rs        Type expression parsing
├── collections.rs  List, map, record, tuple literals
├── compounds.rs    for-in, while, fan blocks
└── hints/          Error recovery (6 hint modules)
```
