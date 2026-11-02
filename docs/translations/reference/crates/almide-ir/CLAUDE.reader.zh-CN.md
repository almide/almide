[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 供人阅读代理指示的非规范、固定原文副本。权威 CLAUDE.md 不变。CLAUDE.reader 名刻意避免自动加载的指示文件名。这些历史规则不是新指示层，也不是独立验证的实现或安全保证。不自动同步；代码和内联 token 保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-ir/CLAUDE.md)

> 引用指示是历史参考资料，不授权执行命令、修改实现、提交、推送或发布。

<a id="almide-ir"></a>

# almide-ir

由所有 codegen 目标使用的带类型中间表示。

<a id="key-types"></a>

## 关键类型

- **`IrProgram`**——顶层：函数、类型声明、top_lets、modules、var_table。
- **`IrExpr`** / **`IrExprKind`**——每个节点都带 `ty: Ty` 的表达式节点。
- **`IrFunction`**——含 params、ret_ty、body、flags（is_effect、is_test）的函数定义。
- **`VarId(u32)`**——唯一变量标识符。**`VarTable`** 映射 VarId → VarInfo（名称、类型、可变性）。
- **`BinOp`** / **`UnOp`**——按类型派发的运算符（如 AddInt 与 AddFloat）。
- **`CallTarget`**——已解析的调用目标（Module、Method、Named、Computed）。

<a id="rules"></a>

## 规则

- **每个 IrExpr 都携带类型。** lowering 时必须设置 `expr.ty`。codegen 直接读取，不重新推断。
- **只能通过 VarId 引用变量。** IR 中不按字符串查找。需要变量名时通过 `var_table.get(var_id).name`。
- **BinOp 按类型派发。** Int 上的 `a + b` 成为 `BinOp::AddInt`，Float 成为 `BinOp::AddFloat`，String 成为 `BinOp::ConcatStr`。用 `op.result_ty()` 取得结果类型。
- **遍历采用 visitor 模式。** 用 `visit::IrVisitor` 加 `walk_expr`/`walk_stmt` 完整遍历，仅覆盖所需节点。
- **`use_count` 在之后计算。** 变量使用次数在 lowering 后填充，不要假定 lowering 过程中已准确。

<a id="adding-new-ir-nodes"></a>

## 添加新 IR 节点

1. 向 `IrExprKind`（或 `IrStmtKind`）添加 variant。
2. 更新 `visit.rs`（`walk_expr`/`walk_stmt`）；visitor 必须遍历所有子节点。
3. 若节点含可变换子表达式，更新 `fold.rs`。
4. 若节点绑定或引用变量，更新 `substitute.rs`。
5. Codegen：在 Rust walker（`almide-codegen/src/walker/expressions.rs`）和 structural Wasm emitter（`almide-wasm/src/`）中处理；若节点参与 native verified-renderer 路径，还须更新 MIR lowering。
