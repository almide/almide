# Architecture

Almide is a ~20,000-line pure-Rust compiler. Zero runtime dependencies — `serde` and `serde_json` are the only crates, used for AST serialization.

## Pipeline

```
Build time (cargo build)
─────────────────────────────────────────────────────
grammar/tokens.toml ──┐
grammar/precedence.toml ──┤
                          ├──▶ build.rs ──▶ src/generated/
stdlib/defs/*.toml ───────┘       │
                                  ├── token_table.rs      (keyword map)
                                  ├── stdlib_sigs.rs      (type signatures)
                                  ├── emit_rust_calls.rs  (Rust codegen dispatch)
                                  ├── emit_ts_calls.rs    (TS codegen dispatch)
                                  └── textmate/tree-sitter patterns

Compile time (almide run/build)
─────────────────────────────────────────────────────
Source (.almd)
    │
    ▼
┌─────────┐     Tokens with line/col
│  Lexer  │──────────────────────────┐
└─────────┘  (uses token_table.rs)   │
    │                                │
    ▼                                │
┌─────────┐     AST (Program)       │
│ Parser  │──────────────────────┐  │
└─────────┘  (hints/ for errors) │  │
    │                            │  │
    ▼                            │  │
┌──────────┐   Resolved modules  │  │
│ Resolver │─────────────────┐   │  │
└──────────┘                 │   │  │
    │                        │   │  │
    ▼                        ▼   ▼  │
┌─────────┐          Diagnostics    │
│ Checker │  ◄── source text ───────┘
└─────────┘  (uses stdlib_sigs.rs)
    │
    ▼
┌──────────┐
│ Lowering │   AST → Typed IR (use-count analysis)
└──────────┘
    │
    ▼
┌──────────────┐
│   Emitter    │   IR → target code
│  ┌────────┐  │
│  │  Rust  │  │──▶  .rs  ──▶  rustc  ──▶  native binary / WASM
│  ├────────┤  │   (borrow analysis, runtime embedding)
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
├── lib.rs               Library crate root (re-exports for tests)
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
│   ├── helpers.rs       Utilities (skip_newlines, peek, expect)
│   └── hints/           Context-aware error recovery hints
│       ├── mod.rs           Hint dispatch
│       ├── catalog.rs       Error message catalog
│       ├── delimiter.rs     Mismatched bracket/paren hints
│       ├── keyword_typo.rs  Keyword typo detection (e.g. "func" → "fn")
│       ├── missing_comma.rs Missing comma/separator hints
│       ├── operator.rs      Operator misuse hints (e.g. "=" vs "==")
│       └── syntax_guide.rs  Syntax pattern suggestions
├── resolve.rs           Import resolution, circular dependency detection
├── check/               Type checker — every error has an actionable hint
│   ├── mod.rs           Checker struct, decl registration, type resolution
│   ├── expressions.rs   Expression type inference
│   ├── calls.rs         Function call dispatch (direct, constructor, module, UFCS)
│   ├── operators.rs     Binary/unary operator type rules
│   └── statements.rs    Statement checking, pattern binding
├── types.rs             Internal type representation (Ty enum, TypeEnv, FnSig)
├── ir.rs                Typed IR (IrProgram, IrModule, IrFunction, IrTypeDecl, IrExpr, IrStmt)
├── lower.rs             AST → IR lowering with use-count analysis
├── diagnostic.rs        Structured errors with file/line, hint, source display
├── stdlib.rs            UFCS resolution, module registry
├── generated/           Auto-generated at build time (DO NOT EDIT)
│   ├── stdlib_sigs.rs       Type signatures from stdlib/defs/*.toml
│   ├── emit_rust_calls.rs   Rust codegen dispatch for stdlib calls
│   ├── emit_ts_calls.rs     TS codegen dispatch for stdlib calls
│   ├── token_table.rs       Keyword table from grammar/tokens.toml
│   ├── textmate_patterns.txt  TextMate grammar patterns
│   ├── tree_sitter_keywords.txt   Tree-sitter keyword rules
│   └── tree_sitter_precedence.txt Tree-sitter precedence rules
├── emit_common.rs       Shared codegen utilities (sanitize)
├── emit_rust/           Rust code generation (IR-based)
│   ├── mod.rs           Emitter struct, EmitOptions, entry points
│   ├── program.rs       Declarations, runtime preamble, main wrapper
│   ├── ir_expressions.rs  IR expression → Rust translation
│   ├── ir_blocks.rs     IR blocks, do-blocks, for-in, match arms
│   ├── borrow.rs        Borrow analysis, clone insertion, single-use optimization
│   ├── core_runtime.txt     Embedded Rust runtime (string, list, map, int, float, result, math)
│   ├── collection_runtime.txt  Collection helpers (sorting, grouping)
│   ├── io_runtime.txt       I/O runtime (fs, env, process, path, args, encoding, csv)
│   ├── json_runtime.txt     JSON runtime (parse, stringify, builder, path API)
│   ├── http_runtime.txt     HTTP client runtime
│   ├── regex_runtime.txt    Regex runtime
│   ├── time_runtime.txt     Time/duration runtime
│   └── platform_runtime.txt Platform detection runtime
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

The compiler originally emitted code directly from the AST, but this led to duplicated logic between Rust and TS emitters. The IR (intermediate representation) sits between the type checker and codegen, providing a normalized, typed tree where every node carries its resolved type. Codegen receives only `&IrProgram` — it never references the AST. This enables:
- **AST-free codegen** — emitters are decoupled from parse-tree details
- Shared optimizations (borrow analysis, clone insertion) applied once
- Easier addition of new targets
- Clearer separation between language semantics and target-specific codegen

### IR structure

The typed IR (`src/ir.rs`, 570 lines) sits between the type checker and codegen. Every node carries its resolved type — emitters never re-derive type information.

**Hierarchy:**

```
IrProgram
├── functions: Vec<IrFunction>     (main module functions)
├── type_decls: Vec<IrTypeDecl>    (main module types)
├── top_lets: Vec<IrTopLet>        (main module constants)
├── var_table: VarTable            (main module variables)
└── modules: Vec<IrModule>         (imported user modules, each self-contained)

IrModule
├── name, versioned_name           (diamond dependency aliasing)
├── functions, type_decls, top_lets
└── var_table                      (module-local variable scope)
```

**Key IR nodes:**

| Node | Purpose |
|------|---------|
| `VarId(u32)` | Unique variable ID — eliminates shadowing ambiguity across scopes |
| `VarTable` | Maps VarId → VarInfo (name, type, mutability, use_count) |
| `IrParam` | Function param with `ParamBorrow` (Own/Ref/RefStr/RefSlice) and optional `OpenRecordInfo` |
| `IrExpr` | Expression with resolved `Ty` and `Span`. 30+ variants including type-dispatched `BinOp` (AddInt vs AddFloat) |
| `CallTarget` | Resolved call: Named (free fn), Module (stdlib), Method (UFCS), Computed (higher-order) |
| `IrTypeDecl` | Record, Variant (with recursive Box tracking), or Alias with visibility |
| `TopLetKind` | Const (literal → `const`) vs Lazy (expression → `LazyLock`) |

**Post-lowering passes:**

1. **Use-count analysis** (`compute_use_counts`): Walks the full IR tree to count references per VarId. Stored in `VarTable.use_count` — avoids re-traversal during codegen.

2. **Borrow analysis** (`emit_rust/borrow.rs`): Lobster-style escape analysis. Starts with all heap-type params (String, Vec, Map) as `Borrow`, then uses fixpoint iteration with inter-procedure analysis to refine to `Owned` where escape is detected. Results feed into `IrParam.borrow` for `&str`/`&[T]` emission.

3. **Single-use optimization**: Variables with `use_count == 1` are moved instead of cloned — the VarId-based tracking prevents the cross-scope aliasing bug that occurred with name-based analysis.

**Design invariants:**
- Pipes (`|>`) and UFCS desugared to `CallTarget::Module` during lowering — emitters see only direct calls
- String interpolation desugared to `StringInterp` with pre-typed parts
- Operators are type-dispatched (`AddInt` vs `AddFloat`) — no runtime type queries in codegen
- Pattern bindings carry `VarId` — no name collisions in nested match arms

### Generics

Almide supports generics for type declarations (`type Pair[A, B] = { first: A, second: B }`) and all stdlib containers. User-defined generic functions are not yet supported — this is a deliberate scope limit, not an oversight. The type checker preserves generic type arguments through `Ty::Named(name, args)` for accurate codegen.

### Why directory modules?

The compiler was originally single-file-per-module. As it grew, `check.rs` (860 lines), `parser/expressions.rs` (653 lines), and the emitters became hard to navigate. These were split into directory modules: `check/` (5 files), `parser/` (8 files + 7 hint files), `emit_rust/` (5 .rs files + 7 runtime .txt files), `emit_ts/` (4 files). The current structure keeps every file under 600 lines while preserving `impl` block cohesion via `pub(crate)` visibility.

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
| Total source | ~20,000 lines of Rust |
| Dependencies | 4 (serde, serde_json, clap, semver) |
| Stdlib modules | 15 (string, list, map, int, float, fs, env, path, json, math, result, random, regex, io, http) |
| Stdlib functions | 282 |
| Targets | Rust, TypeScript, JavaScript, WASM |
| Language tests | 1,700+ (.almd) |
| Compiler tests | 567 (cargo test) |
| Exercises | 17 programs with embedded tests |
| n-body benchmark | 1.74s (Rust-equivalent, opt-level=2) |
