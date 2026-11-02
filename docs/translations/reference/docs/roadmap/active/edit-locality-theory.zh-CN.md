<a id="edit-locality-theory--the-theorem-almide-exists-to-prove"></a>

# edit-locality 理论——Almide 为之存在、要证明的定理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](edit-locality-theory.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](edit-locality-theory.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](edit-locality-theory.zh-CN.md)

> 非规范固定原文读者快照。未改动原文仍为读取方的权威来源。不自动同步，也未独立执行。历史主张、已采纳提案、实现状态及测量证据保留原范围；本翻译不独立验证。内联代码保持原样，包括原语言字符串。 阶段完成明确限于片段范围及8月15日时点：区分无类型frame、有类型modularity、求值器健全性、未证明完备性及受信render接缝。历史正文中初始5个预测仍待验证，不是新确认的MSR改善。Koka、调查编译器及独有性主张归于当时原文调查，未在此独立重核。不主张内存、时间或整个编译器已获证明。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/edit-locality-theory.md)

> 原metadata非活动注记：将有界编辑影响范围从设计直觉变成已证定理。

**状态**：active（基础文档写于 2026-08-15；Stage 1 已作为 `docs/specs/edit-locality.md` 落地）。
**使命联系**：MSR（修改存活率）是一项语义性质的经验投影：源码编辑可观察的影响范围，受限于经过被编辑代码的执行。本文计划将这种性质从设计直觉变成带有实时实验循环的已证定理。

<a id="the-claim"></a>

## 主张

> **Edit Frame（L1）。** 保持定义签名的编辑，不能改变任何不经过该被编辑定义的执行的可观察结果（stdout、stderr、exit code——集合由 `docs/contracts/contracts.toml` 固定）。

几乎没有已实现语言满足这一点。重载解析、隐式 instance、宏、全程序推断、动态作用域 effect handler 都会破坏它：它们让远处编辑改变未编辑代码的含义。Almide 现有纪律——签名中的 `effect`、仅显式传播（ADR-0008、E041/E042）、限定的跨模块访问——恰是 L1 所需前提集合。这些规则因 MSR 逐项采用；L1 是它们共同服务的单一陈述。

<a id="why-this-is-the-anti-koka-position-not-the-diet-koka-position"></a>

## 为什么这是与 Koka 相反的立场，而非精简 Koka

Koka 的核心亮点——动态作用域代数 effect handler——按构造就是 frame 违背机制：`perform op` 的含义取决于调用上下文某处安装了哪个 handler，可能在另一文件中。安装或重排 handler 是远处编辑，却改变未编辑代码行为。**Koka 若不切除 handler，就无法证明 L1。** 因此两种语言位于同一轴的两极：Koka 最大化 effect 表达力，Almide 最大化编辑局部性。未来每个功能提案都面对一个 gate 问题：*是否保持 L1？*（handler、隐式 instance、glob 再导出都无法通过；这正是 gate 在起作用。）

<a id="stages"></a>

## 阶段

1. **陈述并搜索**（DONE 2026-08-15）：`docs/specs/edit-locality.md` 针对当前语言陈述 L1–L3，将每个现有规则映射到其局部性作用，并记录对已实现编译器的反例搜索。每个发现的违背都成为语言修复或明确附加条件，绝不成为未写下的例外。首日成果：在 almide 0.57.0 上复现了**实际存在的跨目标分歧**（V1——LICM 将函数体推断为“pure”的部分运算，从零次迭代循环中投机提升；native trap，wasm 打印）。421 个 `wasm_cross` fixture 及差分 fuzz 都未捕获。尚未写一行证明，定理就发现了真实契约违背。
2. **机械化核心演算**（DONE 2026-08-15）：λ_almd 位于 `crates/almide-edit-belt`，是第三个 0-sorry Lean belt，由 CI `lean-proofs` 作业与 perceus、race 一同进行 kernel-check。演算包含：具有声明签名的顶层定义（含 `effect` flag）、`let`／调用／`Result` 构造和 `match`／显式 `!`（ADR-0008，表示为 `norm`/`abrupt` 结果区分）／`??`／`print`（可观察项），以及带 trace 和每个进入定义台账的 big-step 语义。以下定理均无 sorry：
   - `ev_agree`——求值只依赖实际进入过的定义。一个引理包含整个 L1；因 call 规则只在被调用名称处查表，其他地方不查，所以一页即可证明。
   - `edit_frame` / `edit_frame_observables`——L1 迁移及（结合确定性 `ev_det` 的）可观察结果相同形式。值得记录的发现：无类型 frame 完全不需要保持签名；把 `f` 替换为任意内容，也保持避开 `f` 的执行。签名保持纯粹是有类型部分的假设。
   - `typing_modular`——同签名替换函数体仍保持程序类型正确，其他每个定义的推导字面上还是同一个 proof object（类型判断只查询签名）。
   - `pure_silent`——类型标为 `eff = false` 的代码求值时 trace 为空：即“纯 `fn` 无法产生可观察项”的语义内容（E006 边界，已机械检查）。
   诚实范围：单参调用、仅 `Result` 数据、一个 effect；must-use（E041/E042）仍属实现级诊断，核心模型化 ADR-0008 的运行时含义，而非 lint 表面。
3. **后端作为 refinement**（在片段范围 DONE，2026-08-15）：核心是可执行规范。`Evaluator.lean` 添加以 fuel 为索引的 `evalE` 与 `eval_sound`（返回的任何内容都可由关系导出；结合 `ev_det` 则唯一；有意不证明完备性，只使用 `some` 输出）。约束覆盖生成器全体而非手选案例：`Corpus.lean` 在 λ_almd 可表达片段上确定性生成 48 个有类型程序的语料，kernel-check 每个程序都求值得到可观察项（定理 `corpus_total`、`:= by rfl`），`conformancegen` 发射／固定提交的语料（`proofs/kernel-conformance/`，CI drift-gated；`.expected` trace 本身由求值器固定，属于 `trustCompiler` 类接缝）。`tests/kernel_conformance_test.rs` 在 native 和 wasm 上运行全部语料并核对预期 trace；落地时两个目标均为 48/48，加上契约 C-280 下手写的 7 程序家族。信任接缝是已审查一次的约 80 行 `erase`/`render` 对；三层定位在 `docs/contracts/proven-vs-trusted.md`。首日成果：语料初次运行就捕获两个真实编译器 bug——almide#1428（checker 接受裸 `err(..)` match subject，codegen 发射无效 Rust）和 almide#1429（裸参数尾部导致 v1 renderer 分离 effect fn 的签名与函数体）。剩余是研究级任务，没有在证明器中构建的编译器就难以达到：经验证的 surface-core→λ_almd 变换，以及 Rust 实现的逐 pass simulation 证明。此前片段 gate 充当 ratchet；`docs/specs/edit-locality.md` §3 的 R-group 义务保留在各自台账。
4. **预测循环**（机制运行于 2026-08-15；随着落地后运行积累而关闭）：`proofs/l1-verdicts.toml` 是实验记录簿。每次涉及语义的更改，在任何测量之前记录 L1 verdict 和已提交 MSR 预测（带因果机制的 up/neutral/down）；`scripts/check-l1-verdicts.sh` 在 CI 中检查 schema。Dojo 侧是 `src/l1_loop.almd`（按 dogfood 规则用 Almide 编写，自用立即捕获 `regex.captures` 文档／实现漂移，almide#1432）。它将台账与 `runs/*/summary.md` 的 headline MSR 数值连接，将每个预测状态——confirmed / refuted / inconclusive / awaiting——报告到 `dashboards/l1-loop.md`。被反驳的预测保留为关于理论的发现。初始为 LV-001..005（此方向的 5 次落地，均预测 `up`）；全部等待落地后运行，最后一次计分运行早于该方向，因此循环首个真实数据点将在下一轮 Dojo 到来。能预测测量的理论是其他语言团队没有的一层。精确的 Koka 比较（Survey 4，于 2026-08-15 在 koka 源码核实）：其证明仅在论文中，repo 任何位置都无机械化；812 行 FIP/FBIP verifier 虽每次编译都运行，Core IR typechecker 却三重禁用（默认关闭、在测试套件中注释掉、开启时也只 warning），跨后端差分 harness 存在但从未在 CI 启用。将机械检查定理接入实时测量循环这一组合，所调查 9 个编译器均没有。

<a id="non-goals"></a>

## 非目标

- 与 Koka 竞争 effect 表达力。这场竞争已落后 14 年，获胜代价是失去 L1。
- 证明任何内存或时间性质。可观察集合是 stdout / stderr / exit code，与现有契约台账固定的完全相同。
