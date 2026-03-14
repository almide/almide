# Architecture

Almide is a ~23,000-line pure-Rust compiler. Zero runtime dependencies — `serde` and `serde_json` are the only crates, used for AST serialization.

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
┌──────────┐
│ Lowering │   AST → Typed IR
└──────────┘
    │
    ▼
┌──────────────┐
│   Emitter    │   IR → target code
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
├── lexer.rs             Tokenizer — newline-sensitive (suppressed inside parens/brackets), keywords, interpolated strings
├── parser/              Recursive descent parser
│   ├── mod.rs           Parser struct, token navigation
│   ├── declarations.rs  import, fn, type, test, trait, impl, visibility (pub/mod/local)
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
├── ir.rs                Typed IR definitions (IrExpr, IrStmt, IrDecl)
├── lower.rs             AST → IR lowering (typed intermediate representation)
├── diagnostic.rs        Structured errors with file/line, hint, source display
├── stdlib.rs            Centralized stdlib definitions (signatures, UFCS, modules)
├── generated/           Auto-generated from stdlib/defs/*.toml (DO NOT EDIT)
├── emit_common.rs       Shared codegen utilities (sanitize)
├── emit_rust/           Rust code generation (IR-based)
│   ├── mod.rs           Emitter struct, EmitOptions, entry points
│   ├── program.rs       Declarations, runtime preamble, main wrapper
│   ├── ir_expressions.rs  IR expression → Rust translation
│   ├── ir_blocks.rs     IR blocks, do-blocks, for-in, match arms
│   └── calls.rs         Module call mapping (fs, string, list, map, env, process, ...)
├── emit_ts/             TypeScript/JavaScript code generation (IR-based)
│   ├── mod.rs           TsEmitter struct, entry points
│   ├── declarations.rs  Fn/type/test declarations
│   ├── ir_expressions.rs  IR expression → TS translation
│   └── ir_blocks.rs     IR blocks, match, for-in
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

### Why a typed IR?

The compiler originally emitted code directly from the AST, but this led to duplicated logic between Rust and TS emitters. The IR (intermediate representation) sits between the type checker and codegen, providing a normalized, typed tree. This enables:
- Shared optimizations (borrow analysis, clone insertion) applied once
- Easier addition of new targets
- Clearer separation between language semantics and target-specific codegen

### Generics

Almide supports generics for type declarations (`type Pair[A, B] = { first: A, second: B }`) and all stdlib containers. User-defined generic functions are not yet supported — this is a deliberate scope limit, not an oversight. The type checker preserves generic type arguments through `Ty::Named(name, args)` for accurate codegen.

### Why directory modules?

The compiler was originally single-file-per-module. As it grew, `check.rs` (860 lines), `parser/expressions.rs` (653 lines), and the emitters became hard to navigate. These were split into directory modules: `check/` (5 files), `parser/` (7 files), `emit_rust/` (5 files), `emit_ts/` (4 files). The current structure keeps every file under 600 lines while preserving `impl` block cohesion via `pub(crate)` visibility.

### Effect system

`effect fn` marks functions that perform I/O. The Rust emitter wraps return types in `Result<T, String>` and auto-propagates `?`. The TS emitter erases `Result` — `ok(x)` becomes `x`, `err(e)` becomes `throw`. This means the same source compiles to idiomatic code in both targets.

### Error philosophy

Every diagnostic includes:
1. **What** went wrong (type mismatch, unknown function, ...)
2. **Where** it happened (file:line + source line)
3. **How to fix it** (actionable hint)

This is designed for LLM auto-repair — the model reads the error, applies the hint, and retries. No ambiguous "did you mean?" suggestions.

## Build Pipeline & Optimization

When Almide compiles and runs a program, it goes through two compilation stages:

```
.almd → [Almide compiler] → IR → .rs → [rustc] → binary → execute
```

The `rustc` optimization level differs by command:

| Command | opt-level | Purpose |
|---|---|---|
| `almide run` | 1 | Fast compile for development iteration |
| `almide build` | 0 (default) | Unoptimized build |
| `almide build --release` | 2 | Full optimization for production/benchmarks |
| `almide build --target wasm` | s | Size-optimized for WASM |

**Why `almide run` uses opt-level=1**: During development, you run the same program many times. `opt-level=1` cuts compile time significantly while still applying basic optimizations. For benchmarks or production, use `almide build --release`.

### Impact on performance

The n-body gravitational simulation benchmark (50M steps, 5 bodies) demonstrates this clearly:

| Configuration | Time | vs Rust |
|---|---|---|
| Almide generated code, `rustc -O` (opt-level=2) | 1.74s | 1.03x |
| Native Rust (hand-written) | 1.69s | 1.00x |
| `almide run` (opt-level=1) | 4.32s | 2.56x |

The generated Rust code is **near-identical in performance to hand-written Rust** when compiled with the same optimization level. The 3% overhead comes from minor codegen differences (redundant `as f64` casts, `.clone()` on Copy types) that LLVM optimizes away at `-O2`.

### Where Almide lands in language benchmarks

Based on the n-body benchmark (generated code compiled with `rustc -O`):

```
C        2.10s
C++      2.15s
Rust     2.19s  (benchmarksgame reference)
Almide   1.74s  ← generated Rust, same machine
C#       3.13s
Julia    3.80s
Swift    5.45s
Java     6.02s
Go       6.39s
```

Almide's generated code competes at the Rust/C++ tier because **it is Rust** — the compiler translates `.almd` to idiomatic `.rs` and delegates to `rustc` for optimization.

## Testing

```
spec/                  Almide language tests (almide test spec/)
├── lang/              Language feature tests (expr, control flow, types, ...)
├── stdlib/            Stdlib module tests (string, list, map, ...)
└── integration/       Multi-module tests (generics, modules, extern)
tests/                 Rust compiler unit tests (cargo test, auto-discovery)
exercises/             Exercism-style programs (integration tests)
```

```bash
almide test                      # All .almd tests (recursive)
almide test spec/lang/           # Language tests only
almide test spec/stdlib/         # Stdlib tests only
cargo test                       # Rust compiler tests
```

CI runs all exercises on every push across Rust, TS, JS, and WASM targets.

## Stats

| Metric | Value |
|--------|-------|
| Total source | ~23,000 lines of Rust |
| Dependencies | 4 (serde, serde_json, clap, semver) |
| Stdlib modules | 14 (string, list, map, int, float, fs, env, path, json, math, random, regex, time, io, process, encoding, args, bitwise, hash, csv, http) |
| Targets | Rust, TypeScript, JavaScript, WASM |
| Language tests | 1,500+ (.almd) |
| Compiler tests | 470 (cargo test) |
| Exercises | 15 programs with embedded tests |
| n-body benchmark | 1.74s (Rust-equivalent, opt-level=2) |
