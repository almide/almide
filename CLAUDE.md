# Mission

**Almide is the language LLMs can write most accurately.** Every design decision serves one metric: modification survival rate.

# Critical Safety Rules

- **NEVER run `git checkout`, `git restore`, or `git stash` on files you did not modify yourself.** Other agents may be working on those files concurrently. Reverting their changes destroys their work and cannot be recovered.
- **NEVER run destructive git operations without explicit user confirmation.** This includes `git reset`, `git checkout -- <file>`, `git clean`, and `git stash drop`.
- **If you see unexpected changes in `git status`, ASK the user before touching them.** They may belong to another agent or an in-progress task.

# Project Rules

## Branch Strategy

- **main** — protected. Never commit directly. Only accepts PRs from `develop`
- **develop** — the working branch. All commits go here
- Always confirm `git branch` before committing

## Git Commit Rules

- Write commit messages in **English only**
- No prefix (feat:, fix:, etc.)
- Keep it to one concise line
- Focus on what changed, not why
- Commit messages must be in **English only** (enforced by `english-only` commit-msg hook)

## Development Setup

After cloning, install the git hooks:

```bash
brew install lefthook  # or: https://github.com/evilmartians/lefthook
lefthook install
```

## Project Overview

Almide is a programming language (.almd files) compiled via a pure-Rust compiler with multi-target codegen (Rust, TypeScript, WASM).

- **Architecture**: [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — compiler pipeline, module map
- **Language reference**: [docs/CHEATSHEET.md](./docs/CHEATSHEET.md) — syntax, stdlib, idioms (for AI code generation)
- **Stdlib spec**: [docs/STDLIB-SPEC.md](./docs/STDLIB-SPEC.md) — 381 functions across 22 modules

## Building & Usage

```bash
cargo build --release

almide run app.almd              # Compile + execute
almide build app.almd -o app     # Build binary
almide build app.almd --target wasm  # Build WASM
almide test                      # Find all .almd with test blocks (recursive)
almide test spec/lang/           # Run tests in a directory
almide test spec/lang/expr_test.almd  # Run a single test file
almide test --run "pattern"      # Filter tests by name
almide check app.almd            # Type check only
almide fmt app.almd              # Format source
almide clean                     # Clear dependency cache
almide app.almd --target rust    # Emit Rust source
almide app.almd --target ts      # Emit TypeScript source
almide app.almd --emit-ast       # Emit AST as JSON
```

## Test Structure

`almide test` recursively finds all `.almd` files containing `test` blocks.

- **Inline tests**: Write `test "name" { }` in any `.almd` file
- **Test files**: Use `*_test.almd` suffix for dedicated test files (convention)

```
spec/
├── lang/            Language feature tests (*_test.almd)
├── stdlib/          Stdlib tests (*_test.almd)
└── integration/     Multi-module / integration tests
tests/               Rust compiler unit tests (.rs, Cargo auto-discovery)
```

Run tests:
```bash
almide test                      # All .almd with test blocks (recursive)
almide test spec/lang/           # Language tests only
almide test spec/stdlib/         # Stdlib tests only
```

## Testing Rules

Changes to the compiler MUST be verified against **all exercises and tests**:

```bash
almide test
```

When adding or modifying stdlib functions:
- Add/edit the definition in `stdlib/defs/<module>.toml` (type sig + codegen templates)
- Implement the Rust runtime in `runtime/rust/<module>.rs`
- Implement the TS runtime in `runtime/ts/<module>.ts`
- `cargo build` auto-generates all codegen dispatch — no manual edits needed
- Write a test in `spec/stdlib/` (as `*_test.almd` or inline `test` block)

When modifying codegen:
- Test ownership: variables used after `for...in` must still work
- Test effect fn: `fs.read_text()` inside effect fn must compile without manual `?`
- Test that generated Rust compiles without warnings

## Key Design Decisions

- **Multi-target**: Same IR emits to Rust, TypeScript, or WASM via `--target rust|ts|wasm`
- **Codegen v3**: Nanopass pipeline (semantic rewrites) + TOML template renderer (syntax)
- **Result erasure (TS)**: `ok(x)` → `x`, `err(e)` → `throw new Error(e)`
- **Effect fn (Rust)**: `effect fn` → `Result<T, String>`, auto `?` propagation
- **`==`/`!=`**: Deep equality in TS (`__deep_eq`), `almide_eq!` macro in Rust
- **`+`**: Concatenation for strings and lists (overloaded with addition)
- **`do` block**: With guard → loop. Without guard → auto error propagation block.
- **Diagnostics**: Every error includes file:line, context, and actionable hint
