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

## Testing Rules

Changes to the compiler MUST be verified against **all targets**:

1. **Rust target**: `almide run test.almd` (compile to Rust → rustc → execute)
2. **TypeScript target**: `almide test.almd --target ts | deno run --allow-all -`
3. **JavaScript target**: Verify JS runtime in `emit_ts.rs` (`RUNTIME_JS`) matches Deno runtime (`RUNTIME`)

When adding or modifying stdlib functions:
- Add to **both** `emit_rust.rs` AND `emit_ts.rs` (both RUNTIME and RUNTIME_JS)
- Add to UFCS resolution (`resolve_ufcs_module`) in both emitters
- Add to module recognition (`is_module` check) if new module
- Test with a `.almd` file that exercises the new function

When modifying codegen (emit_rust.rs / emit_ts.rs):
- Test ownership: variables used after `for...in` must still work
- Test effect fn: `fs.read_text()` inside effect fn must compile without manual `?`
- Test that generated Rust compiles without warnings (except unused macros)

## Key Design Decisions

- **Multi-target**: Same AST emits to Rust or TypeScript via `--target rust|ts`
- **Result erasure (TS)**: `ok(x)` → `x`, `err(e)` → `throw new Error(e)`
- **Effect fn (Rust)**: `effect fn` → `Result<T, String>`, auto `?` propagation
- **`==`/`!=`**: Deep equality in TS (`__deep_eq`), `almide_eq!` macro in Rust
- **`++`**: Concatenation for both strings and lists (polymorphic in both targets)
- **`do` block**: With guard → loop. Without guard → auto error propagation block.
