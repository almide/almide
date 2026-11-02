# almide-egg-lab

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



> 原文：[crates/almide-egg-lab/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-egg-lab/README.md)。这是供读者参考的非规范性快照译文，不会自动同步。未对测量值及主张进行独立复核，以原文为准。
>
> 原文注意：PoC 的测试数量、测量与后续阶段描述属于原始实验记录。请与开头的集成状态区分，不要视为当前测量或已确定的未来计划。

**编译器使用的等式饱和规则与 IR 桥接。**

> 当前状态（2026-10-31）：
> [`EggSaturationPass`](../../../../../crates/almide-codegen/src/pass_egg_saturation.rs) 使用此 crate 的规则、桥接和成本模型。它不是孤立的、可以任意删除的实验。以下 PoC 结论、耗时和分阶段问题属于原始实验记录，不是当前测试数量或集成清单。

最初的实验在投入为期 10 个月的 [MLIR Backend + Egg Rewrite Engine](../../../../roadmap/on-hold/mlir-backend-adoption.md) arc 前提出一个问题：*egg 能否真正表达 Almide 现有的 stream-fusion 规则，快速终止，并选择融合后的形式？*

## 结论

**可以。** 小型 PoC（5 个测试）加上真实 IR 桥接（11 个测试，其中 5 个使用真实的 `VarTable` 支持的 beta 归约，覆盖 lift → saturate → **lower** 往返）在 release 构建中约 2.5 s 通过。三个典型形态触发融合，饱和在很小的迭代预算内收敛，不需手动安排阶段顺序便能组合规则。最重要的是，`compose` 和 `and-pred` 标记在 lower 时 beta 归约为带有全新 `VarId` 的真实 `IrExprKind::Lambda` 节点，因此提取输出是可反馈给现有流水线的良类型 `IrExpr` 片段。

| 测试 | 证明的内容 |
|---|---|
| `identity_map_collapses` | `map(xs, identity) ≡ xs` — 单规则重写 |
| `map_map_fuses` | `map(map(xs, f), g) ≡ map(xs, g ∘ f)` — 函子律 |
| `filter_filter_fuses` | `filter(filter(xs, p), q) ≡ filter(xs, p ∧ q)` — 谓词合取 |
| `triple_map_fuses_transitively` | 三段链折叠成一次遍历 — 饱和在迭代/节点预算内收敛 |
| `identity_inside_map_chain_eliminates` | 恒等消除 **+** map 融合**无需显式阶段排序**即可组合 — 选择 egg 而非优先级重写器（GHC RULES、MLIR PDL）的核心动机 |

最后一个测试最关键。在命令式 `pass_stream_fusion` 中，我们必须决定顺序：先消除恒等再融合，还是先融合再发现可折叠？每种顺序都会遗漏一些程序。使用 egg 时不必选择：饱和在 e-graph 中保留两种候选，提取器选择成本最低者。

## 范围（有意保持很小）

- 仅建模三个规则所需的 IR 形态：`map`、`filter`、`fold`、`lam`、`compose`、`and-pred`，以及数值/符号原子。
- **Lambda 替换发生在 lower 时，而不是饱和内部。** E-graph 没有额外的 alpha-renaming 机制就无法表达绑定器（参见 egg 的 `examples/lambda.rs`）。饱和将 `compose` / `and-pred` 保持为零成本标记；随后将提取出的唯一最佳形式 beta 归约为带有新 `VarId` 的真实 `IrExprKind::Lambda` 节点。一次性分配受输出大小约束，既绕开 e-graph 的 alpha-equivalence 问题，也不丢失主 IR 所要求的 fresh-id 纪律。
- 原始实验曾是孤立的。当前 codegen 集成见上方链接；修改或删除此 crate 可能影响编译器输出。

## 包含内容

- **Toy Language PoC**（`src/lib.rs`、`tests::`）：从字符串解析的 `AlmideExpr` 表达式、三个融合规则、五个测试。
- **IrExpr 桥接**（`src/bridge.rs`、`tests/bridge_test.rs`）：将 `almide_ir::IrExpr` 子树（list 组合子＋恒等 lambda 检测）提升为 `RecExpr<AlmideExpr>`，再把提取的形式降低回良类型 `IrExpr`。十一个测试覆盖恒等消除、map/filter 融合、跨规则组合、不透明透传，以及五个验证 lower 输出中新 `VarId` 分配、函数体替换和结构形态的往返用例。

## 尚未回答的问题

这些作为开放问题在 arc 文档中跟踪：

- **感知类型的规则。** 真实 Almide 规则依赖元素类型（`List[Int]` 与 `List[String]`）。egg 通过 `Analysis` 支持这一点，但我们尚未试用。
- **大规模 IR。** 目前测试到三段链。Stage 1 将在 100–1000 个 IR 节点的程序上做基准测试，观察饱和耗时是否仍可接受。
- **面向目标的成本函数校准。** 当前 `FusionCost` 只惩罚循环。真正的目标感知成本（Rust iter-collect、WASM 手动循环、GPU 卸载）属于 Stage 3。
- **e-graph 内的 lambda 替换。** `lower` 时 beta 归约足以证明往返可行，但图内替换（自定义 `Applier` + alpha-renaming，类似 `egg/examples/lambda.rs`）可让饱和看穿 compose 标记，可能发现更多融合。待明确哪些规则需要它后，纳入 Stage 1+ 范围。

## 运行

```bash
cargo test -p almide-egg-lab --release
```

## 原始保留方案的后续提议（历史记录）

若此 PoC 通过评审，MLIR 采用 arc 的 Stage 1（参见 `docs/roadmap/on-hold/mlir-backend-adoption.md`）将：

1. 扩展语言，覆盖整个 `IrExpr` enum
2. 决定保留 lower 时 beta 归约，还是通过自定义 `Applier` 将替换移入 e-graph
3. 添加类型参数化规则
4. 移植剩余 4 个 `pass_stream_fusion` 规则
5. 在现有 Nanopass 流水线前，以可选启用的 pass 运行等式饱和

## 原始拒绝方案（历史记录）

若真实 Almide 程序上的饱和难以处理，则退回现有 Nanopass 框架内的 GHC RULES 风格优先级重写。MLIR 采用 arc 不用 egg 继续进行。在 PoC 阶段，删除 crate 曾是选项；如今不能独立删除，因为当前编译器依赖它。
