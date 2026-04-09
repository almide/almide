# Architecture

Almide is a ~72,000-line pure-Rust compiler organized as a workspace of 9 crates + a CLI binary. Dependencies: `serde` + `serde_json` (AST serialization), `toml` (template loading), `clap` (CLI), `lasso` (string interning).

## Pipeline

```
                          Build time (cargo build)
                    ┌──────────────────────────────────────────┐
                    │  grammar/*.toml ──┐                      │
                    │  runtime/rs/src/  ─┤                     │
                    │  stdlib/          ─┼─→ build.rs ─→ generated/       │
                    │                          │  arg_transforms.rs      │
                    │                          │  stdlib_sigs.rs         │
                    │                          │  rust_runtime.rs        │
                    │                          │  token_table.rs         │
                    └──────────────────────────────────────────┘

                            Run time (almide run)
    ┌─────────────────────────────────────────────────────────────────┐
    │                                                                 │
    │  .almd source                                                   │
    │       │                                                         │
    │       ▼                                                         │
    │  ┌─────────┐   ┌──────────┐   ┌─────────┐   ┌──────────────┐  │
    │  │  Lexer   │──▶│  Parser  │──▶│   AST   │──▶│ Type Checker │  │
    │  └─────────┘   └──────────┘   └─────────┘   └──────┬───────┘  │
    │                                                      │          │
    │                                              expr_types + env   │
    │                                                      │          │
    │                                                      ▼          │
    │                              ┌───────────────────────────────┐  │
    │                              │   Lowering (AST → Typed IR)   │  │
    │                              └──────────────┬────────────────┘  │
    │                                             │                   │
    │                                             ▼                   │
    │                              ┌───────────────────────────────┐  │
    │                              │   Nanopass Pipeline            │  │
    │                              │   (target-specific rewrites)   │  │
    │                              └──────────────┬────────────────┘  │
    │                                             │                   │
    │                                             ▼                   │
    │                      ┌────────────────────────────────────────┐ │
    │                      │   Rust target          WASM target     │ │
    │                      │   Template Renderer    Direct Emit     │ │
    │                      │   (TOML-driven)        (linear memory) │ │
    │                      └──────────────┬─────────────────────────┘ │
    │                                     │                           │
    │                                     ▼                           │
    │                              .rs / .wasm                        │
    └─────────────────────────────────────────────────────────────────┘
```

## Crate Structure

```
almide/                   Workspace root
├── Cargo.toml            Workspace manifest
├── src/                  CLI binary (almide)
│   ├── main.rs           CLI entry: subcommands, orchestration
│   ├── lib.rs            Public API (used by playground WASM crate)
│   ├── resolve.rs        Module resolution (filesystem + git deps)
│   ├── project.rs        almide.toml parsing, PkgId
│   ├── project_fetch.rs  Git dependency fetching
│   ├── diagnostic_render.rs  Diagnostic pretty-printing
│   └── cli/
│       ├── mod.rs         CLI module exports
│       ├── run.rs         almide run: compile → rustc → execute
│       ├── build.rs       almide build: compile → binary / WASM
│       ├── check.rs       almide check: type check only
│       ├── emit.rs        almide emit: output generated source
│       ├── commands.rs    almide test: find + run test blocks
│       └── selfupdate.rs  almide self-update: binary update from GitHub
│
├── crates/
│   ├── almide-base/       Shared primitives
│   │   ├── diagnostic.rs  Error/warning types with file:line + hint
│   │   ├── intern.rs      String interning (lasso)
│   │   └── span.rs        Source span types
│   │
│   ├── almide-syntax/     Parsing
│   │   ├── ast.rs         AST node types (serde-serializable)
│   │   ├── lexer.rs       Tokenizer (42 keywords, string interpolation, heredocs)
│   │   └── parser/        Recursive descent parser with error recovery
│   │       ├── entry.rs         Top-level: program, imports, declarations
│   │       ├── declarations.rs  fn, type, trait, impl, test, top-let
│   │       ├── expressions.rs   Binary, unary, pipe, match, if/then/else
│   │       ├── primary.rs       Literals, identifiers, lambdas, blocks
│   │       ├── statements.rs    let, var, guard, assignment
│   │       ├── patterns.rs      Match arm patterns (variant, record, tuple)
│   │       ├── types.rs         Type expressions (generics, records, functions)
│   │       ├── collections.rs   List, map, record, tuple literals
│   │       ├── compounds.rs     for-in, while, fan blocks
│   │       └── hints/           Smart error hints (typos, keywords, delimiters)
│   │
│   ├── almide-types/      Type system
│   │   ├── types/         Ty enum (Int, String, List, Record, Fn, Variant, ...)
│   │   └── stdlib_info.rs UFCS candidate tables, auto-import module lists
│   │
│   ├── almide-frontend/   Type checking & lowering
│   │   ├── check/         Constraint-based type inference + UFCS resolution
│   │   ├── lower/         AST + Types → IR lowering, VarId assignment
│   │   ├── canonicalize/  Import canonicalization
│   │   ├── type_env.rs    Scoped variables, functions, types, modules
│   │   ├── import_table.rs  Import resolution table
│   │   └── stdlib.rs      Stdlib signature registration
│   │
│   ├── almide-ir/         Intermediate representation
│   │   ├── lib.rs         IrProgram, IrExpr, IrStmt, IrPattern, CallTarget, VarTable
│   │   ├── fold.rs        IR tree walker/transformer
│   │   ├── visit.rs       Read-only IR visitor
│   │   ├── use_count.rs   Variable use-count analysis (move vs clone)
│   │   ├── result.rs      Result expression detection
│   │   ├── effect.rs      Effect inference helpers
│   │   └── wasm_repr.rs   WASM type representation
│   │
│   ├── almide-optimize/   Optimization
│   │   ├── mono/          Monomorphization (generic instantiation, VarId alpha-renaming)
│   │   └── optimize/      DCE, constant propagation, LICM, peephole, stream fusion
│   │
│   ├── almide-codegen/    Code generation
│   │   ├── pass.rs        NanoPass trait, Pipeline, Target enum
│   │   ├── target.rs      Target config: pipeline + templates per target
│   │   ├── template.rs    TOML template engine ({var} substitution)
│   │   ├── walker/        IR → Rust source renderer (target-agnostic)
│   │   ├── emit_wasm/     Direct WASM binary emitter (linear memory, WASI)
│   │   ├── pass_*.rs      Nanopass implementations (20+ passes)
│   │   └── generated/     Auto-generated by build.rs (DO NOT EDIT)
│   │       ├── arg_transforms.rs    Per-function argument decoration rules
│   │       └── rust_runtime.rs      Embedded Rust runtime (include_str)
│   │
│   ├── almide-tools/      Tooling
│   │   ├── fmt.rs         Source code formatter (almide fmt)
│   │   ├── interface.rs   Module interface extraction (almide compile)
│   │   └── almdi.rs       ALMDI metadata format
│   │
│   └── almide-lang/       Language metadata (version, feature flags)
│
├── grammar/               Grammar definitions
│   ├── tokens.toml        Keyword → TokenType mapping
│   ├── almide.toml        Grammar rules
│   └── precedence.toml    Operator precedence table
│
├── codegen/templates/
│   └── rust.toml          Rust syntax templates (~330 rules)
│
├── stdlib/defs/           Stdlib TOML definitions (23 modules, 430+ functions)
│
└── runtime/rs/src/        Rust runtime: 22 modules (string, list, map, json, fs, ...)
```

## Codegen v3: Three-Layer Architecture

All semantic decisions are made in the IR before any text is emitted. The walker sees only typed IR nodes — it never checks what target it's rendering for.

### Layer 1: Nanopass Pipeline

Each pass receives `&mut IrProgram` and rewrites it structurally. Passes are composable and target-specific:

**Rust pipeline:**
```
TypeConcretization → BorrowInsertion → CloneInsertion
  → StdlibLowering → ResultPropagation → BuiltinLowering → FanLowering
```

**WASM pipeline:**
Direct binary emission — no template layer. The `emit_wasm/` module walks the IR and emits WASM bytecode directly, managing linear memory layout, stack frames, and WASI syscalls.

| Pass | Target | What it does |
|------|--------|------|
| StdlibLowering | Rust | `Module { "list", "map" }` → `Named { "almide_rt_list_map" }` + arg decoration |
| ResultPropagation | Rust | Insert `Try { expr }` (Rust `?`) on fallible calls in `effect fn` |
| CloneInsertion | Rust | Insert `Clone` nodes based on use-count analysis |
| BoxDeref | Rust | Insert `Deref` for recursive type access through `Box` |
| BuiltinLowering | Rust | `assert_eq` → `RustMacro`, `println` → `RustMacro` |
| FanLowering | Rust | Strip auto-try from fan spawn closures |
| TailCallMark | WASM | Mark tail-recursive calls for `return_call` emission |
| ClosureConversion | WASM | Lambda capture → explicit env struct passing |

### Layer 2: Template Renderer (Rust target only)

TOML files define syntax patterns. The walker calls `templates.render_with("if_expr", ...)` and gets back Rust syntax:

```toml
# rust.toml
[if_expr]
template = "if ({cond}) {{ {then} }} else {{ {else} }}"
```

~330 template rules. All string rendering is done here — passes never produce text.

### Layer 3: Walker

`walker/` walks the IR tree and renders each node by calling the template engine. It is **fully target-agnostic** — zero `if target == Rust` checks. Target differences are handled entirely by passes (Layer 1) and templates (Layer 2).

Key rendering functions:
- `render_expr()` — expressions (recursively renders sub-expressions)
- `render_stmt()` — statements (let, var, guard, assign)
- `render_type()` — type annotations (named records, generics, tuples)
- `render_pattern()` — match patterns (variants, records, tuples)
- `render_function()` — function declarations with params, return type, body

## WASM Direct Emitter

The WASM target (`emit_wasm/`) bypasses templates entirely and emits binary WASM directly:

- **Linear memory**: Stack allocator on memory 0, scratch buffer on memory 1 (multi-memory)
- **Tail calls**: Native `return_call` / `return_call_indirect` (WASM 3.0)
- **Strings**: UTF-8 in linear memory, length-prefixed
- **Closures**: Explicit environment structs, function table indirect calls
- **WASI**: File I/O, args, env vars via WASI preview1 imports

## Build System

`build.rs` generates code at compile time:

1. **Scans `runtime/rs/src/*.rs`** → extracts function signatures → generates `arg_transforms.rs` (per-function argument decoration: BorrowStr, BorrowRef, ToVec, LambdaClone, Direct)
2. **Reads stdlib definitions** → generates `stdlib_sigs.rs` (function signatures for type checking)
3. **Reads grammar files** → generates `token_table.rs` (keyword → TokenType mapping)

The Rust runtime is embedded in the compiler binary via `include_str!` and prepended to generated `.rs` files.

## Type System

Constraint-based inference with eager unification:

1. **Infer** — Walk AST, assign fresh type variables to unknowns, collect constraints
2. **Solve** — Unify constraints, propagate solutions
3. **Resolve** — Replace inference variables with concrete types in `expr_types`

Key types: `Ty::Int`, `Ty::String`, `Ty::List(Box<Ty>)`, `Ty::Record { fields }`, `Ty::Variant { cases }`, `Ty::Fn { params, ret }`, `Ty::Option(Box<Ty>)`, `Ty::Result(Box<Ty>, Box<Ty>)`.

UFCS resolution: `xs.map(fn)` → checker finds `builtin_module_for_type(List) = "list"` → dispatches to `list.map(xs, fn)`.

## Module System

1. **Resolve** (`resolve.rs`) — Walks `import` declarations, finds `.almd` files (local, git deps, stdlib skip)
2. **Register** (`check/mod.rs`) — `register_module(name, program)` adds prefixed function/type signatures to TypeEnv
3. **Check** — Main program type-checked with all modules registered
4. **Lower** — Each module lowered to `IrModule` (separate function/type namespace)
5. **Codegen** — Module functions emitted with `almide_rt_{module}_{func}` prefix

Stdlib modules (TOML-defined) are never loaded from disk — their signatures come from `build.rs`-generated code, and their runtime is embedded.

## Diagnostics

Every diagnostic includes:
- **Error code** (E001–E010) for programmatic consumption
- **File:line:col** location
- **Source context** with underline
- **Actionable hint** pointing to a specific fix

```
error[E005]: argument 'xs' expects List[Int] but got String
  at line 5
  in call to list.sort()
  hint: Fix the argument type
  |
5 | let sorted = list.sort("hello")
  |                        ^^^^^^^
```

Supported output: human-readable (default) or `--json` for tool integration.
