# Project Rules

## Git Commit Rules

- Write commit messages in **English only**
- No prefix (feat:, fix:, etc.)
- Keep it to one concise line
- Focus on what changed, not why

## Project Overview

Almide is a programming language (.almd files) transpiled to TypeScript and run on Deno.

- `src/parser.ts` — Parser (tokens → AST)
- `src/codegen.ts` — Code generator (AST → TypeScript), includes runtime
- `src/almide.ts` — CLI entry point
- `CHEATSHEET.md` — Language quick reference (used by LLM agents to write Almide)
- `exercises/` — Exercism-style benchmark exercises with tests

## Testing

```bash
# Run a single exercise
bash exercises/run_exercise.sh exercises/<name>/<name>.almd

# Transpile only
deno run --allow-read src/almide.ts <file.almd>
```

## Key Design Decisions

- **Result erasure**: `ok(x)` → `x`, `err(e)` → `throw new Error(e)`. Match with `err(e)` pattern generates try-catch.
- **`do` block**: With guard → `while(true)` loop. Without guard → auto error propagation block.
- **`==`/`!=`** use deep equality (`__deep_eq`), `/` is integer division (`Math.trunc`).
- **`++`** is concatenation for both strings and lists (`__concat`).
