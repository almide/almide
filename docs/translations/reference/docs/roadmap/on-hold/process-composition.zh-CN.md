> 原文描述：将命令管线构建为值，运行它是一个显式步骤；搁置至出现真实需求
<a id="process-composition-a-pipeline-is-a-value-running-it-is-a-separate-step"></a>

# 进程组合：管线是值，运行是独立步骤

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](process-composition.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](process-composition.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](process-composition.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/process-composition.md)
> 这是搁置提案，不授权启动或执行。现有 ProcessCommand API 与未实现管线语法、拟议内存／effect 性质明确区分。需求、静态检查、must-use、终端形式及 fan 关系按原文保留未决。原文绝对化设计论证与先例十年主张仅予保留，此处未独立证实。


> 从 [shell-scripting-surface.md](../../../../../roadmap/active/shell-scripting-surface.md) 的 Phase 4（惰性管线组合）独立出来。**没有推翻其搁置决定** — 本项只为设计提供存放位置，并记下启动条件。
>
> 设计参考：`../almide-references/RESEARCH-shell-composition.md`
> （阅读 plumbum / sh / xonsh 的记录，clone 也在同一目录）。

<a id="現行-api-との境界-2026-10-31"></a>

## 与当前 API 的边界（2026-10-31）

现有 API 通过 `process.output` / `process.start` / `process.run` 执行 `ProcessCommand` 配置值。stdin 传入记录的 `stdin` 字段。当前定义见 [`stdlib/process.almd`](../../../../../../stdlib/process.almd)。以下 `process.cmd` / `pipe_to` / `capture` 是未实现的组合 API 提案；将单个进程表示为配置值的机制本身已经存在。

<a id="状態-保留"></a>

## 状态：搁置

原样引用父项搁置理由：

> 移植本仓库门禁时一次也未需要（Unit 0.46 记录），所以本项也等出现需求再开始。出现一个处理大型中间输出的实际案例时再启动。

**已有先行实现并非需求证据。** 一个库运行十年的事实，只能帮助加快设计，不能帮助提前开始。

<a id="起こす条件"></a>

## 启动条件

观察到下列任意一项后移至 active。

1. **出现一个处理大型中间输出的实例。** 当前 `ProcessCommand.stdin` 传递整个字符串，中间输出驻留内存。哪怕出现一个传输日志或大型转储的门禁，就已满足条件。
2. **出现需要类似 `head` 提前截断的管线。** 下游先结束时若需要停止上游，没有惰性组合就无法表达。
3. **出现需在两处运行同一管线的需求。** 不是值就无法复用。

<a id="何を作るのか"></a>

## 构建什么

将命令变成值，组合后在最后一步运行。

```almide
// 構築 — 純粋。プロセスは起きない
let chain = process.cmd("grep", ["-v", "\\.py"])
  |> process.pipe_to(process.cmd("wc", ["-l"]))

// 実行 — ここだけが effect
let n = chain |> process.capture()!
```

没有用户定义的运算符重载（CHEATSHEET：“No macros, no operator overloading, no implicit conversions”），所以用具名函数组合。`|>` 是函数应用，可原样使用。

<a id="なぜ値にするが-almide-に効くのか"></a>

## 为何“变成值”对 Almide 有用

三点，都不是外观问题。

**1. effect 边界与组合形状一致。** `process.cmd` 和 `process.pipe_to` 是 `fn`，只有终端的 `capture` / `status` / `spawn` 成为 `effect fn`。目前手动把 `process.output` 的结果传给下一个 `ProcessCommand.stdin` 时，中间每步都有 effect，组合途中散布 `!`。变成值后，**effect 集中在一点**。

**2. 可以检查管线。** 作为值，可放入 `let`、比较，并在测试中说明“这个输入构建这棵树”。不启动进程即可测试管线正确性，对门禁移植者有实际价值。

**3. 中间输出不驻留内存。** 实际连线使用 OS 管道，因此不将上游输出保存为字符串。预计这会成为解除搁置的首要理由。

<a id="先行例から取るもの"></a>

## 从先例借鉴什么

`plumbum`（tomerfiliba/plumbum，`plumbum/commands/base.py` 第 725 行）运行同样设计已有十年。借鉴两点。

**分离构建与执行。** `__or__` / `__gt__` / `__getitem__` 仅建立节点，只有 `__call__` 调用 `run()`。副作用入口只有一个。

**不丢失 SIGPIPE 的连线。** `Pipeline.popen` 把上游 stdout 连接到下游 stdin 后，**显式关闭父进程侧的副本**：

```python
srcproc.stdout.close()   # allow p1 to receive a SIGPIPE if p2 exits
```

忘记这一步，即使下游死亡，上游仍会永远运行。Almide 已有 SIGPIPE 处理（`MAIN_SIGPIPE_PRELUDE`），预计可接入，但应记录为**实现时必须处理的陷阱**。通过 `wait()` 同时等待上游的清理也一样。

<a id="採らないもの"></a>

## 不采用什么

- **模式推断**（`xonsh`）— 一行若是有效语言则按语言解释，否则当命令。写错时不是语法错误，而是**变成另一个有效含义**，因此原则上无法诊断。父项已从战略上否决，机制层面也有独立理由。
- **属性分派**（`sh`）— 如 `sh.git.log(...)` 一样动态解析命令名。拼写错误在运行时才失败，直接丢掉诊断带来的收益。

<a id="未解決の設計論点"></a>

## 未决设计问题

开始前需要决定。

1. **是否静态检查命令名。** 目前无人能阻止 `process.cmd("gre", ...)`。在 check 时解析 `PATH` 会引入环境依赖，可能应否决，但必须**明确选择“不做任何事”**。
2. **E042（must-use）是否应作用于 `Pipeline` 值。** 构建为纯操作后，可能构建 chain 却**忘记运行**。类型没有 effect，当前 E042 无法捕捉。
3. **提供多少终端操作。** 当前主要执行形式为 `output` / `run` / `start`（`exec` 为简便形式）。是否需要一边输出到终端一边捕获（相当于 plumbum 的 `run_tee`），以 [zero-committed-shell.md](../../../../../roadmap/active/zero-committed-shell.md) 的门禁移植作为实测材料。
4. **与 `fan` 的关系。** 管线是顺序组合，`fan` 为并行。`A | B` 是“B 等待 A 的输出”，而 `fan` 是独立分支。两者都基于 `|>` 时，能否清楚区分？

<a id="done-criteria"></a>

## 完成标准

（开始后的完成条件，搁置期间不评估。）

- 管线可构建为值，构建为 `fn`，只有终端为 `effect fn`。
- 中间输出不驻留内存（用大型输出实测）。
- 下游提前退出传递至上游（用类似 `head` 的操作实测）。
- 使用以上功能的实际规模示例，附 ```` ```almide check ````，列在 [CHEATSHEET.md](../../../../../CHEATSHEET.md)。

<a id="risks"></a>

## 风险

- **R1 — 仍搁置却只让设计膨胀。** 化解：本项已有启动条件，在观察到之前不开始。设计问题增加不能成为开始理由。
- **R2 — 范围泄漏至交互式 shell。** 管线组合加入后可能觉得“只剩提示符”。化解：父项决定就是边界；若要越界，应附 MSR 增量估计并改写父项决定。
- **R3 — 遗漏 SIGPIPE 连线。** 化解：已在此将上述 `stdout.close()` 与 `wait()` 清理固定为实现检查项。

> 保留的 Almide 注释说明构建是纯操作，不启动进程；只有执行步骤带 effect。Python 注释说明关闭上游 stdout，让 p2 退出时 p1 能收到 SIGPIPE。
