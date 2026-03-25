# Architecture

Almide is a ~22,000-line pure-Rust compiler. Dependencies: `serde` + `serde_json` (AST serialization), `toml` (template loading), `clap` (CLI).

## Pipeline

```
                          Build time (cargo build)
                    ┌──────────────────────────────────────────┐
                    │  grammar/*.toml ──┐                      │
                    │  runtime/rs/src/  ─┤                     │
                    │  runtime/ts/      ─┼─→ build.rs ─→ src/generated/  │
                    │  runtime/js/      ─┤     │  arg_transforms.rs      │
                    │  stdlib/          ──┘     │  stdlib_sigs.rs         │
                    │                          │  rust_runtime.rs        │
                    │                          │  ts_runtime.rs          │
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
    │                              ┌───────────────────────────────┐  │
    │                              │   Template Renderer            │  │
    │                              │   (TOML-driven, target-agnostic)│ │
    │                              └──────────────┬────────────────┘  │
    │                                             │                   │
    │                                             ▼                   │
    │                                     .rs / .ts / .js             │
    └─────────────────────────────────────────────────────────────────┘
```

## Source Map

```
src/
├── main.rs              CLI entry: subcommands, import resolution, orchestration
├── lib.rs               Public API (used by playground WASM crate)
├── ast.rs               AST node types (serde-serializable)
├── lexer.rs             Tokenizer (42 keywords, string interpolation, heredocs)
├── diagnostic.rs        Error/warning types with file:line + hint
├── resolve.rs           Module resolution (filesystem + git deps)
├── project.rs           almide.toml parsing, PkgId
├── project_fetch.rs     Git dependency fetching
├── stdlib.rs            UFCS candidate tables, module lists
├── mono.rs              Monomorphization (generic instantiation)
├── fmt.rs               Source code formatter (almide fmt)
│
├── parser/
│   ├── mod.rs           Parser struct, token stream
│   ├── entry.rs         Top-level: program, imports, declarations
│   ├── declarations.rs  fn, type, trait, impl, test, top-let
│   ├── expressions.rs   Binary, unary, pipe, match, if/then/else
│   ├── primary.rs       Literals, identifiers, lambdas, blocks
│   ├── statements.rs    let, var, guard, assignment
│   ├── patterns.rs      Match arm patterns (variant, record, tuple)
│   ├── types.rs         Type expressions (generics, records, functions)
│   ├── collections.rs   List, map, record, tuple literals
│   ├── compounds.rs     for-in, while, fan blocks
│   ├── helpers.rs       Comma-separated lists, precedence
│   ├── recovery.rs      Error recovery (skip to sync points)
│   ├── diagnostics.rs   Parser error formatting
│   └── hints/           Smart error hints
│       ├── catalog.rs       Rejected keyword → Almide equivalent
│       ├── keyword_typo.rs  Fuzzy keyword matching
│       ├── operator.rs      ! → not, && → and, || → or
│       ├── syntax_guide.rs  Context-specific fix suggestions
│       ├── delimiter.rs     Bracket/paren mismatch hints
│       └── missing_comma.rs Comma insertion suggestions
│
├── check/
│   ├── mod.rs           Checker: constraint-based type inference, module registration
│   ├── infer.rs         Expression inference (Pass 1: walk AST, assign types)
│   ├── calls.rs         Call resolution: UFCS, builtins, constructors, conventions
│   └── types.rs         Constraint solving, type unification
│
├── types/
│   ├── mod.rs           Ty enum (Int, String, List, Record, Fn, Variant, ...)
│   ├── env.rs           TypeEnv: scoped variables, functions, types, modules
│   └── unify.rs         Structural type unification
│
├── lower/
│   ├── mod.rs           AST + Types → IR lowering, VarId assignment
│   ├── expressions.rs   Expression lowering (literals, blocks, lambdas, match)
│   ├── calls.rs         Call target resolution (Module, Method, Named, Computed)
│   ├── statements.rs    Statement + pattern lowering
│   ├── types.rs         Type declaration lowering (records, variants, newtypes)
│   ├── derive.rs        Auto-derive (Eq, Repr, Ord, Hash)
│   └── derive_codec.rs  Codec auto-derive (encode/decode for records + variants)
│
├── ir/
│   ├── mod.rs           IrProgram, IrExpr, IrStmt, IrPattern, CallTarget, VarTable
│   ├── fold.rs          IR tree walker/transformer
│   ├── result.rs        Result expression detection (shared Rust + TS logic)
│   ├── unknown.rs       Unknown type handling
│   └── use_count.rs     Variable use-count analysis (move vs clone decisions)
│
├── codegen/
│   ├── mod.rs           emit(): orchestrates pipeline → walker → output
│   ├── target.rs        Target config: pipeline + templates per target
│   ├── pass.rs          NanoPass trait, Pipeline, Target enum
│   ├── annotations.rs   Pre-pass: collect named/anon records, ctor→enum map
│   ├── template.rs      TOML template engine ({var} substitution)
│   ├── walker.rs        IR → source renderer (target-agnostic, 0 target checks)
│   │
│   │── pass_stdlib_lowering.rs    Module/Method → Named + arg decoration
│   │── pass_result_propagation.rs Insert Try (?) in effect fns (Rust)
│   │── pass_result_erasure.rs     ok(x)→x, err(e)→throw (TS)
│   │── pass_match_lowering.rs     match → if/else chains (TS)
│   │── pass_clone.rs              Clone insertion (Rust borrow analysis)
│   │── pass_box_deref.rs          Recursive type Box/deref (Rust)
│   │── pass_builtin_lowering.rs   assert_eq→macro, println→macro (Rust)
│   │── pass_fan_lowering.rs       Fan block → tokio/Promise.all
│   └── pass_shadow_resolve.rs     let-rebinding → assignment (TS)
│
├── optimize/
│   ├── mod.rs           Optimization pipeline
│   ├── dce.rs           Dead code elimination
│   └── propagate.rs     Constant propagation
│
├── cli/
│   ├── mod.rs           CLI module exports
│   ├── run.rs           almide run: compile → rustc → execute
│   ├── build.rs         almide build: compile → binary / WASM / npm
│   ├── check.rs         almide check: type check only
│   ├── emit.rs          almide emit: output generated source
│   └── commands.rs      almide test: find + run test blocks
│
└── generated/           Auto-generated by build.rs (DO NOT EDIT)
    ├── arg_transforms.rs    Per-function argument decoration rules
    ├── stdlib_sigs.rs       Function signatures for type checking
    ├── emit_rust_calls.rs   Rust codegen dispatch
    ├── emit_ts_calls.rs     TS codegen dispatch
    ├── rust_runtime.rs      Embedded Rust runtime (include_str)
    ├── ts_runtime.rs        Embedded TS runtime (include_str from runtime/ts/)
    └── token_table.rs       Keyword → TokenType mapping

codegen/templates/
├── rust.toml            Rust syntax templates (~330 rules)
└── typescript.toml      TypeScript syntax templates

runtime/
├── rs/src/              Rust runtime: 22 modules (string, list, map, json, fs, ...)
└── ts/                  TypeScript runtime: 22 modules (Deno + Node --strip-types)
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

**TypeScript pipeline:**
```
MatchLowering → ResultErasure → ShadowResolve → FanLowering
```

| Pass | Target | What it does |
|------|--------|------|
| StdlibLowering | Rust | `Module { "list", "map" }` → `Named { "almide_rt_list_map" }` + arg decoration |
| ResultPropagation | Rust | Insert `Try { expr }` (Rust `?`) on fallible calls in `effect fn` |
| ResultErasure | TS | `ok(x)` → `x`, `err(e)` → `throw new Error(e)`, `Try` → identity |
| MatchLowering | TS | `Match { subject, arms }` → `If/ElseIf/Else` chain |
| CloneInsertion | Rust | Insert `Clone` nodes based on use-count analysis |
| BoxDeref | Rust | Insert `Deref` for recursive type access through `Box` |
| BuiltinLowering | Rust | `assert_eq` → `RustMacro`, `println` → `RustMacro` |
| ShadowResolve | TS | `let x = 1; let x = 2` → `let x = 1; x = 2` (TS/JS has no shadowing) |
| FanLowering | All | Strip auto-try from fan spawn closures |

### Layer 2: Template Renderer

TOML files define syntax patterns. The walker calls `templates.render_with("if_expr", ...)` and gets back target-specific syntax:

```toml
# rust.toml
[if_expr]
template = "if ({cond}) {{ {then} }} else {{ {else} }}"

# typescript.toml
[if_expr]
template = "({cond}) ? ({then}) : ({else})"
```

~330 template rules per target. All string rendering is done here — passes never produce text.

### Layer 3: Walker

`walker.rs` (1,676 lines) walks the IR tree and renders each node by calling the template engine. It is **fully target-agnostic** — zero `if target == Rust` checks. Target differences are handled entirely by passes (Layer 1) and templates (Layer 2).

Key rendering functions:
- `render_expr()` — expressions (recursively renders sub-expressions)
- `render_stmt()` — statements (let, var, guard, assign)
- `render_type()` — type annotations (named records, generics, tuples)
- `render_pattern()` — match patterns (variants, records, tuples)
- `render_function()` — function declarations with params, return type, body

## Build System

`build.rs` generates code at compile time:

1. **Scans `runtime/rs/src/*.rs`** → extracts function signatures → generates `arg_transforms.rs` (per-function argument decoration: BorrowStr, BorrowRef, ToVec, LambdaClone, Direct)
2. **Scans `runtime/ts/*.ts` + `runtime/js/*.js`** → embeds as `include_str!` → generates `ts_runtime.rs`
3. **Reads stdlib definitions** → generates `stdlib_sigs.rs` (function signatures for type checking)
4. **Reads grammar files** → generates `token_table.rs` (keyword → TokenType mapping)

The runtime is embedded in the compiler binary. When emitting JS/TS, the runtime preamble is prepended to the output. When emitting Rust, runtime functions are `include_str!`'d into the generated `.rs` file.

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
