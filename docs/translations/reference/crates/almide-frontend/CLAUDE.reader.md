[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative human reader snapshot of canonical agent instructions, fixed to the linked source. CLAUDE.reader is deliberately not an auto-loaded instruction filename. Original files stay unchanged. No new instruction layer, execution authority, automatic synchronization, or independently verified implementation guarantee. Source rules and qualifications are preserved; code remains verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-frontend/CLAUDE.md)

<a id="almide-frontend"></a>

# almide-frontend

Type checking and AST → IR lowering. The semantic analysis core.

<a id="architecture"></a>

## Architecture

<a id="type-checker-check"></a>

### Type Checker (`check/`)

Three-pass constraint-based inference:

1. **Infer** (`infer.rs`) — Walk AST, assign fresh `TypeVar`s, collect `Constraint`s.
2. **Solve** (`solving.rs`) — Unify constraints via structural unification. Produces substitution.
3. **Resolve** — Apply substitution to `TypeMap`, replacing all `TypeVar`s with concrete types.

Output: `TypeMap = HashMap<ExprId, Ty>` — the authoritative source of all expression types.

<a id="lowering-lower"></a>

### Lowering (`lower/`)

AST + TypeMap → `IrProgram`:

- Assigns `VarId` for every variable binding.
- Desugars pipes, UFCS, string interpolation, operators.
- Resolves call targets (Module/Method/Named/Computed).
- Auto-derives Eq, Repr, Ord, Hash, Codec.

<a id="rules"></a>

## Rules

- **Checker is the source of truth.** If something has the wrong type, fix it in the checker — NOT in lowering or codegen.
- **TypeMap populated by checker, consumed by lowering.** Lowering reads `type_map[expr_id]` to annotate IR nodes. It never runs inference.
- **Desugar in lowering, not in the checker.** The checker sees the original AST. Lowering transforms it into the IR form.
- **Error recovery with `Unknown`.** When inference fails, the checker emits a diagnostic and assigns `Ty::Unknown`. This prevents cascade errors but can leak into IR — codegen must handle `Unknown` gracefully (fallback defaults).
- **Exhaustiveness is mandatory.** Match patterns are checked for exhaustiveness in `check/exhaustiveness.rs`. Missing cases produce error E010.
- **Auto-derive generates real IR.** `lower/derive.rs` produces `IrFunction` bodies for convention methods. These are regular functions — codegen doesn't special-case them.

<a id="module-layout"></a>

## Module Layout

```
check/
├── mod.rs            Checker orchestration
├── infer.rs          Pass 1: constraint collection
├── calls.rs          Call resolution (UFCS, builtins, constructors)
├── solving.rs        Pass 2: constraint solving
├── types.rs          TyVarId, Constraint, UnionFind
├── builtin_calls.rs  Special type rules for builtins
├── static_dispatch.rs  Impl block dispatch
├── diagnostics.rs    Error formatting
└── exhaustiveness.rs Match exhaustiveness

lower/
├── mod.rs            Lowering entry, LowerCtx
├── expressions.rs    Expr → IrExpr
├── calls.rs          Call target resolution
├── statements.rs     Stmt + pattern → IrStmt
├── types.rs          Type decl lowering
├── derive.rs         Eq/Repr/Ord/Hash derive
└── derive_codec.rs   Codec derive
```
