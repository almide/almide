# Project Rules

## Git Commit Rules

- Write commit messages in **English only**
- No prefix (feat:, fix:, etc.)
- Keep it to one concise line
- Focus on what changed, not why

## Project Overview

Almide is a programming language (.almd files) compiled via a pure-Rust compiler with multi-target codegen.

- `src/lexer.rs` — Lexer (source → tokens)
- `src/parser.rs` — Parser (tokens → AST)
- `src/ast.rs` — AST type definitions
- `src/emit_rust.rs` — Rust code generation
- `src/emit_ts.rs` — TypeScript code generation (Deno runtime)
- `src/main.rs` — CLI entry point
- `CHEATSHEET.md` — Language quick reference (used by LLM agents to write Almide)
- `exercises/` — Exercism-style benchmark exercises with tests

## Building & Usage

```bash
cargo build --release

# Compile to Rust (default)
almide input.almd > output.rs
rustc output.rs -o output

# Compile to TypeScript
almide input.almd --target ts > output.ts
deno run --allow-all output.ts

# Emit AST as JSON
almide input.almd --emit-ast
```

## Key Design Decisions

- **Multi-target**: Same AST emits to Rust or TypeScript via `--target rust|ts`
- **Result erasure (TS)**: `ok(x)` → `x`, `err(e)` → `throw new Error(e)`
- **Effect fn (Rust)**: `effect fn` → `Result<T, String>`, auto `?` propagation
- **`==`/`!=`**: Deep equality in TS (`__deep_eq`), `almide_eq!` macro in Rust
- **`++`**: Concatenation for both strings and lists (polymorphic in both targets)
- **`do` block**: With guard → loop. Without guard → auto error propagation block.
