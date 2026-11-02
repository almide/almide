原文说明元数据：借鉴 Nushell 的脚本数据模型，不构建交互 shell
<a id="shell-scripting-stance-structured-pipelines-no-interactive-shell"></a>

# Shell 脚本立场：结构化流水线，不做交互 shell

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](shell-scripting-surface.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](shell-scripting-surface.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](shell-scripting-surface.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/shell-scripting-surface.md)

> 原文注记：测量属于2026-09-12、v0.62.0、Darwin 24.4.0，不是普遍性能或独立重测。竞品 shell、MSR、易用性主张是原文设计理由，不是独立验证排名。table/interp REPL/一步解析提案及暂缓 streaming 不表示 API 已交付；动态行糖衣仍已撤回。处理错误结果不等于必须检查每个进程退出码；显式启动 shell 时 argv 传递并不提供该保护。50→124 与增量81算术不一致，按原文保留；旧 self_host_registry 添加流程属于历史记录，不是已执行的当前指南。 要求处理动态 lookup 失败，本身不等于编译期验证动态列名；静态名称检查主张适用于类型化 record 边界，不扩展至任意动态 lookup。

> 起点是“能否让 Almide 像 Nushell 一样好用？”这一问题。答案是**一半可以，另一半不应追求**；本文划定界线。
> 相邻项目：[zero-committed-shell.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/zero-committed-shell.md)（向内）。本项向外。

<a id="決定"></a>

## 决定

**从 Nushell 借鉴数据模型，而非终端。**

让结构化数据流经管道的书写体验 — 争取实现。交互 shell 层（命令解析、作业控制、重定向、行编辑、提示符）**不做**。

目标是“替代 bash 脚本”，不是“替代 zsh”。

<a id="なぜ対話シェルを作らないのか"></a>

## 为什么不做交互 shell

三个理由，任何一个都足够。

**1. 对 MSR 毫无作用。** 作业控制、命令解析、glob 展开不是语言的工作，而是终端的工作。本语言的任务是 modification survival rate（[llm-first-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md)）；交互 UI 甚至不在测量对象内。实现成本高，MSR 增量为零。判断标准如此，因此不采纳。

**2. 这条路线已反复失败。** xonsh、Elvish、Ammonite — “让自己的语言成为交互 shell”的尝试已有十多年，没有一个取代 bash/zsh。另一方面，Nushell 获得的成果并非因为交互 UI 做得好，而是**重新设计了数据模型**。应该模仿的是这一部分，只取成功部分。

**3. 交互 shell 要求与静态类型正面冲突。** 交互 shell 体验建立在“列在运行时决定”之上。Almide 若要适应，只能放松类型，这会直接削弱 MSR（见下文“不引入动态行”）。

<a id="いま実際どこにいるか2026-09-12-実測v0620darwin-2440"></a>

## 实际现状（2026-09-12 实测，v0.62.0，Darwin 24.4.0）

| | 实测 |
|---|---|
| `almide run script.almd`（warm） | **20ms** |
| `almide run --target wasm`（warm） | 38ms |
| REPL 1 个表达式（warm，wasm 路径） | 约40ms |
| REPL，累积80行定义的会话 | 1.57s（约19ms/行 — 到80行仍平坦） |
| 含 `process.exec` 的 REPL 表达式 | 0.82s（wasm 路径回退至 cargo 路径） |

**20ms 与 python3 同级，作为脚本语言已完全实用。** 这是本项前提，后续判断全部建立在此数字之上。

<a id="既にある材料api-名は現行リファレンスに追従上の時間計測は-2026-09-12-の記録"></a>

### 已有材料（API 名称跟随当前参考，以上计时是2026-09-12 的记录）

- `|>` 与 [`list`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/list.md) 操作 — `map` / `filter` / `fold` / `sort_by` / `group_by` / `partition` / `window` / `unique_by` / `take_while`。Nushell 的 `each` / `where` / `reduce` / `sort-by` / `group-by` 大多有对应物。
- callback 含 `!` 就**直接切换为 fallible 形式**（ADR-0006，#1041 已解决）。含 effect 的流水线不会退回 `var` + `for`。
- [`process.*`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/process.md)：`exec` / `output` / `run`，`ProcessCommand` 的 `dir` / `stdin` / `timeout_ms`，以及 native 的 `start` / `poll` / `wait` / `stop` / `read_new_stdout` / `read_new_stderr`。不使用旧名 `exec_in` / `exec_with_stdin` / `exec_status` / `spawn`。
- `fs.*`：`glob`（匹配器缺口已在 #1805 解决）/ `walk` / `stat` / `list_dir` / `fold_lines` / `read_lines`。
- `io.*`：`read_all` / `read_line` / `read(io.stdin()!, n)!` / `read_exact` / `write`。
- shebang：`#!/usr/bin/env -S almide run`（`spec/cli/shebang.almd`）。
- REPL：无子命令的 `almide`（`src/cli/repl.rs`）。
- 行本身已作为具名 record 返回 — `FileStat { size, is_dir, is_file, modified }`、`ProcessStatus { code, stdout, stderr }`。

`process.exec(cmd, args)` 传入 argv 列表，所以不会做 shell 字符串的 word splitting 或展开（用户显式启动 `sh -c` 等除外）。`Result` 与 E042 要求处理错误结果，但不保证一定检查子进程退出码。`exec` 把非零退出变成 `err`，而 `output` 在非零退出时仍返回 `ok(ProcessStatus { code, stdout, stderr })`，`run` 也返回退出码，因此调用方检查这些 code 值。

<a id="無いもの"></a>

### 缺少什么

1. **表格渲染器** — `stdlib/` 无对应内容。这在感受上占 Nushell 体验的一半。
2. **一步边界解析** — `exec` → `json.parse` → Codec decode 需要三步。
3. **惰性流水线组合** — 无 `A | B` 流式处理，中间输出驻留内存。
4. **行编辑** — REPL 是原始 `read_line`，方向键、历史召回、补全、多行编辑全无。
5. 进程间管道语法、命令解析、作业控制（= 已决定不做的东西）。

<a id="動的行は入れない--前案の撤回"></a>

## 不引入动态行 — 撤回前案

讨论初期把“将 `value.field(v, "name")!` 写成 `v.name` 的语法糖”视为核心。**这是错误，予以撤回。**

`value.field` 返回 `Result`，E042 强制处理。糖衣化为 `v.name` 等于**将已检查的失败替换成运行时失败**。列名拼写错误不再编译时失败 — 不仅不能提高 MSR，反而让 LLM 最容易犯的一类错误静默通过。

Almide 已明确写出正确路径：[CHEATSHEET.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) §JSON & the wire：

> Typed Codec 是所有 JSON 工作的默认路径。动态 `json.*` API 仅用于探索和无 schema 透传 — 如果知道结构，就声明类型。

外部命令输出也是同一种“wire”。**在边界一次转换为 record，之后流经静态类型流水线。** 这比 Nushell 动态表更有利于 modification survival：更改列名时，Nushell 在运行时发现，而 Almide 在编译时指出全部引用。

因此 Phase 3 不是“让动态访问更轻松”，而是“**让边界处转换为 record 只需一步**”。

<a id="the-gate-language-boundary-との関係"></a>

## 与 the gate language boundary 的关系

没有重叠。

| | [the gate language boundary](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/zero-committed-shell.md) | 本项 |
|---|---|---|
| 方向 | 向内 — 本仓库自己的门禁用什么编写 | 向外 — 成为用户与 LLM 编写的语言 |
| 对象 | 既有、新增门禁 | 新编写脚本 |
| 成败 | 结构化门禁以 Almide 编写，并由台账强制 | 书写体验与采用 |

那边点名的两个 blocker **均已 closed**（#1040 超时，#1041 effectful combinator）。那边已通过测量展示可行性 — 字节匹配移植7个门禁、约1,300行 bash，期间发现**8个缺陷**。本项建立在其上。

那边在2026-09-12 替换了目标（旧“committed shell 归零”→“读取结构化数据的门禁用 Almide，grep 后即结束的门禁用 bash”）。原因是实测六周内 committed shell 从50 → 124个，增量81中有50个是 `scripts/check-*.sh` — 即新门禁全以 bash 编写。经过记录在那边。

<a id="phase"></a>

## 阶段

<a id="phase-1--table-モジュール"></a>

### Phase 1 — `table` 模块

将 `List[A]` 绘为带边框表格。仅这一项就最能提升“Nushell 感”，而且完全不碰语言表面。

实现机制已有：若 `A: Codec`，通过 `A.encode` 得到 `Value`，通过 `value.keys` 获取列名。因此可用纯 Almide 编写为 `stdlib/table.almd`。

```
table.print(rows)            // 罫線つき
table.render(rows) -> String // 文字列を返す形
table.print_with(rows, cols) // 列を選ぶ
```

代码注释翻译：table.print 带边框；table.render 返回字符串；table.print_with 选择列。

MSR 意义不在外观。没有它，**LLM 每次都手写填充循环** — 包括位数计算和全角宽度处理，对修改极其脆弱。消除整个类别。

`stdlib/` 添加流程遵循 [CLAUDE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/CLAUDE.md) 的 Testing Rules（self_host_registry 注册、`spec/stdlib/` 测试、3-way oracle 桥接）。

<a id="phase-2--repl-を-almide-interp-に載せ替える"></a>

### Phase 2 — 将 REPL 切换到 `almide-interp`

[#1490](https://github.com/almide/almide/issues/1490) 已凭“大 UX 改善、小改动、还能用真实输入 dogfood oracle”独立排其第2。`crates/almide-interp` 已有12,005行树遍历求值器，运行于链接后的 IR。

当前 REPL 每次输入都将整个会话送过流水线，是 O(n²) 结构。但**实测到80个定义仍约19ms/行且平坦**；固定开销主导时，二次项尚不可见。因此不是“现在太慢而困扰”的问题，**缺少行编辑更主导体验**。着手时，将 interp 切换与行编辑（rustyline 同类）作为同一单元加入。

含 `process.exec` 表达式让 wasm 路径回退至0.82s 的台阶，在 interp 路径也会消除。

<a id="phase-3--境界パースを一手にする"></a>

### Phase 3 — 一步边界解析

将外部命令输出一步变为 record。

```
process.exec_json[T](cmd, args) -> Result[T, String]   // T: Codec
text.columns(s) -> List[List[String]]                   // JSON を話さないコマンド向け
```

代码注释翻译：前者要求 T: Codec；后者面向不输出 JSON 的命令。

前者不过是 `exec` → `json.parse` → `T.decode` 的组合，但**正是这三步让人觉得“加类型麻烦”，转而逃向动态访问**。一步即可让 Codec 成为默认路径。本项中对 MSR 作用最大的是这里。

<a id="phase-4--遅延パイプライン合成保留"></a>

### Phase 4 — 惰性流水线组合（暂缓）

流式 `A | B`。`ProcessCommand.stdin` 能传入输入字符串，但单凭这一点不是进程间惰性流水线组合，大量中间输出仍驻留内存。本仓库门禁移植从未需要它（Unit 0.46 记录），因此本项也在有需求前不动手。出现一个处理大量中间输出的实例再启动。

<a id="採らないもの記録として"></a>

## 不采纳的内容（留档）

- **交互 shell 层** — 理由如上。
- **动态行访问语法糖** — 见上文“不引入动态行”。反 MSR。
- **顶层语句**（不再需要 `fn main`）— 5行脚本要3行仪式确是事实，但 **LLM 不介意样板**。这是人的书写舒适度问题，不是 MSR 问题，不值得为之改语言表面。若实测出现人的不满，再评估。

<a id="done-criteria"></a>

## 完成条件

- stdlib 有 `table`，并有 `spec/stdlib/` 测试和3-way oracle 桥接。
- REPL 完全不经 rustc/cargo，且有行编辑（历史、补全、多行）。
- `exec` 执行的外部命令 JSON 输出**一步**变成类型化 record。
- 使用上述能力的实际规模脚本例子刊于 [CHEATSHEET.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md)，带 ```` ```almide check ````（按 #1483 规则，刊出即成为承诺）。

<a id="risks"></a>

## 风险

- **R1 — 范围溢出到交互 shell。** 表格出来后必然有人说“是不是只剩提示符了”。缓解：本项决定就是界线，跨越须附上 MSR 增量估计并重写本文。
- **R2 — `table` 为显示方便扭曲类型表面。** 缓解：`table` 只建立在 `Codec` 上，不为渲染向类型侧加要求。
- **R3 — Phase 3 糖衣成为绕开 Codec 的通道。** 缓解：只引入要求 `T: Codec` 的 `exec_json[T]` 形式。不做返回 `Value` 的版本。
