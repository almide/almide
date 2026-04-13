# Almide Compiler Crates

> **Active redesign**: [docs/roadmap/active/codegen-ideal-form.md](../docs/roadmap/active/codegen-ideal-form.md) — codegen の理想形に向けたリファクタ計画。新しいパスや emit 修正を入れる前に一読。

## Pipeline

```
Source (.almd)
  → almide-syntax    Lex + parse → AST
  → almide-frontend  Type check + lower → IR
  → almide-optimize  Monomorphize + DCE → IR
  → almide-codegen   Nanopass + emit → Rust / WASM
```

## Dependency Graph

```
almide-base           Interned strings (Sym), spans, diagnostics
  ↕
almide-types          Ty enum, unification, stdlib info
almide-syntax         Lexer, parser, AST nodes
  ↕
almide-lang           Re-export facade (types + syntax)
  ↕
almide-ir             Typed IR, VarTable, visitors
  ↕
almide-frontend       Type checker, constraint solver, AST→IR lowering
almide-optimize       Monomorphization, DCE, constant propagation
almide-codegen        Nanopass pipeline, TOML templates, walker, WASM emit
almide-tools          Formatter, module interface, language server
```

## Core Design Principles

1. **Type checker is source of truth.** All expression types come from `TypeMap` (populated by almide-frontend). Lowering and codegen trust it — they do NOT re-infer types.

2. **IR carries full type info.** Every `IrExpr` has a `ty: Ty` field. Codegen must never need to query the type checker at emit time.

3. **VarId eliminates shadowing.** All variables are assigned unique `VarId(u32)` during lowering. No string-based variable lookup in IR or codegen.

4. **Desugar once in lowering.** Pipes (`|>`), UFCS (`x.method()`), string interpolation (`"${expr}"`) are desugared in almide-frontend's lowering pass. Codegen never sees these forms.

5. **Nanopass isolation.** Each codegen pass does one semantic transformation. The walker is target-agnostic — it never checks `if target == Rust`. Target differences are encoded in pass selection and TOML templates.

6. **String interning everywhere.** All identifiers, type names, field names are `Sym` (interned, `Copy`). Compare with `==`, not string matching. Use `almide_base::intern::sym()` to intern, `.as_str()` to read.

## When Adding a New Feature

- **New syntax** → almide-syntax (parser) → almide-frontend (checker + lowering) → almide-codegen (passes + templates)
- **New stdlib function** → `stdlib/defs/<module>.toml` + `runtime/rs/<module>.rs` + WASM runtime in `emit_wasm/rt_*.rs`
- **New type** → almide-types (Ty variant) → almide-frontend (inference rules) → almide-ir (IR nodes) → almide-codegen (emission)
- **New codegen target** → almide-codegen (pass pipeline + TOML template + target entry)
