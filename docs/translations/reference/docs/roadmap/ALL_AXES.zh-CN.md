<a id="all-axes--総合格闘技の勝ち筋"></a>

# All Axes — 全能竞争的取胜路径

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ALL_AXES.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ALL_AXES.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ALL_AXES.zh-CN.md)

> 非规范性历史策略参照版，不是当前宣传或认证证据。固定原本权威且不变。不自动同步、重新核实来源、独立benchmark/Dojo/证明或认证审计。排名、唯一性、外部项目时间线、CompCert credit、MSR/LOC及性能数字均为当时原作者主张，本次未验证且不泛化。保留materialize/libm、fan/fusion、LSP后续订正和旧行，payload与仅Int范围、已结构化与未结构化的差异亦按原文保留。资格化/GPU/生态目标不是已实现保证。原文缺失审阅标题fragment保留为源缺陷，不虚构目标。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ALL_AXES.md)

> 为“既夺小众冠军腰带，也夺综合腰带”而制定的各轴策略。
>
> 若 [ROAD_TO_1_0.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md) 是**时间轴**（0.65→0.99 版本阶梯，2026-09-27 重划），本文就是**评价轴**。
> 同一工作从不同切面观察，不与台账竞争；台账行在此作为“该轴的证据”引用。
> 开始新轴工作时，**先在本文定义胜利条件，再分配到台账 decade**。
>
> **2026-09-13 后继增量**：[active/arena-breakthroughs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md) 根据全领域竞技场（15 工具链）及相邻 8 种语言机制阅读，固定各轴的**实测排名、败北、未测量**与突破点 T1–T9。本文胜利条件不变，当前值更新及“退出轴”的重新进入条件由后继文档负责。

<a id="前提-2026-年に単一の最強は存在しない"></a>

## 前提：2026 年不存在单一“最强”

外部调查（2026-08-13，四个方向、约 120 来源）结论明确：编译速度之王（Zig / tsgo）、运行性能之王（LLVM）、证明之王（CompCert）是**各不相同的实现**，没有处理系同时征服所有轴。Carbon 获得顶尖人员与 Google 支持，从宣布到 0.1 仍历时约 6 年。

因此，**按轴定义“最强”，按轴改变取胜方式**。有些轴可以正面竞争，有些必须改变赛场才能生存。以下区分它们。

<a id="軸マップ"></a>

## 轴地图

| 轴 | 当前值（实测） | 胜利条件（可测量出口） | 取胜路径 | 主战场 |
|---|---|---|---|---|
| **A. 编辑循环速度** | check 22.7ms（268 行）/ **10k 行 p95 53.3ms、斜率 1.13**（#1334）/ warm build 266ms / cold 644ms | ~~10k 行规模无关~~ **达成**，debug 路径无 rustc 和链接器 | **正面竞争**（已知工程） | 0.4x, 0.6x |
| **B. 运行性能** | 相对手写 Rust **1.00×**（n-body、spectral-norm）～1.18×（FFT），materialize 为主的工作负载约 1.6× | ①保持 ratchet ②**语义优化低于 1.0×** ③GPU 另一个量级 | **不在标量 codegen 挑战 LLVM** | 0.5x, 0.6x |
| **C. 正确性、信任** | 契约 260 / 3-way oracle / Lean 带 / PCC checker 1,400 行 | 可交付第三方的 qualification dossier，所有 build 有 TV 证书 | **现路线＋生成型 fuzz** | 0.7x, 0.8x |
| **D. LLM writability** | MSR 100%(30/30) / MiniGit 20/20、233 LOC（5 种语言最小） | **MSR 成为赛场**（第三方复现、其他语言同条件） | **主场，成为定义者** | 0.5x |
| **E. 工具链 DX** | 单一二进制＋内置 LSP＋playground | 完备 agent 接口（JSON 诊断＋fix-it＋MCP） | **几乎已拿下，剩余 agent 面** | 0.5x |
| **F. 生态** | MVS＋git deps，无 registry，stdlib 41 模块 959 函数自托管 | 外部 package **由 agent 编写**，通过 MSR gate 并发布 | **工程不能赢，以命题复利竞争** | 1.0 以后 |

---

<a id="軸-a-編集ループ速度--正面から殴ってよい"></a>

## 轴 A：编辑循环速度——可以正面竞争

**当前值**（README 自动生成，2026-08）：`almide check` 22.7ms / warm build 266.1ms / cold build 643.9ms / cold wasm 299.1ms。但**测量对象很小**，单凭这些不能证明规模无关。

**2026-08-13 已实测规模侧**（[#1334](https://github.com/almide/almide/issues/1334)，`scripts/check-edit-loop-scale.sh`）。语料为本仓库 stdlib 本身（手写 303 模块 / 30,624 行，无合成内容），按字节顺序嵌套前缀形成阶梯，每级交错测量 40 次：

| 规模 | `check` p50 | p95 | µs/行 |
|---:|---:|---:|---:|
| 2,047 行 | 18.4ms | 19.5ms | 2.87 |
| 5,456 行 | 31.2ms | 32.3ms | 3.29 |
| **10,151 行** | **52.1ms** | **53.3ms** | 3.78 |
| 20,378 行 | 97.4ms | 99.5ms | 4.06 |
| 30,624 行 | 141.5ms | 145.1ms | 4.06 |

**目标 1 达成（10k 行 p95 53.3ms < 100ms），但不能将 ms 设为门禁**：同一提交、同一机器，load average 6.0 时 p95 52.9ms，load 42.4 时 p95 332.4ms（6 倍）。与 #1337 的 `listbuild` 行遭遇同一陷阱，所以 ratchet 管理的是**同一次运行内无量纲比值**：边际 check 时间的对数—对数斜率 **1.13**（1.0 为线性，2.0 为平方；上述七倍负载变化中只变 0.6%），以及 10k 行级相对空项目底线的 **4.4×**。`run` 受 rustc、链接器成本主导，不列入此阶梯（0.66–0.67 消除这些成本本身）。

**胜利条件**：
1. ~~10k 行 dogfood（0.46/0.57）下 `check` p95 < 100ms~~ **达成（53.3ms，#1334）**；`run` p95 < 500ms 等待 0.66–0.67。
2. debug 路径中 **rustc 与链接器消失**（0.66–0.67）。
3. README 公开数字并由 ratchet 管理（0.69）——**check 侧已达成**。

**依据与取胜路径**：2026 年快速编译案例都在“从开发循环移除 LLVM”（Zig self-build 75s→20s、增量 228–288ms / Roc 3.4s→35ms / tsgo **没有增量也快 10 倍**，其中一半来自原生化、一半来自共享内存并行）。Almide 已是纯 Rust，前半已完成，最初候选为 **Cranelift debug tier ＋ 模块并行 ＋ flat index-based 布局（#1316）**。旧 0.6x 分配已被 2026-09-27 后继阶梯更改，不视为现行发布计划。都是已知工程，不需发明。

**台账行**：0.47–0.49（查询/缓存，但[粗粒度已足够](#査読-047048-はクエリエンジンではない)）；最初 0.61–0.69 方案（cranelift → DWARF → 默认切换 → JIT → 函数级增量）属历史。#1311 为行/秒 ratchet 参考。

**缺少的行**：无（[#1334](https://github.com/almide/almide/issues/1334) 已于 2026-08-13 落地）。0.47/0.49（查询层/模块缓存）是否开工原本等待这些数字：斜率 1.13 不是“不加查询引擎就崩溃”的形态，测量支持 matklad 2026-02 与 Pyrefly 的路线（**粗粒度缓存已足够**）。斜率越过 ratchet 时重新判断。

---

<a id="軸-b-実行時性能--土俵をずらして勝つ"></a>

## 轴 B：运行性能——改变赛场取胜

**当前值**（`docs/project/BENCHMARKS.md`，CI ratchet 下）：n-body 1.135s / spectral-norm 0.685s，**相对手写 Rust 1.00×**；仅 FFT 为 1.18×（2026-08-13 修正输入生成形式，#1338）。wasm 路径约为 native 的 1.13–1.16×。

**2026-08-13 实测更新了本轴的一项判断。** 将 FFT 2^22 按阶段拆解（启动/输入生成/内核，交错 11 次中位数，输出到 17 位一致）：

| 生成形式 | 生成 | **内核** | 总 wall | 内核相对 Rust |
|---|---:|---:|---:|---:|
| 预分配（`list.repeat` ＋ 索引赋值） | 66.4ms | **82ms** | 154.3ms | **0.872×** |
| `list.range \|> flat_map` | 116.0ms | 92ms | 213.9ms | 0.979× |
| Rust 参考（手写） | 39.3ms | 94ms | 138.9ms | 1.000× |
| 当前基准（append 循环） | 69.1ms | 101ms | 175.9ms | 1.074× |

表中两行已在当天（2026-08-13）解决。`list.range |> flat_map` 生成异常高，是因为 **flat_map lambda 为每个元素堆分配中间列表**；将尾部固定长度字面量降为返回 Rust 数组后，与 append 循环打平（[#1337](https://github.com/almide/almide/issues/1337)，以三条 `listbuild` 行常设于 perf suite 并 ratchet 化）。基准自身也改用预分配，FFT 行 1.27× → 1.18×（[#1338](https://github.com/almide/almide/issues/1338)）。**剩下的是 materialize 本身。**

**内核已经领先手写 Rust 13%**（进程启动为 5.9ms 对 5.6ms，无差异，已单独用 hello-world 实测）。即最初“标量 codegen 是天花板”的判断被**测量推翻**：实际瓶颈不是 codegen，而是**列表 materialize（最佳形式也为 Vec 的 1.69×）**。不在指令选择上挑战 LLVM 的方针不变，但 B 的败因集中到一点：[#1004](https://github.com/almide/almide/issues/1004)。

<a id="b1守り-ratchet-維持--materialize-コストの返済--この軸の最優先"></a>

### B1（防守）：保持 ratchet、偿还 materialize 成本——**本轴最高优先级**

维持 `scripts/check-perf-ratio.sh` 文化，缩小 materialize 差距（0.55–0.56 / [#1004](https://github.com/almide/almide/issues/1004)）。如上述实测所示，这已不是“避免输”，而是**获胜的前提**：内核 0.872× 而总 wall 仍 1.111×，原因全在此。

> **2026-08-13 profile 订正**（[string-gap-1004.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md)）：本轴所谓“RcCow 表示成本”并不存在。`String` 以 1:1 降为 Rust `String`，`RcCow` 专用于 Bytes/Matrix（`crates/almide-codegen/templates/rust.toml`）。而且 `listbuild` 的约 1.6× 甚至不是 materialize，全部来自**确定性软件 libm**（`sin`/`cos`）；换成平台 libm 后，已生成代码领先手写 Rust 1.17×。materialize 真正影响的是 **lambda 主体廉价的列表管道**，其中仅 `list.range` 实体化就达 6.6×（30M 元素，`IterChain` 从未在 Rust target 触发）。应缩小的是这个差距。

相关：[#1337](https://github.com/almide/almide/issues/1337)（推荐惯用法反而最慢）、[#1338](https://github.com/almide/almide/issues/1338)（公开基准自身使用最慢形式）。

<a id="b2攻め-意味論が許す最適化で-handwritten-rust-を抜く--1330httpsgithubcomalmidealmideissues1330"></a>

<a id="b2攻め-意味論が許す最適化で-handwritten-rust-を抜く--1330"></a>

### B2（进攻）：凭语义允许的优化超越手写 Rust ← [#1330](https://github.com/almide/almide/issues/1330)

Almide 有 Rust 没有的信息：**值语义、effect 带来的纯性证明、确定性数据并行（fan）、HKT stream fusion（Phase 1–3 已发布）**。手写 Rust 用 `for` 循环的内容，Almide 可“因能证明合法”而融合、并行化、向量化。这与 Fortran 胜 C、Futhark 胜手写 CUDA 是同一方式，**不与 LLVM 竞争**（在 LLVM 上方提供语义）。

**胜利条件（胜利宣言形式）**：至少两个命名工作负载**低于 1.0×**（即 Almide 更快），带 CI ratchet 发布到 README。候选为 map/filter/fold 链（fusion 有效）和 `fan` 确定性数据并行。明确手写 Rust 侧是“人类没有手写同样优化的普通 Rust”（不声称胜过用 `unsafe` 调优的 Rust）。

**2026-09-08 已宣言**（`docs/project/BENCHMARKS.md` “Faster than ordinary Rust”、README native-victory 块、`check-perf-ratio.sh` VICTORY 行）：**binarytrees 0.35× / 0.32×、treealloc 0.31× / 0.30×**（M4 Pro，各两个规模，交错 9 次中位数）；同源设置 `ALMIDE_REGION_OFF=1` 后为 1.25× / 1.11×，全部收益来自 **region window**（#1991，用 bump arena 对 `check(make(d))` 一次回退）。CI runner 为 0.61×（glibc 释放 `Box` 成本低）。门禁保护的是“主张本身”（ratio < 1.0）及消融下界，而非 ± 波段。实测发现**最初两个候选今天不能赢**：native `fan` 顺序执行（`fan.map` 为 `Rc<dyn Fn>` 顺序，`fan { }` 整块只有一个线程，AutoParallel 不看 `RuntimeCall` 因而不起作用），fannkuchredux 0.96–1.06× / mandelbrot 1.01–1.12×；`|>` 链 stream fusion 不在 Rust target 触发（perf README “Not yet covered” 的 6.6×）。宣言明确两个胜例都用同一个机制。下一条路是 `fan` 真并行与启动 IterChain。

> **2026-10-20 补记**：上述两个“今天不能赢”均已解决。native `fan` 真正并行，fannkuchredux 成为第三条 VICTORY 行（`scripts/perf-ratio-baseline.txt` 为 0.326，`ALMIDE_FAN_SEQUENTIAL=1` 消融为 3.07×）。native `|>` 链降为计数 iterator，不构造 `list.range` 的 `Vec`（30M 元素，6.6× → 1.29×，最大 RSS 1.5 MB；perf README “Not yet covered” 的 2026-10-20 重测）。后者仍无 ratchet 行。

<a id="b3別階級-fan--gpu-で桁を変える--1331httpsgithubcomalmidealmideissues1331"></a>

<a id="b3別階級-fan--gpu-で桁を変える--1331"></a>

### B3（另一量级）：fan → GPU 改变量级 ← [#1331](https://github.com/almide/almide/issues/1331)

`--target wgsl` 已存在。既然 `fan` 是确定性数据并行语法，同源降到 GPU 的演示就是“参加另一量级”，不必与 CPU 冠军正面交锋。

**胜利条件**：一个真实工作负载，同一 `.almd` 源在 CPU、GPU 两路给出**相同输出**（3-way oracle 扩展），相比单线程 C 展示数量级差异。

**明确不做**：自行编写优化 pass，在通用标量代码上挑战 LLVM 指令选择。投入产出最差，连 Cranelift 在数值热点上也比 LLVM 慢 15–50%。

---

<a id="軸-c-正しさ信頼--現路線が既に最強圏ただし穴が一つ判明した"></a>

## 轴 C：正确性与信任——现路线已在顶尖范围，但发现一个缺口

**当前值**：契约台账 260 / native、wasm、interp 的 3-way oracle / Perceus Lean 带 / PCC checker 1,400 行（信任基础从 10 万行压缩）/ nightly fuzz / wasm_cross 394 fixture、frontier 0。

外部调查结论：**未发现其他语言项目具有这套组合（差分 oracle＋绑定可执行证据的契约台账＋定理＋PCC）**。CompCert 在 2026-03 的 ATR 42/72 首次取得 DO-178C/DO-333 编译器使用 credit，形成“已证明编译器获得认证信用”的商业先例。0.7x–0.8x 资格化路线有真实市场。

<a id="2026-08-13-に判明した構造的な穴--fixture-では塞げない"></a>

### 2026-08-13 发现的结构性缺口——fixture 无法填补

#1322 根治揭示：**native-v1 与 wasm 共用 almide-mir，MIR 层 bug 能让两路同样出错，394 个 parity fixture 仍全绿通过**。事实上，自 v0.57.0 潜伏的标量 let 别名错误编译正以此途径绕过所有门禁。只有与 v0（`--no-verified`）路径对照才发现。

**结论（本轴最重要项）**：
1. **保留 v0 与 almide-interp 作为“交叉质询者”**。没有不同谱系实现的验证原则上看不见共享层 bug。考虑 v0 退役前，先确定角色承接者。但 interp 弃权很多（cross-target corpus 投票率 281/402 = 69.9%），弃权不是裁决。**仅对标量 Int 控制流，承接者已确定**：下方构造性 oracle 完全不需要第二实现，所以 v0 退役也保留该范围的交叉质询。其他范围（字符串、浮点、集合、effect）未定，v0 与 interp 仍是唯一质询者。
2. **在 linked IR 上做生成型差分 fuzz（Rustlantis 型）**（[#1332](https://github.com/almide/almide/issues/1332)）。fixture 捕获回归，生成器捕获未知分歧。Rustlantis 找到 22 个 bug（LLVM 12 / Cranelift 1）。**第一批落地（2026-08-13）**：`tools/xtarget-fuzz` 的 **identity family**，“从答案倒推”构造程序，把预期输出作为源码字面量。可审判**单一路径**，不只看差分，因此即便两路一致也可判错（对上述结构性缺口有效的唯一判定）。**第二批落地（2026-09-06）**：同样构造时 oracle 的 **composition family**，抽取运算符（`??` `!` `?` `|>` match、元组提取）× payload 类（Int / String / 含 heap 元组 / 记录）× 位置（尾部 / let 绑定 / 调用参数）的笛卡尔积，让字面量 K 往返后打印 K。覆盖手写 spec 只能填“踩到的格子”的运算符交互 bug 族（#1904 #1942 #1926 #1873 #1925）。确定性样本为 `spec/wasm_cross/composition_fuzz_batch.almd`（C-347）。健全性是构造性的：每块有逆运算、生成时计算的修正常数，或分支两臂平衡。另以 `|acc|` 保守上界保证唯一截断运算（`*m` / `/m` 的闭合侧）精确。覆盖仅限标量 Int 控制流（字符串、浮点、集合有意排除——无廉价逆运算）。**首次测量：2,080 个程序 / 0 findings**（seed 13322026081301，25 分钟，reject 0、wall 1）。这不是“正确性证明”，而是**附样本数的实测**，副产物发现一项 wasm subset debt（`int.bxor` 条件性 unlinked）。下一步不是扩展字符串、浮点，而是将 nightly 的一个 shard 固定为 `--family identity`，建立**带 oracle 覆盖的下限**。
3. 0.73 translation-validation 证书恰作用于此层（Alive2 / Crocus 的实绩模式）。

**台账行**：0.71–0.79（证明覆盖 → Critical profile）、0.81–0.89（资格化套件产品化）、项目轨道 #577/#579（企业、监管机构关系）。

---

<a id="軸-d-llm-writability--ホームグラウンド階級の定義者になる"></a>

## 轴 D：LLM writability——主场，成为量级定义者

**当前值**：MSR 100%（30/30，Sonnet 4.6，almide-dojo 测量）。MiniGit Bench（同模型 `claude-sonnet-5`、20 次试验、同提示、同 harness，2026-07-15）**20/20 通过、233 LOC 为 5 种语言最小、agent 完成时间也最短**（[比较表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/README.md#same-model-snapshot-2026-07)）。

**外部状况**：agentlanguages.dev 收录约 40 种 AI 导向语言，设计理念趋同（一致拼写、显式类型、JSON 诊断、契约），但**仅 Almide 有端到端“编辑能否存活”指标**。“modification survival rate”的学术采用案例为零。

**胜利条件（0.59）**：MSR **成为赛场**——第三方可用公开 harness 复现，其他语言也可同条件测量。只要指标仅由自己测量，它就是主张而非排名。

**有效投入（有实证）**：
- **循环胜过语料**：Idris zero-shot 22/56 → 编译错误循环 54/56。Qwen3-4B 获得编译器访问后 18.0% → 97.4%。训练数据不足不能在 pass@1 消除，但可在 MSR 消除。
- **反馈质量排序**：组合 63.6% > 测试失败 57.9% > 最小 53.1% > **原始编译错误 49.2%** → #1312（带 applicability 的 fix-it）、#1313（测试失败结构化）直接影响 MSR。
- **语法约束解码**：低资源语言精度/成本比最佳（CangjieBench）→ #1310（发布 Lark/EBNF）。
- **无人测过 effect 的效果** → dojo#2（A/B）会成为文献首批数字。

**诚实反证**：mame/ai-coding-lang-bench 中，原型规模下动态语言（Ruby/Python/JS）最快最便宜，后加类型检查耗时 1.6–3.2 倍。**初次实现速度不能赢**。因此主张必须是“编辑存活”而非“首次速度”，且需要 0.59 的数字。

---

<a id="軸-e-ツールチェーン-dx--ほぼ手中"></a>

## 轴 E：工具链 DX——几乎已拿下

**当前值**：单一二进制含 `run/build/test/check/fmt/compile/add/deps/dep-path/clean/install/self-update/
explain/docs-gen/fix/ide/repl` 与 **内置 LSP**（hover / completion / documentSymbol / formatting / definition / signatureHelp / codeAction）。playground、VS Code 扩展、tree-sitter 文法、grammar 单一来源。

2026 年基本门槛（Gleam / Astral / Zig / Deno 模式＝一个二进制含 fmt、test、LSP、pkg）**已满足**。

> **订正记录**：2026-08-13 外部研究成果称“LSP 是与 2026 年门槛相比的最大缺口”，这是调查方不知道 `src/cli/lsp.rs` 的错误。LSP 已内置。

**剩余行（agent 接口）**：
- ~~#1312 带 applicability 标签的 fix-it → `almide fix` 诊断驱动化~~ ——**已落地**（2026-09-13 确认：`Applicability` 存在，但机器应用仅 4 code；其余见 [arena-breakthroughs.md T3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md) / #2149）。
- ~~#1313 `almide test` 失败输出结构化（expected/found diff ＋ `--json`）~~ ——**已落地**（`test_report.rs`，`--json` 每失败一个对象）。
- ~~[#1333](https://github.com/almide/almide/issues/1333) **MCP server / Claude Code plugin 定义**~~ ——**已落地**：`almide mcp`（stdio MCP，5 工具）、`tools/claude-plugin/`（MCP ＋ LSP）、`.claude-plugin/marketplace.json`。每个工具仅通过子进程读取 CLI 现有 JSON 输出，不另建 MCP 输出路径（[docs/mcp.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/mcp.md)）。仍未结构化的是 #1313 测试失败详情与 #1312 applicability 标签。
- ~~#1314 内置 snapshot 测试~~ ——**已落地**（`testing.assert_snapshot`、`--update-snapshots`、`--ci`）。

---

<a id="軸-f-エコシステム--唯一工学では勝てない軸"></a>

## 轴 F：生态——唯一不能靠工程取胜的轴

**当前值**：MVS ＋ git 依赖 ＋ lockfile（2026 年形式分析再次确认 MVS 多项式性）。但并非 install/build 不执行代码：`native/`、`[native-deps]` 及其 build script、`@extern` 存在信任边界，依赖的 `[permissions].native` 与 install 的 `--allow-native` 范围不同。参阅 [CLI install](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/cli.md#almide-install) / [package system](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)，不将其说成某蠕虫路径结构上不存在的普遍保证。无 registry。stdlib **自托管** 41 模块、959 函数。

**为何工程不能赢**：库数量是历史的函数，不能用编译器质量换来。按常规路线，Almide 不会迎来拥有一万 package 的一天。

**凭什么竞争——命题的复利**：若 Almide 命题成立，**agent 能批量生产库**。预演已完成（stdlib 41 模块自托管）。下一阶段是“外部 package 由 agent 编写、通过 MSR gate 后发布”的管道，这是其他语言无法构造的非对称性（其 agent 依赖训练数据；训练数据多的语言既有库也多，故无新进入需要）。

**胜利条件（1.0 以后）**：registry ＋“agent 编写、gate 放行、人类批准”的发布管道运行，并有 10 个通过该管道的外部 package。

**此前如何战斗**：坚持到小众腰带（轴 C、D）带来客户。资格化市场（轴 C）**不是按生态大小选择的市场**，它将成为最初收入来源。

---

<a id="依存順序--なぜこの順か"></a>

## 依赖顺序——为什么这样排序

```
軸D (MSR土俵化) ──┐
軸E (agent面)  ──┼→ 0.5x: 指標と道具を先に固める（測れないものは改善できない）
軸C (ファザー) ──┘

軸A (cranelift) ──→ 編集ループの当初案（旧0.6x割付、現在の出荷予定ではない）。flat IR規律(#1316)が前提
軸B2 (意味論最適化) ─┘     RcCow(0.55-56)で表現を確定してから

軸C (証明・資格化) ──→ 0.7x-0.8x: codegen が安定してから積む

軸F (エコシステム) ──→ 1.0以降: 1.0 の信用が前提
軸B3 (GPU別階級)  ──┘
```

保留图中文字译文：轴 D（让 MSR 成为赛场）、E（agent 面）、C（fuzzer）→ 0.5x：先固定指标和工具，不能测量就不能改善。轴 A（cranelift）→ 最初编辑循环提案，旧 0.6x 分配，不是当前发布计划，前提为 flat IR 纪律（#1316）；B2（语义优化）在 RcCow（0.55–56）确定表示后接入。轴 C（证明、资格化）→ 0.7x–0.8x，codegen 稳定后叠加。轴 F（生态）、B3（GPU 另一量级）→ 1.0 之后，以 1.0 信用为前提。

原则与台账相同：**“测量仪器先于手术”**。本文另有一条：**不能用数字写胜利条件的轴，不开工**。不是“变快”，而是“10k 行 p95 < 100ms”；不是“性能获胜”，而是“命名工作负载低于 1.0×”。

---

<a id="明示的に降りる軸"></a>

## 明确退出的轴

| 退出项 | 理由 |
|---|---|
| zero-shot / pass@1 获胜 | 训练数据量主导。原型规模动态语言最快最便宜（mame 实测），赛场是 MSR。 |
| 通用标量 codegen 超越 LLVM | 无人达成。连 Cranelift 在数值热点也慢 15–50%，转投语义优化。 |
| 采用 MLIR | 以纯 Rust 的 `cargo build` 换 C++ monorepo 依赖与不稳定 API。仅核心价值为异构计算的语言选择。 |
| 细粒度查询引擎（salsa 级） | matklad 2026-02 / Pyrefly / rust-analyzer 的迁移税。显式 import 语言粗粒度足够。 |
| 以 stack switching 为前提的并发模型 | 未标准化，WASI 0.3 async 是答案。 |
| 巨大 stdlib | 冻结成本重（维持 GRAND_PLAN 判断）。 |

---

<a id="この文書の維持"></a>

## 本文维护

- 改变某轴胜利条件时，**在同一 PR 更新台账（ROAD_TO_1_0.md）相应 decade**。
- 有新数字时更新当前值栏，并注明来源（README 自动生成块 / BENCHMARKS.md / dojo）。
- 只有 **CI gate 或 ratchet 保护该主张时**才能写“赢了”。
