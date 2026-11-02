<a id="fan-semantics-audit-what-the-seven-fan-forms-do-today"></a>

# Fan 语义审计：七种 fan 形式当前做什么

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-semantics-audit.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-semantics-audit.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](fan-semantics-audit.zh-CN.md)

> 固定原文的非规范性历史读者快照，不是当前实现指南或独立跨语言比较。未修改原本仍为权威来源；不自动同步，未新运行测试、基准或源码验证。审计自身证据固定于0746eb689（10月17日），带10月18日状态追加，与下方读者版原文固定点不同。已修复 issue、余下发现及已决定但落地中的 Duration 取消均保留各自时间语境。原文 pure 分类仍允许 println，不保证没有可观测输出。他语言主张、“唯一”主张、select_ok／丢弃说明及绝对确定性描述均为历史原文断言，未在此独立重新确立。代码、标识符及参考保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-semantics-audit.md)

> 最后更新：2026-10-17 · 固定提交 **0746eb689**（develop）。每项主张都引用该提交的 `file:line`、在 native 和 wasm 上运行过的 [research/fan-audit/probes/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/fan-audit/probes) 探针，或 `tests/fan_acceptance_test.rs` 中的测试。

本审计记录固定提交上存在的语义，不提出新语义。由此引出的决策见 [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md)（2026-10-18接受），其他语言的对比见 [fan-cross-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-cross-language.md)。

> **2026-10-18状态。** 自 dialect epoch 14 起，下述表面语法不同：`fan.any { }` / `fan.any(xs, f)`（§4）已删除；同样的选择用 `a() ?? b() ?? fallback` 和 `list.find_map(xs, (x) => f(x)?)`，两者按定义均为顺序。`fan.race`（§5）改为 `fan.min(clock)`，必须提供 clock 参数，全失败文本为 `fan.min: no branch
> succeeded within budget`。`fan.min(duration…)` 已决定，在 #3788 跟踪。§6–§10的发现及其 issue 按记录保留。#3778（计量器）和 #3780（ordered 分支）已于2026-10-18修复：native 默认路由读取共享 meter plan，只对 `+` 计费；到达 `@ordered` stdlib 函数的 fan 在每个目标上都按顺序运行分支。#3779和#3784也在同日修复：parallel twins 为每个分配到的块保留一条 timeline，因此在 `fan { list.map }` 内也由最低索引 trap 胜出，轻量 `fan.map` 不再为每个元素支付注册成本。

逐个形式按顺序跟踪：语法 → 类型检查 → lowering → 运行时（native/wasm、pure/effect）。路径包括 `almide run`（native）、同命令加 `ALMIDE_FAN_SEQUENTIAL=1` / `ALMIDE_FAN_COST_OFF=1`，以及在嵌入式宿主上运行的 `almide run --target wasm`（`src/cli/run.rs:511-550`）。解释器（`crates/almide-interp`）没有 CLI 入口，因此其行为只读源码，从未运行。

<a id="summary"></a>

## 概要

| 形式 | 选择 | 失败后继续运行？ | 停止什么？ | Native | Wasm |
|---|---|---|---|---|---|
| `fan { a, b }` | 值元组；最低索引 Err | 是，每个分支 | 无 | 线程（effect 分支也一样） | 按顺序 |
| `fan.map(xs, f)` | 输入顺序列表；最低索引 Err | 是，每个元素 | 无 | pure lambda：pool；effect：按顺序 | 按顺序 |
| `fan.settle { }` / `(xs, f)` | 每个 Result | 是 | 无 | 按顺序（lowering 为元组 / `list.map`） | 按顺序 |
| `fan.any { }` / `(xs, f)` | 输入顺序首个 Ok | 首个 Ok 即停 | 后续元素不启动 | 按顺序 | 按顺序；fs-read prefetch 启动所有读取 |
| `fan.race { }` / `(xs, f)` | compute 最少的 Ok；同值按输入顺序 | 是，每个分支 | 截断带预算分支 | 按顺序 | 按顺序 |
| `fan.bounded(c) { }` | 消耗 ≤ 预算则 Ok | – | 截断本体 | 内联 | 内联 |
| `fan.timeout(d) { }` | 没有检查发现 deadline 则 Ok | – | 截断本体 | 内联 | 内联 |

两类形式可有 effect：`fan`、`map`、`settle`、`any` 的块形式和 mapper 形式。四种形式检查本体 pure（E006）：两种 `race`、`bounded`、`timeout`。每种形式都需要 effect fn 上下文（E007）。

<a id="1-fan--a-b-"></a>

## 1. `fan { a, b }`

**路径**：
- **解析器：** `crates/almide-syntax/src/parser/compounds.rs:272-310`。拒绝 `let`/`var`（283）、循环（286）、`;`（295-296）及空块（304-306）。
- **检查：** effect fn 外报 E007；捕获 `var` 报 E008（`crates/almide-frontend/src/concurrent_reach.rs:1-21`）。
- **Lowering：** 带 `!` 的分支变为 thunk scope（`crates/almide-frontend/src/lower/expressions_fan_arms.rs:30-49`）。
- **Native：** `mark_inline_fans`（`crates/almide-codegen/src/pass_fan_lowering.rs:82-120`）和 `render_fan`（`crates/almide-codegen/src/walker/expressions_fan.rs:4-38`），配合模板 `fan_expr` / `fan_single` / `fan_inline`（`crates/almide-codegen/templates/rust.toml:541-559`）及每分支输出 timeline（`crates/almide-codegen/src/prelude_fan.rs`）。
- **Wasm：** 顺序 desugar（`crates/almide-mir/src/lower/desugar_fan.rs:555-680`）。

| 项目 | 行为 | 证据 |
|---|---|---|
| 成功 | 按分支顺序排列的值元组 | 测试 `results_and_output_follow_input_order…` |
| 错误 | **最低索引** Err，即使后续分支在时间上先失败 | 测试 `the_lowest_index_error…`（`block err A err`）；`desugar_fan.rs:636-640` |
| 顺序 | native：分支同时启动、任意顺序完成。结果和输出按分支顺序（timeline 按分支顺序刷新，`prelude_fan.rs:9-15`）。Wasm：全部按分支顺序。 | 测试 `results_and_output…`（虽然 C 先完成，仍为 `end A, end B, end C`） |
| 失败后 | 每个分支仍运行 | 测试（`ran C`） |
| 取消 | 无。Err 旁有不终止分支会挂起。trap 等待其之前的分支，不等之后的（ADR-0024 D6），也不阻止后续分支开始（`prelude_fan.rs:92-107` 只标记并等待）。 | 审计探针 P7–P9（块形式，验收笔记中） |
| 副作用 | Err 时保留每个分支输出。trap 时保留较低分支输出和 trap 分支部分输出，丢弃较高分支输出。进程状态（`env.set`）共享：**native 与 wasm 不一致（#3780）**。 | 探针 `A/p19_env_race.almd` |
| 空／全失败／不终止 | 空为 parse error；全失败返回最低索引；不终止会挂起 | parser 304-306；测试 |
| 覆盖范围 | native 对每个≥2分支的块使用线程，除非分支捕获 Rc 值（`fan_inline`，顺序）或单分支块（`fan_single`）。`ALMIDE_FAN_SEQUENTIAL` 对块**无效**。Wasm 按顺序，但 pure scalar `list.map` 分支运行实例并行（`crates/almide-wasm/src/fan_par.rs:1-38`）。 | `expressions_fan.rs:15-20` |

`fan { list.map(xs, f) }` 是块形式的数据并行情况：
- **Native：** `almide_rt_list_par_map`（`runtime/rs/src/list.rs:1125-1154`）。它**没有逐元素 trap 顺序**：时间上首次 trap 胜出（#3779，探针 `A/p18_trap_order.almd`，忽略的测试 `a_trap_in_a_parallel_list_map…`）。
- **Wasm：** 宿主 worker 实例（`crates/almide-wasm-run/src/host_fan.rs`）。trap 时顺序重跑，因此最低索引胜出。

<a id="2-fanmapxs-f"></a>

## 2. `fan.map(xs, f)`

**路径**：
- **检查：** `crates/almide-frontend/src/check/static_dispatch.rs:202-283`。回调必须返回 `Result[B, String]`。
- **Native 路由：** `crates/almide-codegen/src/pass_rust_lowering_fan.rs:496-507, 577-597`。Send-safe 类型上的 pure lambda 字面量进入带成本模型（`list.rs:303-343`）的 `almide_rt_fan_map_par`（`runtime/rs/src/fan.rs:60-106`）。effect 回调仅在唯一 effect 为嵌套 fan 时并行（`Purity::for_fan`、`tests/fan_nested_effect_parallel_test.rs`）；其他情况使用顺序 `almide_rt_fan_map`（`fan.rs:18-38`）。
- **Wasm：** `stdlib/fan_map.almd:1-27`，顺序。

| 项目 | 行为 | 证据 |
|---|---|---|
| 成功 | 输入顺序的 `ok(list)` | 测试 `results_and_output…`（`map [36, 1050, …]`） |
| 错误 | 最低索引 Err（`fan.rs:24-35, 110-121`） | 测试（索引6先失败时仍是 `err e2`、`err bad3`） |
| 顺序 | 并行：获分配块的 worker 按索引写 slot。顺序：开始 = 完成 = 返回 = 输入顺序。 | 测试 |
| 失败后 | 每个元素运行（ADR-0024 D1） | 测试（`e2` 后的 `run 3, run 4, run 5`） |
| 取消 | 无。trap 时并行路由仍等待较低元素（`fan.rs:97`）；worker 在 trap 后不停止领取块（`list.rs:643-680`）。 | — |
| 副作用 | effect 回调在每条路径均按顺序，保留每个 effect | 测试 `effects_already_performed…` |
| 空／全失败 | `ok([])`；最低索引 Err | 测试 `empty_input…` |
| 覆盖范围 | native pure：并行；native effect：顺序，除非回调仅有嵌套 fan；wasm：顺序 | 验收笔记中的探针计时 |

<a id="3-fansettle--a-b--and-fansettlexs-f"></a>

## 3. `fan.settle { a, b }` 和 `fan.settle(xs, f)`

- **块：** `lower_fan_settle`（`expressions_fan_arms.rs:5-28`）将其 lowering 为普通**元组字面量**。
- **Mapper：** `crates/almide-frontend/src/lower/calls.rs:65-71` 将其 lowering 为 `list.map(xs, f)`。

两者在每个目标均顺序执行。`almide_rt_fan_settle`（`runtime/rs/src/fan.rs:184-203`）没有生成来源。

| 项目 | 行为 | 证据 |
|---|---|---|
| 结果 | 包含每个 Result 的元组（块）或列表（mapper）；普通分支包装于 `ok` | 测试 `the_lowest_index_error…`（`settle 5`） |
| 顺序 | 所有地方均输入顺序 | — |
| 失败后 | 每个元素运行；**trap** 停止其后所有元素 | 探针 P11/P12（验收笔记） |
| 空 | 块：parse error；mapper：`[]` | 测试 `empty_input…` |
| 拒绝 | thunk 列表 `fan.settle([...])` 为 E027；effect fn 外为 E007 | — |

<a id="4-fanany--a-b--and-fananyxs-f"></a>

## 4. `fan.any { a, b }` 和 `fan.any(xs, f)`

**路径**：
- **Parse：** 块变为 `fan.__any_block([() => a, …])`（`crates/almide-syntax/src/parser/fan.rs:296-321`）。
- **Lowering：** 块改名为 `any`，mapper 改为 `any_map`（`lower/calls.rs:55-64`）。
- **Native：** `almide_rt_fan_any` / `almide_rt_fan_any_map`（`runtime/rs/src/fan.rs:148-179`）。
- **Wasm：** `stdlib/fan_any.almd` 或 `crates/almide-wasm/src/fan.rs:86-90` 的 accumulator。fs-read 回调走 prefetch lowering，`crates/almide-wasm/src/fan.rs:74-85`。

| 项目 | 行为 | 证据 |
|---|---|---|
| 成功 | **输入顺序**首个 Ok（不是时间首个） | 测试 `any_runs_nothing_after_the_winner…` |
| 全失败／空 | 常量 `err("fan.any: all candidates failed")`；丢弃元素自身错误 | 测试 `empty_input…` |
| 顺序 | 开始 = 完成 = 返回 = 输入顺序 | 测试（`probe -1, probe -2, probe 5`） |
| 胜者之后 | 胜者后没有元素开始（`probe 7` 永不打印） | 测试 |
| 取消 | 未开始元素只是从不启动。例外：wasm 中读取文件的回调会被**预取**，所有读取预先发出，落败者被丢弃（op 42 / subtask.cancel）。读取没有可见副作用，ADR-0024 修订（`docs/adr/0024-…md:443-445`）允许此事。 | 探针 `B/any_fs_import.almd` |
| 副作用 | 按顺序保留胜者之前每个元素的 effect | 测试（`ran a, ran b`） |
| 覆盖范围 | 每个目标都允许 effect 分支和回调；所有地方顺序 | `static_dispatch.rs:322-359` |

<a id="5-fanrace--a-b--fanracexs-f-and-budgeted-forms"></a>

## 5. `fan.race { a, b }`、`fan.race(xs, f)` 及带预算形式

**路径**：
- **Parse：** `parser/fan.rs:206-273`。分支以 `,` 或换行分隔；拒绝 `;`（探针 `B/semi.almd`）。
- **检查：** `crates/almide-frontend/src/check/infer_calls_closures.rs:363-462`。在计量区域检查分支及 mapper 为 **pure**；结果是 `Result[T, String]`。
- **Lowering：** `crates/almide-frontend/src/lower/expressions_fan.rs:273-685`，顺序 fold，将每个分支提取为 `__almd_bounded_N(budget, …)`，保留最小 `(spend, index)`。比较为严格 `<`（`:435`、`:623`），相同则取较早分支。
- **死运行时：** `almide_rt_fan_race`（`runtime/rs/src/fan.rs:123-142`）及 `rewrite_race_head`（`desugar_fan.rs:305-340`）没有生成来源；thunk 列表调用为 E027（`static_dispatch.rs:284-307`）。

| 项目 | 行为 | 证据 |
|---|---|---|
| 成功 | 消耗最少的 Ok；相同按输入顺序 | 测试（`race 205 105 105`、`race map 15`） |
| Err 分支 | 自行失去资格，即使更便宜 | 测试（`race err-skip 5000`） |
| 全失败／空／全超预算 | `err("fan.race: no branch completed within budget")`，无预算时也一样 | 测试 `empty_input…` |
| 顺序 | 每个目标都按输入顺序逐个运行分支 | `expressions_fan.rs:316` |
| 胜者之后 | 每个分支运行；后续分支 trap 会中止程序 | 探针 `B/race_trap.almd` |
| 取消 | 带预算分支在下一计费点截断并返回零值（`render_native_op_families.rs:39-48`、`crates/almide-wasm/src/fuel.rs:114-127`）。没有并发，因此没有等待。 | 探针 `B/race_diverge_budget.almd` |
| 预算范围 | 块形式：同一预算给**每个分支**，只求值一次（`expressions_fan.rs:302-309`）。mapper 形式：每元素（`:541-575`）。不提供预算时上限为 `i64::MAX` ns。 | 探针 `B/race_map.almd` |
| 副作用 | 分支 pure（E006）。仅 `println`、trap 或不终止可被观测。 | 测试 `effects_inside_race…` |
| 覆盖范围 | Int 和 String 胜者。**Float/List 胜者在两个目标均无法编译（#3782）。** | 探针 `B/rt_*.almd` |
| 跨目标 | **胜者可能不同（#3778）** | 探针 `B/race_flip.almd`；忽略的测试 |

无预算的不终止分支（探针 `B/race_diverge_nobudget.almd`）在 native 立即结束，在 wasm 运行多年。两者都受同一 `i64::MAX` ns 上限截断；native 中 LLVM 将无副作用 fuel 倒计数折叠为闭式。答案相同，仅墙钟时间不同，所以这不是语义差异。长但有限的 race 在两个目标一致（探针 `B/race_long_nobudget.almd`）。

<a id="6-fanboundedcomputexn--body-"></a>

## 6. `fan.bounded(compute.X(n)) { body }`

**路径**：
- **Parse：** `parser/fan.rs:127-159`；本体为完整块。
- **检查：** `infer_calls_closures.rs:331-357`。本体 pure，预算必须为 `Compute`，Result 类型本体是错误。
- **Lowering：** `lower/expressions.rs:188-226` 与 `lower/expressions_fan.rs:142-260` 生成 `budget_enter → body → budget_exit`。
- **Native：** 默认 v1 MIR render；`ALMIDE_FORCE_METER_SPLICE=1` 和 `ALMIDE_FORCE_METER_CODEGEN=1` 选择另外两条路由（`src/cli/mod.rs:240-341`）。shim 在 `render_native_shims.rs:262-292`。
- **Wasm：** `crates/almide-wasm/src/fuel.rs:16-61, 119-195`。

| 项目 | 行为 | 证据 |
|---|---|---|
| 成功 | 当且仅当消耗 ≤ `floor(ns / 3)` 单位时为 `ok(v)` | 测试（3006 ns时 `bounded at ok 496551`，3005 ns时 `under`） |
| 错误 | `err("fan.bounded: budget exhausted")` | 测试 |
| 顺序 | 一个本体，在调用方线程内联 | — |
| 超过时 | 下一计费点截断；向外直到该区域的每个包含函数均返回零值 | `fuel.rs:114-118` |
| 副作用 | effect 为 E006。允许 `println`（ADR-0022），保留截断前输出。 | 测试 `effects_already_performed…`（每条路径9行 `iter`） |
| 边界情况 | 空本体在 native 为 `ok`，**wasm 为 E082（#3783）**；没有计费点的本体即使预算为零也总会完成；源码不能嵌套区域（E007） | 探针 `C/e4*.almd`、`C/n1_nocharge.almd`、`C/n3_nest.almd` |
| 单调用参数 | 在 `{ f(g(x)) }` 中，`g(x)` 于计量器开始**之前**执行（`walker/expressions_fan.rs:167-168`）。此行为未文档化，是 ADR-0028 Q6。 | 探针 `C/m1_meter_shape.almd` |
| 跨目标 | **判定可能不同（#3778）**：无循环 wrapper 入口和字符串拼接 | 探针 `C/m2_wrap_diverge.almd`、`C/concat_cut.almd`；忽略的测试 |

<a id="7-fantimeoutdurationxn--body-"></a>

## 7. `fan.timeout(duration.X(n)) { body }`

**路径**：
- **Parse：** `parser/fan.rs:163-200`。旧 `fan.timeout(100, f)` 为 E027（探针 `C/l1_legacy.almd`）。
- **检查：** `infer_calls_closures.rs:470-496`。与 `bounded` 相同规则，类型改为 `Duration`。
- **Native：** `render_native_shims.rs:164-216`，单调 `Instant`。
- **Wasm：** `fuel.rs:65-99`，读取 host op 34，即 `SystemTime`（#3781）。

| 项目 | 行为 | 证据 |
|---|---|---|
| 成功／错误 | 当且仅当没有计费点检查发现 `now ≥ deadline` 时 Ok；否则 `err("fan.timeout: deadline exceeded")` | 探针 `C/t1_basic.almd` |
| 检查时机 | 仅在计费点（函数入口、循环头、拼接）；绝不在单个操作中途 | `render_native_op_families.rs:50-53` |
| 副作用 | effect 为 E006（`C/e1_timeout_effect.almd`）；保留打印 | 测试 `effects_inside_race…` |
| 可重复性 | 不承诺：截断点依赖机器（`C/t2_vary.almd`）。`ALMIDE_OMEGA` replay **仅 native 可用（#3781）**。 | 探针 `C/t3_replay.almd` |
| 边界情况 | 与 `bounded` 相同：空本体在 wasm 为 E082，无计费本体永不停止 | 探针 `C/e4b_empty_timeout.almd`、`C/n1_nocharge.almd` |

<a id="8-logical-cost-the-compute-meter"></a>

## 8. 逻辑成本（`Compute` 计量器）

- **单位。** 一个计费单位为3 ns：`CM1_NS_PER_CHARGE`，2026-08-02重新校准，版本 v0.3（`crates/almide-types/src/time_units.rs:9-18`）。预算按 `units = ns / 3` 转换。
- **计费点：**
  - 每个函数入口1；
  - 每次循环头求值1（所以 n 次迭代成本 n + 1）；
  - 每次字符串拼接 `1 + len/16`（仅 native 默认路由，#3778）；
  - codegen 计量器中每次 closure hop 1（`crates/almide-mir/src/charge_probe.rs:3-4, 67-103`、`crates/almide-codegen/src/pass_meter_charge.rs:10-19`）。
  - stdlib 本体从不计费。
  - Wasm 和 codegen 计量器免除无循环、非递归用户函数的入口（`crates/almide-ir/src/meter_plan.rs:5-9, 126-130`）；native 默认路由不免除（#3778）。
- **预算范围。** `bounded`：整个本体。race 块：每分支获得完整预算。race mapper：每元素获得完整预算。区域与外围区域做 min-cap；源码不能嵌套它们。
- **数值依赖编译器。** 消耗定义在由 inliner 和 lowering 决定的 IR/MIR 计费点上。因此无需源码变更，编译器更新就可能改变 race 胜者或 bounded 判定。计量程序在全程序范围关闭 LICM（`pass_licm.rs:38-43`）、展开（`unroll.rs:8-10`）和 accumulator TRE（`emit.rs:125-133`），以固定计数。当前同一次构建的两条 native 路由就已不一致（#3778）。

<a id="9-effect-admission"></a>

## 9. Effect 准入

| 形式 | effect 分支／回调 | 诊断 |
|---|---|---|
| `fan { }`、`fan.settle { }` | 允许 | — |
| `fan.map`、`fan.settle(xs, f)`、`fan.any`、`fan.any { }` | 允许（lambda 或裸 effect fn） | — |
| `fan.race { }`、`fan.race(xs, f)` | 拒绝 | E006 `cannot call effect function '…' inside a fan.race region` |
| `fan.bounded`、`fan.timeout` | 拒绝 | E006，提示“fan.bounded 计量确定性计算，所以本体为 PURE” |
| effect fn 外任意形式 | 拒绝 | E007（块）；`fan.map` 给出**无编码**错误（`static_dispatch.rs:176-180`） |
| 捕获的 `var` | 拒绝 | E008 |

证据：测试 `effects_inside_race_bounded_and_timeout_are_refused`、测试 `fan_outside_an_effect_fn_and_a_captured_var_are_refused`、探针 `C/a*.almd`。

<a id="10-discrepancies"></a>

## 10. 不一致

有三类条目：

- **在此修正**：注释或文档说错了，本 PR 不改变行为而修正文字。
- **Issue**：实现违反承诺。已提交 issue，这里不改变行为。
- **ADR**：尚无承诺；问题在 ADR-0028 中。

| # | 位置 | 原描述 | 实际运行 | 处理 |
|---|---|---|---|---|
| 1 | `runtime/rs/src/fan.rs:8-10, 123-147` | race 返回首个 settle 的 thunk；“并行 + 墙钟非确定性”；race/settle 保留 `thread::scope` | frontend 中按字典序最小的 `(spend, index)` fold；`almide_rt_fan_race` / `almide_rt_fan_settle` 不可达 | 在此修正（注释） |
| 2 | `crates/almide-mir/src/lower/desugar_fan.rs:1-6` | “`fan.race([…]) ≡ t0`”；any 回退到最后一个 thunk 的结果 | race head 不可达；any 返回常量 Err | 在此修正 |
| 3 | `static_dispatch.rs:146-147, 189-197, 205-209, 331-333` | “已删除的 race”；“auto-`?` 传播”；any mapper“尚未实现” | 仅 race *调用*有 tombstone；auto-? 为 E041；any mapper 已可用 | 在此修正 |
| 4 | `runtime/rs/src/fan.rs:41-45`、`pass_rust_lowering_fan.rs:164-165` | “有意拒绝 String”；Rc 类型保持顺序 | String/List/Set/Map 是 Send-safe，走并行（`:185-186`） | 在此修正 |
| 5 | `docs/SPEC.md` §fan、`docs/specs/effect-system.md:313-317`、C-004 | `fan.race` 在0.42.0删除，仅运行 thunk[0]；`fan.any(thunks)` | race head 和 mapper 可用；thunk 列表为 E027 | 在此修正（SPEC、effect-system）；C-004措辞 → 先 als（见下） |
| 6 | SPEC、ADR-0001:111 | race 分支之间使用 `;` | `;` 是 parse error | 在此修正（SPEC；ADR-0001是决策记录，原样保留） |
| 7 | SPEC §fan.bounded | “必须单函数调用”；“嵌套预算 min-cap”；`fan.timeout` 为“未来” | 完整块本体；源码不能嵌套；timeout 存在且 pure | 在此修正 |
| 8 | ADR-0024 Status | “尚未实现任何部分” | D1、D5、D6已实现；D3/D4/step 4未实现 | 在此修正（状态行） |
| 9 | ADR-0024 D7 | `ALMIDE_FAN_SEQUENTIAL=1` 是消融开关 | 只影响 par twins，不影响块 | 在此修正（D7下的注记） |
| 10 | `stdlib/fan_any.almd:3-4` | 后续元素永不求值 | wasm fs-read prefetch 启动全部读取 | 在此修正 |
| 11 | `crates/almide-interp/src/eval.rs:146-153` | Err 分支短路 | 求值每个分支，再取首个 Err（只读未运行） | 在此修正 |
| 12 | `src/cli_args.rs:41-44` | `--target wasm` 在 `wasmtime` CLI 运行 | 在嵌入式宿主运行 | 在此修正 |
| 13 | CHEATSHEET:691、696 | `fan.settle`“对 scalar”；“分支为并行兄弟” | 也支持 String/List；settle 顺序 | 在此修正 |
| 14 | `lower/expressions.rs:270-271`、`expressions_fan.rs:479-480`、`ast.rs:263-268`、`infer_calls_closures.rs:326-330, 361` | “while-scan”；“每分支是单个 pure 调用”；bounded 本体“单调用” | `ForIn`；接受表达式分支与块本体 | 在此修正 |
| 15 | CHEATSHEET:728、C-204、C-205、C-294 | 每个目标同胜者／判定 | native 默认计量器 ≠ wasm 计量器 | Issue #3778 |
| 16 | C-200、`list.rs:1141-1142` | trap 等待较低元素 | native `fan { list.map }` 选择时间首个 trap | Issue #3779 |
| 17 | ADR-0024 D3、C-004 | 到达 ordered op 的 fan 顺序运行 | 无 ordered-op 分析；native 上 `env.set` 竞争 | Issue #3780 |
| 18 | C-208、cli.md“到处 replay” | 每个目标可 ω replay | 仅 native；测试空洞 | Issue #3781 |
| 19 | CHEATSHEET:705-708、SPEC §13.5 | race 对 `T` 泛型 | Float/List 胜者无法编译 | Issue #3782 |
| 20 | — | （无承诺） | 空 bounded/timeout 本体：native ok、wasm E082 | Issue #3783 |
| 21 | — | （无承诺） | 单调用参数在 meter / deadline 外运行 | ADR-0028 E；#3790已修复 |
| 22 | ADR-0024 D4 | fan 顺序运行时给提示 | 不发出提示 | ADR-0028测试计划（诊断） |
| 23 | C-200 / ADR-0024 D6 | 基础层在 trap 后“停止启动元素” | 没有停止启动；仅限制输出 | ADR-0028测试计划 |
| 24 | 诊断 | — | clock 类型不匹配与“普通值”错误没有编码；一个 E006 连带 E001/E041；`fan.settle(xs, f)` 非 Result E001 文本混乱 | ADR-0028 §Diagnostics |
| 25 | `docs/specs/cli.md`、C-006、C-214 | “fan.timeout 不存在／仍已删除” | `fan.timeout(d) { }` head 存在 | C-006 / C-214措辞 → 先 als |

第5、25行等契约文字修正会改变现有契约的*陈述*。`scripts/check-als-pin.sh` 将陈述与 judge ledger 比较，因此编辑需先经过 almide/als。这里只列出它们，本 PR 不进行其中任何修改。
