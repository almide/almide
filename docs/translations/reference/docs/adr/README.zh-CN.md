<a id="architecture-decision-records"></a>

# 架构决策记录

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[日英混合正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/README.md)的非规范性阅读译本，不会自动同步。原本仍具权威性；任何规范效力均属于原本，不属于本阅读版。日期、状态、实现及验证方面的主张作为原本陈述保留，未经独立重新验证。模板代码块连同其中的日文均原样保留。

按时间顺序记录语言设计与架构的**决策**、**依据**以及**被否决的替代方案**。

<a id="なぜ-adr-か"></a>

## 为什么使用 ADR

`docs/roadmap/` 说明“接下来做什么”，`docs/specs/` 说明“现在是什么”。两者都不保存**为什么这样决定**。结果，同样的讨论会在半年后重演，曾被否决的方案会在丢失依据的情况下重新出现。ADR 提供这第三个维度。

[REJECTED_PATTERNS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/REJECTED_PATTERNS.md) 是“不采用的功能”列表，而 ADR 保存**达成该决策的过程**（调查的证据、比较的替代方案、证伪条件）。否决事项列入 REJECTED_PATTERNS 时，以 ADR 作为依据链接。

<a id="形式"></a>

## 格式

`NNNN-kebab-case-title.md`。编号连续，不要重命名（链接会失效）。

```markdown
# ADR-NNNN: Title in English

- **Status**: Proposed | Accepted | Superseded by [ADR-MMMM](./MMMM-....md)
- **Date**: YYYY-MM-DD
- **Context**: 何が問題だったか（決定の前に読者が知る必要のある事実）
- **Decision**: 何を決めたか（一文で言い切る）
- **Rationale**: なぜそれか（証拠つき）
- **Alternatives**: 検討して却下した案と、その理由
- **Consequences**: 何が良くなり、何を払うか
- **Falsifier**: 何が起きたらこの決定を撤回するか
- **References**: 一次情報の URL
```

未改动模板中的日文说明含义如下：
- Context：问题是什么，即读者在决策前需要知道的事实。
- Decision：决定了什么，用一句话明确说清。
- Rationale：为何这样选择，并附证据。
- Alternatives：考虑后否决的方案及其理由。
- Consequences：哪些方面改善，需要付出什么代价。
- Falsifier：发生什么情况会撤回此决策。
- References：第一手资料的 URL。

<a id="規則"></a>

## 规则

1. **Status 必须如实反映情况。** 实现与决策不一致时，修订 ADR 或将其标为 Superseded。被搁置不管的 ADR 会重演 SPEC.md §13。
2. **不要删除被否决的方案。** 否决理由正是避免讨论重演的资产。如果后来的证据表明否决有误，用新的 ADR 将其取代，并写明**上次判断为什么错了**。
3. **Falsifier 必不可少。** 无法写出撤回条件的决策，不是决策而是偏好。
4. **引用一手资料。** 不要写“据说……”，而应给出规范、官方文档或源代码的 URL 及逐字引用。

<a id="読み方と後続決定への案内"></a>

## 阅读方法与后续决策指引

ADR 正文记录决策当时的证据、否决理由与修订历史。`Accepted` 表示设计已被采纳，不表示所有项目均已实现。正文中保留的“未实现”和旧 API 应结合后来的补充及后续 ADR 阅读。当前语言规范请参阅 [specs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/)，公开函数请参阅[标准库](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md)。

以下指引旨在避免误读。它不修改 ADR 正文，也不对尚未决定的设计作出新的采纳或撤回。

| 记录 | 如何阅读后续决策与实现状态 |
|---|---|
| 0001 | `fan.race` 的名称、时钟参数、计量成本范围请参阅 [0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) 已采纳的决策。关于时间单位本身的决策保持不变。 |
| 0002・0003・0012 | 显式传播参见 [0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md)，带类型错误与 lambda 的合流参见 [0012](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md) 和 [0021](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md)。0003 严格拒绝 `CustomE → String` 与实现之间的差异仍记录在 0021 D5 和 Falsifier 3 中，本索引不解决该差异。 |
| 0004 | 关于 `result.context` 的描述，请结合 D2 的否决和 Alternatives 7 阅读。当前公开函数可在 [result](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/result.md) 中确认。 |
| 0012・0021 | 不要仅凭开头的实现前说明就判断尚未动工。带类型标记和 callback 的传播已在 [typed_error_callback_channel_test.almd](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/spec/lang/typed_error_callback_channel_test.almd) 等测试中验证。 |
| 0020・0023 | [0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md) 取代 0020 §5.3 的 export 形式。请结合正文后来添加的实现注记和 [Wasm 构建路径](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/build_wasm.rs) 阅读。不要把 serve export 的实现等同于 HTTP client 所有路径或 host-op 废止工作的完成。 |
| 0025・0026 | 0025 §2.4 的 `process` / `Env` 分类请参阅 0026 D2 后来的实现注记和 [effect category 定义](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-ir/src/effect.rs)。0026 开头的“除 fixpoint 外均未实现”是后来补充之前的状态。 |
| 0026 对外部函数边界的引用 | 最后补充中的“ADR-0027 §5”是尚未解决的引用。[0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) 是关于检查 harness 的决策，不能作为外部函数边界的依据。本索引不推断替代规范。 |
| 0029 | 请参阅 Status 和 Open 中 2026-10-18 的裁定。开头保留的“questions … still need a ruling”是裁定之前的说明。被否决的 test-level 属性方案作为正文中的被否决方案保留，并非另一份现行 ADR。 |

编号 0013 用于两份不同的记录。为保留现有链接，不进行重命名；下方索引通过标题区分它们。

<a id="index"></a>

## 索引

| # | 标题 | 状态 | 日期 |
|---|---|---|---|
| [0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md) | 确定性预算以时间单位书写 | 已采纳 | 2026-08-01 |
| [0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md) | 失败可能性与 effect 是正交的轴；`-> T!` 标记 pure-fallible | 已采纳 (2026-09-24 修订：effect 蕴含 fallible，§D6) | 2026-08-05 |
| [0003](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0003-error-type-conversion-at-propagation.md) | 传播点的错误类型转换 — 有损转换必须显式写出 | 已采纳 | 2026-08-05 |
| [0004](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0004-error-branchability-doctrine.md) | 错误可分支性 — String 保持为终端；采用原则、lint 和 context，而非标签 | 已采纳 | 2026-08-05 |
| [0005](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0005-operators-desugar-to-stdlib.md) | 值级运算符是具名 stdlib 函数的语法糖展开 | 已采纳 | 2026-08-05 |
| [0006](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0006-fallibility-polymorphic-hofs.md) | HOF 的一位失败可能性多态；解散 try_* 家族 | 已采纳 (2026-09-27 修订：E 从 callback 流入，撤回 D4 — ADR-0021) | 2026-08-05 |
| [0007](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0007-partition-replaces-collect.md) | 移除 result.collect；partition 是收集全部错误的实质 | 已采纳 | 2026-08-05 |
| [0008](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0008-explicit-propagation-only.md) | 传播始终显式 — 废除 auto-?，仅使用 `!`，Result 必须使用 | 已采纳 | 2026-08-05 |
| [0009](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md) | 函数类型补齐能力四象限；fn 参数默认透明 | 已采纳 | 2026-08-06 |
| [0010](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0010-option-type-shorthand.md) | T? 是通用 Option 简写 — 适用于每个类型位置，按原子紧密绑定，由 fmt 规范化 | 已采纳 | 2026-08-06 |
| [0011](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md) | 执行基底是自由变量 — 而某个 arm 的输出是确定性主张中最后的漏洞 | 提议中 (2026-10-04 修订：wasm 计算并行性以实例为单位，绝不共享内存，§D2a) | 2026-08-07 |
| [0012](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0012-typed-error-refinement-in-the-marker.md) | 错误表面的最终状态 — 精化保留在标记（`T!E`）中，擦除仍为默认 | 已采纳 (D4-4 由 ADR-0021 替换) | 2026-08-11 |
| [0013](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-corpus-stays-id-keyed.md) | 一致性语料继续以契约 ID 为键；否决按语义重组，以门禁检查命名纪律 | 已采纳 | 2026-08-20 |
| [0013 (String)](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-string-shared-immutable-buffer.md) | String 成为共享的不可变缓冲区；修改操作采用写时复制 | 已采纳 | 2026-09-04 |
| [0014](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0014-expectations-are-derived.md) | 固定的期望值要么由机器推导，要么不存在 — 绝不手工维护 | 已采纳 | 2026-08-20 |
| [0015](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0015-reference-evaluator-is-fresh-source-level-python.md) | 参考求值器是黑盒协议背后全新、源码级、由裁判方拥有的求值器 — 以 λ_almd 为起点，通过 abstain 台账防止倒退（修订：Rust） | 已采纳 | 2026-08-21 |
| [0016](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0016-numeric-determinism-is-normative.md) | 数值确定性是只书写一次的同一规范性族 — 而非各目标碰巧共有的性质 | 已采纳 | 2026-08-21 |
| [0017](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0017-flight-profile-is-a-normative-subset.md) | bounded profile（原称 flight profile）是语言的规范性子集，在任何 checker 强制执行之前先在此规定（裁定时修订：`@bounded`、章节 `bounded.md`、前缀 `B`） | 已采纳 | 2026-08-21 |
| [0019](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0019-suppression-is-announced-dated-reasoned-acknowledged.md) | 抑制需公告、注明日期、说明理由并得到确认 — 在该构造存在之前就固定其形式 | 已采纳 | 2026-09-22 |
| [0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) | 提供服务的 handler 不能访问 `var`；状态存放在独立于主机的 KV store 中；app 是值，每个主机都是薄壳 | 已采纳 (§5.3 的 export 形式由 ADR-0023 取代) | 2026-09-27 |
| [0021](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0021-lambda-failure-channel-is-the-join-of-its-bangs.md) | lambda 的失败通道携带其各个 `!` 一致认可的错误类型 — 先考虑上下文类型，再取合流，String 为顶端 | 已采纳 | 2026-09-27 |
| [0022](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md) | pure fn 中允许输出和 abort 内建函数 — 只写入或 abort，绝不读取，且不使调用者成为 effectful | 已采纳 | 2026-09-27 |
| [0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md) | Wasm HTTP 是 `wasi:http@0.3` — 使用 HTTP 的程序构建为 p3 component，嵌入式主机提供相同的 import，任何主机都不提供自定义 HTTP op | 已采纳 | 2026-09-28 |
| [0024](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) | `fan` 在每个目标上并发执行 effect callback，如同顺序执行 — `fan.map` 运行每个元素，编译器选择基底，不让编写者接触并发概念 | 已采纳 | 2026-09-30 |
| [0025](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0025-wasm-process-is-a-private-host-capability.md) | Wasm `process` 是私有主机能力 — `almide:process/spawn` 仅在 op 集合指明时包含，标准主机在加载时拒绝它，受 `[permissions] proc` 限制 | 已采纳 | 2026-10-03 |
| [0026](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md) | effect category 随透明 fn 类型传播 — 集合无需注解即可流经 callback，采用两级层次，推断 ⊆ manifest ⊆ host 构成一条链 | 已采纳 (D1–D4) | 2026-10-03 |
| [0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) | effect 函数在固定的主机世界中检查 — 带种子的 sandbox，对主机决定的事项给出性质判定，网络使用 loopback fixture | 已采纳 (D1–D8, 2026-10-17) | 2026-10-17 |
| [0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) | 以 `??` / `list.find_map` 替换 `fan.any`；将 `fan.race` 改名为必须带时钟的 `fan.min`；将成本范围限定到 meter 版本 | 已采纳 | 2026-10-18 |
| [0029](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0029-assert-aborts-runs-its-closure-in-a-child.md) | `testing.assert_aborts(expected, closure)` 只在子进程或全新实例中运行其闭包 — 闭包不捕获任何内容，每次 abort 都计入，`testing.assert_throws` 在 dialect epoch 14 退出 | 已采纳 (2026-10-18) | 2026-10-17 |
| [0030](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0030-compile-cost-is-a-contract.md) | 编译成本是一项契约 — 每个阶段都有可计数预算，源码不能提高预算，超出预算产生已定义的诊断 | 已采纳 | 2026-10-24 |
| [0031](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0031-raw-pointers-carry-provenance.md) | 程序可通过其读取的指针携带来源信息 — `RawPtr` 拆分为不透明的 `Handle` 和带边界检查的 `Region`，外部指针仅在其 `@extern` 指明长度时才是 `Region` | 已采纳 | 2026-10-27 |
| [0032](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0032-a-response-content-type-is-its-body-type.md) | 响应的 Content-Type 就是其正文的类型 — `Body` 为每种正文类型提供媒体类型，头部写入不能指定 Content-Type，可承载脚本的类型只能由其安全模块构建 | 已采纳 | 2026-10-29 |

## 原本注意事项

原本本身区分采纳与实现，记录尚未解决的差异与引用，并保留两条编号为 0013 的记录。本阅读版保留这些区别及缺少 0018 的情况，不替其解决问题、不推断外部函数规范，也不把历史实现注记转化为当前保证。
