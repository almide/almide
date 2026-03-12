# Mission

**Almide is the language LLMs can write most accurately.** Every design decision serves one metric: modification survival rate.

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

Almide is a programming language (.almd files) compiled via a pure-Rust compiler with multi-target codegen.

See [ARCHITECTURE.md](./docs/ARCHITECTURE.md) for the full compiler pipeline and module map.

### Module Structure

```
src/
├── main.rs              CLI dispatch, compile pipeline
├── cli.rs               Commands: run, build, test, check, fmt, clean, init
├── ast.rs               AST types (Program, Decl, Expr, Stmt, TypeExpr)
├── lexer.rs             Tokenizer
├── parser/              Recursive descent parser (7 files)
├── resolve.rs           Import resolution
├── check/               Type checker (5 files)
├── types.rs             Internal type system (Ty, TypeEnv, FnSig)
├── diagnostic.rs        Error reporting with file/line and hints
├── stdlib.rs            UFCS resolution, module registry
├── generated/           Auto-generated from stdlib/defs/*.toml (DO NOT EDIT)
├── emit_common.rs       Shared codegen utilities
├── emit_rust/           Rust code generation (5 files)
├── emit_ts/             TypeScript code generation (4 files)
├── emit_ts_runtime.rs   Embedded JS/TS runtime
├── fmt.rs               Code formatter (AST → source)
└── project.rs           almide.toml, dependency management
stdlib/defs/             TOML stdlib definitions (14 modules, 203 functions)
```

`build.rs` reads `stdlib/defs/*.toml` at compile time and generates type signatures + codegen dispatch into `src/generated/`. See [stdlib/README.md](./stdlib/README.md) for the full spec.

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
- Implement the runtime in `src/emit_rust/core_runtime.txt` (and/or `src/emit_ts_runtime.rs`)
- Add UFCS mapping to `stdlib.rs` `resolve_ufcs_candidates` (if method-callable)
- `cargo build` auto-generates all codegen — no manual `stdlib.rs` or `calls.rs` edits needed
- Write a test in `spec/stdlib/` (as `*_test.almd` or inline `test` block)

When modifying codegen:
- Test ownership: variables used after `for...in` must still work
- Test effect fn: `fs.read_text()` inside effect fn must compile without manual `?`
- Test that generated Rust compiles without warnings

## Key Design Decisions

- **Multi-target**: Same AST emits to Rust or TypeScript via `--target rust|ts`
- **Result erasure (TS)**: `ok(x)` → `x`, `err(e)` → `throw new Error(e)`
- **Effect fn (Rust)**: `effect fn` → `Result<T, String>`, auto `?` propagation
- **`==`/`!=`**: Deep equality in TS (`__deep_eq`), `almide_eq!` macro in Rust
- **`++`**: Concatenation for both strings and lists
- **`do` block**: With guard → loop. Without guard → auto error propagation block.
- **Diagnostics**: Every error includes file:line, context, and actionable hint
