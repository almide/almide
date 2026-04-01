<!-- description: Split compiler into workspace crates for build parallelism and API boundaries -->
# Crate Split

Split the monolithic compiler into a Cargo workspace with focused crates.

## Motivation

- 71k lines in a single crate → slow incremental builds
- codegen alone is 30k lines — changing IR shouldn't rebuild codegen and vice versa
- Clear API boundaries enable reuse (LSP, playground, external tools)
- Build parallelism: once IR is built, frontend/codegen/tools compile in parallel

## Architecture

```
almide-base      Sym, Span, Diagnostic                    ~400 lines
almide-lang      ast, types (Ty/unify/constructor),        ~10k lines
                 lexer, parser
almide-ir        IR definitions, visit, verify,            ~3k lines
                 effect, annotations
almide-codegen   walker, 20 nanopass passes,               ~30k lines
                 emit_wasm, template
almide-frontend  check, canonicalize, lower,               ~8k lines
                 import_table, stdlib, generated sigs
almide-optimize  optimize, mono                            ~2.5k lines
almide-tools     fmt, interface, almdi                     ~2k lines
almide           CLI (main, cli/, resolve, project)        ~3k lines
```

## Dependency Tree

```
almide-base         (no deps)
almide-lang       → base
almide-ir         → base, lang
almide-frontend   → base, lang, ir
almide-optimize   → ir, lang (types)
almide-codegen    → base, lang, ir, wasm-encoder, toml
almide-tools      → base, lang, ir
almide            → all
```

## Design Decisions

- **ast + types in same crate (almide-lang)**: Bidirectional dependency — `Expr.ty: Option<Ty>` and `VariantPayload::Record` contains `ast::Expr`. Breaking this requires removing `Expr.ty` (ExprId→Ty map), a larger refactor for later.
- **TypeEnv stays in main crate**: Depends on `import_table` which depends on `stdlib`. Only `Ty`/`unify`/`constructor` moved to almide-lang.
- **WASM and Rust emit NOT split**: 20 nanopass passes are shared across targets. Splitting would require a codegen-core + codegen-rust + codegen-wasm triple, with marginal incremental build benefit. Feature flags (`target-rust`, `target-wasm`) can conditionally compile targets within one crate.
- **EffectMap and CodegenAnnotations moved to almide-ir**: Originally defined in codegen but stored on `IrProgram`. Moved to break the IR→codegen circular dependency.
- **Re-export pattern**: Each extracted module has a thin re-export stub in the main crate (`pub use almide_lang::ast::*;`) so all existing `crate::` paths continue to work without mass rewriting.

## Progress

| Phase | Crate | Status |
|-------|-------|--------|
| 1 | almide-base | Done (2026-04-01) |
| 2 | almide-lang | Done (2026-04-01) |
| 3 | *(merged into Phase 2)* | — |
| 4 | almide-ir | Done (2026-04-01) |
| 5 | almide-codegen | Done (2026-04-01) |
| 6 | almide-frontend, almide-optimize, almide-tools | Done (2026-04-01) |

## Phase 5: almide-codegen (done)

The largest extraction (~55k lines). Key decisions:

- **Dependencies**: almide-base, almide-lang (ast, types, stdlib_info), almide-ir, wasm-encoder, toml.
- **stdlib_info moved to almide-lang**: `STDLIB_MODULES`, `is_stdlib_module`, UFCS resolution tables moved from main crate's `stdlib.rs` to `almide_lang::stdlib_info` to break the codegen→main crate circular dependency. Main crate re-exports.
- **Generated files**: codegen crate has its own `build.rs` generating `arg_transforms.rs` and `rust_runtime.rs` to `src/generated/`. Main crate's build.rs now only generates `stdlib_sigs.rs`.
- **Dead code removed**: `emit_rust_calls.rs` (unused), `token_table.rs` (unused), `textmate_patterns.txt`, `tree_sitter_*.txt` removed from main crate's generated/.
- **`#![recursion_limit = "512"]`** needed for `wasm!` macro expansion.

## Phase 6: almide-frontend, almide-optimize, almide-tools (done)

Zero cross-group dependencies — clean three-way split.

- **almide-frontend** (~5.7k lines): `check/`, `canonicalize/`, `lower/`, `import_table.rs`, `stdlib.rs`, `type_env.rs`, `generated/stdlib_sigs.rs`. Has own `build.rs` for stdlib_sigs generation. Main crate's build.rs is now empty.
- **almide-optimize** (~2.5k lines): `optimize/`, `mono/`. `mono/propagation.rs` codegen dependency resolved by adding `wasm_types_compatible()` to almide-ir (no wasm-encoder dep).
- **almide-tools** (~1.5k lines): `fmt.rs`, `interface.rs`, `almdi.rs`.
- **almide (CLI)** (~3.6k lines): `main.rs`, `cli/`, `resolve.rs`, `project.rs`, `project_fetch.rs`. lib.rs is a pure `pub use` re-export map (no stub files).
- **AUTO_IMPORT_BUNDLED** moved to `almide_lang::stdlib_info`.
- **Re-export stubs removed**: lib.rs consolidates all module aliases via `pub use crate as module;`. 18 stub files deleted.

## Future: Breaking ast↔types Cycle

Remove `Expr.ty: Option<Ty>` from AST and use `TypeMap = HashMap<ExprId, Ty>` populated by the checker.

**Analysis complete (2026-04-01):**

Join points (only 2):
1. `ast::Expr.ty: Option<Ty>` — checker sets in 4 places, lower reads in ~50 places
2. `types::VariantPayload::Record(Vec<(Sym, Ty, Option<ast::Expr>)>)` — default expressions

Implementation plan:
1. Add `TypeMap = HashMap<ExprId, Ty>` to checker, populate instead of `expr.ty = Some(...)`
2. Add TypeMap to `LowerCtx`, helper method `ctx.ty(expr) -> Ty`
3. Replace all `expr.ty.clone().unwrap_or(...)` in lower/ (~50 sites) with `ctx.ty(expr)`
4. Remove `Expr.ty` field from `ast::Expr`
5. Change `VariantPayload::Record` default from `Option<ast::Expr>` to `Option<ExprId>`, store actual Exprs in side table
6. Split: almide-syntax (ast, lexer, parser) + almide-types (Ty, unify, constructor)

This is all-or-nothing (~50 files, 4 crates). ExprId already exists on Expr.
