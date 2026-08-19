# almide-ir

Typed Intermediate Representation consumed by all codegen targets.

## Key Types

- **`IrProgram`** — Top-level: functions, type declarations, top_lets, modules, var_table.
- **`IrExpr`** / **`IrExprKind`** — Expression nodes with `ty: Ty` on every node.
- **`IrFunction`** — Function definition with params, ret_ty, body, flags (is_effect, is_test).
- **`VarId(u32)`** — Unique variable identifier. **`VarTable`** maps VarId → VarInfo (name, type, mutability).
- **`BinOp`** / **`UnOp`** — Type-dispatched operators (AddInt vs AddFloat, etc.).
- **`CallTarget`** — Resolved call destination (Module, Method, Named, Computed).

## Rules

- **Every IrExpr carries its type.** `expr.ty` must be set during lowering. Codegen reads it directly — it does NOT re-infer.
- **VarId is the only way to reference variables.** No string-based lookups in IR. If you need a variable's name, go through `var_table.get(var_id).name`.
- **BinOp is type-dispatched.** `a + b` on Int becomes `BinOp::AddInt`, on Float becomes `BinOp::AddFloat`, on String becomes `BinOp::ConcatStr`. Use `op.result_ty()` for the result type.
- **Visitor pattern for traversal.** Use `visit::IrVisitor` + `walk_expr`/`walk_stmt` for exhaustive traversal. Override only the nodes you need.
- **`use_count` is post-computed.** Variable use counts are populated after lowering. Don't assume they're accurate during lowering itself.

## Adding New IR Nodes

1. Add variant to `IrExprKind` (or `IrStmtKind`).
2. Update `visit.rs` (`walk_expr`/`walk_stmt`) — the visitor MUST traverse all children.
3. Update `fold.rs` if the node has transformable sub-expressions.
4. Update `substitute.rs` if the node binds or references variables.
5. Codegen: handle in walker (`walker/expressions.rs`) and WASM emitter (`emit_wasm/expressions.rs`).
