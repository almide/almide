> 原文描述：编译器腐化的分布与有参考依据的治理措施
<a id="rot-eradication--the-decay-map-and-its-reference-backed-cures"></a>

# rot-eradication — 腐化分布与有参考依据的治理措施

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](rot-eradication.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](rot-eradication.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](rot-eradication.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/rot-eradication.md)
> 历史 active 地图及工作顺序与后面的已落地节并存：后者中 R1 完成、R3 仅前半完成，R2 创建时形状的最终设计仍未完成。R1 段将 fs_metadata_family 称作 R6 的 link-set 类别，但地图把 link-set 分歧标为 R4；保留此原文不一致，不暗中修正。调查普遍性、计数、安全及性能判断都是固定原文主张，并非新审计或当前待办。


**状态**：active（在 result-family 系列及 #1418 行动之后，于 2026-08-15 写下基本原则；以下每项主张都有那些工作中的测量或事件支持）。
**调查证据**：`../almide-references/RESEARCH.md`（调查 1–3：调用结果布局、新变量纪律、合并糖衣消除、payload 绑定知识、值位置提前 return、codegen 纳入集合 — 全部附 rustc/Swift/Zig/Roc/Lean/Koka/Grain clone 的 file:line 引用）。

<a id="the-laws-what-every-surveyed-compiler-agrees-on"></a>

## 法则（所有受调查编译器一致之处）

1. **类型选择布局，名称选择代码。**（调查 1，8/8。）已作为 result-family 系列落地。
2. **新 ID 来自上下文拥有的单调分配器，绝不来自子树扫描。**（调查 2，4/4。）已作为 band allocator 落地。
3. **糖衣只消除一次，进入通用机制；payload 类别改变输出的测试，而不改变所走路径。**（调查 2 — Roc 的 `??` 脱糖与我们的逐字符相同。）
4. **逐值知识就是值的类型／布局，创建时附着；投影通过全函数导出子知识；不存在“未跟踪”。**（调查 3a — Roc `Local{layout_idx}`、rustc `Place→Ty`。）
5. **形状与所有权分为不同 pass。**（调查 3a — Roc 的 match lowering 是全域且布局驱动的；ARC 稍后决定 borrow/incref。）
6. **发散分支丢弃合流续延：剩余分支 ≤1 时将续延复制进去，否则具体化为局部 join point。**（调查 3b — Koka UnReturn；我们的尾部复制脱糖已是复制那一半。）
7. **codegen 纳入集合由已输出调用产物经传递遍历计算，绝不由平行预测计算。**（调查 3c — rustc collector 遍历 codegen 所遍历的同一 MIR；Roc 删除输出 LIR 不可达的一切，backend 编译整个 store。）

<a id="the-rot-map--cures"></a>

## 腐化分布 → 治理措施

<a id="r1--the--route-zoo-coupling-web-active-rot-1418"></a>

### R1 — `??` 路径群的耦合网（活跃腐化；#1418）
一夜实测三种失败模式：微调名称表使正常路径不稳定（实验已回退）；caps 计数器静态预测 route helper 调用（#1079 耦合）；推测尝试污染跟踪集合（已通过 `speculate` 修复）。**治理（法则 3）：将 route 的函数体移入 value-match 机制，作为其 payload-class 分支，然后一次性删除链、准入门禁、计数器额度及 `ALMIDE_QQ_DESUGAR` 实验 pass。** 仪器已落地：match 优先反转（scalar：214 match / 1 fallback）、`ALMIDE_QQ_NO_ROUTES` 删除就绪探针（25 → 2：fs_metadata_family = R6 的 link-set 类别；unwrap_or_heap_payload = route 函数体迁移所涵盖的 subject 分类残留）。

<a id="r2--the-per-value-tracking-side-sets-1414"></a>

### R2 — 逐值跟踪辅助集合（#1414）
约 20 个以 ValueId 为键的集合；“未跟踪”是隐含状态；插入不可回退（`speculate` snapshot 是绷带，不是根治）；已有一次 bind/subject 不对称漏出（已修复）。**治理（法则 4+5）：创建值时附着逐值形状（Roc 的 `Local{layout_idx}` 形态；对我们而言是由邻接 `fresh_value` 的构造器分配 `ValueId → ValueShape`）；投影全域导出子形状，DROP／所有权路由移至稍后针对同一形状的 pass。`result_family` 是首个全域部分，一次性 seeding 入口是迁移载体。** 通过使该状态不可表示，消除 `speculate` clone 成本及“对未跟踪 subject 做 match”的 wall 类别。

<a id="r3--value-position-propagation-arms-1421-wrong-code"></a>

### R3 — 值位置传播分支（#1421，错误代码）
值位置的 `match r { ok(v) => v, err(_) => f()! }` 输出无效 merge（接受但错误；`??` 反转中的 guard 将其隔离）。**治理（法则 6）：Koka 格 — 传播分支不消耗 merge；若剩余分支 ≤1，将续延推入其中（已有尾部复制机制），否则将续延具体化为合成 join fn。** 去除 `fallback_propagates` guard，关闭最后的 scalar route fallback。该治理的承重一半 — 指定帧目标的 `Op::Return`，使传播分支能够发散 — 现为独立系列：[return-op-eradication.md](../../../../../roadmap/active/return-op-eradication.md)。它还删除仅因缺少该 op 而存在的 `!` 位置脱糖群（`BRANCH_PASSES` 中约 9 行）。

<a id="r4--the-link-set-divergence-envcwd-class-unfiled-until-now"></a>

### R4 — link-set 分歧（env.cwd 类别；此前未起票）
lowering 路径变化时，哪些 self-host 函数体被编译，与 lowering 输出的调用不一致。**治理（法则 7）：遍历 LOWERED 函数输出的调用名至不动点（即 codegen 消费的产物），导出 self-host 纳入集合，删除所有平行预测，并增加 render 兜底：未解析内部符号为零，作为结构门禁而非 wall 消息。**

<a id="r5--residual-name-registries-purity-never_errauto_wrap-router"></a>

### R5 — 残余名称 registry（purity、NEVER_ERR/AUTO_WRAP、router 特例）
与已清除的 family 表同类，目前靠测试维持。**治理（调查 1 的 Swift/Lean 模式）：名称导出的事实在边界处仅折叠一次进入 signature/registry；下游绝不再按名称导出。由 registry 导出 materialized set（result-family 系列尚未结束的尾部）就是模板。** 机会性推进：触碰某个 registry 时就迁移它。

<a id="r6--the-harness-reports-walls-as-panicked"></a>

### R6 — 框架将 wall 报为“panicked”
造成完整的诊断绕路（12 次“panic”实际是正常 wall）。**治理：跨目标框架在报告行中区分 wall/build-fail/run-fail/diverge。** 小规模，立即处理。

<a id="r7--include-splicing-blinds-rust-analyzer"></a>

### R7 — include! 拼接使 rust-analyzer 失明
最关键的安全 crate 没有可用 IDE 诊断（整夜虚假 E0308／unlinked-file 噪声）。**治理：使用真正的 `mod` 文件（行数上限政策可按模块而非文件计数），或使用认识拼接根的 workspace rust-analyzer 配置。** 工具决策，低风险，对评审吞吐量回报高。

<a id="landed-2026-08-15-the-night-of-the-constitution"></a>

## 已落地（2026-08-15，写下基本原则的夜晚）

- **R1 完成** — #1418 已关闭。路径群已删除（+148/−1054）：match 机制是唯一的 `??` 路径；准入门禁、操作数解析器、计数器额度和实验 pass 已消失；三个字节测试固定删除。通过 `speculate`、类型化 seeding、两种 payload 宽度的 subject-ANF 以及 merge 所有权约定，就绪探针由 25 → 0。closure 和 fallible-carrier 的 route 函数体移入该路径，成为 payload-class 分支。
- **R3 前半** — 已消除错误代码被接受的情况（值位置 scalar match 拒绝传播分支；所有已知生成者都走 `(match …)!` 语句改写，现在覆盖两种 payload 类别）。在值位置接受这些分支的 join-point lowering 仍是 #1421 的工作。
- **R2 一次 seeding 里程碑** — `seed_variant_value_shape` 成为所有位置上 variant 读取形状及 drop-route seeding 的单一类型化入口；逐位置漂移类别已关闭。创建时附着形状的最终目标仍未完成。
- **记录 R6 症状** — 就绪探针的盲点（unit test 与 spec/lang 位于 wasm_cross corpus 之外）加重了 wall 与 panic 的报告混淆；处理 R6 时两者一并处理。

<a id="order-and-gates"></a>

## 顺序与门禁

R3 → R1（R3 解锁最后的 scalar fallback；R1 迁移随后清空就绪探针）→ R4（其门禁使 R1 删除切片可安全验证）→ R2（利用 R1 暴露接缝的深层治理）→ R5/R6/R7 机会性推进。
每阶段使用既有门禁（parity corpus、verify-trust、ratchet），另加专用仪器：R1 = `ALMIDE_QQ_NO_ROUTES` 意外计数（目标 0）及 `ALMIDE_DBG_QQ` fallback 计数（目标 0）；R2 = 辅助集合数（目标：删除这些集合）；R4 = 未解析符号 render 门禁（目标：存在且为 0）。
