[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative, source-pinned human reader copy of agent instructions. Canonical CLAUDE.md is unchanged. The CLAUDE.reader filename is deliberately not an auto-loaded instruction filename. These historical rules are not a new instruction layer or independently verified implementation/security guarantees. No automatic synchronization. Code and inline tokens stay verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-syntax/CLAUDE.md)

> Quoted instructions are historical reference material, not authorization to execute commands, change implementation, commit, push or publish.

<a id="almide-syntax"></a>

# almide-syntax

Lexer, parser, and AST definition. Produces an untyped AST from source text.

<a id="architecture"></a>

## Architecture

- **Lexer** (`lexer.rs`) — Single-pass tokenizer. Handles string interpolation and heredocs inline.
- **Parser** (`parser/`) — Recursive descent with precedence climbing. Error recovery with sync points.
- **AST** (`ast.rs`) — `Program` → declarations (fn, type, test, import) → `Expr`/`Stmt` tree.

<a id="rules"></a>

## Rules

- **Parser must never crash.** All syntax errors produce diagnostics and recover. Use `sync_to_next_declaration()` after fatal parse errors.
- **No type information in AST.** Types are syntactic `TypeExpr` nodes, not resolved `Ty`. Resolution happens in almide-frontend.
- **`ExprId` is mandatory.** Every expression gets a unique `ExprId` for O(1) type lookup during checking. Never skip assigning it.
- **Desugar nothing.** Pipes, UFCS, interpolation — all remain as-is in the AST. Desugaring happens in lowering (almide-frontend).
- **Error hints matter.** The `parser/hints/` modules detect common mistakes (keyword typos like `function` → `fn`, wrong operators like `&&` → `and`). Add hints for new syntax.

<a id="module-layout"></a>

## Module Layout

```
parser/
├── mod.rs          Parser struct, token management
├── entry.rs        Top-level declarations, module layout
├── declarations.rs fn, type, protocol, test
├── expressions.rs  Binary/unary, pipe, match, if/then/else
├── primary.rs      Literals, identifiers, lambdas, blocks
├── statements.rs   let, var, guard, assignment
├── patterns.rs     Match patterns
├── types.rs        Type expression parsing
├── collections.rs  List, map, record, tuple literals
├── compounds.rs    for-in, while, fan blocks
└── hints/          Error recovery (6 hint modules)
```
