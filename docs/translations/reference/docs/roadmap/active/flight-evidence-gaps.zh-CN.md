> 原文描述：实地内部审计发现（2026-07-03）——将 DAL-A 理念与 DAL-A 证据之间的实测距离列为 7 项发现，附纠正工作项和验收标准。
<a id="flight-evidence-gaps--実地監査所見台帳"></a>

# Flight Evidence Gaps — 实地审计发现台账

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flight-evidence-gaps.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-evidence-gaps.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flight-evidence-gaps.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-evidence-gaps.md)

> 原文注记：这是固定原文的历史内部审计及带日期的后续记录，不是新测量或外部 DAL/DO-178C/DO-330 资格认定。最初七项发现与后加 F8 并存。保留假设及后续纠正：F4 负载→locale，F6 tarball 怀疑→共享 working copy 的 make install。初始 parity 分项之和不等于所述总数；四个目标头部与六个更新文件也不同。八月表保留“仅允许 baseline 减少”以及 TOR 未编纂／已编纂并列，末尾仍列编纂未完；不暗中修正这些源文不一致。“结构上不可能”仅限所述刻印门禁，不是普遍安全保证；证明或 agent 独立性并非合资格人类批准。关闭前提出的精度范围不是新保证。未执行引用命令、CI 修改或测量。


> **定位**：[certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) 负责将标准分解为 5 种机制及 ALS 设计，[flight-qualification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md) 负责 DO-330/333 映射；本文件则以审计发现（finding）的形式，固定**2026-07-02～03 实地工作中（约 40 个 compiler/MIR 修改提交，parity 126→163，消灭 silent-miscompile 类别）作为参与者从内部观察到的证据体系缺口**。每项发现均以“本次会话实际发生的事实”为证据，附标准层面的含义、纠正工作项和可机械验证的验收标准。
>
> **总体评价**：设计保证理念（fail-safe 大原则、可追溯性、棘轮）面向 DAL-A，但证据体系相当于 DAL-D。填补距离所需的是本文的 7 项发现，而不是单个 bug 修复。

---

<a id="所見一覧深刻度順"></a>

## 发现列表（按严重程度排序）

| # | 发现 | 严重程度 | 对应的标准机制 |
|---|---|---|---|
| F1 | 规格就是实现（v0）本身——oracle 循环 | 最严重 | ①独立规范性规格 |
| F2 | 验证覆盖率错觉——门禁绿灯 ≠ 输出正确 | 最严重 | ③验证分级 |
| F3 | 证明与实现之间的缺口——lowering 是受信代码 | 严重 | ③ DO-333 formal credit |
| F4 | 验证非确定性——把 flaky 当作“已知”而容忍 | 严重 | ③验证流程的稳健性 |
| F5 | 缺乏独立性——修改者移动棘轮 | 严重 | ⑤流程与责任 |
| F6 | 配置管理事件原因未明——二进制替换 | 中等 | ⑤配置管理 |
| F7 | 已知数值不兼容——float.parse 边界舍入 | 中等 | ①数值精度要求 |

---

<a id="f1--仕様が実装v0そのものoracle-循環"></a>

## F1 — 规格就是实现（v0）本身（oracle 循环）

**证据（2026-07-03 实例）**：MIR 路径的 `json.parse` 自托管实现是对 `runtime/rs/src/json.rs` 的**逐条抄写**（包括代理项合成、宽松分隔符以及错误措辞）。`string.trim` 的 Unicode 化也从 oracle 反查 `char::is_whitespace` 的 25 个码点。float.parse 的错误措辞（"cannot parse float from empty string"）跟随 Rust 的 `ParseFloatError` 显示。**“正确”的定义始终是 v0 实现**，v0 的 bug 因而被固定为规格 bug（事实上，`list.chunk(-1)` 把 `chunks(n as usize)` 中 usize 重解释这一 Rust 实现细节抄成了“规格”）。

**标准层面的含义**：DO-178C 的需求必须独立于实现存在。在实现＝规格的循环中，无法定义对 High-Level Requirements 的符合性验证。

**纠正**：certification-grade CG-1（ALS 规范化）本身就是纠正措施。本文追加要求：
- ALS 各节须写成**规范性判断**，而不是“因为 v0 这样做”。对于源自 v0 实现细节的行为（chunk 的 usize 重解释、parse_string 的宽松性），ALS 应**明确裁决采纳或拒绝**，并留下理由记录（rationale）。
- 将抄写得到的自托管实现（json_parse / string_trim / float_parse / list_chunk）固定为本发现的清单，待 ALS 完成后**改接规格引用**。

**验收标准**：除 `check-contracts.sh` 的三层可追溯性（CG-1 exit criteria）外，上述 4 个自托管实现的头部注释应引用 ALS 节编号，而非 oracle 实现文件。

**进展（2026-07-04）**：在 `docs/specs/als/text-and-numbers.md` 制定规范节 ALS-T1～T5（trim 的 White_Space 集合；float.parse 精确舍入值规范＋错误措辞；json.parse 的 RFC 8259＋裁决；list.chunk 负大小裁决——**明确说明从 v0 实现细节提升而来**；case mapping 的 Unicode 规范＋Final_Sigma＋区域设置无关裁决）。6 个抄写自托管文件的头部已改为 ALS 节引用（oracle 循环后半段，即 CG-1 的 spec-keying／三层可追溯性，尚未完成）。

**进展 2（2026-07-04）**：**实现 spec-keying 机制**——契约的 `spec = "ALS-xx"` 字段以及 check-contracts.sh 的解析验证（契约引用不存在的节则门禁 FAIL，已做 mutation 测试）。初始 6 个契约已关联：C-001/C-002→ALS-T6 终止约定，C-007→ALS-T7 top-let 求值时机（这两节新制定），C-020→T5、C-021→T1、C-024→T2。三层 spec↔contract↔fixture 机制已运行。CG-1 剩余＝推广到全部 active 契约（瓶颈在 ALS 节的撰写，是文档项目）。

<a id="f2--検証カバレッジの錯覚ゲート-green--出力正"></a>

## F2 — 验证覆盖率错觉（门禁绿灯 ≠ 输出正确）

**证据（2026-07-03 实例）**：corpus-wall 宣称 "zero silent miscompiles" 并显示绿灯的背后，`match` 的线性化（依次执行两个分支）仍以**执行两个 println 的输出破坏**形式存在。门禁声称的是证书健全性（ownership/caps），不是输出正确性。真正验证输出的 output-parity 网络覆盖 573 个 spec 文件中的 **163** 个（wall 75＋v0fail 23＋skip 297 未验证）。此外，**编译器本身的结构覆盖率从未测量**——别说 MC/DC，连 statement coverage 都没有。没有人知道哪些代码行从未被测试执行过。

**标准层面的含义**：DAL-A 是 object code 层面的 MC/DC。连前提的 statement coverage 都没有，便无法声称验证活动完整。线性化问题的存活正是“住在覆盖缺口里的 bug”的实物标本。

**纠正工作项**：
1. 在 CI 引入 `cargo llvm-cov`（或 grcov），在 `almide test spec/`＋corpus-wall＋output-parity 执行下**测量并公布 almide-mir／almide-codegen 的行覆盖率**（先直面当前数值，再设目标）。
2. 将未覆盖区域中“影响输出的分支”登记为 evidence ladder 的缺口。
3. 分类 output-parity 的 297 个 skip（例如无 `fn main`），从 wall/v0fail 中分离**不是不能验证，而是尚未着手验证**的项目。
4. 将门禁主张措辞与验证内容对齐（corpus-wall 的“zero silent miscompiles”改为“zero certificate violations”——主张与证据一致是审计第一步）。

**验收标准**：CI 输出覆盖率数值，仪表盘有“编译器本身覆盖率”一行。门禁输出措辞与实际验证对象一致。

**进展（2026-07-03）**：通过 `proofs/coverage.sh` 首次测量（手动 llvm-cov 流水线；cargo-llvm-cov 编排把错误对象报告为 0.00%，故未采用）：**almide-mir 行 65.76%／函数 72.20%**（测试套件＋spec/wasm_cross 232 个 fixture 的 render 负载）。corpus-wall／receipt／TRUSTED_BASE／classify_corpus 四处门禁措辞已改为“totality + certificate claim, NOT output correctness”。同日实地证实措辞修正正确：在 corpus-wall 绿灯背后，发现并修复了 `prim.handle(<literal>)` 落入 deferred-Const 0 的 silent-miscompile（第 6 类）。下一步是退役 deferred-Const fallback（记录在 TRUSTED_BASE 边界图）→**已退役**（strict value mode，2026-07-03）。

**进展 2（2026-07-04）**：扩大负载后重新测量——almide-mir＋almide-codegen 测试套件＋spec 全部可执行 fixture 的 v1 render＋**v0 生产路径**（`almide test spec/` 完整流水线）。TOTAL 行覆盖率 65.89%。逐文件台账固定在 `proofs/COVERAGE.md`（最低为 defunc 引擎 control_p5 的 43%，附下一批测试编写对象）。

<a id="f3--証明と実装の間隙lowering-は信頼コード"></a>

## F3 — 证明与实现的缺口（lowering 是受信代码）

**证据（2026-07-03 实例）**：有 Coq 证明的是 checker 内核与 Coown 模式（CoownLoop.v／CoownCompose.v）。但**生成**证书的 lowering 本体是 Rust 受信代码，仅本次就修复了 5 处输出破坏（线性化、never-err strip 的表示不一致、Const-0 全局值、lambda ctx 缺少 layout、_start 的栈残留）——**全都在证明之外**。“kernel-proven path”中的 proven 并非端到端。

**标准层面的含义**：DO-333 的 formal credit 只适用于已证明的性质。lowering 正确性目前只由差分测试支撑，其覆盖网就是 F2。

**纠正工作项**：
1. 在 docs/contracts/ 以一张**边界图**明确写出“什么已证明、什么是受信代码”（目前散布在文件注释中）。这是审计员首先要求的图。
2. 缩小受信代码部分：将 lowering 输出不变量（如“已 Bind 的 Result 进入跟踪集合”“materialize 与 drop 分类使用相同类型判定”，本次修复均曾违反的类别）机械化为 **post-pass 检查而非运行时 assert**。添加与现有 `assert_names_resolvable`／ConcretizeTypes 门禁同型的跟踪集合一致性门禁。
3. 长期汇入 certificate-format-v1／value-rc-cert 的逐函数证书化。

**验收标准**：存在边界图，CI 运行跟踪集合一致性门禁，能为本次 5 类 bug 各指出防复发门禁或 fixture。

<a id="f4--検証の非決定性flaky-を既知で容認"></a>

## F4 — 验证非确定性（把 flaky 当作“已知”而容忍）

**证据（2026-07-03 实例）**：output-parity 全量运行时，append_accumulator(_heap)／list_eq(_float_bool)／string_codepoint(_index) 中 2～3 个文件**仅在机器负载下**失败，单独运行总是 byte-match。原因是 harness 的 20 秒 `alarm` 超时与并行 cargo 执行的 CPU 竞争。我曾两次“手动恢复 baseline”后继续——研究速度上虽合理，**验证流程本身非确定的事实**仍在，这种容忍损害证据能力。

**标准层面的含义**：不能复现的验证结果不是结果。“单独能过”是额外主张，不是证据替代品。

**纠正工作项**：
1. output-parity 超时采用**带重试**方式，而非仅 wall-clock（超时时只单独重跑一次，并采用该结果；再失败才是真失败）。
2. 隔离全量运行负载（相当于 `--test-threads` 的串行化，或由脚本强制禁止门禁期间 cargo build；已有 flock 先例）。
3. 废除“手动编辑 baseline”，只以 `--update` 输出为准（与 F5 联动）。

**验收标准**：连续 5 次全量运行 REGRESSION 输出为零。此后 baseline 文件 git 历史不再出现手动编辑提交。

**关闭（2026-07-03）**：真因不是负载，而是 **sort/comm 的排序区域设置**（Python 字节顺序 sorted 与 shell 区域设置顺序 sort 将 `.` 和 `_` 的顺序反转——同一文件同时被列为“新增”和“退化”的矛盾日志是决定性证据）。门禁固定 `LC_ALL=C`，重试采用 solo-once（所有非 match 判定在安静时以 60s 重新判断）。已达验收标准：连续 5 次全量绿灯。两天来误诊为“机器负载”——正应了本发现的教训：不要容忍非确定性，要根治。

<a id="f5--独立性の不在変更者がラチェットを動かす"></a>

## F5 — 缺乏独立性（修改者移动棘轮）

**证据（2026-07-03 实例）**：本次会话中，我独自完成了 (a) 修改 lowering，(b) 执行验证，(c) 用 `--update`＋手动恢复更新 parity baseline，(d) 改写一个测试期望值（将 `match_arm_heap_payload_binding_aliases_the_subject` 改为预期墙）。(d) 是正当修改（旧测试固定了 miscompile 行为），但**判断其正当性的也是修改者本人**。

**标准层面的含义**：DO-178C 的 verification independence（DAL-A/B）意味着“制作者不决定合格与否”。自动门禁是强缓解因素，但棘轮、期望值、筑墙三种权限集中于修改者。

**纠正工作项**：
1. 规定 baseline／期望值／筑墙(KnownBroken) 修改必须分离为**专用提交**（lefthook 拒绝实现与 baseline 修改同处一个提交）。
2. 要求这些专用提交的消息写明修改理由及非退化证据（单独运行记录）。
3. 长期：交由人或独立 agent 审查批准（组织侧、⑤的范围）。

**验收标准**：存在 lefthook 规则，最近 20 个提交中实现与 baseline 混合为零。

**关闭（2026-07-03）**：`scripts/check-ratchet-separation.sh`＋lefthook pre-commit。已用负例（实现＋baseline 混合→exit 1）与正例（清洁→exit 0）验证。同日 float_parse／total_order／case_unicode 三个棘轮全部以分离提交实施。

**补记（2026-09-18，在 v0.63.0-rc1 发布范围发现）**：range 形式（#2183）以**最新**规则判断范围内所有提交，因而追溯性地将规则引入前的提交判红（main..develop 中 19 个，17 个为追溯）。纠正：
1. 保护线（CI 中为 develop）上的提交按**父提交时点的门禁**判断——规则只约束引入之后的修改。以 pre-commit 模式重放该版门禁，把 `git diff --cached` 替换为该提交差分（是重放，不是重新实现）。
2. 变更集提交由**保护线尖端与父提交两者的门禁**共同判断——变更集不能为自己放宽门禁（独立审查实证了“移除门禁→混合提交→恢复”的绕过并将其堵住）。
3. 门禁本身与处置台账归类为验证产物，不能搭载于实现提交。
4. 已进入保护线的真实违规（不是追溯，而是违反当时规则）无法拆分，故通过 `proofs/ratchet-dispositions.txt` 的**处置记录**关闭。记录须与目标提交及被标记文件集合完全一致，署名不同于作者的审查者，并陈述证据。变更集不能关闭自己的提交。首次为 #2076／#2115 两项。

**坦诚说明独立性**：设计与实现由 Claude Opus 5 完成，审查由不同模型（Claude Fable 5.1）的另一 agent 在空白上下文中进行三轮；初稿发现 1 个阻断级与 6 个小缺口，修改后的复审又发现 4 个（其中 1 个对处置台账属阻断级），全部变成负例控制（`scripts/check-ratchet-separation-negative.sh`，正例 5＋负例 22）。这是 **agent 之间的独立性**，不能替代 DO-178C 要求的人类独立验证（flight-organization B4）。处置记录 reviewer 栏用于记录谁重新推导了证据，不宣称是有资格的人类批准。

<a id="f6--構成管理インシデント未解明バイナリ置換"></a>

## F6 — 配置管理事件原因未明（二进制替换）

**证据（2026-07-03 实例）**：工作中 `~/.local/bin/almide` **无解释地**被替换为 v0.27.13（Jun 22 构建），导致两项 spec 失败。我用 `make install` 恢复后继续，**未查清原因（什么、何时、为何覆盖）**。可能是并行 agent、cron 或另一会话，但尚未识别。

**标准层面的含义**：这是验证环境配置未受控的真实事件。无法声称“用该二进制取得的证据”的同一性。

**复发（2026-07-03 约 14:00，刻印门禁检测）**：F6-2 刻印引入的**当天再度发生**，覆盖率测量在刻印门禁正确 FATAL 停止——使用错误二进制采集证据已在结构上不可能。现场调查结果：`~/.cargo/bin/almide`（0.14.8，Apr 18）、cron、LaunchAgents、org 内脚本全部无责。替换后 mtime 仍保留 Jun 22，呈现发布 tarball 解包＋mv（保留 mtime 传输）的特征——最有力的假设是**本机并行会话重新安装了 GitHub 发布版 v0.27.13**。未采用 uchg 等预防，因为会破坏并行工作的正当覆盖；永久措施是刻印检测＋停止。

**机制确定（2026-07-04，关闭）**：全盘扫描发现 almide 二进制仅在 `~/.local/{bin,almide}` 及**本仓库 target/release**（无其他 checkout、worktree、缓存）。因此全部替换经由本仓库 `make install`，责任来源是**共享同一 working copy 的并行会话**（也与同期 frontend/codegen 5 个文件 M 状态、org 新增 ceangal 仓库一致）。旧 0.27.13 出现于并行会话构建另一分支／提交的时间段。现有刻印已完成对策（用错误二进制采集证据在结构上不可能），并常驻 watcher（/tmp/almide-binwatch.log）以现场记录进程名。

**纠正工作项**：
1. 保留本发现作为事件记录（已完成——本文）。
2. 门禁执行脚本开头将 `almide --version`＋二进制 mtime/hash **刻入日志**，若与预期构建不符立即 fail（一行防御使复发可检测）。
3. 验证执行时将 toolchain 刻印（rustc/wasmtime/coqc 版本）纳入 receipt，连接 certificate-format-v1。

**验收标准**：所有 proofs/*.sh 输出二进制刻印，版本不符即 fail。

**关闭（2026-07-03）**：全部门禁引入 `proofs/lib/stamp.sh`。当天以 FATAL 停止捕获两次真实漂移（再次发生的 0.27.13 替换＋stdlib 嵌入更新前的旧 workspace 构建）——错误二进制证据采集在结构上变得不可能。

<a id="f7--既知の数値非互換floatparse-境界丸め"></a>

## F7 — 已知数值不兼容（float.parse 边界舍入）

**证据（2026-07-03 实例）**：自托管 float.parse 使用 mant×10^k 缩放实现，在 denormal（4.9e-324）、最大值（1.797e308）边界及超过 19 位的尾数处，相对 v0（Rust strtod＝Eisel-Lemire）仍有约 1ULP 的舍入差异。这是 parity 唯一实际 MISMATCH。

**标准层面的含义**：对有数值精度要求的系统（航空正是如此），已知舍入不兼容本身就是阻碍。浮点不存在“差不多正确”。

**纠正工作项**：
1. 自托管实现精确十进制→f64 转换（Eisel-Lemire 或 Simple Decimal Conversion 的纯 Almide 移植，独立 brick，工作量大）。
2. 在此之前，ALS 明示当前实现的**精度保证范围**（如“10^±22 内 15 位以下 exact，边界 1ULP”级别的定量表述），并用契约 fixture 固定范围。

**验收标准**：`spec/wasm_cross/float_parse.almd` byte-match，或者 ALS 规范性地确定精度范围且 fixture 验证之（任一即可关闭发现）。

**关闭（2026-07-03）**：实现 800 位高精度十进制（Simple Decimal Conversion）slow path，`float_parse.almd` byte-match（最小 denormal 5e-324、f64 最大值、超过 19 位尾数、half-even、巨大指数全部与 Rust 一致）。fast path（15 位以下且 |exp|≤22）仍使用原有 Clinger 安全域。

---

<a id="f8--新規所見2026-07-03-夕-ローカル-pcc-チェーンが暴いた-cert-会計違反"></a>

## F8 — 新发现（2026-07-03 傍晚）：本地 PCC 链暴露的证书记账违规

**经过**：作为 F6-2 的一环，本地安装 Coq（Rocq 9.1.1），首次全量执行 `proofs/corpus-wall.sh` 的 checker phase（此前委托 CI，本地从未运行）。结果：**kernel-proven checker REJECT ownership witness**（20393 个对象中，`spec/integration/codegen_effect_fn_test.almd::parse_positive_even`——链式 monadic `!` 的 err 重构——其证书行是裸 `m`，即无获取事件的 move-out）。

**含义**：该函数执行已由 parity 逐字节验证（err 路径 fixture 也绿灯），未观察到实际伤害（double-free/leak）。但证书被拒绝即意味着它位于“accept ⟹ safe”网络**之外**——存在未获证明保护的 in-profile 函数。与 mir>ir 门禁一样，这是“看似绿灯是因为验证未运行”的实例，也是 F2（覆盖率错觉）的追加证据。

**纠正工作项**：
1. 定位责任路径并修复记账：为 `err(e) => err(e)` 重构（mod_p6 的 monadic desugar）中的 payload move-out 配对获取事件（Dup/`a`），若确为双重所有权则作为真实 bug 修复。
2. 永久保留 `ownership.names`（证书行↔函数名并列记录，已在本发现调查时添加），使 REJECT 可立即二分定位到函数。
3. 将 corpus-wall 的 checker phase 设为**本地必需**（coqc 已安装）——“委托 CI”正是本次未执行的温床。
4. 检查 CI 是否真的执行此 phase（若执行，CI 也应变红；若不红，则 CI 侧也未执行＝配置缺口）。

**验收标准**：本地 corpus-wall 包括 checker phase 全部绿灯，REJECT 为零。

**进展（2026-07-03 夜）**：用自制平衡器列出所有违规证书行盘点，从**大量→2 项**。
1. **merge move-in 记账**（已根治）：嵌套 monadic `!` 的内部 merge dst 被外层 Consume 时出现裸 `m`；为 released merge dst 配对记入来自 arm 的 move-in `i`（物理上 arm −1 与 merge +1 是同一引用的交接）。backing gate（unbacked +1 检查）也与相同 released-merge 集合保持同步。已确认 checker ACCEPT。
2. **via_if 类（第 8 个 miscompile，已根治）**：在 `let v = if c then boom(x) else boom(y)`（effect fn 的 auto-`?` 在各 arm 放入 `Try` 的 scalar-typed if）中，scalar 路径默默剥掉 Try，生成**对 Result 块 handle 加 +100 的原始运算**——首次由 checker 的 leak 行（裸 `i`）指出真实误编译。将 tail-dup 对象扩大为“heap 类型或 arm 直接包含 Try/Unwrap 的 bind”，交给既有 monadic desugar 根治（via_if byte-MATCH，baseline 176 零退化，MIR 507/0）。
3. **剩余类别也根治（同日深夜）**：`effect_tco::checked`（declared-Result tail-if 不发出 Consume，`EndIf {{ val }}` 实质为 move，追加 val-move 规则并排除循环机制值）；`bytes_set_value_semantics::rotate`（Dup 初始化 slot 事件因 of 解析与直接 ValueId 分裂为两行，将 slot 事件统一为 object_of 解析，收敛为 "a(id)m"）。

**关闭（2026-07-03）**：`proofs/corpus-wall.sh` **包括 checker phase 在内本地完全绿灯**——kernel-proven checker ACCEPT 全部 witness：ownership 20876 对象／names 4248／caps 3481／caps-transitive 243。这是项目历史上首次全量本地运行 PCC 链。副产物是根治 1 类真实误编译（via_if）及 3 类证书记账问题。同日第三次二进制替换也被刻印 FATAL 捕获（并行会话活跃，F6 永久措施有效）。

<a id="優先順位と依存"></a>

## 优先级与依赖

```
F6-2 (刻印)         ── 1日級、即効、単独
F4 (flaky 根絶)     ── 数日級、単独 — 以後の全証拠の信頼性の前提
F5 (権限分離)       ── 数日級、単独
F2-1 (coverage 計測) ── 数日級、単独 — 現状の直視
F3-2 (整合ゲート)   ── 週級 — 今回のバグ 5 クラスの再発防止
F1 (ALS)            ── certification-grade CG-1 に合流（最長・最重要）
F7 (strtod)         ── 独立 brick、F1 の精度規範が先でも可
```

保留的历史图示译文：F6-2（刻印）——约一天，立竿见影、独立。F4（根除 flaky）——数天、独立，是之后所有证据可靠性的前提。F5（权限分离）——数天、独立。F2-1（覆盖率测量）——数天、独立，直面现状。F3-2（一致性门禁）——周级，防止本次五类 bug 复发。F1（ALS）——汇入 certification-grade CG-1，耗时最长、最重要。F7（strtod）——独立 brick，也可先制定 F1 精度规范。

顺序为小而见效快的 F6-2 → 恢复证据可靠性的 F4/F5 → 直面现状的 F2-1 → 结构性的 F3/F1。**F1 与 F2 关闭之前，声称“航空品质”必须明确 evidence ladder 等级并限定范围**（克制无条件的 "flight-grade" 表述）。

---

<a id="再測定-2026-08-12--所見の現在地1-ヶ月の保証工事後"></a>

## 重新测量 2026-08-12 — 一个月保证工程后的发现现状

距七月初次审计一个月，经历 Stage 1～4 工程（scalar-read/WAT-prelude/libm 审计、三方投票门禁、分类器修复、发布封存、G-F4 应用）后的**重新测量**。台账只追加，上述发现正文保持为当时的记录，不改写。

| # | 七月时点 | 2026-08-12 实测 | 剩余 |
|---|---|---|---|
| F1 | 部分（spec-keying 机制运行，尚需推广全部契约） | **机制全数覆盖**：230/230 契约已 `spec =` keyed，check-contracts 双向强制（引用不存在的节＝FAIL，全部 ALS 正规节均被 ≥1 契约引用）。抄写自托管头部已改为 ALS 节引用（json_parse→ALS-T3 等）。另 **interp 提升为可执行规格**：三方投票 254/374 fixture（native==wasm==interp 字节一致），弃权有分类台账（HEAP_BOUNDARY 120／BRIDGEABLE 0）＋shrink-only 棘轮，semantics-manifest 全 20 模块实测 | **ALS 节撰写覆盖**（语法元素侧完整性＝Stage 3 门禁剩余一半），以及 HEAP_BOUNDARY 120 的 interp 堆弧线（#1226） |
| F2 | 部分（已测覆盖率 65.89%，已改措辞） | 措辞与验证内容的一致性已**结构化**：proven-vs-trusted.md（边界图）以一张图列明“各门禁声称／不声称什么”。覆盖率已 **CI 棘轮化**（fuzz-nightly 的 coverage-ratchet job #566，仅允许 baseline 减少）。#1183 类经 scalar-read 审计（63 arm 全分类，UNGUARDED 0，CI 门禁）**按整个族消灭**。同型审计已横向扩展至 WAT prelude 63 fn／platform-libm 23 次调用 | **MC/DC 仍为零**（别说 DAL-A object-code MC/DC，连分支覆盖率都未测量，只有行覆盖率）。下一步引入分支覆盖率 |
| F3 | 未关闭（无边界图、无一致性门禁） | item 1 **关闭**：docs/contracts/proven-vs-trusted.md 已作为审计员边界图存在，CLAUDE.md 引用。item 2 部分：mir==ir caps 门禁、rollback lifted_mark 检查、released-merge backing 检查等对应“本次五类 bug”的门禁个别存在（F8 根治群），但尚未整合为**单个有名称的跟踪集合一致性门禁** | 将一致性检查群建成台账（bug 类别与门禁对应表）。汇入逐函数证书化仍为长期事项 |
| F4-F8 | 关闭 | **维持关闭**并强化：F5 ratchet-separation 今日仍作用于全部 PR（baseline 修改用分离提交）。F6 刻印今日在 gate.sh 以 FATAL 捕获 PATH/workspace 不符（功能证据）。**fuzz 分类器 Slow/Hang 分离**（#1235）增强验证确定性，已知性能债务（#1229）不污染 correctness 指标 | — |

<a id="stage-5-欠損成果物リストgoal-指示の-3-点実測"></a>

### Stage 5 缺失产物清单（goal 指令的三项，实测）

| 产物 | 现状 | 评价 |
|---|---|---|
| 工具运行要求（TOR） | CLAUDE.md Usage 节＋TRUSTED_BASE.md 零散对应。作为 DO-330 格式（预期运行环境、输入约束、已知限制的运行规避）**尚未编纂** | **已编纂（proofs/TOR.md）**——TOR-1～9 运行要求（每行指定 enforced-by 检测机制，check-tor-refs.sh 在 CI 强制引用一致）＋运行视角的已知限制表。尚需整理为 G-F6 套件 |
| 验证工具自身的验证 | checker＝Coq 证明（qualified-by-proof，45 个定理＋coqchk＋cross-version）。**门禁脚本群已形成负测试文化**（check-contracts mutation 测试、三项审计门禁负测试、release-seal 伪造／未填写负测试）。fuzz harness 有单元测试（分类器纯函数化） | **清单已运行**（proofs/gate-verification.toml＋check-gate-verification.sh，33 门禁、5 分类，以 shrink-only 上限直面 UNVERIFIED）。#1244 清理后 18→**2**。剩余两项（check-host-determinism／check-browser-determinism）是需要带 wasm32 std 的主机的环境门禁 |
| 已知问题台账正式化 | GitHub issues＋发布说明已知限制（0.57.0 的 #1229）＋`fuzz-perf` 标签＋flagged-for-revision 机制＋**发布封存**（proofs/releases/，自 v0.57.0 起，证据绑定标签） | **PARTIAL**——散布。为发布封存 `[recorded]` 添加 known-problems 节即可闭合形式 |

**总体评价更新**：从七月“理念 DAL-A、证据相当于 DAL-D”，变为证据体系已有**“独立规范规格＋可执行规格＋覆盖审计＋封存”的骨架**。剩余结构性距离是 (a) MC/DC（F2 剩余），(b) ALS 撰写覆盖（F1 剩余＝Stage 3），(c) TOR 编纂（Stage 5），(d) 外部采用实绩——(d) 需要市场时间，而非工程。**继续**克制无条件 "flight-grade"（维持明确 evidence ladder 等级）。
