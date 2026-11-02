[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 固定到链接原文的、供人阅读权威代理指示的非规范副本。CLAUDE.reader 刻意避开自动加载指示的文件名，原本不变。不是新指示层、执行授权、自动同步或独立验证的实现保证。保留原文规则与限定，代码不变。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-frontend/CLAUDE.md)

<a id="almide-frontend"></a>

# almide-frontend

类型检查与 AST → IR lowering，是语义分析核心。

<a id="architecture"></a>

## 架构

<a id="type-checker-check"></a>

### 类型检查器（`check/`）

三遍基于约束的推断：

1. **推断**（`infer.rs`）——遍历 AST，分配新的 `TypeVar`，收集 `Constraint`。
2. **求解**（`solving.rs`）——通过结构统一来统一约束，产生替换。
3. **解析**——将替换应用到 `TypeMap`，把所有 `TypeVar` 替换为具体类型。

输出：`TypeMap = HashMap<ExprId, Ty>`——全部表达式类型的权威来源。

<a id="lowering-lower"></a>

### Lowering（`lower/`）

AST + TypeMap → `IrProgram`：

- 为每个变量绑定分配 `VarId`。
- 对 pipe、UFCS、字符串插值、运算符脱糖。
- 解析调用目标（Module/Method/Named/Computed）。
- 自动派生 Eq、Repr、Ord、Hash、Codec。

<a id="rules"></a>

## 规则

- **检查器是权威来源。** 若类型错误，应在检查器中修复，而不是在 lowering 或 codegen 中修复。
- **TypeMap 由检查器填充、lowering 使用。** lowering 读取 `type_map[expr_id]` 为 IR 节点标注类型，绝不运行推断。
- **在 lowering 脱糖，不在检查器脱糖。** 检查器看到原始 AST，lowering 将其变换为 IR 形式。
- **用 `Unknown` 恢复错误。** 推断失败时，检查器发出诊断并赋予 `Ty::Unknown`。这防止级联错误，但可能泄漏到 IR；codegen 必须妥善处理 `Unknown`（fallback 默认值）。
- **穷尽性必需。** 在 `check/exhaustiveness.rs` 中检查 match 模式的穷尽性，缺失情况产生 E010。
- **自动派生生成真实 IR。** `lower/derive.rs` 为约定方法生成 `IrFunction` 函数体。这些是普通函数，codegen 不作特殊处理。

<a id="module-layout"></a>

## 模块布局

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

原文布局表说明：check/ 中 mod.rs 负责检查器协调，infer.rs 负责第1遍约束收集，calls.rs 负责调用解析（UFCS、内建、构造器），solving.rs 负责第2遍约束求解，types.rs 包含 TyVarId/Constraint/UnionFind，builtin_calls.rs 负责内建特殊类型规则，static_dispatch.rs 负责 impl block 派发，diagnostics.rs 负责错误格式化，exhaustiveness.rs 负责 match 穷尽性。lower/ 中 mod.rs 为入口与 LowerCtx，expressions.rs 负责 Expr → IrExpr，calls.rs 解析调用目标，statements.rs 负责 Stmt＋pattern → IrStmt，types.rs 负责类型声明 lowering，derive.rs 派生 Eq/Repr/Ord/Hash，derive_codec.rs 派生 Codec。
