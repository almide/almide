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
almide-codegen    → base, lang, ir
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
| 5 | almide-codegen | Pending |
| 6 | almide-frontend, almide-optimize, almide-tools | Pending |

## Phase 5: almide-codegen (next)

The largest extraction (~30k lines). Key considerations:

- **Dependencies**: almide-base, almide-lang (ast, types), almide-ir. Does NOT depend on check/lower/stdlib.
- **Internal structure**: `mod.rs` (entry), `pass.rs` (nanopass framework), `target.rs`, `template.rs`, `walker/` (8 files), `emit_wasm/` (38 files), 20 `pass_*.rs` files.
- **Blockers**: `generated/` files (`emit_rust_calls.rs`, `arg_transforms.rs`, `stdlib_sigs.rs`) are included by codegen. These must either move into almide-codegen or stay in the main crate with codegen referencing them.
- **build.rs**: Currently generates files into `src/generated/`. Needs to generate into the codegen crate, or codegen includes them via `include!()`.
- **annotations.rs**: Already moved to almide-ir. codegen re-exports it.
- **pass_effect_inference.rs**: Effect/FunctionEffects/EffectMap already moved to almide-ir. The pass itself stays in codegen.

### Steps

1. Create `crates/almide-codegen/` with Cargo.toml depending on base, lang, ir
2. Move `src/codegen/` contents (except `annotations.rs` re-export)
3. Handle `generated/` files — either move or use `include!()` from main crate build
4. Fix `crate::` references → `almide_lang::`, `almide_ir::`, `almide_base::`
5. Leave re-export stub in main crate: `pub use almide_codegen::*;`
6. Test all targets (Rust + WASM)

## Phase 6: almide-frontend, almide-optimize, almide-tools

After codegen is extracted, the remaining `src/` modules split naturally:

- **almide-frontend**: `check/`, `canonicalize/`, `lower/`, `import_table.rs`, `stdlib.rs`, `types/env.rs`, `generated/stdlib_sigs.rs`
- **almide-optimize**: `optimize/`, `mono/`
- **almide-tools**: `fmt.rs`, `interface.rs`, `almdi.rs`
- **almide (CLI)**: `main.rs`, `cli/`, `resolve.rs`, `project.rs`, `project_fetch.rs`

## Future: Breaking ast↔types Cycle

The clean long-term fix is removing `Expr.ty: Option<Ty>` from the AST and using an external `HashMap<ExprId, Ty>` populated by the checker. This would allow:
- almide-syntax (ast + lexer + parser) — no type system dependency
- almide-types (Ty, TypeEnv, unify) — no AST dependency

This is a separate refactor with ~50 files touched. Track independently.
