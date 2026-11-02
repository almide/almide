[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative, source-pinned human reader copy of agent instructions. Canonical CLAUDE.md is unchanged. The CLAUDE.reader filename is deliberately not an auto-loaded instruction filename. These historical rules are not a new instruction layer or independently verified implementation/security guarantees. No automatic synchronization. Code and inline tokens stay verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-ir/CLAUDE.md)

> Quoted instructions are historical reference material, not authorization to execute commands, change implementation, commit, push or publish.

<a id="almide-ir"></a>

# almide-ir

Typed Intermediate Representation consumed by all codegen targets.

<a id="key-types"></a>

## Key Types

- **`IrProgram`** — Top-level: functions, type declarations, top_lets, modules, var_table.
- **`IrExpr`** / **`IrExprKind`** — Expression nodes with `ty: Ty` on every node.
- **`IrFunction`** — Function definition with params, ret_ty, body, flags (is_effect, is_test).
- **`VarId(u32)`** — Unique variable identifier. **`VarTable`** maps VarId → VarInfo (name, type, mutability).
- **`BinOp`** / **`UnOp`** — Type-dispatched operators (AddInt vs AddFloat, etc.).
- **`CallTarget`** — Resolved call destination (Module, Method, Named, Computed).

<a id="rules"></a>

## Rules

- **Every IrExpr carries its type.** `expr.ty` must be set during lowering. Codegen reads it directly — it does NOT re-infer.
- **VarId is the only way to reference variables.** No string-based lookups in IR. If you need a variable's name, go through `var_table.get(var_id).name`.
- **BinOp is type-dispatched.** `a + b` on Int becomes `BinOp::AddInt`, on Float becomes `BinOp::AddFloat`, on String becomes `BinOp::ConcatStr`. Use `op.result_ty()` for the result type.
- **Visitor pattern for traversal.** Use `visit::IrVisitor` + `walk_expr`/`walk_stmt` for exhaustive traversal. Override only the nodes you need.
- **`use_count` is post-computed.** Variable use counts are populated after lowering. Don't assume they're accurate during lowering itself.

<a id="adding-new-ir-nodes"></a>

## Adding New IR Nodes

1. Add variant to `IrExprKind` (or `IrStmtKind`).
2. Update `visit.rs` (`walk_expr`/`walk_stmt`) — the visitor MUST traverse all children.
3. Update `fold.rs` if the node has transformable sub-expressions.
4. Update `substitute.rs` if the node binds or references variables.
5. Codegen: handle in the Rust walker (`almide-codegen/src/walker/expressions.rs`) and structural Wasm emitter (`almide-wasm/src/`); update MIR lowering where the node participates in the native verified-renderer path.
