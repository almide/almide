<a id="design-philosophy"></a>

# 设计理念

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](DESIGN.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](DESIGN.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](DESIGN.zh-CN.md)



> 原文：[docs/design/DESIGN.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/DESIGN.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，英语原文）。这是面向读者的非规范性快照译文，不会自动同步。保留原文的时间、适用范围与主张；翻译时未进行独立测量或实现验证。 原文的 MSR 规则及将首次成功作为唯一竞争轴的说法，与同一固定版本 CLAUDE.md 和 demo/make-verify/README.md 关于 MSR 天花板效应与指标替换的说明不一致，作为历史记述保留。不要视为当前实测结果或已执行的采用规则。他语言比较及“消除”“始终”等效果也是原文设计主张，并非本译文验证的保证。

Almide 针对**最少思考 token**优化：LLM 对语法、语义、修复策略或缺失抽象需要考虑的分支越少，代码生成就越快、越便宜、越可靠。这意味着减少生成、补全和修复过程中的分支，同时提供适当工具，使 AI 无须围绕缺失的抽象临时拼凑方案。

<a id="surface-ambiguity-removed"></a>

## 消除表层歧义

| 歧义来源 | 其他语言 | Almide | 对 token 分支的影响 |
|---|---|---|---|
| Null 处理 | `null`、`nil`、`None`、`undefined` | 只有 `Option[T]` | 消除 null 检查幻觉 |
| 错误处理 | `throw`、`try/catch`、`panic`、错误码 | 只有 `Result[T, E]` | 错误路径始终在类型中可见 |
| 泛型 | `<T>`（与 `<` `>` 混淆） | `[T]` | 不与比较产生解析歧义 |
| 循环 | `while`、`for`、`loop`、`forEach`、递归 | 集合迭代用 `for x in xs { }`，条件循环用 `while cond { ... }` | 每种形式只有一个目的 |
| 提前退出 | `return`、`break`、`continue`、`throw` | 最后表达式作为函数结果；`guard ... else` 用于结构化退出，循环内使用 `break`/`continue` | 没有提前 return 的混淆 |
| Lambda | `=>`、`->`、`lambda`、`fn`、`\x ->`、块 | 只有 `(x) => expr` | 一种语法，没有替代形式 |
| 语句结束 | `;`、可选 `;`、ASI 规则 | 换行分隔 | 没有插入歧义 |
| 条件分支 | 带可选 `else` 的 `if`、三元 `?:` | `if/then/else`；省略 `else` 时产生 Unit | 没有悬空 else |
| 副作用 | 可在任意位置隐式出现 | `effect fn` 标记 effect 调用；普通 `fn` 也允许 output/abort 内置函数 | 限制每个位置的可调用集合 |
| 运算符含义 | 自定义运算符语法、隐式强制转换 | 固定运算符词汇；已声明的 protocol 符合关系可将运算符分派到 requirement method。没有隐式强制转换 | 解析遵循操作数类型和 protocol 要求 |
| 类型转换 | 隐式扩展、强制转换 | 只有显式转换 | 没有隐藏的类型变化 |

<a id="semantic-ambiguity-removed"></a>

## 消除语义歧义

| 歧义来源 | Almide 的做法 | 对 LLM 的意义 |
|---|---|---|
| 名称解析 | 自动导入 core 模块（`int`、`string`、`list`、`map`、`math`、`datetime` 等）；有 effect 的平台模块（`fs`、`env`、`io`、`json`、`random` 等）需要显式 `import` | LLM 不用猜可用名称；core 操作始终可用 |
| 类型推断 | 仅局部——函数签名必须有注解 | 不跨远距离定义推断 |
| 重载 | 无——名称不参与 ad-hoc 重载解析 | 没有分派歧义 |
| 隐式转换 | 无——使用 `int.to_string(n)`，绝不自动强制转换 | 每次转换在源码中可见 |
| Protocol 查找 | 类型声明满足 protocol；convention method（`fn Type.method`）提供实现——没有全局隐式实例解析 | 无须全局实例搜索 |
| 方法解析 | 规范调用带模块限定；考虑接收者的 UFCS 由 checker 解析 | 解析遵循显式接收者/作用域规则 |
| 声明顺序 | 函数可自由相互引用 | 没有前向声明混淆 |
| 导入风格 | `import module` 或 `import self as alias`——每种目的一个形式；别名仅用于 self-import 和子模块；core 模块自动导入 | 两种导入形式，没有 `from`，没有 `*` |

<a id="the-effect-system-as-generation-space-reducer"></a>

## 将 `effect` 系统用作生成空间缩减器

`effect fn` 首要目的不是安全功能——它是**代码生成的搜索空间缩减器**。

- 普通 `fn` 拒绝直接 effect 调用；output/abort 内置函数和透明回调参数有文档规定的例外
- `effect fn` 明确标出 I/O 边界 → LLM 准确知道副作用在哪些位置合法
- effect 不匹配在编译时被发现 → 错误调用在执行前被拒绝
- 仅凭函数签名就能告诉 LLM 每个位置可调用什么，无须阅读函数体

这意味着 LLM 只看当前函数签名及其导入就能生成代码——无须全局分析。

<a id="concurrency-fan--boring-on-purpose"></a>

## 并发：`fan`——有意保持平淡

Almide 使用显式 fork/join，不暴露 `async`/`await`。普通 `fan` 会 join 每个 sibling，并按源码/列表顺序报告第一个错误，而不是按实际时间报告最先发生的失败。

两种常见形式：

- **`fan { a(), b() }`**——将表达式作为一个有作用域的单元执行，等待全部完成，按声明顺序返回元组；编译器和 host 选择执行基础设施
- **`fan.map(xs, f)`**——执行每个元素并按列表顺序收集结果；能否并行执行取决于回调与目标

规则尽量少：

- `fan { }` 只在 `effect fn` 内有效——纯函数不能 fork
- 若某个 arm 返回 `Err`，块会在 join 所有 sibling 后返回按 arm 顺序的第一个 `Err`
- `fan` 内不能捕获 `var`——只能读取外层作用域的 `let` 绑定（防止数据竞争）
- 没有非结构化 `spawn`——所有并发都有作用域

普通 `fan` 不取消 sibling。budget/deadline 形式有单独的协作式截断规则；参见 [SPEC §13](../../../../SPEC.md#13-concurrency-fan)。

<a id="ufcs-why-two-forms-is-acceptable"></a>

## UFCS：为什么两种形式可以接受

当 UFCS 解析选中 `f` 时，`x.f(y)` 将 `x` 作为第一个参数传入。这不是解析时无条件重写：

- **规范解析使用模块限定的函数形式**：`module.fn(args)`——模块前缀使解析无歧义
- **自动导入使模块可用**，不是让每个函数名全局可用；使用 `string.len(s)` 或 `s.len()` 等接收者语法
- **方法形式只是为链式调用提供的语法糖**：`x.f(y).g(z)` 从左到右阅读
- checker 使用接收者类型和作用域解析 convention method、局部/导入函数与内置模块方法（`calls_ufcs.rs`）
- 将来的 formatter 会规范化为规范形式，消除风格漂移

<a id="iteration-forin--while"></a>

## 迭代：`for...in` + `while`

两种循环结构，各有明确用途：

- **`for x in xs { ... }`**——遍历集合。适合列表和 map 键的自然选择。兼容 effect（循环体内可以 I/O）。
- **`while cond { ... }`**——基于条件的循环。条件为真时运行。具有简单循环条件时的直接选择。

<a id="compiler-diagnostics-single-likely-fix"></a>

## 编译器诊断：一个最可能的修复

Almide 的诊断旨在让**每个错误恰好指向一种修复**。这对 LLM 修复循环至关重要：

- 被拒绝的语法（前缀 `!x`、`return`、`class`、`null`）带有指出准确 Almide 等价写法的提示
- 每个解析位置的预期 token 保持为小型、可枚举集合
- parser 恢复不猜测——以精确位置和预期快速失败
- `_` 占位和 `todo()` 标记未完成代码；不保证未完成程序能成功编译或运行

```
Prefix '!x' is not boolean negation in Almide
  Hint: Use 'not x' for boolean negation, not '!x'.

'return' is not valid in Almide at line 12:5
  Hint: Use the last expression as the return value, or 'guard ... else' for early exit.
```

<a id="stdlib-naming-conventions"></a>

## 标准库命名规则

标准库遵循严格命名规则，减少 LLM 猜测：

| 约定 | 规则 | 示例 |
|---|---|---|
| 模块前缀 | 规范形式是 `module.function()`；UFCS 解析后，接收者语法可以替代限定调用 | `string.len(s)`、`list.get(xs, i)`、`map.get(m, k)`（core 模块自动导入） |
| 谓词前缀 | 返回布尔值的函数用 `is_` | `string.is_empty(s)`、`list.is_empty(xs)`、`map.contains(m, key)` |
| 返回类型一致性 | 可能失败的查找返回 `Option`，可能失败的 I/O 返回 `Result`，不会失败的纯转换返回普通值 | `list.get() -> Option[T]`、`fs.read_text() -> Result[String, String]`（effect fn） |
| 没有同义词 | 每个操作一个名称，没有别名 | `len` 而非 `length`/`size`/`count` |
| 对称配对 | 逆操作采用对应名称 | `read_text`/`write_text`、`split`/`join`、`to_string`/`to_int` |
| 没有名称语义漂移 | 只有跨模块语义匹配时才复用相同操作名 | `string.len` 和 `list.len` 都表示“计数元素”，不在同名之下隐藏不同操作 |

<a id="what-almide-sacrifices"></a>

## Almide 牺牲了什么

这些是有意的权衡——为了让 LLM 生成可靠而舍弃的事物：

| 牺牲项 | 原因 |
|---|---|
| 原始表达力 | 每个概念只有一种惯用写法。Almide 提供恰当抽象（如 `map`、`for...in`），但不提供实现同一件事的多种方式。 |
| 自定义运算符 | 没有自定义运算符语法。现有运算符可以通过已声明的 protocol（`Eq`、`Numeric`、顺序要求）分派；没有隐式强制转换。 |
| 元编程 | 没有宏、反射、代码生成。语言表层固定。 |
| ad-hoc 多态 | `protocol` 定义必需的 convention method。类型显式声明满足关系（`type Dog: Eq, Serializable`）。通过扁平 convention method（`fn Dog.serialize(...)`）实现，无 `impl` 块。内置 convention（Eq、Repr、Ord、Hash、Codec）是 protocol。泛型约束：`fn f[T: Protocol](x: T)`——单态化，没有动态分派。没有隐式实例解析，没有 orphan rule。 |
| 命名/默认参数 | 支持默认参数（`fn f(x: Int, y: Int = 0)`）；命名参数用于明确含义（`f(x: 1, y: 2)`）。没有可变参数。 |
| 多种返回风格 | 没有 `return` 关键字。最后表达式始终是值。无例外。 |
| 多样语法糖 | 小型规范表层配合文档规定的语法糖，如 `T?`、`T!E`、UFCS 和记录字段简写。具体文法优先于设计口号。 |
| DSL 能力 | 没有运算符定义，没有自定义语法。Almide 代码始终像 Almide。 |

这些不是缺失功能——而是**保持生成空间聚焦的有意约束**。目标不是为极简而极简，而是确保每个抽象有一条清晰路径。

<a id="the-msr-rule-how-features-are-added--rejected"></a>

## MSR 规则（如何添加 / 拒绝功能）

> **设计规则**：对人类美观、对 LLM 却是陷阱的语法不予采用。

每次语言变更都根据其 **MSR delta** 评估——即在 [almide-dojo](https://github.com/almide/almide-dojo) 上，针对 `llama-3.3-70b`、`llama-3.1-8b` 和（可用时）`Claude Sonnet 4.6`，以 `T=0` 运行 3 次测得的重试成功率变化。

满足以下任一项则**拒绝**变更：
- 使任一模型的 MSR 中位数下降 ≥ 3 个百分点，或
- 为已有规范形式的事物引入第二种规范形式（分裂 LLM 训练数据），或
- 以生成可预测性为代价改善人类审美。

满足以下任一项则**接受**变更：
- 可测量地提升 MSR，**或**
- 消除诊断后重试也无法修复的一类错误（天花板效应）。

从 `v0.14.5` 起的发布说明包含 **MSR delta 表**，让每次发布对自身影响负责。`docs/roadmap/active/` 中的新功能提案必须引用所针对的失败模式及预期 MSR 变化。

<a id="beauty-axis-vs-msr-axis"></a>

## 美观轴与 MSR 轴

Almide 不在以下方面竞争：
- 纯 FP 美学（Elm、Haskell 在此更强——它们为这种美牺牲了 LLM 易写性）。
- 依赖类型 / 线性类型 / 将 effect 作为一等类型（研究语言在此胜出——代价是易写性断崖式下降）。
- DSL 表达力、宏、元编程。

Almide 只在一件事上竞争：**LLM 首次尝试就写出正确代码**。上面的每项权衡，以及将来的每项权衡，都以这条轴为裁决标准。
