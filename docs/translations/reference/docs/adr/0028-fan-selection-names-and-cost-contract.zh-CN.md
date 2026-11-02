<a id="adr-0028-replace-fanany-with---listfind_map-rename-fanrace-to-fanmin-with-a-required-clock-admit-the-real-time-clock-and-scope-the-cost-contract-to-a-meter-version"></a>

# ADR-0028：以 `??` / `list.find_map` 取代 `fan.any`，把 `fan.race` 改为必须提供 clock 的 `fan.min`，接纳实时时钟，并把成本契约限定于 meter version

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0028-fan-selection-names-and-cost-contract.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0028-fan-selection-names-and-cost-contract.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0028-fan-selection-names-and-cost-contract.zh-CN.md)

> 固定原文的非规范性历史读者快照，未修改原本为权威来源。不自动同步，未独立运行基准、证明当前安全性或执行历史命令。设计获接受不证明所有后续实现已落地：保留测试计划状态，包括meter-version gate、有序进程后续事项及等待#3788的Duration clock。证据审计pin 0746eb689不同于本读者版原文pin。协作取消在边界停止未获胜方并等待其结束，已执行effect保留；wasm顺序调度合法，但不是native同时执行。跨语言的概括性措辞和成本等价属于原文当时主张，此处未独立重验；同一meter version范围不等于跨版本稳定。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md)

- **状态**：已接受（2026-10-18）。Q1 于2026-10-17批准，附一项修订（没有弃用窗口）；
  其余问题均于2026-10-18通过与其他语言设计比较而决定（§决定）。没有未决问题。
- **日期**：2026-10-17（提议；同日跨语言命名比较后修订），2026-10-18（接受，
  取代首轮修订的临时名称 `fan.first_ok` / `fan.min_cost` — 见§被否决的替代方案）。
- **范围**：`fan` 的选择形式（`fan.any`、`fan.race`）、其诊断与文档；语言原先没有的
  实时选择；`fan.min` 和 `fan.bounded` 据以选择的 `Compute` meter 契约；
  以及2026-10-17审计发现的语义问题（#3778–#3783）。
- **证据**：[fan-semantics-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-semantics-audit.md)
  （固定于0746eb689）、[fan-cross-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-cross-language.md)、
  `tests/fan_acceptance_test.rs`、issue #3778–#3784。
- **落地版本**：dialect epoch 14（`fan-selection-names`、
  `proofs/dialect-epochs.toml`）。`fan.min` 的实时时钟和
  `fan.timeout` 的 I/O 函数体属于后续 #3788。

<a id="context"></a>

## 背景

1. **Almide 提供五种选择中的三种。**

   | 种类 | 选择什么 | 本 ADR 之前的 Almide |
   |---|---|---|
   | K1 | 输入顺序中的首次成功 | `fan.any` |
   | K2 | 实际时间中首次成功 | – |
   | K3 | 首次完成，无论成功失败 | – |
   | K4 | 最小逻辑成本（相同则按输入顺序） | `fan.race` |
   | K5 | 收集全部 | `fan.map` / `fan.settle` / `fan { }` |

   K2 和 K3 是刻意缺失的。其答案依赖调度，而 ADR-0024 N7 及 SPEC §13.5
   （定理 T9）将这种情况排除在确定性 tier 之外。
2. **两个名字在被误读时都能静默编译。** 预期 JS `Promise.race`（K3）的写作者，
   从 pure arm 的 `fan.race { a(), b() }` 得到 K4。
   预期 `Promise.any`（K2）的写作者，从 `fan.any` 得到 K1。
   两者都不触发诊断。只有 I/O arm 的 `race` 遇到 E006。
3. **成本值依赖编译器。** 消耗在 inliner 和 lowering 决定的 IR/MIR charge site 计数
   （审计§8）。同一次 build 的两条 native 路径彼此不一致，也与 wasm 不一致
   （#3778）。本 ADR 之前“所有 target 和 machine 上胜者/判定相同”的承诺
   （CHEATSHEET、C-204/C-205）没有提到编译器版本。
4. **最常见的实际用途没有写法。** “询问两个服务器，采用先回复的那个”（K2）
   正是其他语言 `race` / `any` 的用途；跨语言表在 JS、Rust、Python、Go、Haskell、
   Kotlin、Swift 中展示了它。Almide 的回答是“无法表达”。

<a id="naming-in-other-languages"></a>

## 其他语言的命名

每行是库文档规定的含义，按选择种类排列。

| 名称 | 所在位置 | 选择种类 | 顺序保证 |
|---|---|---|---|
| `race` | JS `Promise.race` | K3 首次结束，包括拒绝 | 无（墙钟时间） |
| `race` | Haskell `async` `race :: IO a -> IO b -> IO (Either a b)` | K3 首次完成；**取消未获胜方** | 无 |
| `select` / `select!` | Rust `futures`、`tokio`；Kotlin `select`；Go `select`（规范规定在就绪 case 中随机选择） | K3 | 无，或按设计随机 |
| `firstCompletedOf` / `anyOf` / `WhenAny` | Scala `Future`；Java `CompletableFuture.anyOf`；C# `Task.WhenAny` | K3 | 无 |
| `any` | JS `Promise.any`；Rust `futures::select_ok`（同类） | K2 | 无 |
| `findAny` / `find_any` | Java `Stream.findAny`；Rayon `find_any` | 任意匹配，**明确非确定性** | 无 |
| `any` | Python `any()`、Rust `Iterator::any`、Haskell `any` | Bool，而非选择 | – |
| `find_map` / `find_value` / `firstNotNullOfOrNull` / `collectFirst` | Rust `Iterator::find_map`、OCaml `List.find_map`；Elixir `Enum.find_value`；Kotlin；Scala | **K1** — 均为 LIST/iterator 函数，从不是并发原语 | 输入顺序 |
| `findFirst` / `find_first` / `find_map_first` | Java `Stream.findFirst`；Rayon `find_first`、`find_map_first` | **K1** | 输入顺序 |
| `asum` / `<\|>` / `or_else` / `.catch(() => b())` | Haskell `Alternative`；Rust `Result::or_else`；JS | **两三个候选的 K1**：惰性求值的 fallback 链 | 输入顺序 |
| `min_by_key` | Rust `Iterator::min_by_key`：“多个元素同为最小值时，返回第一个” | **与 K4 完全相同的规则**（最小 key，相同则取第一个） | 相同时按输入顺序 |

结果：

- **所有使用 `race` 一词的生态中，它都表示“最先完成”**，Haskell 还会取消未获胜方。
  Almide 的 `race` 既不是最先完成，也不取消。
- **`any` 从不表示有序选择。** 它表示“无顺序保证”
  （`findAny`、`find_any`、`Promise.any`）或 Bool。
- **有序的首次成功在各处都是 list 函数**，少数候选时则为惰性 fallback 链。
  没有其他语言把它做成并发组合子，因为它按定义就是顺序的。
- K4 的规则（最小 key，相同则取第一个）是 `min_by_key` 文档中的契约。
  约定俗成的词是 **`min`**。
- Almide 曾经做过一次。0.42.0 删除 `fan.race(thunks)`，因为“名字承诺了语言刻意不提供的
  时序行为”（SPEC §fan、C-004）。仍在使用的 `fan.race` head 用同名重演同一缺陷。

<a id="how-other-compilers-rename-a-public-name"></a>

## 其他编译器如何重命名公开名称

| 编译器 | 机制 | 用户看到什么 |
|---|---|---|
| Swift | `@available(*, deprecated, renamed: "new")` | 带有一键改为新名称的 fix-it 的警告 |
| Kotlin | `@Deprecated(message, ReplaceWith("new(...)"), level)`，跨 release 依次 WARNING → ERROR → HIDDEN | IDE 替换所有用法；旧名消失前保留明确错误 |
| Rust | `#[deprecated(note)]`、由 `cargo fix` 应用的机器可应用建议；破坏性写法使用 edition（`cargo fix --edition`） | `rustc` 打印替代项；一条命令迁移 crate |
| Go | API 移动的 `go fix` 改写器 | 一条命令迁移 module |
| Almide | stdlib fn 的 `@deprecated(since=N, use="…")`、`proofs/dialect-epochs.toml` 中的 `[[deprecation]]`、诊断中的 `try:` 替代项及 E027 tombstone | 相同的三个组成部分 |

其他语言最终都会将旧名称变成*仍指出替代项*的错误，而非未知标识符。
决定（2026-10-17）：Almide 跳过警告窗口直接到这个终态，因为重命名只是一个 token 的机械修改，
corpus 较小，而静默窗口会让误读继续编译一个 epoch。

<a id="logical-cost-the-contract-to-state"></a>

## 逻辑成本：应陈述的契约

- **单位。** 一个 charge unit。通过带版本的校准常数 CM-1（v0.3 = 3 ns/unit，
  `crates/almide-types/src/time_units.rs`）从声明时间转换。
- **Charge site。** 严格采用 `docs/specs/als/deterministic-time.md` 中的表：
  - fn 入口，但无循环且非递归的 callee 除外（C-294）；
  - 每次 loop-head 求值；
  - bulk 字符串拼接按 `1 + len/16`；
  - 没有其他项。
  #3778 显示当时 native 与 wasm 各违反不同的行。修复应让两条 leg 都遵守表，而非修改表（下文 C）。
- **Budget 范围。**
  - `fan.bounded`：函数体。
  - `fan.min(compute…) { }`：每个 arm 分别获得完整 budget。
  - `fan.min(compute…, xs, f)`：每个元素分别计量。
  - 外围 region 取最小上限；当时 source 不能嵌套 region。
- **参数。** 对 `{ f(g(x)) }` 这样的单调用函数体，`g(x)` 在 meter 开始前执行
  （审计§6）。下文 E 将它放入 region。
- **版本管理。** 判定和胜者是（program、inputs、**meter version**）的函数。
  同一 meter version 内，它们在所有 target 和 machine 上相同。
  只有 charge 表、豁免规则或校准常数改变时，meter version 才改变。
  release notes 中随 spend ledger 差异公告变更。spend ledger 是 gate：
  每 region 消耗固定的 fixture corpus（`fuel_bounded_boundary.almd`、
  `fuel_dyn_charge.almd`，以及 acceptance test 的 `heavy(1000)` = 1002 units）；
  任一固定消耗变化都会使 CI 失败，除非同一个 PR 同时提升 meter version。

<a id="decision"></a>

## 决定

每项决定背后的规则：**名称说明选择什么；clock 的类型说明以什么计量，以及在哪个 tier 运行。**
`compute.*` 表示 pure 函数体及所有 machine 上同一答案；`duration.*` 表示 oracle tier —
允许 effect，判定是被记录的 host input。

1. **Q1（2026-10-17）：立即退役旧名，不设弃用窗口。**
   从 dialect epoch 14 开始，`fan.race` 和 `fan.any` 是指出替代项的 E027 tombstone。
   parser 报告 block head（`crates/almide-syntax/src/parser/fan.rs`、`renamed_fan_head_text`），
   checker 以相同文本报告调用写法（`static_dispatch.rs`）。
2. **A（2026-10-18）：删除 `fan.any`，不是重命名。** source 顺序中的第一个 Ok 是
   `a() ?? b() ?? fallback`；list 上是 `list.find_map(xs, (x) => f(x)?) ?? fallback`。
   两者已经存在；`??` 仅在左侧失败时求值右侧，`list.find_map` 接受 effect callback —
   2026-10-18 在 native 和 wasm 上实测。理由：其他语言（Rust `find_map`、OCaml
   `List.find_map`、Elixir `find_value`、Kotlin `firstNotNullOfOrNull`、Scala `collectFirst`）
   都将它放在 list 库中，两个候选的形式在各处都是惰性 fallback 链
   （`or_else`、`.catch(() => b())`、`<|>`）。对顺序选择提供 fan 形式，等于同一含义两种写法。
3. **修订后的 Q1（2026-10-18）：`fan.race` 改为 `fan.min(clock)`，clock 参数必填。**
   `fan.min(compute.ms(5)) { a(), b() }` 和 `fan.min(compute.ms(5), xs, f)`；
   `fan.min { … }` 和 `fan.min(xs, f)` 是 parse 错误。理由：`min` 是 `min_by_key`
   对该规则使用的词，而强制 clock 防止把 `fan.min { 3, 5 }` 读成最小值。
   临时名 `min_cost` 被否决，因为 clock 类型已经说明最小化的对象。
4. **H（2026-10-18）：接纳实时时钟** —
   `fan.min(duration.ms(n)) { … }` / `(duration…, xs, f)`：arm 一同开始，最先以 Ok 完成者获胜，
   arm 可执行 I/O，**从入口共享一个 deadline**（而非每 arm 一个）；
   在未获胜 arm 的下一个 charge site 或 I/O 边界取消它们，并在该形式返回前等待它们结束。
   未获胜方已执行的 effect 保留，stdout 被丢弃，只返回胜者的值。
   native 并行运行 arm；wasm 在具备 async 前按顺序运行
   （ADR-0024 D8；arm 0 成功时获胜是合法调度）；ω record/replay 固定胜者 index。
   理由：这是其他语言 `race` / `any` 服务的用途，取舍（每次运行胜者可能不同）
   也是各语言接受的取舍。结构化停止采用 Haskell `race`、Python `TaskGroup`、Kotlin、Swift 的方式；
   拒绝 JS 式继续放任运行，因为 Almide 没有非结构化 spawn。
   共享一个 deadline 正是 `Promise.race([a, b, timer])`、tokio `timeout(dur, select!)`、
   `asyncio.wait(tasks, timeout=…)` 和单个 Go `ctx` 的方式；
   每 arm 一个 deadline 会在顺序 wasm leg 上成倍增加墙钟时间。
   `fan.timeout(duration…)` 按相同取消规则接纳 I/O 函数体。
   这部分逆转首稿“排除 K2 和 K3”的决定：纳入 K2，继续排除 K3，因为失败 arm 被跳过，绝不选中。
   后续：#3788；在它落地之前，`Duration` clock 对 `fan.min` 是类型错误。
5. **G（2026-10-18，×）：`fan.bounded` 保持与 `fan.timeout` 不同的名字。**
   按 clock 类型区分的规则不扩展到 cut 形式。
   理由：每种 runtime 对 fuel/gas 限制和墙钟 deadline 都用不同名字 — wasmtime 的
   `set_fuel` 与 `set_epoch_deadline`、Ethereum 的“gas limit”与 timeout、tokio 的 `timeout`
   与 `timeout_at`。不同种类限制使用不同名称是其他语言的设计，“timeout”在各处都指墙钟时间。
6. **B（2026-10-18）：成本契约限定于 meter version**，如§逻辑成本所述。
   理由：Ethereum 在 hard fork（EIP-150、EIP-2929）时按 fork id 重新定价 gas；
   wasmtime 文档说明每 instruction 的 fuel 可能随版本改变。
   跨版本承诺会禁止所有 inliner 修改。
7. **C（2026-10-18，#3778）：native 和 wasm 遵循 charge 表。**
   native 停止对无循环 wrapper 入口 charge，wasm 开始对字符串拼接 charge；
   现有胜者和判定可能随 meter version 移动。
   理由：同一张表的两个实现出现分歧，是共识 bug（Ethereum client 视为最高优先级），而非容差。
8. **D（2026-10-18，#3780）：arm 到达有序进程操作的 `fan { }` 在 native 上也顺序运行。**
   操作为 `env.set`（加入 ADR-0024 D3 集合）。理由：Rust 在 edition 2024 将 `env::set_var`
   改为 `unsafe`，Go 的 `t.Setenv` 在 `t.Parallel` 下 panic — 其他语言拒绝该组合；
   Almide 通过串行化保持确定性，使 native 与 wasm 观测一致。check 阶段拒绝仍可作为未来收紧措施。
9. **E（2026-10-18）：`fan.bounded` / `fan.timeout` / `fan.min` 的单调用函数体参数位于计量 region 内。**
   理由：gas 和 wasmtime fuel 统计每条执行的 instruction；跳过参数工作的限制不是限制。
10. **F（2026-10-18）：`fan.min` 全失败文本为 `fan.min: no branch
    succeeded within budget`。** 必填 clock 意味着总有 budget，因此一种文本同时覆盖
    “所有 arm 返回 Err”与“所有 arm 被 cut”；`succeeded` 也说明前一种情形。
    （JS `Promise.any` 报告“All promises were rejected”；文本说明失败，而不是写作者未设置的 budget。）

<a id="alternatives-rejected"></a>

## 被否决的替代方案

- **A，保留两个名字。** 其他语言将 `race` 读成“最先完成”，将 `any` 读成“无序”。
  保留会使两种误读继续静默通过。（这是首稿的建议，只对 JS 权衡了改名成本，
  没有检查 `race` 和 `any` 的共同含义在各生态中覆盖多广。）
- **临时名称 `fan.first_ok` / `fan.min_cost`**（首轮修订，2026-10-17）。
  `first_ok` 被否决，因为没有其他语言把有序逐个尝试放在并发库里；选择是顺序的，
  已由 `??` 和 `list.find_map` 表达。`min_cost` 被否决，因为 clock 类型已经说明最小化对象，
  后缀冗余且封闭了接纳 `Duration` clock 的途径。
- **clock 可选的 `fan.min`。** `fan.min { 3, 5 }` 读起来是最小值；
  参数才是将 `min` 变成 arm 选择的关键。
- **`fan.min(duration…)` 的逐 arm deadline。** 其他语言没有，且顺序 leg 上总墙钟时间会乘以 arm 数。
- **按 clock 类型将 `fan.bounded` 统一到 `fan.timeout`（G）。** 按决定5否决。
- **将 K2 排除在语言之外**（首稿 Q2）。这是其他语言 race 最常见的用途；取舍是各方接受的，
  oracle tier（ω record/replay）已为 `fan.timeout` 存在。
- **JS 式放任未获胜方。** Almide 没有非结构化 spawn，而该形式返回后仍运行的未获胜方就属于这种情况。
- **跨版本冻结 meter。** 这会禁止所有带 region 的 program 发生 inliner 和 lowering 变化。

<a id="consequences"></a>

## 后果

- **收益**：
  - 每个选择名称以其他生态的词语说明选择对象，其他语言带来的习惯转化为指出正确形式的诊断。
  - 最常见的实际形态（“哪个服务器先回复”）获得支持结构化取消的写法。
  - 一个规则 — clock 类型决定 tier — 同时覆盖 `fan.min` 和 `fan.timeout`。
  - 升级编译器的用户可从 meter version 判断胜者是否可能变化。
- **代价**：
  - 一条带三个 break 的 epoch entry（14）；改写例子、CHEATSHEET、llms.txt 和 Dojo task。
  - `fan.any` runtime（九个 self-hosted `any_map` cell、chain desugar）在删除落地前是死代码。
  - spend ledger 多一道 gate；release notes 多一个 meter version 数字。
  - `fan.min(duration…)` 和 `fan.timeout` 的 I/O 函数体需要 native 取消机制
    （在 charge site 和 I/O 边界检查协作 flag）及 wasm 顺序调度。

<a id="falsifier"></a>

## 可证伪条件

可用性评估（`docs/design/fan-usability-eval.md`）比较 C1（带 D-a/D-c 的旧名称）
与 C2（已接受的表层）。如果 C2 的 F1 + F2 + F5 误读率没有比 C1 至少降低三分之一，
则改名未获得本 ADR 所称的收益，首先应重新审视诊断文本。

<a id="test-plan"></a>

## 测试计划

| 修改 | 测试 | leg | 状态 |
|---|---|---|---|
| Q1 / A：旧名报指出替代项的 E027 | `tests/diagnostics/e027-fan-race-renamed-head`、`e027-fan-any-removed`、thunk-list case；`scripts/check-dialect-epochs.sh` 看到 epoch 14 的 `breaks` | check | 随 epoch 完成 |
| A：替代形式顺序且惰性 | `spec/wasm_cross/fan_any_early_winner.almd`（`??` 链：后续候选不运行）、`fan_any_allfail.almd`（全部候选失败 → fallback）、mapper fixture 中带 effect callback 的 `list.find_map` | native, wasm | 随 epoch 完成 |
| Q1：`fan.min` 必须提供 clock | `fan.min { }` 和 `fan.min(xs, f)` 的 parse 错误；`tests/fan_surface_matrix_test.rs` | check | 随 epoch 完成 |
| F：全失败文本 | `fan.min(compute.ms(1)) { err("a"), err("b") }` → `fan.min: no branch succeeded within budget` | 4 legs | 随 epoch 完成 |
| C / #3778：meter 遵循表 | `tests/fan_acceptance_test.rs`：取消忽略 `a_loop_free_wrapper_costs_the_same_on_every_leg` 和 `a_cut_body_that_interpolates_stops_at_the_same_point_on_every_leg` | 4 legs | 完成（#3778：native 默认路径读取 `meter_plan` 并对 `+` 而非 interpolation charge；`fuel_wrapper_entry_exempt.almd`、`fuel_interp_uncharged.almd`） |
| B：meter version gate | 重新计算每个固定 spend，除非 `METER_VERSION` 同时改变否则失败的 script | CI | 后续 |
| #3779：`fan { list.map }` 的 trap 顺序 | 取消忽略 `a_trap_in_a_parallel_list_map_is_the_lowest_index_one`；运行20次 | native par / seq, wasm | 2026-10-18完成（每个分配 block 一条 timeline；`fan_list_map_trap_order.almd`、acceptance test 取消忽略；#3784 同次修改关闭） |
| D / #3780：有序操作令 fan 顺序运行 | 取消忽略 `fan_arms_that_share_process_state_agree_across_targets`；codegen test 验证有 `env.set` 的 block 生成顺序形式，无该操作时不如此 | 4 legs | 后续 |
| E：计量单调用参数 | `fan.bounded(compute.ns(30)) { id(heavy(100000)) }` 在所有 leg 为 Err；`fan.timeout` 和 `fan.min` 同样（`tests/fan_acceptance_test.rs` `a_single_call_bodys_arguments_are_inside_the_region`、`spec/wasm_cross/fuel_single_call_args.almd`） | 4 legs | 完成（#3790） |
| H：`fan.min(duration…)` | 实时首个 Ok 获胜；未获胜方 println 丢弃；其 effect（写入文件）保留；仅在未获胜方停止后返回；`ALMIDE_OMEGA` 在两个 target 重放胜者 index；wasm 取 arm 0 | native, wasm | #3788 |
| H：共享一个 deadline | 两个 arm 各需 deadline 的60%：顺序 leg 中第二个被 cut | native, wasm | #3788 |
| H：带 I/O 函数体的 `fan.timeout(duration…)` | 函数体发出 request，cut 在下个边界生效，Err 属于 deadline | native, wasm | #3788 |
| #3783（空函数体）、#3781（wasm ω replay）、#3782（Float 胜者） | 在 #3786 落地 | 4 legs | 完成 |

<a id="ratification-record"></a>

## 批准记录

| 问题 | 判定 | 日期 |
|---|---|---|
| Q1 改名，无弃用窗口 | ○ | 2026-10-17 |
| 临时名称 `first_ok` / `min_cost` | × — 由 A 和 `fan.min` 取代 | 2026-10-18 |
| ① `fan.any` 的 list 形式 → `list.find_map` | ○ | 2026-10-18 |
| ② `fan.race` → `fan.min`，clock 必填 | ○ | 2026-10-18 |
| ②b 接纳 `fan.min(duration…)` | ○ | 2026-10-18 |
| ②c 未获胜方在下一边界取消并等待其结束；允许 I/O | ○ | 2026-10-18 |
| A `fan.any` block 形式 → `??` 链 | ○ | 2026-10-18 |
| B 成本契约限定于 meter version | ○ | 2026-10-18 |
| C meter 遵循表（#3778） | ○ | 2026-10-18 |
| D `env.set` arm 在 native 顺序执行（#3780） | ○ | 2026-10-18 |
| E 计量单调用参数 | ○ | 2026-10-18 |
| F 全失败文本 | ○（`no branch succeeded within budget`） | 2026-10-18 |
| G 将 `fan.bounded` 统一到 `fan.timeout` | × | 2026-10-18 |
| H 实时时钟共享一个 deadline | ○ | 2026-10-18 |
| Q7 空函数体，Q8 wasm ω replay | ○ — 在 #3786 落地 | 2026-10-18 |

<a id="references"></a>

## 参考资料

- ADR-0001 (deterministic time units), ADR-0011, ADR-0020, ADR-0022, ADR-0024
- C-004, C-005, C-185, C-199, C-200, C-204, C-205, C-207, C-208, C-294, C-320
- Haskell `async`: [race](https://hackage.haskell.org/package/async/docs/Control-Concurrent-Async.html#v:race) · Java [Stream.findFirst / findAny](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/stream/Stream.html)
- Rayon [find_first / find_any / find_map_first](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html) · Rust [Iterator::min_by_key](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.min_by_key), [Iterator::find_map](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.find_map), [Result::or_else](https://doc.rust-lang.org/std/result/enum.Result.html#method.or_else) · OCaml [List.find_map](https://ocaml.org/manual/5.2/api/List.html) · Elixir [Enum.find_value](https://hexdocs.pm/elixir/Enum.html#find_value/3)
- 按种类区分限制： wasmtime [Store::set_fuel](https://docs.rs/wasmtime/latest/wasmtime/struct.Store.html#method.set_fuel) / [set_epoch_deadline](https://docs.rs/wasmtime/latest/wasmtime/struct.Store.html#method.set_epoch_deadline) · tokio [timeout](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) / [timeout_at](https://docs.rs/tokio/latest/tokio/time/fn.timeout_at.html) · Ethereum gas repricing [EIP-150](https://eips.ethereum.org/EIPS/eip-150), [EIP-2929](https://eips.ethereum.org/EIPS/eip-2929)
- 环境与并行： Rust [std::env::set_var (unsafe since edition 2024)](https://doc.rust-lang.org/std/env/fn.set_var.html) · Go [testing.T.Setenv](https://pkg.go.dev/testing#T.Setenv)（“不能用于并行测试”）
- 共享deadline： tokio `timeout(dur, select!{…})` · Python [asyncio.wait](https://docs.python.org/3/library/asyncio-task.html#asyncio.wait) (`timeout=`) · Go [context.WithTimeout](https://pkg.go.dev/context#WithTimeout)
- 重命名： Swift [`@available(renamed:)`](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/attributes/#available) · Kotlin [`@Deprecated` / `ReplaceWith`](https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-deprecated/) · Rust [`cargo fix`](https://doc.rust-lang.org/cargo/commands/cargo-fix.html)
- MDN [Promise.any](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/any) / [Promise.race](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/race)
