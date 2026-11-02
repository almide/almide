原文 description 元数据：Fan v2 设计和迁移历史；当前选择表面遵循 ADR-0028 与 SPEC
<a id="fan-v2--execution-policy-design-and-migration-record"></a>

# Fan v2 — 执行策略设计与迁移记录

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-v2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-v2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](fan-v2.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md)

> 原文注记：六 head 矩阵、race 可选预算、thunk 迁移、Wave 待办、MSR 理由是历史设计，不是第二套当前语法或测得存活率保证。开头 ADR-0028 导航优先于旧 any/race 名及可选时钟规则：当前 min 必须 Compute，Duration min 仍拒绝/待做，timeout 已存在，T1 有完成记录。Compute 确定性限定 meter 版本，不是任意编译器版本不变；host-relative timeout 不保证跨 host 触发决定一致。旧 pure/effect、Result 条件、trap window、min-cap 均保留范围。record 新增与待做 edition 迁移分开。保留旧例、计数，未执行 compiler/test/benchmark。

<a id="current-navigation"></a>

## 当前导航

权威表面为 [SPEC §13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#13-concurrency-fan)，后于本设计的 [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) 确定选择名称和时钟：

- `fan.any` 已移除；使用 `??` / `list.find_map`。
- `fan.race` 更名为 `fan.min`；当前调用必须带 Compute 时钟。Duration min 是 ADR-0028 设计决定，待 #3788，当前 checker 拒绝。Compute 属确定性层；逻辑成本限定于 meter 版本，不在任意编译器版本间保持不变。
- `fan.timeout(duration.ms(n)) { body }` 已存在，并非待实现墓碑。
- 下述8月 T1-1/T1-2 项已在 [BURNDOWN.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/charge-probe/BURNDOWN.md) 完成。
- 当前接受/拒绝形态由 `tests/fan_surface_matrix_test.rs` 覆盖；下方历史六 head 矩阵不是第二份当前语法规格。

<a id="implementation-records"></a>

## 实现记录

> **实现状态（worktree-stage1-charge-probe，2026-08-02）**：表面已实现 — 全部 block head（fan{} / any / settle / race / bounded，settle 为 tuple 契约）、mapper 形 any/settle、budgets（compute.* 构造器 + S3 代数 + min-cap）、Result arm 跳过 Err、裸形 Result（含 native Res carrier）、完整 block body/arm。fixtures = spec/wasm_cross/fuel_* / time_* / fan_settle_tuple；契约 = C-202..C-207。未完：strict cap + unwind（T1-1）、metered-clone（T1-2）— research/spike/charge-probe/BURNDOWN.md 为准。若与本文表格不符，以 BURNDOWN/契约账本为准。
>
> **#1388（2026-10-18）**：实现无标记 head 的 record 形 `fan { a: e1, b: e2 }` — 值为匿名记录，arm 经与 tuple 形相同的 Fan 节点执行（lowering 为 tuple 形 + 解构 + record literal），契约 C-386（serial elision 和 source order 首个 Err）。无 `!` 的 Result 字段为 E041（为 settle 集成赋予“保留为 Result”解释留余地）。未完：退役 `fan.settle { }` tuple head 及从 tuple 形迁移（edition 边界）。

> 原始章程：[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md)。下文保留引入时的设计理由、迁移计划，不覆盖当前语法/契约。
>
> 在 [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md)（语义）上，将 fan 的**整个表面**纳入一套语法的设计。[concurrency-stance.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md) 的决定（确定性数据并行）是前提，不改变。

<a id="導入時の統一テーゼ"></a>

## 引入时的统一论点

**fan 是“执行策略”的语法。** 全部 fan 语法是以下的1个单元：

    head（ポリシー） × form（block = 静的 arm 列 | mapper = 動的リスト）

上式意为 head（策略）× form（block = 静态 arm 序列 | mapper = 动态列表）。

除此之外没有其他形态。从表面**彻底去掉** thunk（`() => …`）。

v2 前 fan 表面混用3种调用约定：block 为表达式序列，`fan.map` 为 (list, mapper)，`fan.any`/`fan.settle` 为 thunk 列表。各 combinator 的 auto-wrap 契约也不同（“race/any/settle thunk 自动包装，map mapper 不包装”）。v2 将其替换为**一张 head × form 笛卡尔积**。

<a id="導入時の-v2-マトリクス履歴--現行文法ではない"></a>

## 引入时 v2 矩阵（历史，不是当前语法）

| head | 选择规则 | block form（静态 arm） | mapper form（动态） | arm 效果上限 | 并行化 |
|---|---|---|---|---|---|
| （无标记）all | 全部，列表顺序首个 Err | `fan { a; b }` → `(A, B)` | `fan.map(xs, f)` → `List[B]` | 允许 effect | 可（观测按列表顺序） |
| settle | 不选择，全收集 | `fan.settle { a; b }` → `(Result[A], Result[B])` | `fan.settle(xs, f)` → `List[Result[B]]` | 允许 effect | 可 |
| any | **index 最小**的成功 | `fan.any { a; b }` → `T` | `fan.any(xs, f)` → `T` | 允许 effect | 串行（语义本身串行） |
| race | **(spend, index) 最小**的成功 | `fan.race { a; b }` → `T`（`fan.race(compute.ms(500)) { … }` 为可选发散 guard） | `fan.race(xs, f)` / `fan.race(compute.ms(500), xs, f)` → `T`（**已实现 2026-08-03，T7-1** — f 为 pure 1参数 lambda，必须返回 `Result`，budget 逐元素，同分按列表顺序） | **pure**（Rung 0） | 可（带剪枝） |
| bounded | 计量单个 body | `fan.bounded(compute.ms(100)) { body }` → `T` | —（与 map 组合） | **pure** | — |
| timeout | 环境切断（壁钟 — oracle 层，[ADR-0001 S8](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)） | `fan.timeout(duration.s(5)) { body }` → `T` | — | 允许 oracle | 相对于 host |

- block form 的 arm 是**表达式**（与 `fan {}` 相同 arm 契约：Result auto-unwrap，禁止捕获外层 `var`）。any/race arm 统一为同一 `T`；all/settle 为 tuple，可以异型。
- mapper form 的 mapper **返回 Result**（统一为 `fan.map` 现有契约，把引导 pure mapper 去 `list.map` 的 #547 诊断扩展到全部 head）。
- bounded / timeout 取**单个 body**（普通块，可写 `let`），不是 arm 序列。在语法表明确“N-arm head”与“body head”两类。
- 空 arm 序列、空 body 是编译错误（继承旧 race 空列表规则）。

**auto-wrap 不统一改为按 form 的规则**：block arm 总是 `fan {}` 式（auto-unwrap），mapper 总是 `fan.map` 式（Result 必须）。head 个别例外归零。

<a id="thunk-全廃が買うもの"></a>

## 全面去掉 thunk 带来什么

<a id="1-表面から---が消える"></a>

### 1. `() =>` 从表面消失

```almide
// v2 導入前の v1
fan.any([() => http_fetch(a), () => http_fetch(b)])
fan.any(mirrors |> list.map((m) => () => fetch(m)))

// v2
fan.any { http_fetch(a); http_fetch(b) }
fan.any(mirrors, (m) => fetch(m))
```

保留示例的注释区分 v2 引入前的 v1 与 v2。

fan-concurrency.md §3.4“为什么需要 thunk”原解释为 fan.* **是函数**，所以延迟需要 thunk。v2 全 head 为语法，整节就从语言中删除。动态形从“构建 thunk 列表再传入”两阶段，变为“列表 + mapper”一阶段，代码严格更短。

<a id="2-wasm-の-wall-クラスが-1-つ死ぬ"></a>

### 2. 消灭一个 wasm wall 类别

当时动态 thunk 列表（非 literal）在 v1 无法表示 `List[funcref]`；`desugar_fan.rs` 只 inline literal 和 let 绑定 literal，真正动态列表落入 purity wall。mapper form 是“数据列表 + 一个闭包”，与 `fan.map` 同形，所以**仅改变语法就能消灭该 wall 类别**，无语义牺牲。

<a id="3-予算引数がラベルを必要としなくなる"></a>

### 3. budget 参数不再需要标签

语言**没有**标签参数语法（logical-time-async.md 初稿误以为已有 `limit:` 先例，已纠正）。原设计利用 fan head 为语法，将 `ticks:` / `ms:` 作为 fan 自身语法元素提供；但 **[ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md) 使标签本身也不再必要**：预算用带单位构造函数（`compute.ms(100)` / `duration.s(5)`），由类型承载区别。无需给 parser 添加 head-args 标签解析，v2 表面进一步缩小。

```almide
fan.bounded(compute.ms(100)) { body }   // 決定的時計（Compute 型）
fan.timeout(duration.s(5))   { body }   // 壁時計（Duration 型）
fan.bounded(compute.ms(n))   { body }   // 変数もそのまま
fan.bounded(100)                        // 型エラー: expected Compute, found Int
```

保留示例注释的全文说明：确定性时钟（Compute 类型）；壁钟（Duration 类型）；变量也可直接使用；类型错误：expected Compute, found Int（期望 Compute，得到 Int）。

单位名称集合为 `ns`/`us`/`ms`/`s`/`min`/`h`，作为完备性矩阵纳入 gate。**不添加新 literal 语法**：`100ms` 一类数字后缀需扩展 lexer，仅为 fan 表面扩大词法规则不划算（未来若添加，其脱糖目标就是此函数形）。

<a id="導入時に整理した各-head-の意味論後続変更は上の-current-navigation-参照"></a>

## 引入时整理的各 head 语义（后续变化见上方当前导航）

- **all / settle**：维持现状，C-004 / C-199 / C-200 连同 statement 保留。arm 输出缓冲（列表顺序 flush，退役 C-004 EXCEPTION）是与 logical-time-async.md Rung 1 共享部件的独立工作流。
- **any**：把现有脱糖（match 链）已定义的**串行 fallback**提升为正式语义：“按列表顺序尝试，首个 Ok 就**停止**。先前候选效果发生，后续候选效果不发生。”any 不是并发语法，而是 **fallback 语法**，须在规格中明确（仅 pure arm 可投机执行，作为不可观察优化）。全失败时 Err 为账本常量（不变）。
- **race / bounded**：语义采用 logical-time-async.md 全部条款（lockstep ≡ least-spend、per-branch 预算、trap 可见窗口、EVM 式 min-cap、CM-1）。v2 将表面确定为 block form。**bounded + `??` 成为主要习惯用法**：

  ```almide
  let plan = fan.bounded(compute.ms(100)) { optimal_plan(g) } ?? greedy_plan(g)

  let ans = fan.race(compute.s(1)) {
    exact_solve(input)
    heuristic_solve(input)
  } ?? default_answer
  ```

- **timeout（原分阶段计划）**：曾暂缓至 Stage 4（Path C / R_Ω）。此处只确定重新引入形态：`fan.timeout(duration.s(5)) { body }`，中断点为 charge site（logical-time-async 统一中断点原则），契约类为 oracle-relative。

<a id="導入時の完備性規則matrix-gate-の設計"></a>

## 引入时完备性规则（matrix gate 设计）

1. 全部 head 具有 block form。
2. 只有“可针对同型动态数据量产 arm 的 head”（all / settle / any / race）有 mapper form。bounded / timeout 是单 body head，故无 mapper，这是刻意省略。
3. bounded **必须**有预算参数（bound 就是存在理由），race **可选**（选择无需预算，预算只作发散 guard；[ticks-interface-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md)）。timeout 接受壁钟 duration。无标记 fan / map / any / settle 无预算参数，因为可组合 `fan.bounded` 达到预算维度（原样采用 logical-time-async 完备性规则）。
4. 添加新 head 时，PR 接受条件是给表添行，填满所有列（选择规则、两种 form 类型、效果上限、可并行性），即 API 族矩阵原则。

<a id="当初の移行--何が壊れてどれだけか"></a>

## 原迁移 — 破坏什么、多少

**不变**：`fan { }` 和 `fan.map(xs, f)`，最常用2个表面连字符都不变。契约（C-004/C-005/C-199/C-200）连同 statement 保留。

**破坏性**：`fan.any(thunks)` / `fan.settle(thunks)` 的 thunk 列表形。改为 E0xx 迁移墓碑（check 时，附重写例）。不共存：同一意义两种拼写，是本 repo 反复偿付的债务。

影响范围已测量：repo 内使用为 **spec 8个文件约~60行**（stdlib 1条注释，CHEATSHEET 0条）。逐个阅读 fixture 固定什么再迁移；如撤去 `fan.race`（#1024）所教，机械替换会造出“通过却断言错误内容的测试”。C-005 系 statement 更新为新形式拼写（行为相同，契约主张本身不变）。

<a id="却下したものv2-の輪郭は否定側で確定する"></a>

## 拒绝的选项（从否定侧确定 v2 轮廓）

| 候选 | 判定 | 理由 |
|---|---|---|
| `fan.rush`（返回最快者，败者继续运行） | **永久拒绝** | 选定后败者效果继续泄漏的语法无法在确定性模型中赋义，是“无法赋予列表顺序含义的东西不进入语言”的实例 |
| `fan.spawn` / `fan.link` | 放到未来远景 | 非结构化/通道要待 logical-time-async“未来远景”（确定性并发）基础就绪；不进 v2 |
| 通用标签参数 | 不引入 | 属语言整体调用约定问题，不借 fan 夹带。ADR-0001 使预算连标签都不再需要，此问题也消失 |
| trailing lambda（`fan.map(xs) { x => … }`） | 不引入 | 只变成2种拼写，不增加意义 |
| `fan.map(xs, limit: n, f)` | 不纳入 | 从未实现（fan-concurrency-next 的 ✅ 过时，checker 固定2参数）。不改变观测的调度 hint 待需求获证明，再通过其他机制（如 pragma）提供 |
| `fan.all { }`（无标记别名） | 不引入 | 最频繁形态应最短，别名是共存债务 |

<a id="msr--v2-が生存率に効く形"></a>

## MSR — v2 如何帮助存活率

- **决策树更浅**：“静态列举（block）还是按数据量产（mapper）”×“全要（无标记/settle）、首成功（any）、最便宜成功（race）、仅上限（bounded）”。LLM 只选这2 × 4项，全单元类型可从表读出。
- **thunk 消失**使“忘写/多写 `() =>`”编辑破坏类归零。
- **arm 契约统一**（block auto-unwrap，mapper 必须 Result），让替换 head（any → race 等）的编辑不需重学类型契约也能存活。
- 每次运行相同保持不变：v2 没有任何非确定性单元（timeout 置于 oracle 层，按契约类别分离）。

<a id="当初の実装-wave現在の未完了一覧ではない"></a>

## 原实施 Wave（不是当前未完成清单）

1. **Wave 1 — 表面统一（无需 fuel，现在即可）**：any/settle 的 block + mapper form、thunk 列表形 E0xx 墓碑、迁移 spec 8文件、重写 SPEC.md §13、CHEATSHEET、formatter、interp bridge、新建 matrix gate。parser 在 `fan.IDENT` 后添加 `(head-args)` 和 `{`/`(` 分支。同时**从 AST/IR 删除 async/await 遗骸**：lexer 已无关键字，parser 的 `async_` 固定 false（2026-08-01 确认），但仍有 `ExprKind::Await` / `IrExprKind::Await` / `Decl::Fn` 的 `r#async` 字段，以及 fmt/interp/optimize 空壳 arm。把“语法无法写出”提升为“无法表示”。
2. **Wave 2 — 完成确定层**：以 v2 形式实现 `fan.bounded`（logical-time Stage 2）→ `fan.race`（Stage 3），E027 修订也在此。
3. **Wave 3 — oracle 层**：`fan.timeout(duration.s(5)) { }`（Stage 4）。

Wave 1 可独立启动是 v2 要点之一：表面统一不等 fuel 语义实现。

<a id="反証条件"></a>

## 反证条件

- parser 的 `fan.IDENT ( … ) { … }` 分支与既有表达式语法冲突（member call + 后续 block 歧义）。→ 只要 fan 是保留字、从 primary 专门解析，应不发生，但须在 Wave 1 开始用 fixture 确定。
- any/settle 迁移出现 pin 被破坏的 fixture。→ 按 #1024 步骤逐文件读 pin。8个文件是半天工作量。
- race mapper form 没有真实用途。→ 单元仅声明，Wave 2 只实现 block，matrix gate 明示“已声明、未实现”。（2026-08-03 生效：`tests/fan_surface_matrix_test.rs` 机械固定6 head × 2 form 全单元。**同日按用户指示以实现关闭该单元（T7-1）**，gate 已把 race mapper 移至 implemented。bounded / timeout mapper 仍按设计缺席。改单元须在同一 PR 更新 gate。）
- dojo 测量发现“block 还是 mapper”二选一不利于 LLM。→ 给 MSR task bank 加 fan v2 集合测量。设计对错由测量决定。
