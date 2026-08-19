# almide-frontend

Type checking and AST → IR lowering. The semantic analysis core.

## Architecture

### Type Checker (`check/`)

Three-pass constraint-based inference:

1. **Infer** (`infer.rs`) — Walk AST, assign fresh `TypeVar`s, collect `Constraint`s.
2. **Solve** (`solving.rs`) — Unify constraints via structural unification. Produces substitution.
3. **Resolve** — Apply substitution to `TypeMap`, replacing all `TypeVar`s with concrete types.

Output: `TypeMap = HashMap<ExprId, Ty>` — the authoritative source of all expression types.

### Lowering (`lower/`)

AST + TypeMap → `IrProgram`:

- Assigns `VarId` for every variable binding.
- Desugars pipes, UFCS, string interpolation, operators.
- Resolves call targets (Module/Method/Named/Computed).
- Auto-derives Eq, Repr, Ord, Hash, Codec.

## Rules

- **Checker is the source of truth.** If something has the wrong type, fix it in the checker — NOT in lowering or codegen.
- **TypeMap populated by checker, consumed by lowering.** Lowering reads `type_map[expr_id]` to annotate IR nodes. It never runs inference.
- **Desugar in lowering, not in the checker.** The checker sees the original AST. Lowering transforms it into the IR form.
- **Error recovery with `Unknown`.** When inference fails, the checker emits a diagnostic and assigns `Ty::Unknown`. This prevents cascade errors but can leak into IR — codegen must handle `Unknown` gracefully (fallback defaults).
- **Exhaustiveness is mandatory.** Match patterns are checked for exhaustiveness in `check/exhaustiveness.rs`. Missing cases produce warnings.
- **Auto-derive generates real IR.** `lower/derive.rs` produces `IrFunction` bodies for convention methods. These are regular functions — codegen doesn't special-case them.

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
