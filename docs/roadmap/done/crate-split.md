<!-- description: Split compiler into workspace crates for build parallelism and API boundaries -->
<!-- done: 2026-04-01 -->
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
almide-syntax    ast, lexer, parser                        ~8k lines
almide-types     Ty, unify, constructor, stdlib_info       ~2k lines
almide-lang      re-export: syntax + types                 (thin shim)
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
almide-syntax     → base
almide-types      → base
almide-lang       → syntax, types              (re-export shim)
almide-ir         → base, lang
almide-frontend   → base, lang, ir
almide-optimize   → ir, lang (types)
almide-codegen    → base, lang, ir, wasm-encoder, toml
almide-tools      → base, lang, ir
almide            → all
```

## Design Decisions

- **ast + types split (almide-syntax + almide-types)**: Bidirectional dependency broken in Phase 7 (`Expr.ty` → TypeMap, `VariantPayload::Record` default exprs removed), split in Phase 8. almide-lang remains as a re-export shim for backward compatibility.
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
| 7 | Break ast↔types cycle (TypeMap, VariantPayload) | Done (2026-04-01) |
| 8 | Split almide-lang → almide-syntax + almide-types | Done (2026-04-01) |

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

## Phase 7: Breaking ast↔types Cycle (done)

Both directions of the bidirectional dependency eliminated:

1. **ast→types**: Removed `Expr.ty: Option<Ty>` from `ast::Expr`. Checker now populates `TypeMap = HashMap<ExprId, Ty>` (`Checker.type_map`). LowerCtx reads from the TypeMap via `expr_ty()`.
2. **types→ast**: Simplified `VariantPayload::Record(Vec<(Sym, Ty, Option<ast::Expr>)>)` to `VariantPayload::Record(Vec<(Sym, Ty)>)`. The default expressions were never read from VariantPayload — lowering reads them from `ast::FieldType.default` directly.

Files changed: ~25 across 4 crates (almide-lang, almide-frontend, almide-codegen, almide CLI).

## Phase 8: Split almide-lang → almide-syntax + almide-types (done)

Clean split enabled by Phase 7's cycle elimination:

- **almide-syntax** (~8k lines): ast, lexer, parser → almide-base only
- **almide-types** (~2k lines): Ty, unify, constructor, stdlib_info → almide-base only
- **almide-lang** (thin shim): `pub use almide_syntax::*; pub use almide_types::*;` — downstream crates unchanged
- **TypeMap** stays in almide-frontend (bridges ExprId from syntax and Ty from types)

Zero downstream changes required — almide-lang re-exports preserve all existing `almide_lang::ast::*` and `almide_lang::types::*` paths.
