# 跨目标决策：哪些决策读取同一个来源

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](cross-target-decisions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](cross-target-decisions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](cross-target-decisions.zh-CN.md)



> 原文：[docs/cross-target-decisions.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/cross-target-decisions.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。这是面向读者的非规范性快照译文，不会自动同步。保留原文的时间、适用范围与主张；翻译时未进行独立测量或实现验证。 原文是 2026-09-29 的审计记录，不表示当前修复状态。

**决策**是编译器对程序回答的一个语义问题。例如：“这个调用会写入其参数吗？”“这个表达式可以稍后执行吗？”“这些流水线阶段可以逐元素执行吗？”

两个目标必须对每个问题给出相同答案。当 native 端与 wasm 端从**不同的权威来源**求得答案时，答案可能不一致。两端随后就会以不同方式运行同一个程序。通常，两端各自的测试都不会发现，因为每一端在内部是自洽的。

这类问题有一个共同形态：一个表只存在于一端，另一端没有。三个例子如下：

- **经 lowering 的模块集合。** native 将所有已解析的内置标准库模块 lowering 到 IR。wasm 端（`src/wasm_leg.rs`）会跳过所有具有无函数体 `= _` 接口的内置模块：`list`、`map`、`string`、`bytes` 等。
- **标准库调用背后的实现。** native 调用 Rust 运行时函数体。wasm 从注册表链接自举实现。
- 重复了声明中已有内容的**手工维护列表**。

本页是 2026-09-29 审计时的清单。它记录的规则是：一个问题有**一个**回答函数，每一端都调用它。

## 清单

| 问题 | native 读取 | wasm 读取 | 现在的唯一来源 | 门禁 |
|---|---|---|---|---|
| 调用会写入哪些参数？（branch lift 拒绝将写入外部变量的分支提取成函数） | 已 lowering 模块的 `IrFunction::mutated_params`，以**不带限定的**名称为键，因此模块函数会覆盖同名用户函数（#2948） | 相同的表，但不含内置模块（#2931），后来增加了声明回退 | `almide_ir::mut_args`：标准库函数在每一端都从声明回答；用户函数从自己的作用域回答 | `branch_lift::tests::every_stdlib_mut_fn_declines_the_lift_whether_or_not_its_module_is_lowered`，从声明枚举，并故意让已 lowering 副本的表出错 |
| 通过注册表链接的标准库调用会写入哪些参数？（调用点的写时复制） | 运行时 `&mut` 表（`generated/runtime_fn_modes.rs`），然后是声明 | **已链接实现**的参数，它们不带 `mut`（#2949） | 接口声明，`almide_ir::mut_args` | `declared_mut_agrees_with_runtime`（native 表 == 声明，每个 `@intrinsic`） |
| 被捕获的变量是否需要共享存储？（C-319） | `Mutability::Var`，或闭包体的 `&mut` 借用 | 六个 bytes 写入函数加上 `Assign` 的手工列表（#2951） | 每次标准库调用都使用 `almide_ir::mut_args` | `cells::tests::every_stdlib_mut_write_to_a_captured_var_makes_a_cell` |
| 这个 `var` 是否曾被写入？（`var`→`let` 降级） | 共享 lowering：只统计 `Assign` 系列语句，因此只通过 `list.push(xs, ..)` 写入的变量变成 `let`，native 把它的快照放入闭包（#2952） | 相同，但 wasm 的 cell 扫描统计了该调用 | 传给声明为 `mut` 的参数的变量算作被写入 | 枚举式的 `use_count::mut_arg_write_tests` |
| 此表达式能否提前、推后、执行两次或完全不执行？ | LICM：自己的 `binop_may_trap` | 小型标量函数内联器：仅要求“没有调用”，因此丢弃了会触发 trap 的参数（#2947） | `almide_ir::speculation` | 单元测试；C-001 测试夹具 |
| 顶层 `let` 的初始化器何时执行？（C-007） | 惰性执行，仅对**直接的** `/`/`%` 强制在启动时执行（#2954） | 始终在启动时 | 当且仅当不适合安全的推测执行时强制执行（`almide_ir::speculation`） | C-007 测试夹具 |
| 流水线各阶段能否逐元素执行？（流融合） | IR 函数体的纯度，加上 `EFFECT_MODULES`/`TOTAL_MODULES`，再加上“至多一个会中止的阶段” | 自己的 7 模块拒绝列表，没有中止规则，因此用户模块回调的输出发生交错（#2953） | `almide_ir::fusion`：模块分类词汇、trap 规则、阶段顺序规则 | 单元测试；C-001 测试夹具 |

## 已检查且一致

以下各项经过探测或代码阅读，确认两端从同一个来源回答，或由于其他原因保持一致。

- **共享优化器：** fold、DCE、propagate、无符号再 fold、optional-chain desugar。它们是语法性的，不读取各端独有的表。DCE 的 `is_pure` 将会触发 trap 的 `BinOp` 视为纯操作。每一端都相同，因此未使用的 `let q = a / 0` 在所有端都会被丢弃。这是语言问题，不是分歧。
- **`mono` / `ir_link` / `top_let_storage::global_init_order`。** 它们读取 `program.modules`，但任何答案都不依赖内置模块是否存在。
- **`mutual_tco` / `accum_tre`。** 它们根据 `IrParam::is_mut` 拒绝带有 `mut` 参数的函数，两端使用相同的 IR。
- **`fan`。** native 在线程中运行 `fan { a; b }` 块；wasm 顺序运行。这是文档中记载的 C-004 交错执行例外。
- **LICM 将不终止的调用提升到零次迭代循环之前。** 已探测：两端一致。
- **现有 MIR 的线性化 `if`**（`almide-mir/src/lower/control.rs`，无调用的两个分支都会执行）。已用触发 trap 的分支探测：两端一致。现有实现正在退役（#2930 / #2935）。

## 已知且在其他地方跟踪

- **#2503。** effect 被调用方被排除在 wasm 调用点的写时复制之外，作为记录字段的 mut 参数也一样（`param_mut` 要求 `!is_effect`；`lower_mut_param_arg` 只接受 `Var`）。native 在这两种情况下都在绑定时克隆。
- **`almide-mir/src/purity.rs`。** 其中的 `PURE_MODULES` 是 MIR lowering 和内联器读取的 capability-witness 允许列表。它是第三套模块分类词汇：`zlib` 在这里是纯的，在融合中却是 effect。当前它只允许返回标量的标准库调用进入内联函数体，而这些仍保留为调用，因此没有发现分歧。它应统一到 `almide_ir::fusion` 的词汇。

## 添加决策时

若新 pass 提出上述某个问题，应调用回答函数，不要重新推导答案。若提出新问题，应从**程序所表达的内容**作答：声明、已检查的签名或 IR 节点。不要从某一端碰巧 lowering 或链接了什么来作答。然后在此添加一行，并附上枚举注册表而非手工列表的门禁。
