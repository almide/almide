<a id="native-projection-ownership-2067-2069-2070"></a>

# Native projection 的所有权（#2067、#2069、#2070）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](native-projection-ownership-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](native-projection-ownership-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](native-projection-ownership-review.zh-CN.md)

> 固定原文的非规范性历史阅读快照，以原本为准。不自动同步。所述测试、测量、实现状态和保证是原文主张，不是独立重跑或当前验证。未执行引用代码或命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/native-projection-ownership-review.md)

通过 match 读取 `o.hit` 会克隆整个 Option payload，读取 `list.get(xs, i)` 或 `xs[i].pos` 会复制整个 record。重建 record 也会克隆字段，即使输入已被拥有且不再使用。

参考编译器是 Rust，克隆于 `../almide-references/rust`，版本为 `0d31508599a7814a7044e9a7a871e3dc5f037753`。其 `compiler/rustc_mir_build/src/builder/expr/as_operand.rs` 区分 place 与 rvalue，并携带操作数的临时生命周期；move 作用于 Place，包括 projection。本审查把两个问题分开：能否在使用方所需的持续时间内借用一个 place，以及其所有者能否转移投影出的字段。

只读 Option/Result match 借用稳定的字段对象，或使用返回引用的 list lookup。模式绑定只能被读取；捕获、修改以及 arm 中使用原始来源都会放弃该优化。借用的模式绑定从拥有集合中移除。在终端 match 前紧邻的单次使用 alias，会在活跃性计数前折回原 projection。绑定与 match 之间若存在修改或其他语句，就不进行这种折回。

通过稳定的带索引 list 访问成员，会借用被索引的 record。边界和负索引行为都保持不变。产生临时 list 的调用保留原来的拥有式索引宏：把克隆移到宏外会超出临时输入的生命周期。String 匹配保留既有 MatchSubject coercion。

对拥有的字段进行最后一次读取时会 move。如果剩下的每次根使用都是这些直接 projection 之一，重建 record 就可以 move 多个不同字段。重复字段、后续整 record 使用、循环迭代及可复用捕获保留必要复制。借用推断把堆字段重建的输入标记为会消耗，调用者的活跃性分析则在原值被再次使用时保留它。由 TCO 管理的参数仍服从 TCO 自身的 move 计划。

既有 stream-fusion PR 从报告中的纯 list.map 形态中移除 Rc 回调。本修改移除每个元素的输入字段克隆。每个输出 record 仍需要它自己的被捕获 String 值；当前拥有式 String 布局要求每次输出进行这一次克隆。

回归覆盖检查生成的 Rust，并执行 native 与 WASM 两者：直接／经 alias 的 match、索引读取、缺失／负 optional 索引、拥有式重建、重复字段、来源复用、来源修改、捕获的 record、临时 list 输入及 string match。

验证：全部 439 个 native 测试文件通过，全部 439 个默认 target 文件也通过（427 个 WASM、12 个 native fallback）。43 项 nanopass 测试、13 项 codegen snapshot、11 项 loop-ownership 测试、projection 回归测试及 borrow/capture 回归测试通过。在既有 codopsy 阈值下，codegen 保持 90/A；commissioned WASM crate 也保持 90/A。

CI cross-target fixture `mg_rebuild_two_phase` 暴露了另一条生命周期边界：模块全局 `Var` 会生成克隆的快照，而不是稳定的 Rust place。共享的捕获单元也有同样边界。针对这些根的索引借用保留拥有式索引路径；普通局部／参数 projection 保留引用路径。固定版 Rust 的 `as_operand` 实现说明，操作数只在其临时作用域内有效。回归测试跨越两次全局 list 重建，同时覆盖调用实参读取和标量字段读取。
