原文描述元数据：对照 timeout 形态 API 审计 ticks：无法写出的数字问题、三项修正。

<a id="ticks-interface-audit--is-the-demanded-api-shape-different"></a>

# Ticks 接口审计——所需的 API 形态是否不同？

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ticks-interface-audit.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ticks-interface-audit.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ticks-interface-audit.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md)

> 原文注记：历史正文应按开头的取代及纠正通知理解。下文 race 预算可选化及否决类似时间的 tick 单位均作为记录保留，并非恢复为现行规则。原文明示纠正后一否决：确定性换算不会破坏确定性。提议的 flag、Stage 4 接口、优先级变更和证明范围主张未在此独立验证。代码示例保留历史拼写与原日文注释。

> **命名已被 [ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md) 取代。**
> 本审计的“无法写出的数字”问题和工具估算议题仍然存在，但修正 1 的 race 预算可选化不是当前契约。[ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) 将 `fan.race` 改名为 `fan.min`，并把 clock 参数设为必需。不过，否决“将类似时间的单位换算为 tick”的一节是**错误**的——混淆了严重性（确定性完全不会被破坏），ADR-0001 纠正了该否决并采用时间单位。下文作为当时记录保留。
>
> 宪章：[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md)。在 `ticks:` 确定（be4e7d4c）后，仔细审视它与其他语言 timeout 类接口之间的不协调，以及“实际所需 API 形态是否不同”的记录。结论为三项设计修正，已反映于各文档。

<a id="他言語の形と違和感の正体"></a>

## 其他语言的形态与不协调的根源

| 系统 | 形态 | 单位 | 数字的提供者 |
|---|---|---|---|
| Go | `context.WithTimeout(ctx, 5*time.Second)` | 时间 | 人类直觉。deadline 通过 context **传播** |
| JS | `AbortSignal.timeout(5000)` | 时间 | 人类直觉 |
| Rust/tokio | `timeout(Duration::from_secs(5), fut)` | 时间 | 人类直觉 |
| Python | `asyncio.wait_for(coro, timeout=5.0)` | 时间 | 人类直觉 |
| Erlang | `receive after 5000` | 时间 | 人类直觉 |
| Verse | race 分支中的 `Sleep(5.0)` | 时间 | 人类直觉（timeout = race 的一个分支） |
| EVM | gas limit | 计算量 | **工具**（eth_estimateGas）+ 余量 |
| Wasmtime | fuel | 计算量 | **宿主运维者**凭经验校准 |

前 6 行的共同点是“单位为时间”且“数字能凭直觉给出”。只有后 2 行是计算量，两者都**不让人凭直觉写数字**——由工具或运维校准。

不协调的根源就在这里。`ticks:` 不是 timeout 的替代拼写，而是**另一种量**；时间预算（SLA、响应性、终止等待外部 I/O）由 oracle 层的 `ms:`（Stage 4）及宿主边界承担。这种分工是正确的。**问题不是单位，而是数字**：

> 谁也无法估计 `ticks: 100_000`。CM-1 是带版本的抽象成本表，人类和 LLM 都没有预先直觉。而且预算与 body 的实现耦合——重构 `optimal_plan`，100_000 就会失效。**把会随编辑失效的魔法数字设为必需参数，对于倡导 modification survival 的语言，近乎自相矛盾。**

<a id="修正-1--race-の-ticks-を任意化する"></a>

## 修正 1——将 race 的 `ticks:` 设为可选

race 的选择规则（成功结果中 (spend, index) 最小者）**不需要**预算。预算只负责防止发散。无预算 race 的定义明确：

```almide
fan.race { exact(p); heuristic(p) }   // 予算なし — 発明すべき数字がない
```

- 语义是现有定义在 n = ∞ 下的特例。运行分支 0，其 spend 成为后续 cap（在首次完成出现前没有 cap）。
- 终止性：若发散分支位于完成分支之前（按列表顺序），则挂起。这与 `fan {}` 的既有规范（“存在不终止的兄弟分支时挂起是规范行为”，#1023）是**同一风险类别**，不是新漏洞。对需要保护的分支构成（例如搜索失控），加上 `ticks:`：

```almide
fan.race(compute.s(1)) { search_a(p); search_b(p) }   // ガード付き
```

- `fan.bounded` 的 `ticks:` **仍为必需**——施加界限正是该语法存在的理由。
- Lean / 模型定理以预算为参数，在 n = ∞ 时仍原样成立（含发散的情况只是到不了有限观察；合流主张针对终止情况）。

对 MSR 的作用：最常见情况（2～3 分支的组合）不再需要凭空发明常数。LLM 只需写 `fan.race { a; b }`；“本想设 timeout，却把时间写进 ticks”的误用压力，也因根本不用写数字而消失。

<a id="修正-2--数字の供給者はツールとホスト境界"></a>

## 修正 2——数字由工具和宿主边界提供

需要数字时，依据测量而非直觉填写。

1. **校准循环**：Stage 1 的 `--fuel-probe` 直接成为面向用户的测量仪器——通过 `--ticks-report` 实测各 region / 程序消耗的 tick，加上余量作为预算。运作形式与 EVM 的 estimateGas 相同。CM-1 修订（semver-major）时重新校准。
2. **宿主边界的确定性预算**：`almide run --ticks n` / `almide test --ticks n`——用 bounded main 包裹整个程序。数字的所有者成为 **harness / CI**，源码不含常数。与壁钟 CI 超时不同，这是**不会 flaky 的挂起终止器**（相同输入在相同 tick 被截断）。对于确定性地限制 agent 所写代码这一战场（async-world-claim 点名的战场），反而这是最快能形成的产品。实现只是“用 bounded 包裹 main”，没有任何新语义。

<a id="修正-3--timeout-ではないを明文化する"></a>

## 修正 3——明确写出“这不是 timeout”

- ticks 是计算量预算，不是时间预算。需要时间的读者应使用 oracle 层的 `fan.timeout(ms:)`（Stage 4），或宿主边界（`timeout 5 ./app`）。fan-v2 诊断也加入此引导（与既有 pure 约束提示同形）。
- Go context **通过值**携带的 deadline 传播，在 Almide 中由嵌套 min-cap **通过语义**携带——内层 region 隐式继承外层余量（EIP-150 风格）。不存在对应 cancellation token / AbortSignal 的用户可见取消 API（取消是结构性的、属于优化，不出现在观察中，所以没有可传递的对象）。

<a id="単位を時間にしないとだめではへの回答"></a>

## 回答“单位难道不应该是时间吗？”

比较表前 6 行全是时间，因此这个问题必然出现。答案分两层。

**确定性层不能采用时间——这不是偏好，而是定理（T9）。** 壁钟是 host 速度的函数，与“观察 = (程序, 输入) 的函数”矛盾。一旦采用时间，native ⇄ wasm 观察等价、record/replay、零 flaky 就同时崩溃——回到 0.29.0 移除 `fan.timeout` 的原始原因。这不是不肯让步，而是不存在可让步的对象。

**用时间回应对时间的需求。** 比较表表明，主流需求（SLA、响应性、及时止损）具有时间形态，oracle 层 `fan.timeout(ms:)` 负责承接。本审计的结果是**提高其优先级**：B2 不是 Lane 2 的“附赠”，而是与 B1 同级的交付项，作为对时间形态需求的正式回答。ticks 不是时间的替代品，而是给需要**时间无法换来之物**（确定性、replay、sandbox、零 flaky CI）的人提供的独立产品——分为两层本身就是设计对这个问题的回答。

**考虑后否决的折中方案——把类似时间的单位换算成 tick。** 在 CM-1 中 pin 一个参考速率（例如参考机器上 1ms ≈ N tick），允许源码写 `budget: 100.ms` 这样的时间式形式，在编译时将常数换算为 tick。确定性可以保留，但允许写 `100.ms` 却与壁钟不一致的 API，会**反向重演 `fan.timeout(1000)` 的误读事故**（看起来像时间就会被当成时间——0.29.0 的教训）。否决。不过，直觉的连接在工具端完成：`--ticks-report` 在 tick 数旁**同时列出本机实测时间**（`52,000 ticks (≈0.4ms here)`）——与 gas 生态用 estimateGas 和 gas/sec 行情连接直觉是同样的解法。

<a id="反証条件"></a>

## 反证条件

- 无预算 race 的挂起在 dojo / 实际使用中被观察为事故类别 → 将 race 的 `ticks:` 恢复为必需，并带上纠正标记反转本文这一节。
- `--ticks-report` 的实测每逢 CM-1 修订都大幅波动，导致校准不实用 → 在台账侧加强 CM-1 稳定性契约（限制常数变化频率）。
- 宿主边界 `--ticks` 没有需求（无人使用）→ 从阶梯中移除。反之，若需求集中于此，则降低 fan.bounded 语言内表面的优先级并重新分配资源。

保留代码注释的翻译：第一个示例为“无预算——没有需要发明的数字”；第二个为“带保护”。
