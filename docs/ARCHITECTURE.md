# Architecture

Almide is a ~7,600-line pure-Rust compiler. Zero runtime dependencies — `serde` and `serde_json` are the only crates, used for AST serialization.

## Pipeline

```
Source (.almd)
    │
    ▼
┌─────────┐     Tokens with line/col
│  Lexer  │──────────────────────────┐
└─────────┘                          │
    │                                │
    ▼                                │
┌─────────┐     AST (Program)       │
│ Parser  │──────────────────────┐  │
└─────────┘                      │  │
    │                            │  │
    ▼                            │  │
┌──────────┐   Resolved modules  │  │
│ Resolver │─────────────────┐   │  │
└──────────┘                 │   │  │
    │                        │   │  │
    ▼                        ▼   ▼  │
┌─────────┐          Diagnostics    │
│ Checker │  ◄── source text ───────┘
└─────────┘     (for error display)
    │
    ▼
┌──────────────┐
│   Emitter    │
│  ┌────────┐  │
│  │  Rust  │  │──▶  .rs  ──▶  rustc  ──▶  native binary / WASM
│  ├────────┤  │
│  │   TS   │  │──▶  .ts  ──▶  deno
│  ├────────┤  │
│  │   JS   │  │──▶  .js  ──▶  node
│  └────────┘  │
└──────────────┘
```

## Module Map

```
src/
├── main.rs              CLI dispatch, file loading, compile pipeline
├── cli.rs               Command implementations (run, build, test, check, fmt, clean, init)
├── ast.rs               AST types (Program, Decl, Expr, Stmt, TypeExpr, Pattern)
├── lexer.rs             Tokenizer — newline-sensitive, keywords, interpolated strings
├── parser/              Recursive descent parser
│   ├── mod.rs           Parser struct, token navigation
│   ├── declarations.rs  module, import, fn, type, test, trait, impl
│   ├── expressions.rs   Precedence climbing (pipe > or > and > compare > add > mul)
│   ├── primary.rs       Literals, identifiers, error recovery hints
│   ├── compounds.rs     if/match/lambda/do/block/list/for-in
│   ├── statements.rs    let, var, assign, guard, expr-stmt
│   ├── patterns.rs      Pattern matching (wildcard, constructor, record, some/none/ok/err)
│   ├── types.rs         Type expressions (Simple, Generic, Record, Fn, Variant)
│   └── helpers.rs       Utilities (skip_newlines, peek, expect)
├── resolve.rs           Import resolution, circular dependency detection
├── check/               Type checker — every error has an actionable hint
│   ├── mod.rs           Checker struct, decl registration, type resolution
│   ├── expressions.rs   Expression type inference
│   ├── calls.rs         Function call dispatch (direct, constructor, module, UFCS)
│   ├── operators.rs     Binary/unary operator type rules
│   └── statements.rs    Statement checking, pattern binding
├── types.rs             Internal type representation (Ty enum, TypeEnv, FnSig)
├── diagnostic.rs        Structured errors with file/line, hint, source display
├── stdlib.rs            Centralized stdlib definitions (signatures, UFCS, modules)
├── emit_common.rs       Shared codegen utilities (sanitize)
├── emit_rust/           Rust code generation
│   ├── mod.rs           Emitter struct, EmitOptions, entry points
│   ├── program.rs       Declarations, runtime preamble, main wrapper
│   ├── expressions.rs   Expression → Rust translation
│   ├── calls.rs         Module call mapping (fs, string, list, map, env, process, ...)
│   └── blocks.rs        Blocks, do-blocks, for-in, match arms
├── emit_ts/             TypeScript/JavaScript code generation
│   ├── mod.rs           TsEmitter struct, entry points
│   ├── declarations.rs  Fn/type/test declarations
│   ├── expressions.rs   Expression → TS translation
│   └── blocks.rs        Blocks, match, for-in
├── emit_ts_runtime.rs   Embedded JS/TS stdlib runtime (Deno + Node)
├── fmt.rs               Code formatter (AST → formatted source)
└── project.rs           almide.toml parsing, git-based dependency management
```

## Design Decisions

### Why compile to Rust, not LLVM?

Almide targets LLM-generated code. Correctness matters more than compile speed. By emitting Rust, we get:
- Memory safety without a GC (ownership model is implicit in codegen)
- Excellent error messages from `rustc` as a second-pass verifier
- Zero-effort access to WASM via `--target wasm32-wasip1`
- Sub-MB static binaries with no runtime

### Why no traits or generics (yet)?

Almide's type system is intentionally simple. For LLM code generation, a flat module system with UFCS is more predictable than trait resolution. The type checker uses `Ty::Unknown` to handle generic containers — `List[T]`, `Option[T]`, `Result[T, E]` work, but user-defined generics don't exist yet. This is a deliberate scope limit, not an oversight.

### Why directory modules?

The compiler was originally single-file-per-module. As it grew, `check.rs` (860 lines), `parser/expressions.rs` (653 lines), and `emit_rust.rs` became hard to navigate. The current structure keeps every file under 600 lines while preserving `impl` block cohesion via `pub(crate)` visibility.

### Effect system

`effect fn` marks functions that perform I/O. The Rust emitter wraps return types in `Result<T, String>` and auto-propagates `?`. The TS emitter erases `Result` — `ok(x)` becomes `x`, `err(e)` becomes `throw`. This means the same source compiles to idiomatic code in both targets.

### Error philosophy

Every diagnostic includes:
1. **What** went wrong (type mismatch, unknown function, ...)
2. **Where** it happened (file:line + source line)
3. **How to fix it** (actionable hint)

This is designed for LLM auto-repair — the model reads the error, applies the hint, and retries. No ambiguous "did you mean?" suggestions.

## Testing

```bash
almide test               # Run tests/ directory
almide run file.almd      # Compile + execute (tests embedded in source)
almide check file.almd    # Type check only
```

The `exercises/` directory contains 17 Exercism-style programs (affine-cipher, bob, collatz, config-merger, etc.) that serve as integration tests. CI runs all of them on every push.

## Stats

| Metric | Value |
|--------|-------|
| Total source | ~7,600 lines of Rust |
| Dependencies | 2 (serde, serde_json) |
| Max file size | 589 lines (lexer.rs) |
| Stdlib modules | 11 (string, list, map, int, float, fs, env, path, json, http, process) |
| Targets | Rust, TypeScript, JavaScript, WASM |
| Exercises | 17 programs with embedded tests |
