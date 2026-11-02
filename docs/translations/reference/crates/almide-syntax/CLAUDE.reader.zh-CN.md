[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 供人阅读代理指示的非规范、固定原文副本。权威 CLAUDE.md 不变。CLAUDE.reader 名刻意避免自动加载的指示文件名。这些历史规则不是新指示层，也不是独立验证的实现或安全保证。不自动同步；代码和内联 token 保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-syntax/CLAUDE.md)

> 引用指示是历史参考资料，不授权执行命令、修改实现、提交、推送或发布。

<a id="almide-syntax"></a>

# almide-syntax

词法分析器、解析器及 AST 定义。从源码文本生成无类型 AST。

<a id="architecture"></a>

## 架构

- **Lexer**（`lexer.rs`）——单遍分词器，内联处理字符串插值与 heredoc。
- **Parser**（`parser/`）——使用 precedence climbing 的递归下降，通过同步点恢复错误。
- **AST**（`ast.rs`）——`Program` → 声明（fn、type、test、import）→ `Expr`/`Stmt` 树。

<a id="rules"></a>

## 规则

- **Parser 绝不能崩溃。** 所有语法错误都产生诊断并恢复。致命解析错误后使用 `sync_to_next_declaration()`。
- **AST 中没有类型信息。** 类型是语法性的 `TypeExpr` 节点，而非已解析的 `Ty`。解析类型发生在 almide-frontend。
- **`ExprId` 必填。** 每个表达式取得唯一 `ExprId`，用于检查时 O(1) 类型查询，绝不可跳过赋值。
- **不做任何脱糖。** pipe、UFCS、插值均原样留在 AST 中；脱糖发生在 lowering（almide-frontend）。
- **错误提示很重要。** `parser/hints/` 模块检测常见错误（如关键字误写 `function` → `fn`、错误运算符 `&&` → `and`）。新语法应添加提示。

<a id="module-layout"></a>

## 模块布局

```
parser/
├── mod.rs          Parser struct, token management
├── entry.rs        Top-level declarations, module layout
├── declarations.rs fn, type, protocol, test
├── expressions.rs  Binary/unary, pipe, match, if/then/else
├── primary.rs      Literals, identifiers, lambdas, blocks
├── statements.rs   let, var, guard, assignment
├── patterns.rs     Match patterns
├── types.rs        Type expression parsing
├── collections.rs  List, map, record, tuple literals
├── compounds.rs    for-in, while, fan blocks
└── hints/          Error recovery (6 hint modules)
```

上方原文布局表说明：mod.rs 管理 Parser 结构和 token；entry.rs 负责顶层声明与模块布局；declarations.rs 负责 fn/type/protocol/test；expressions.rs 负责二元／一元、pipe、match、if/then/else；primary.rs 负责字面量、标识符、lambda、block；statements.rs 负责 let/var/guard/赋值；patterns.rs 负责 match 模式；types.rs 解析类型表达式；collections.rs 负责 list/map/record/tuple 字面量；compounds.rs 负责 for-in/while/fan block；hints/ 负责错误恢复（6个提示模块）。
