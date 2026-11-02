原文说明元数据：哪些门禁属于 Almide、哪些保留 bash，以及如何守住界线
<a id="the-gate-language-boundary"></a>

# 门禁语言边界

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](zero-committed-shell.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](zero-committed-shell.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](zero-committed-shell.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/zero-committed-shell.md)

> 原文注记：这是目标变更与历史测量记录，不是本翻译已执行的指令。移植时本地字节一致接受条件，不同于缺失的持续 CI twin/原本/生成物对照；不声称后者完成。已实现 reads 分类及子命令列举，不同于仍须完成的 twin 晋升。计数、成本、缺陷、旧 CI 位置是原文时点观察，不是新盘点。此处50 + 81 - 7 = 124；新增数不是净增。旧 timeout 名称及保证须结合当前参考限定；触发时行为一致不保证各 host 触发判断一致。

> **2026-09-12 替换了目标。** 旧目标是“本仓库不留一个 `.sh` 文件”。六周实测否定了它，因此重新划线。经过与旧目标记录保留在下方“为何替换目标”。文件名不变以维持引用。
>
> 向外的对应项：[shell-scripting-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/shell-scripting-surface.md)（作为用户与 LLM 编写语言的 shell 脚本）。本项向内 — 本仓库自己的门禁用何种语言写。

<a id="決定"></a>

## 决定

**读取结构化数据的门禁用 Almide 写。grep 后结束的门禁可以保留 bash。**

不是“committed shell 归零”。零作为目标是错的。

<a id="なぜこれ以外にないのか"></a>

## 为何只能如此

决定因素不是偏好，而是**边际成本与复用的不对称**。

每个 bash 门禁成本低。新文件、CI 加1行、`proofs/gate-verification.toml` 加1行 — **只改3处**，文件完全独立。代价是**复用为零**：60个 `scripts/check-*.sh` 各自重写 `grep`、`awk`、`sed`。

Almide 最初成本高。`tools/almide-gates/src/toml/mod.almd` 是291行子集 TOML reader，`src/mdmeta/mod.almd` 是68行 markdown front matter 读取。bash 三次 `grep` 就能做的事，被重写成类型化 parser。

**这笔投资已经付清。** 下一个读取 TOML、markdown、台账的门禁作者，可免费使用359行 parser。也就是说，仅针对读取结构化数据的门禁，**现在 Almide 更便宜**。Unit 0.46 移植发现的8个缺陷，全发生在此类“判定规则”一侧（见后文）。

反之，对 `grep -q foo file || exit 1` 形态的门禁，这项投资毫无作用。修改处从3增至5–6，要编辑共享245行文件，却没有收益。**界线就在这里。**

<a id="線の定義"></a>

## 界线定义

| 门禁读取什么 | 语言 | 示例 |
|---|---|---|
| 结构化数据（TOML / markdown / 台账 / 生成物对照） | **Almide** | contracts.toml 一致性、roadmap README 再生成、台账计数 |
| 字符串有无、简单形态（grep 后失败） | **bash** | 禁止拼写、退役 API 再现、文件存在 |

难以判断时归入结构化侧。若在写 parser，就是结构化。

<a id="どう守るか"></a>

## 如何守住

只定规则不会被遵守。这正是六周实测的结果（见下）。

使用 `proofs/gate-verification.toml`。行数以台账为准，每行含 `path` / `class`（KERNEL_PROVEN / MUTATION_TESTED / NEGATIVE_TESTED / EXERCISED / UNVERIFIED）/ `evidence`，由 `scripts/check-gate-verification.sh` 对列举、遗漏、过时进行拒绝。UNVERIFIED 在**只可下降的上限**下逐步减少。

分类字段 `reads` 已实现，取 `structured` / `scalar` / `UNCLASSIFIED`。**声明 `reads = "structured"` 的 `.sh` 会被检查拒绝。**

- `UNCLASSIFIED` 由台账上限管理收缩，不要复制旧固定值79行。
- 新门禁必须声明，检查缺失、未知分类、依据不足。
- `tools/almide-gates/src/main.almd` 子命令也已按 `tools/almide-gates:<cmd>` 列举；引入它本身已非待办。

**不建立统计 `.sh` 数量的棘轮。** 旧目标完成条件曾要求它，但既已决定 grep-and-exit 门禁留在 bash 是正确做法，数量就可合法增加。数量不再是指标。

<a id="twin-は検証してから昇格する"></a>

## twin 验证后再晋升

Unit 0.46 将7个门禁移植到 Almide。这7个 — `contracts-readme` / `conformance` / `roadmap-readme` / `check-contracts` / `output-parity` / `stamp` / `fuzz-track-record` — 位于 `tools/almide-gates/`，用 `almide test` 跑单元测试。

**但不存在与 `.sh` 原本的对照。** CI 实际只跑 `.sh` 一侧（`ci.yml:1132`、`ci.yml:1240`、`ci-cross-target.yml:142`）；作为 live 门禁使用的 twin 仅1个 `bench`。`main.almd` 字节匹配测试仅覆盖 `docs/contracts/conformance.md` 与 `docs/contracts/README.md` 2个文件。

本文之前写着“the byte-diff runs in CI”（字节 diff 在 CI 运行）。**只是写了，没有实现。** 保留记录 — 声明风险缓解却不实现，比不声明更糟。

因此固定顺序：

1. **构建 diff。** 对相应门禁在 CI 验证“Almide twin 输出 == `.sh` 原本输出 == 已提交生成物”。
2. **确认绿色。**
3. **此时才晋升。** 让 Almide 成为 live 门禁，删除 `.sh`。

不得无依据删除 `.sh`，否则坏了也察觉不到。

**第1个是 `roadmap-readme`。** 它在7个中最小、最安全（仅读 markdown 输出表，无副作用），却作为 `main.almd` 默认子命令而没有任何验证。而 `docs/roadmap/README.md` 确已漂移（2026-09-12 发现8项缺失），所以这1步**同时解决 README 过时与 twin 无验证**。

<a id="なぜ目標を差し替えたか"></a>

## 为何替换目标

旧目标是“`git ls-files '*.sh'` 为0”。2026-08-01 起草时为50文件 / 6,159行。

**2026-09-12 实测：124文件 / 13,579行。**

| | |
|---|---|
| 新增 | 81 |
| 删除 | 7 |
| **新增中 `scripts/check-*.sh`** | **50** |

六周间，新 CI 门禁**无一例外全以 bash 编写**。相对7个移植门禁，新流入约7倍。不是移植慢，而是水龙头一直开着。

没人发现，因为旧完成条件“A gate fails CI when the count goes up”（数量增加时门禁让 CI 失败）**未实现**。

可得两点：一是**只挂规则不会被遵守** — 所以上面“如何守住”改用台账声明与机器检查。二是**六周实际选择也可能是作者每次正确判断的结果** — 对一次 grep 的门禁确实没理由用 Almide。零这个目标才是不符合现实的部分。

<a id="残すもの--unit-046-の測定"></a>

## 保留的部分 — Unit 0.46 测量

目标已替换，但本项最有价值的是测量，这不变。

以**字节一致为接受条件**移植7个门禁，最后1个还对**15个 mutation**验证。

| | |
|---|---|
| 替换的 bash | 7个门禁，约1,300行 |
| 编写的 Almide | 约2,300行，14模块 |
| 测试 | 47个，针对 bash 只能经 I/O 到达的判定规则 |
| 移植发现的缺陷 | **8** |

8个中**5个同形** — **因复制比抽象便宜而被复制、在编写当天全部一致的规则**。散在11个脚本的 locale 固定、散在5个提取器的 unquote 表达式、写在2处的 evidence 类 enum。

这不是作者懒惰。shell 无模块也无类型检查，**复制确实是最便宜选择**。移植能发现不是因为聪明，而是 byte-diff 拒绝了“看似合理的改写”。

<a id="タイムアウト--当初の経緯と現在の境界"></a>

## 超时 — 原始经过与当前边界

当前子进程期限为 `ProcessCommand.timeout_ms`（[process reference](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/process.md)）。`fan.timeout(duration.ms(n)) { body }` 也作为 oracle 层存在，并非仍被移除。确定性计算上限用 `fan.bounded(compute.ms(n))`，按 clock 选择用 [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) 的 `fan.min`。下文是 #1040 时点设计经过，不是决定当前语法可用性的表。

`output-parity` 需给子进程设时限，当时委托给 `perl -e 'alarm …; exec …'`。当时 `fan.timeout` 在0.29.0 已移除 — 给**纯计算**设时限会让结果成为机器的函数，破坏 native ⇄ wasm 字节一致。

当时界线是：**只有被包围操作已在字节一致契约之外，才允许时限。**

| 包围对象 | 已非确定？ | 时限 |
|---|---|---|
| `process.exec_status`（启动进程） | yes | **允许** |
| `http.get`（网络） | yes | **允许** |
| pipe / socket 读取 | yes | **允许** |
| 纯 `fan` 兄弟 | **no** | **不允许 — 0.29.0 移除的就是它** |

契约分两点：

> **保证字节一致**：若期限触发，两 target 输出同样错误值、消息、退出路径。
> **不保证**：是否触发。这是 host 的函数。

已在 [#1040](https://github.com/almide/almide/issues/1040) **落地** — 当时以 `process.exec_status_timeout` 进入 effect 表面。当前名称及将 `fan.timeout` 重引入 oracle 层，遵循本节开头的当前参考。

<a id="解けた障害"></a>

## 已解决障碍

旧版称为“the two primitives that block the rest”（阻塞其余工作的两个 primitive）的2项，**均已 closed**。

- [#1040](https://github.com/almide/almide/issues/1040) — 上述超时，已落地。
- [#1041](https://github.com/almide/almide/issues/1041) — `list.map` callback 纯粹，导致含 effect 流水线退为 `var` + `for` 的问题。已解决（ADR-0006，callback 内 `!` 实例化 fallible 形式）。

但 `tools/almide-gates/` 仍有13处 `var` + `for`（`parity_sweep.almd:71-79` 等）。这是 #1041 前写的代码，不是语言限制。重写与摩擦实测一并推进。

<a id="既知の摩擦almide-側でゲートを書くときのコスト"></a>

## 已知摩擦（在 Almide 侧编写门禁的成本）

既然划线，Almide 侧门禁会增加。记录实测摩擦。

- **新增1个门禁要改5–6处**（bash 为3）。其中之一是**所有门禁共用的245行 `main.almd`** — 加 import、加 `cmd` match arm、加 root 默认 match arm。
- **陷阱**：门禁型命令（返回退出码）必须在 arm 内 `process.exit`，以避开 `main.almd` 末尾面向 generator 的 `println`。不知道就会踩。
- **无模板**（bash 侧也无）。
- **CI 运行 Almide 门禁需要 Rust toolchain**，bash 无需额外内容。
- `list.sort_by` 接受 key extractor 而非 comparator，且无 `string.compare`，所以降序需“先升序后 reverse”（记录于 `main.almd:70`）。

**不凭想象提前修复。** 在下一个1项（`roadmap-readme` diff）中实际踩到，按遭遇顺序修。

<a id="done-criteria"></a>

## 完成条件

- `proofs/gate-verification.toml` 有分类字段，声明结构化的门禁若用 bash 写，CI 失败。`UNCLASSIFIED` 上限只减不增。
- 台账列举含 `tools/almide-gates/` 子命令。
- 每个7 twin 均有 CI 中绿色的“Almide 输出 == `.sh` 输出 == 已提交生成物”diff。
- 按确认绿色的先后晋升 twin，删除对应 `.sh`。
- 新增结构化门禁用 Almide 写（台账强制）。

<a id="risks"></a>

## 风险

- **R1 — 为移植而移植。** 无人运行的脚本不值得做成程序。缓解：界线定义范围，grep-and-exit 在范围外，不移植才对。
- **R2 — twin 偏离 `.sh` oracle。** 缓解：**实现 diff**（旧版只声明未实现，正是 R2 成真的路径）。diff 绿之前不删 `.sh`。
- **R3 — 混淆确定性计算上限与环境期限。** 缓解：区分 `Compute`、`Duration` 契约，不声称墙钟是否触发跨 target 不变。以当前 SPEC / ADR-0028 为准，不把0.29.0 移除改读成当前禁止。
- **R4 — 界线无法判定。** 若每次都争论“结构化还是 grep”，台账字段就流于形式。缓解：上文默认拿不准就归结构化。若在写 parser，就是结构化。
