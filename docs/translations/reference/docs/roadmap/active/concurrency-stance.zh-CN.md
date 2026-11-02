原文 description 元数据：并发模型决策及其确定的 fan 家族修复
<a id="concurrency-stance-deterministic-data-parallelism"></a>

# 并发立场：确定性数据并行

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](concurrency-stance.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](concurrency-stance.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](concurrency-stance.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md)

> 原文注记：这是0.44决策及2026年7～8月调查记录，不是重新验证的普遍并发保证。开头严格列表顺序观测等价原则与后文承认的 stdout/stderr 交错例外并存。旧缓冲方案对测得的 trap 情况并非必要，打印兄弟的工作留在本 Unit 外，但末尾实施顺序仍列该方案。旧 race 不可能说受8月1日确定性 fuel 修订限定。wasm32 线程断言属于历史 target 语境，不是所有 WebAssembly 配置的普遍陈述。数量、时间、诊断、关闭主张保留原文测量范围；文件/网络效果在账本观测外。未独立运行实现、fixture、benchmark 或契约。

> 对 issue [#1000](https://github.com/almide/almide/issues/1000) 的回答。阶梯行 0.44。
> 这一决策推导出全部4项未解决 `fan` 问题（#1023 / #1024 / #1025 / #1026）的正确答案。

<a id="決定"></a>

## 决策

**Almide 的并发模型是“确定性数据并行”。**

`fan` 是**调度语法，而非语义语法**。执行可以并行，但程序的可观察行为（stdout / stderr / 退出码 / 返回值）定义为**与按列表顺序串行求值严格相同**。

无法赋予列表顺序含义的东西，不进入语言。

不采用 structured concurrency（goroutine / channel 对应机制）。

<a id="なぜこれ以外にないのか"></a>

## 为什么没有其他选择

决定性因素并非选了新方针，而是**决定舍弃两个已有却不相容的承诺中的哪一个**。

`docs/SPEC.md` 对同一功能说了相反的话。

- §13.1“native 使用真实线程，wasm 按声明顺序串行 — **无论哪种方式结果都相同**”
- §13.2“`fan.race(thunks)` — **先完成者胜出**”

“无论哪种执行结果都相同”和“快者胜出”不能同时成立。前者要求独立于时序，后者把时序本身作为结果。

而前者不能舍弃。native ⇄ wasm 字节一致正是契约账本存在的理由，`docs/contracts/contracts.toml` 的196项契约都建立在此等价性上。wasm32 没有线程，因此时序依赖语法在 wasm 侧**原则上无法实现**。`fan.timeout` 在0.29.0被撤去也是同一原因 — 它是“唯一不是程序和输入的函数的 stdlib 表面”。

此外，这门语言的使命是成为 LLM 最能准确编写的语言，即 modification survival rate。同一输入每次运行输出都变的代码，会破坏该测量本身。**对 Almide，确定性不是性能特征，而是正确性定义的一部分。**

所以舍弃的是“快者胜出”。

<a id="4-件への帰結"></a>

## 对4项问题的影响

<a id="1024-fanrace--撤去済2026-07-31develop-にマージ"></a>

### #1024 `fan.race` — **已撤去（2026-07-31，合并至 develop）**

已完成：checker E027 墓碑；从 C-004 标题及 statement 删除 `fan.race` 条款；从 C-005 删除 head-Err 条款和 `spec/wasm_cross/fan_race_err.almd`；更新 SPEC.md §9.8/§13.2、README、DESIGN；添加诊断 fixture `e027-fan-race-removed`；迁移6个调用点。契约 gate 绿（198项契约 / flagged 0），spec 324 + examples 11个文件绿。

**迁移不能靠机械替换。** `fan.race(ts)` ≡ `ts[0]()`，而 `fan.any` 在 thunk[0] 失败时行为不同（any 寻找下一个 Ok），所以机械替换 head 为 Err 的情况，会变成**通过但断言错误内容的测试**。迁移时逐一阅读文件固定了什么：

- `fan_race_test.almd` — race 专用，删除（由 E027 fixture 替代）
- `fan_race_any_wasm.almd` → `fan_any_wasm.almd` — 删除 race 情况，保留 any
- `fan_value_regression_test.almd` — “Result 绑定通过 `??`/`==`”改为 any；“2个捕获 thunk 共享1个列表（E0308）”的**核心就是两个 thunk 的形状**，所以改为 `fan.settle`（不与下方 any 测试重复）
- `fan_pure_thunks` / `fan_var_thunk_list` / `fan_deterministic` — 删除 race 行，或改用走相同 lowering 路径的 `fan.any`

还发现了**墓碑连锁**：`e027-fan-timeout-removed/fixed.almd` 在0.29.0将 `fan.race` 指为 `fan.timeout` 的迁移目标，本次撤去使其不再编译。**墓碑迁移目标必须是仍存活的表面** — 指向死表面的 hint 会在下次撤去时悄然坏掉。

<a id="1024元の分析"></a>

### #1024（原始分析）

实现已经按列表顺序正确运行。C-004 也正确。只有 SPEC.md 在说谎。

但不能止于“修文档”。阅读 IR 可见，`desugar_fan.rs::rewrite_race_head` 把 `fan.race([t0, t1, …])` **直接替换为 `t0`**。其他 thunk 甚至不会执行。也就是说，`fan.race(ts)` 是与 `ts[0]()` **完全等价的空包装**。

确定性模型下，无法给 `race` 这个名称赋予含义；即使能，也只会重复已有表面。
（**2026-08-01 修订**：**没有度量时**此句仍成立。当 fuel 提供可跨分支比较的确定性度量时，可以定义观测与列表顺序串行求值严格相同的 race。设计见 [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md)。本立场“无法赋予列表顺序含义的东西，不进入语言”的原则在其中仍保持。）与 `fan.timeout` 同等处理 — **E027 系 check 时墓碑 + 迁移 hint**（直接调用 `ts[0]()`，或想跳过失败则用 `fan.any`）。不让两者共存。

**实施内容**：将 checker 的 `race` arm 替换为 E027 墓碑。从 C-004 标题和 statement 删除 `fan.race` 条款（`fan.any`/`map`/`settle` 不变）；从 C-005 删除 head-Err 条款和 `spec/wasm_cross/fan_race_err.almd`；更新 SPEC.md §9.8/§13.2、README、DESIGN；添加诊断 fixture `e027-fan-race-removed`。契约 gate 绿（198项契约 / flagged 0）。

撤去时发现1处**墓碑连锁**：`e027-fan-timeout-removed` 的 `fixed.almd`（0.29.0作为 `fan.timeout` 迁移目标而编写）使用 `fan.race`，本次撤去使其不能编译。**墓碑迁移目标必须是仍存活的表面** — 指向死表面的 hint 在下次撤去时会悄然坏掉。已更新为 `fan.any`，也从 `fan.timeout` 的 hint 文本中删除 `fan.race`。

保留 `fan.any`（列表顺序第一个 Ok）。它具有跳过失败这一不重复的含义。

<a id="1023-err-は兄弟をキャンセルしない--完了2026-07-31"></a>

### #1023 Err 不取消兄弟 — **完成（2026-07-31）**

删除 SPEC.md §9.9 / §13.1 的“兄弟会被取消”描述。

确定性模型下**不得实现取消**。被取消兄弟的副作用执行到哪里取决于时序，会破坏确定性。`fan` 始终 join 全部兄弟，块的值取**列表顺序第一个 `Err`**。

“有不停机兄弟就永久挂起”在此立场下**是规格，不是 bug**。串行求值不终止的程序，并行求值也不终止。要明确写出。

**实施内容**：从 SPEC.md §9.8 / §13.1 / §13.3 删除取消描述，明确 join-all 和列表顺序 Err。实现从一开始就正确 — 用两个兄弟都失败的程序测量，native 和 wasm 都报告 `Error: fail:first`（列表顺序首项）。特意使“先失败者”与“列表顺序首项”不同，若按 wall-clock 规则就会给出另一个答案。用 **C-199** + fixture `spec/wasm_cross/fan_block_err_list_order.almd` 固定（199项契约 / flagged 0）。

<a id="1026-trap-した兄弟がプロセスを落とす--完了2026-07-31ただし設計は測定に否定された"></a>

### #1026 trap 的兄弟使进程退出 — **完成（2026-07-31），但设计被测量否定**

这是真正的漏洞。C-004 只固定结果顺序，statement 内含**副作用交错在 native 按 wall-clock、在 wasm 按串行**的例外。账本棘轮不计这种“statement 内例外”，因此等价性主张留下未计数的裂口。

向理想方向靠拢。**缓冲 native 各 arm 的 stdout / stderr，在 fan 决出结果后按列表顺序 flush**。wasm 串行执行已是列表顺序，这是让 native 对齐 wasm 的改动。结果：

- C-004 的 EXCEPTION 条款可以退役，使账本等价性主张更强一层
- 能定义 trap 含义 — arm k trap 时 flush arm 0..k-1 输出，k 及之后不 flush。两个 target 相同
- 可以为此收敛行为编写新的 `fan` × trap 契约

代价是流式性（长期运行 arm 的输出直到结束才可见）。确定性优先。账本观测范围是 stdout / stderr / exit code，文件写入及网络不在主张内 — 这是结构边界，不是隐藏例外。

**测量后的纠正（2026-07-31）**：上面的缓冲设计建立在“trap 情况发生偏离”的**假设**上。实际测量发现 **trap 情况已经收敛** — 即使放一个睡眠1.5秒的兄弟，两个 target 都在约~0秒中断，stdout 为空，stderr 为 `Error: division by zero`，exit 1，一致。也就是说 #1026 只需要契约，不需要实现。用 **C-200** + fixture `spec/wasm_cross/fan_sibling_trap.almd` 固定。

缓冲仍是**另一个问题**的答案：C-004 EXCEPTION 条款处理的“两个兄弟都打印”情况下，native 按 wall-clock 交错而 wasm 串行。这与 trap 独立，不属本 Unit 范围。幸好没有凭假设开始大型实现 — 先测后定是正确的。

<a id="1025-兄弟が同じ可変コレクションを-alias-できる--1027-の修正で閉じた"></a>

### #1025 兄弟能 alias 同一可变集合 — **由 #1027 修复关闭**

唯独这一项无论并发模型如何都坏了。frontend 接受，native 因 rustc E0382 而 ICE，wasm 静默丢弃改动。E008 正确拒绝 `var` 捕获，但**经调用参数产生的同一危险完全未检查**。

确定性模型无法定义两个 arm 触碰同一可变绑定的含义（若赋予列表顺序含义就等于串行执行，并行没有意义）。

**2026-07-31 调查发现根因比 fan 深一层。** #1025 虽作为 fan alias 问题报告，fan 却连必要条件都不是：

```almide
effect fn pusher(xs: List[Int], n: Int) -> Result[Int, String] = { xs.push(n); ok(n) }
let shared: List[Int] = []
let a = pusher(shared, 1)!
let b = pusher(shared, 2)!
// native: len=2 / wasm: len=0
```

保留示例的注释报告 native 的 len=2、wasm 的 len=0。

UFCS 形式 `xs.push(n)` 绕过 mut 参数检查（相同函数体若写为 `list.push(xs, n)`，会正确以 E007 拒绝）。原因是 `check/calls_ufcs.rs` 给 `validate_mut_args` 传入不含 receiver 的参数列表 — UFCS 脱糖后 receiver 是参数0，而 `list.push(mut xs, x)` 的 mut 参数恰好是 index 0。

这不是 wall 或 trap，而是 **checker 接受的程序发生跨 target 输出偏离＝错误字节**，正属契约账本存在理由的那一类。**先修 [#1027](https://github.com/almide/almide/issues/1027)，再处理 #1025**。**2026-07-31 已修复** — 在 `check_call_target_builtin_ufcs` 知道解析键的位置，以 receiver 为参数0调用 `validate_ufcs_mut_args`。上述 repro 在两个 target 都变为 E007，合法 `mut` receiver 继续工作。324个 spec 文件绿 + 诊断 fixture `e007-ufcs-mut-receiver`。

并且 **#1025 仅凭 #1027 修复就关闭了**。通向 fan 兄弟 alias 的路径只有两条，且都已封堵：

- 参数没有 `mut` 声明却用 UFCS 破坏性修改 → **E007**（#1027 修复）
- 参数有 `mut` 声明 → 调用方必须为 `var` 绑定 → fan 内 `var` 捕获为 **E008**（原本已存在）

原样运行 issue repro，两 target 都为 E007；`mut` 声明版两 target 都为 E008。不需要 fan 专用新规则 — 只需所有语法同等强制 mut 性，未做到这一点是唯一漏洞。立场（确定性数据并行）与结论一致：两个 arm 触碰同一可变绑定的程序本来就不能写出。强制 mut 性后，#1025 归约为“两个 arm 传同一个 `mut` 参数”，仅凭声明就能判定。

<a id="実装順序044-unit"></a>

## 实施顺序（0.44 Unit）

1. **#1027 — UFCS mut 参数漏检（错误字节，最高优先）**
2. #1025 — fan 兄弟间 alias（#1027 后仅凭声明可判定）
3. #1024 — `fan.race` 墓碑 + 修正 SPEC.md
4. #1023 — 从 SPEC.md 删除取消描述，以 fixture 固定列表顺序 Err
5. #1026 — 缓冲 arm 输出 + 按列表顺序 flush，退役 C-004 EXCEPTION，新建 trap 契约
6. 若能收拢 interp 的 `fan.race` / `fan.any` abstain 则收拢（扩展第三审级）

<a id="この決定が閉じないもの"></a>

## 此决策没有关闭什么

未来若需 structured concurrency，**可重审**此决策。但前提是先按每种语法在 contract 中确定 wasm 侧答案（串行 fallback、wall 或带 flag 的 divergence）。此决策是说现在无理由支付该成本，不是说“不可能”。
