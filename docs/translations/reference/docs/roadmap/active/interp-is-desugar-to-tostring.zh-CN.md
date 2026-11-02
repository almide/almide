<!-- description: Historical interpolation desugaring decision and its scoped correctness rationale -->
<a id="stringinterp-is-not-special-syntax--desugar-it-to-concat--to_stringpart"></a>

# StringInterp 不是特殊语法 — 将其脱糖为 `concat + to_string(part)`

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](interp-is-desugar-to-tostring.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](interp-is-desugar-to-tostring.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](interp-is-desugar-to-tostring.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/interp-is-desugar-to-tostring.md)
> 原文本身已限定较强的历史正确性主张：v0退役、Bool/Float函数体存在、有限byte比较不证明全部组合，仍依赖desugar/formatting/ownership/renderer。保留而不执行旧构建指令，不把旧缺失转换表当作现状。scratchpad wiki引用及个人salvage path是历史指针，此处未验证为可用成果物。

<a id="historical-design-scope-of-the-correctness-claim"></a>

## 历史设计；正确性主张的范围

这是 2026 年 6 月 17 日的脱糖决定及工作记录。其中保留 v0 oracle 的指令与缺失转换清单不是当前实现指令；v0 已退役，当前 stdlib 已包含 Bool 和 Float 转换函数体。

下文“静默误编译在结构上不可能”的说法夸大了脱糖所确立的事实。移除一条延迟插值路径可以消除该路径的失败模式，但正确性仍依赖脱糖、格式化函数体、ownership 和 renderer。有限的 byte 比较是证据，不是对所有组合的证明。参见[已证明与受信任的区别](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md)。下文保留原推理，同时加上这一限定。

理想终态（2026-06-17 确定）。取代按位置处理的 interp-lowering 方案。

<a id="the-principle"></a>

## 原则

`"a=${x}, b=${y}"` 在语义上不过是 `concat("a=", to_string(x), ", b=", to_string(y))`。
所以正确架构是：**frontend 把 `StringInterp` 脱糖为 `ConcatStr` 链，每个 Expr part 包在所属类型的 `to_string` 中**，MIR/lowering 完全看不到 `StringInterp` node。这样，插值使用已经 self-host 且 byte 验证的 `__str_concat` 和各类型的 `to_string` primitive。`interp_str_lowerable` 谓词与“385/460 被排除”的集合**不复存在** — 它们只因 interp 被当作特殊 lowering 构造而存在。

这正是 trust-spine 理念：**把特殊语法化为已证明的 primitive。**leaf 部分（commit 83a72efa）增加谓词及按位置处理，是错误方向（更多特例）。正确方向是消除特殊性。

<a id="status-2026-06-17--step-2-shipped-v1-only-v0-oracle-untouched-independently-verified"></a>

## 状态 2026-06-17 — step 2 仅在 v1 交付（v0 oracle 不变），经独立验证

脱糖已落地（commit f81a93a0，develop-v1）于 **v1 lowering**（`crates/almide-mir/src/lower/mod.rs` 的 `desugar_string_interp`），而非共享 frontend。这有意推迟 frontend 脱糖（layer 1），以保持 v0 风险为零。证据：step-2 diff 仅触及 `almide-mir`（另加新 `stdlib/bool.almd`）；`git diff crates/almide-frontend` 为空；v0 的 `emit_string_interp`（原位于 `emit_wasm/calls_string.rs`，随 #782 删除 — 见 git 历史）未变。退役的是 v1 内部的 `interp_str_lowerable` 谓词，不是 v0 oracle。dual-oracle 完整保留，并且就是 gate：将 v1 脱糖输出与 v0 输出逐 byte 比较（corpus-wall (a)=250 个 interp 与 v0 byte 一致；另有独立 golden：Int/String/Bool/edge 在 wasmtime 上 byte 一致，Float/compound 明确 WALL）。+22 caps 增益经对抗验证为健全（可达 Stdout 的 interp operand 传递暴露 Stdout → witness `|0`、declared∅ → REJECT，绝不假 green）。门槛：caps 4134（ACCEPT）、mir>ir 0、FORBIDDEN 0、428 tests、3-property ACCEPT。

<a id="the-oracle-preservation-law-non-negotiable--binds-the-eventual-frontend-desugar-layer-1"></a>

### ORACLE 保留法则（不可协商）— 约束最终的 frontend 脱糖（layer 1）
把脱糖移入共享 frontend，是唯一会触及 v0 interp codegen 的步骤。它已推迟；尝试时必须遵守：**不得删除 `emit_string_interp`。**运行两条路径，在 CI 对整个 corpus 作 byte 比较，仅在一致性获证明后退役 `emit_string_interp`（translation-validation，将 #570 模式用于脱糖本身）。“先删再赌脱糖”被禁止 — 在取得形式化替代之前，绝不删除 oracle。此前，v1-only 脱糖以零 v0 风险提供 trust-spine 收益。

<a id="budget-two-independent-hard-sub-problems-dont-underestimate--clean-framework--done"></a>

### 预算：两个独立的困难子问题（不要低估 — 清晰框架不等于完成）
脱糖框架清晰，但与 v0 byte 一致有两个独立、已知困难的格式化 wall。各自落地前，其 interp part **明确 WALL**（已验证：Float `${f}` → 点名 `float.to_string` 的 Unsupported）— 绝不静默误编译。
- **`float.to_string` = dtoa（task #63）** — 必须逐 byte 匹配 v0 的 `f64` Display：正确的最短 round-trip 舍入、整数值 `.0` 后缀、NaN / Inf / -0.0，以及科学计数法阈值。（历史：float native↔wasm 显示漂移。）完全复现 v0 formatter 的已救回 dtoa WIP 是 step 4 的主体，不是脚注。
- **复合 repr（`${list}` / `${record}` / `${tuple}`）= task #64** — 必须精确复现观察到的 v0 格式：括号、分隔符、字符串元素引号、嵌套、空值情况。self-host 的 `emit_repr_value` 必须镜像 v0 实际输出，而非只是看起来合理。

<a id="what-it-reveals-the-real-work-is-total-to_string-display"></a>

## 揭示的问题：真正工作是全域 `to_string`（Display）

一旦 interp = `to_string(part)`，剩余工作只有：**每个类型都有 `to_string`**。corpus 中 385/460 个“损坏”的 interp，仅因该类型缺少 `to_string` 而损坏：

| part | 所需内容 |
|---|---|
| `${n}` Int | `int.to_string` ✓（完成） |
| `${s}` String | 恒等变换 ✓ |
| `${list.len(xs)}` | materialize 调用 → 将 Int 包入 `int.to_string` |
| `${b}` Bool | `bool.to_string`（`if` → "true"/"false"）— 简单，缺失 |
| `${f}` Float | `float.to_string` = **dtoa** — 困难，缺失 |
| `${xs}` `${rec}` | `List.to_string` / record Debug-repr（`emit_repr_value` self-host）— 缺失 |

**所以“StringInterp”实际是“全域 `to_string`（Display）”问题。**`to_string` 全域后，interp 到处都无需额外工作即可运行。

<a id="this-redraws-tail-vs-essential"></a>

## 重新划分“尾项与核心”

- **`to_string` family — Int / Bool / Float(=dtoa) / List / record Display — 是核心。**interp 与 `print`/`println` 都经过它，到处复用。
- **超越函数（sin/cos/tan/log_gamma）才是真正尾项** — interp 和 print 从不到达它们。

**结果：`float_dtoa` 不是偏离到尾项。**它是 *`float.to_string` 的前提*，属于核心。只因尚未建立“interp = `to_string`”视角，才像没有动机的尾项。采用该视角，**dtoa 就是优先核心工作**（已救回到 `/Users/o6lvl4/cctmp/float_dtoa.almd.salvage`，将其恢复）。反之 trig 确实是尾项（低优先级正确 — 不过已完成，也没问题）。

<a id="verification-becomes-compositional"></a>

## 验证变成可组合的

- 当时：对每个位置（bind/call-arg/tail/match-arm/concat）做 interp byte 一致验证 = 组合爆炸。
- 理想：每个类型的 `to_string` 只做一次 byte 一致验证 → 使用该类型的每个 interp 自动获验证。证明一次 `int.to_string` → `"${n}"`、`"x=${n}"`、`"${a+b}"` 全部通过组合获验证。

<a id="wall-vs-hole-dissolves-structurally"></a>

## “wall 与 hole”在结构上消解

使用脱糖后，类型**没有 `to_string`** 的 part 无法脱糖 → **显式 frontend 错误（明确 WALL）**。能脱糖的 part 使用已证明的 primitive。**静默误编译（以及当时 corpus-wall 漏掉的 invalid-wasm）在结构上不可能** — 已没有“延迟 Opaque interp”路径可输出损坏 wasm。（诊断确认当时 non-leaf interp 会发出 INVALID WASM，明显失败却未被 corpus-wall 捕获 — 脱糖完全移除该路径。）

<a id="implementation-layers-the-care-points"></a>

## 实现层次（注意点）

1. **Frontend 脱糖**：`StringInterp{parts}` → 每种类型使用 `to_string(part)` 的 `ConcatStr` fold。frontend 与 v0 共享，因此也改变 v0 的 interp codegen 路径（从 `emit_string_interp` 到 concat+to_string）。必须**经 dual-oracle 验证**：脱糖形式对每种 part 类型均与 v0 当时 `emit_string_interp` 输出 byte 一致（包括复合 repr、Float 显示边界情况）。这就是 gate — 此处不容许 v0 regression。
2. **无需额外工作即可 count==lower**：脱糖位于 frontend，因此 `count_ir_calls`（harness）及 `lower_function` 都看到脱糖后的 `ConcatStr`+`to_string` IR — 没有特殊谓词，没有 count-vs-lower 分歧（整个 [[../../../.claude .../project_v1_gate_count_vs_lower]] 陷阱消失）。
3. **v1 interp 覆盖 == v1 `to_string` 覆盖。**某 part 的 `to_string` 在 v1 self-host → 可用；某 part 的 `to_string` 尚未 self-host → 该 `to_string` 调用在 v1 lowering 中必须是**明确 Unsupported wall**，而非 unlinked-call → invalid wasm。因此前提是：**未注册／未链接 stdlib 调用的 v1 lowering → `Unsupported`（wall），绝不 invalid wasm。**（或者，在类型没有 `to_string` 时由 frontend 脱糖拒绝；但这也会在 v0 拒绝，除非 v0 有该 `to_string`，否则是 v0 regression。v0 对每个类型都有 Display，故脱糖对 v0 应始终成功；v1 wall 则是 unlinked-call → Unsupported。）
4. 然后**补齐各类型 `to_string`**（每个都通过组合自动启用其 interp）：Bool（简单）→ 恢复 Float=dtoa（救回的 WIP，核心）→ 复合 repr（List/record Display）。

<a id="build-order"></a>

## 构建顺序
1. v1：未链接 stdlib 调用 → `Unsupported`（结构性 wall 前提）。2. Frontend 脱糖 StringInterp → concat+to_string，dual-oracle byte 匹配 v0（退役 `interp_str_lowerable` + 按位置处理）。3. `bool.to_string`。4. 恢复 `float.to_string`（dtoa）。5. 复合 Display。
每步的检测器是按类型的 `to_string` byte 一致，而非按位置的 interp。
