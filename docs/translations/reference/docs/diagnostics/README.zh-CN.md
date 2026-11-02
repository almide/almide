# Almide 诊断代码 — 简体中文译文

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



这是对应英文原文固定提交 `9a95b28519b90ae535dc9918bfed04805dc4675c` 的非规范性阅读译文。编译器嵌入的英文原文和诊断代码台账均未修改。本译文不会将 `almide explain` 或 `--list` 的输出本地化。修改原始代码页的语言链接也会改变 CLI 输出，因此语言切换入口设在本索引。

[Source @ 9a95b285](https://github.com/almide/almide/blob/9a95b28519b90ae535dc9918bfed04805dc4675c/docs/diagnostics/README.md)

这是检查器和规范化器输出的 `EXXX` 代码参考。用 `almide explain <code>` 从 CLI 阅读；用 `almide explain --list [--json]` 查看每个代码的严重程度、首次发布版本和 fix-it 判定。新增代码时，既要添加文档，也要在 [codes.toml](../../../../diagnostics/codes.toml) 添加一行；`tests/explain_list_test.rs` 保证列表与编译器输出的代码集合一致。

| 代码 | 说明 |
|---|---|
| [E001](E001.zh-CN.md) | 类型不匹配 |
| [E002](E002.zh-CN.md) | 未定义的函数 |
| [E003](E003.zh-CN.md) | 未定义的变量 |
| [E004](E004.zh-CN.md) | 参数数量错误 |
| [E005](E005.zh-CN.md) | 参数类型不匹配（构造器／函数调用） |
| [E006](E006.zh-CN.md) | 效应隔离：纯 fn 调用了 effect fn |
| [E007](E007.zh-CN.md) | 位于 effect fn 外的 `fan` 块 |
| [E008](E008.zh-CN.md) | 并发执行的主体可触及 `var` |
| [E009](E009.zh-CN.md) | 对不可变绑定重新赋值 |
| [E010](E010.zh-CN.md) | 不穷尽的 match |
| [E011](E011.zh-CN.md) | 在 pure fn 的闭包内修改可变 var |
| [E012](E012.zh-CN.md) | 重复定义 |
| [E013](E013.zh-CN.md) | 对非记录值访问字段／缺失字段 |
| [E014](E014.zh-CN.md) | 不可达的 match arm |
| [E015](E015.zh-CN.md) | 可能重新实现了标准库 |
| [E016](E016.zh-CN.md) | 需要相等性或哈希的位置包含函数 |
| [E017](E017.zh-CN.md) | 在 enum 类型名上构造记录 |
| [E018](E018.zh-CN.md) | 无法推断元素类型的空集合 |
| [E019](E019.zh-CN.md) | 有歧义的构造器 |
| [E020](E020.zh-CN.md) | 类型声明冲突 |
| [E021](E021.zh-CN.md) | 无效的记录构造或模式 |
| [E022](E022.zh-CN.md) | `!` 运算符位于可传播错误的上下文之外 |
| [E023](E023.zh-CN.md) | 字段类型不满足派生协议 |
| [E024](E024.zh-CN.md) | 整数字面量超出范围 |
| [E025](E025.zh-CN.md) | 无法推断具体类型（无约束的类型槽位） |
| [E026](E026.zh-CN.md) | 无法用 `[]` 索引 String |
| [E027](E027.zh-CN.md) | fan 组合器的废弃记录：旧写法会指出替代写法 |
| [E028](E028.zh-CN.md) | main() 不接受参数 |
| [E029](E029.zh-CN.md) | 未知类型名 |
| [E030](E030.zh-CN.md) | 类型没有顺序 |
| [E031](E031.zh-CN.md) | 已停用的范围写法 |
| [E032](E032.zh-CN.md) | 将不可变绑定传给 `mut` 参数 |
| [E033](E033.zh-CN.md) | 在定义模块之外构造不透明类型 |
| [E034](E034.zh-CN.md) | 将错误通道运算符用于非 Option/Result 操作数 |
| [E035](E035.zh-CN.md) | 根据错误消息文本分支（警告） |
| [E036](E036.zh-CN.md) | map_err 的 lambda 从不使用错误值（警告） |
| [E037](E037.zh-CN.md) | 不兼容类型之间的相等性比较 |
| [E038](E038.zh-CN.md) | `??` 与其回退值分离 |
| [E041](E041.zh-CN.md) | 隐式传播已移除 |
| [E042](E042.zh-CN.md) | 此语句丢弃了 Result（must-use） |
| [E043](E043.zh-CN.md) | 公开与内部的 try_* 写法 |
| [E044](E044.zh-CN.md) | main() 返回 Unit |
| [E045](E045.zh-CN.md) | 对非元组使用元组索引，或索引越界 |
| [E046](E046.zh-CN.md) | 调用参数中的占位符 `_` |
| [E047](E047.zh-CN.md) | 字符串字面量中的无效转义 |
| [E048](E048.zh-CN.md) | 目标类型不具有的变体模式 |
| [E049](E049.zh-CN.md) | `let ... in <expr>` 是 OCaml/Haskell 语法 |
| [E050](E050.zh-CN.md) | 本地 `fn` 或 `let` 与选择性导入名称冲突 |
| [E051](E051.zh-CN.md) | 文件标记的 dialect 超出此编译器支持范围 |
| [E052](E052.zh-CN.md) | 调用已弃用函数 |
| [E053](E053.zh-CN.md) | 未知属性 |
| [E054](E054.zh-CN.md) | fmt 验证失败 |
| [E055](E055.zh-CN.md) | 对 Result 使用 `?.` |
| [E056](E056.zh-CN.md) | 对 Option 使用 `?` 没有作用（警告） |
| [E057](E057.zh-CN.md) | fn 没有函数体 |
| [E058](E058.zh-CN.md) | 无法哈希的 Map 键 |
| [E059](E059.zh-CN.md) | for 的迭代对象不可迭代 |
| [E060](E060.zh-CN.md) | import 整理（警告） |
| [E061](E061.zh-CN.md) | 顶层 `let` 绑定到函数值 |
| [E062](E062.zh-CN.md) | shebang 缺少 `-S`（警告） |
| [E070](E070.zh-CN.md) | `@bounded` 要求计数循环 |
| [E071](E071.zh-CN.md) | 计数循环内不得分配 |
| [E072](E072.zh-CN.md) | `@bounded` 循环内不得使用 `break`/`continue` |
| [E073](E073.zh-CN.md) | bounded 配置内不得递归 |
| [E074](E074.zh-CN.md) | `@bounded` 调用 `@bounded`（或一阶纯标准库函数） |
| [E075](E075.zh-CN.md) | 不得构造运行时长度的堆对象 |
| [E076](E076.zh-CN.md) | `@bounded` 的 effect 受能力约束 |
| [E077](E077.zh-CN.md) | bounded 配置内不得进行 Float 运算（暂定） |
| [E078](E078.zh-CN.md) | 计数循环内不得提前退出 |
| [E080](E080.zh-CN.md) | or 模式的备选项绑定变量 |
| [E081](E081.zh-CN.md) | 函数在所选 wasm 目标上不可用 |
| [E082](E082.zh-CN.md) | --target wasm 的 wasm 路径无法 lower 此程序 |
| [E083](E083.zh-CN.md) | 编译器所有权缺陷：生成的退出未实现已检查的 ExitPlan |
| [E084](E084.zh-CN.md) | 字面退出码超出退出状态范围 |
| [E085](E085.zh-CN.md) | 标准库之外使用 `@intrinsic` 或 `prim.*` |
| [E086](E086.zh-CN.md) | 值将比其 scoped allocation 活得更久 |
| [E087](E087.zh-CN.md) | scoped 区域内不允许此操作 |
| [E088](E088.zh-CN.md) | 递归调用保留当前 scoped activation |
| [E089](E089.zh-CN.md) | 值没有已定义的字符串形式 |
| [E090](E090.zh-CN.md) | `@extern(c)` 签名没有 C 表示 |
| [E092](E092.zh-CN.md) | `@pure` fn 并不纯 |
| [E093](E093.zh-CN.md) | 类型实参数量不正确 |
| [E094](E094.zh-CN.md) | 类型别名形成循环 |
| [E095](E095.zh-CN.md) | 传给 `http.serve` 的应用读取局部变量 |
| [E096](E096.zh-CN.md) | 将带 `mut` 参数的 fn 用作函数值 |
| [E097](E097.zh-CN.md) | `type`、`protocol` 或 `test` 前的 `pub` |
| [E098](E098.zh-CN.md) | 传给 `testing.assert_aborts` 的闭包捕获局部变量 |
| [E099](E099.zh-CN.md) | `testing.assert_aborts` 不在测试块的直接调用中 |
| [E100](E100.zh-CN.md) | 位于内存安全保证之外 |
| [E101](E101.zh-CN.md) | 源码看起来与编译内容不同 |
| [E102](E102.zh-CN.md) | `regex.new` 中的无效模式 |
| [E103](E103.zh-CN.md) | 对无法进行常量时间比较的 Secret 使用 `==` |
| [E104](E104.zh-CN.md) | 跳过的记录字段没有默认值 |
| [E109](E109.zh-CN.md) | 纯 fn 通过函数值运行 effect fn |
| [E110](E110.zh-CN.md) | 不可信输入只能解码到输入类型 |
| [E111](E111.zh-CN.md) | 类型过大 |
| [E112](E112.zh-CN.md) | 泛型实例过多 |
| [E113](E113.zh-CN.md) | match 过于复杂，无法检查穷尽性 |
| [E114](E114.zh-CN.md) | 泛型 fn 以不断增大的类型到达自身 |
| [E116](E116.zh-CN.md) | 响应的 Content-Type 由其正文类型写入 |
| [E117](E117.zh-CN.md) | `http.typed` 接受没有安全模块拥有的字面量媒体类型 |
| [E118](E118.zh-CN.md) | 媒体类型所有者未获授权、不唯一，或类型不是字面量 |
| [E119](E119.zh-CN.md) | 借给 `@export(c)` fn 的 Region 逃逸出该次调用 |
| [E420](E420.zh-CN.md) | 函数可见性违规 |

已退役代码：**E039**（result.collect/collect_map 的弃用窗口，函数已移除，实质替代是 `result.partition`）与 **E040**（json.*/value.* 别名的弃用窗口）各在一个发布版本中触发，随后在别名删除时退役（#1078）。退役写法如今是普通 [E002](E002.zh-CN.md)；迁移映射记录在 [docs/stdlib/json.md](../../../../stdlib/json.md#renamed-operations)。
**E091**（`var` 后的模式，0.66.0）在 0.67.0 退役，因为 `var` 开始接受 `let` 的模式（#3149）：`var (x, y) = p` 和 `var { a, b } = r` 合法，并将每个名称绑定为 `var`，该代码因此不再有内容需要报告。

泄漏到输出中的四位代码（`E0001` 及以上）是 **rustc** 错误，不是 Almide 错误，表示编译器存在代码生成缺陷。请报告至 <https://github.com/almide/almide/issues>。

<a id="authoring-guide"></a>

## 编写指南

每篇文档应包含：

1. 说明代码含义的**单行摘要**。
2. **常见情况**——触发它的最小代码片段。
3. **诊断形态**——实际 `error[EXXX]: ...` 输出的样子，尤其是 `hint:` 和存在时的 `try:` 部分。Dojo 与其他测试框架依赖这些形态。
4. **修复方法**——按频率／概率排序。
5. **原因**——不明显时说明设计依据，让拥有较大上下文窗口的 LLM 能推理该规则是否适用于自己的情况。
6. **相关内容**——相邻代码与速查表章节的交叉引用。

<a id="fix-its-and-applicability-1312"></a>

## Fix-it 与适用性（#1312）

知道源代码范围及替换内容的诊断本身就是修复。`almide fix` 不为它们维护自己的重写表，而是收集编译器输出的所有 fix-it，应用可由机器应用的项，再检查并迭代。能够精确表述修复的新诊断，在合入当天就能获得自动修复。

附加修复时需要作二选一的选择，构建器强制要求这一选择：不能在不指定适用性的情况下附加替换范围。

| 构建器 | 适用性 | 由谁应用 |
|---|---|---|
| `.with_machine_fix(line, col, end_col, text)` | `machine-applicable` | `almide fix`，无人值守 |
| `.with_suggested_fix(line, col, end_col, text)` | `maybe-incorrect` | 人或模型选择后应用 |
| `.with_alternative(line, col, end_col, text)` | `maybe-incorrect` | 人或模型，表示另一种理解，绝不作为优先项 |
| `.with_try(text)`（无范围） | `unspecified` | 无人应用，仅展示 |

每个构建器都写入诊断的 `repair` 字段（#2149）：`primary`（范围精确的编辑）、`alternatives`、`example`（仅供展示的片段）。`almide fix`、LSP quickfix 和 `check --json`（`"repair"`）均读取这同一个字段。

**仅将 `with_machine_fix` 用于改写拼法**：值相同、类型相同、求值相同，对作者意图恰有一种解读。删除语言没有的关键字（E049）符合要求。按编辑距离挑选的重命名不符合：它能编译，却可能调用错误函数。任何将 `T` 改为 `Option[T]` 的修改也不符合，因为应用它只是移动错误，而不是解决错误。

**范围必须精确。** 从真实 token 或 `Span` 推导，绝不通过搜索预期文本来猜测。绑定到猜测范围的 fix-it 会静默改写错误的字节，严格来说比没有 fix-it 更糟。因此无法取得精确范围时，退回 `with_try`，让人阅读片段。`with_machine_fix` 拒绝不能指向真实源码的范围（行／列为 0，范围倒置），将其降为仅展示，而非相信它。

两方面都有门禁：`tests/diagnostic_harness_test.rs` 断言测试夹具集内每个锚定范围的 fix-it 都声明适用性、能干净地应用、能编译，并重现 `fixed.almd`。

每篇文档的 `## Fix-it verdict` 都与二进制作双向核对（#2149）。每个 **mechanical** 代码必须在至少一个夹具中输出可机器应用的 `repair.primary`，且 `almide fix` 必须将该夹具变成通过检查的程序（`tests/diagnostic_coverage_test.rs`）。其他代码均不得输出这种修复（`tests/diagnostic_harness_test.rs`）。

<a id="the-two-way-repair-ratchet-2149"></a>

## 双向修复棘轮（#2149）

- **有诊断侧**——`tests/diagnostics/<case>/repair.grep`：每行一个待匹配字符串，每个字符串都必须出现在该案例的诊断告诉编写者要做的事情中（消息、提示、`try:`、任何 `repair` 替换）。它固定修复本身，因此提示若退回“修正参数类型”（#2097），构建就会失败。
- **无诊断侧**——`tests/diagnostics/silent/*.almd`：这些有效程序具有过去曾引出错误或误导诊断的形态（#2345、#2097、#2254、#2096、#2607、#2093/#2095，以及每项机械 fix-it 中接近适用但不应适用的案例）。每个程序都必须在完全没有诊断、包括没有警告的情况下通过检查（`tests/diagnostic_silent_test.rs`）。添加时，编写程序并在头部注释说明其保护的形态，再提高 `SILENT_FLOOR`。

<a id="source-caveats"></a>

## 需要核对的原文问题

译文不会猜测实现行为，从而擅自合并或纠正原文中的不一致。下列内容需要另行核对实际行为和自动修复的适用条件。

- E002 / E003 / E013：正文的修复建议与末尾的 Fix-it 判定，对无人值守应用范围的说明存在差异。
- E009：pure loop 的递归建议，需要与当前 `CLAUDE.md` 关于已知次数循环的方针核对。
- E015：正文将其描述为警告，固定 verdict 文案却使用 error。
- E027：原文将 #3788 作为未来条件，不表示当前实现已完成。
- E043：上方的重写示例带有末尾 `!`，自动 fix 说明没有。
- E048：正文称“三种形式”，表格却有五行。
- E049：正文说明删除 `in`，末尾 verdict 则说明重写为代码块。
- E052：表格也将运算符重写列为自动应用对象，随后却说明“只有第一行”。
- E116 / E117：关联项中的 E118 说明称“仅标准库类型”，但 E118 正文也允许依赖包和应用的 owner/grant。

- E100：原文对所有目标的内存安全性以及唯一的损坏途径使用了较强表述。本译文并未独立证明实现没有缺陷或整个产物安全。请对照[证明与信任边界](../../../../contracts/proven-vs-trusted.md)核对保证范围。
- E103：请勿将原文的“恒定时间”扩展为与输入长度无关，或整个容器处理过程都不会泄露时间信息的保证。
