<a id="road-to-10--065--099-バージョンラダー"></a>

# 通往 1.0 — 0.65 → 0.99 版本阶梯

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ROAD_TO_1_0.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ROAD_TO_1_0.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ROAD_TO_1_0.zh-CN.md)

> 非规范性固定原文历史路线图读者版；固定原文仍是正本且不变。不自动同步，未独立重跑。当前版本、issue数量、发布状态和测量值属于原文各日期，不是今日情况。计划门槛及资格鉴定工作是要求或历史报告，不代表当前达成、独立证明或认证。以not planned关闭不等于完成。8月1日的分支保护命令和发布流程仅作为事故历史保留，不授权执行或修改设置；取消的CI不是成功CI。原文传递性验证的主张仍受所述代码差异为空和后续版本全绿的边界约束。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md)

> **当前 v0.64.0（2026-09-27）**。将从 0.65 到 1.0 的所有 minor 版本按 decade（0.6x 剩余部分 / 0.7x / …）划分为各个阶段，给每个 minor 分配一项“代表性大功能”的台账。**88 个 open issue 已全部分配到本台账的某处**（见下方“台账完整性”）。
>
> **这是一份时间轴文档**。从评价轴（速度 / 性能 / 正确性 / LLM writability / DX / 生态）看同样工作的取胜路径与胜利条件，见 [ALL_AXES.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ALL_AXES.md)。工作单位（Bolt）的粒度见 [v1 Bolt Backlog](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-bolt-backlog.md)，本表各行将这些 Bolt 归入版本。

<a id="2026-09-27-に引き直した理由"></a>

## 为什么在 2026-09-27 重新编排

旧台账（2026-07-30 起草，从 0.41 → 0.99 共 59 行）的顺序是：0.6x = cranelift，0.7x = 证明和 Critical profile，0.8x = 资格鉴定套件，0.9x = 规范性规格。实际进展如下。

- **0.7x〜0.9x 的内容提前关闭了**。flight-grade 阶段（2026-08-27〜28）关闭了 #566 / #567 / #568 / #569 / #570 / #572 / #573 / #574 / #575 / #576 / #776，随后又关闭 #571（08-30）/ #865（09-18）/ #586（09-21）。ALS 规范化 #530 和 interp 规范语义化 #564 也于 08-27 关闭。旧台账引用的约 80 个 issue 中，仅剩 7 个 open（#924 / #1003 / #1005 / #1315 / #1331 / #1388 / #2146）。
- **0.6x 用于 structural 路径，而非 cranelift**。从零新建的 wasm 引擎在 v0.60.0 成为默认，0.61〜0.64 用于收尾（统一借用分析、leak 修复、http layer、`almide survive`）。#1005（cranelift）自创建后没有评论。
- **项目轨道（#577〜#585）在 2026-08-13 关闭（not planned）**。这些属于法律、商业、组织方面，不是本 repo 能承担的工程工作，因此移至 #586 和 [flight-organization](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md)。这不是宣称某事已经完成。

因此，不是逐行修补，而是**从当前 open 的 issue 重新构建 0.65 之后的安排**。0.41〜0.64 的实际记录保留在下方“发布记录”。

<a id="読み方と運用ルール"></a>

## 阅读方式与运作规则

- **decade = 一个阶段**。具有主题和出口门槛。decade 边界（0.70、0.80、0.90）是门槛发布 — 用一次发布固定该 decade 的出口审计。
- **每个 minor = 一行**。补丁版本（0.65.x）不受限制。decade 内版本号前后移动没有问题 — 不变量是 decade 门槛，而非版本号。findings 或 dogfooding 插入时，将现有行在 decade 内后移，**留下注记**（不要悄悄换号）。
- **创建新 issue 时，在同一个 PR 中将其分配到本台账的某处**。不要创建未入账的 issue。
- **Unit 只有在 tag 存在时才关闭**（见下方 2026-08-01 记录）。
- **用 AI-DLC 执行** — Intent = decade / Unit = 行 / Bolt = 工作循环，人只在 Mob 点（`mob` 标签）介入。Bolt 列表及状态见 [v1 Bolt Backlog](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-bolt-backlog.md)。

<a id="順序の根拠"></a>

### 顺序依据

1. **bug > chore > feature，regression 和 release blocker 优先级最高** — 放在每个 decade 的开头
2. **先让交付物具备信任依据** — 证书目前仅在 incumbent 路径上，默认 structural 路径尚无。必须在把证明迁移到 structural **之后**才删除 incumbent（顺序反过来会使证明消失）
3. **性能工作先于尝试次数工作**（2026-09-08 方针）— 表示和速度仍在变化时测 MSR，数字很快就过时。MSR 实测和公开安排在 0.8x
4. **先有测量仪器，再做手术** — perf 仪器（0.77）是解读 native 优化结果的前提。survive A/B（0.84）再解读之后的数字
5. **破坏性的表面修改集中在规范冻结前**（0.95 = edition 行）
6. **最后冻结规范** — 冻结是完成声明，不是愿望

---

<a id="06x-残り--出荷物に信頼を載せる"></a>

## 0.6x 剩余部分 — 让交付物具备信任依据

使默认交付的 structural 路径成为唯一持有证书的路径。退役 incumbent wasm emitter（约 2.6 万行）。**瓶颈是默认路径的证明覆盖**（certified wasm_cross 当前为 789 个中的 1 个，`d9945cd57`）。

| 版本 | 大功能 | Issue |
|---|---|---|
| 0.65 | **清除release blocker与bug**——`process.exit` 126..255（regression）、wasm `bytes.repeat` 与native `matrix.linear_f32_row_no_bias` 漏上限检查（两项I-divergence）、产值分支的 `panic`、误导诊断、static musl binary | [#2780](https://github.com/almide/almide/issues/2780), [#2782](https://github.com/almide/almide/issues/2782), [#2783](https://github.com/almide/almide/issues/2783), [#2769](https://github.com/almide/almide/issues/2769), [#2771](https://github.com/almide/almide/issues/2771), [#2772](https://github.com/almide/almide/issues/2772), [#2777](https://github.com/almide/almide/issues/2777) |
| 0.66 | **HTTP serving前半**（ADR-0020 step 0–4）——经声明fn类型的effect fn值、router墙、E008一般化（epoch 6）、instance-closed serve参数、`kv` 模块、native并发serve | [#2705](https://github.com/almide/almide/issues/2705), [#2696](https://github.com/almide/almide/issues/2696), [#2697](https://github.com/almide/almide/issues/2697), [#2698](https://github.com/almide/almide/issues/2698), [#2699](https://github.com/almide/almide/issues/2699), [#2665](https://github.com/almide/almide/issues/2665) |
| 0.67 | **HTTP serving后半**（ADR-0020 step 5–8）——SIGTERM drain/flush、按HTTP语义比较serve、stock `incoming-handler@0.2` export、LLM docs（最后）。关闭#2705 | [#2692](https://github.com/almide/almide/issues/2692), [#2700](https://github.com/almide/almide/issues/2700), [#2659](https://github.com/almide/almide/issues/2659), [#2701](https://github.com/almide/almide/issues/2701) |
| 0.68 | **structural lowering剩余归零**——清空route census（91行，shrink-only #2741），撤除直达incumbent路径 | [#2743](https://github.com/almide/almide/issues/2743)–[#2751](https://github.com/almide/almide/issues/2751), [#2752](https://github.com/almide/almide/issues/2752) |
| 0.69 | **structural witness阶梯**——call参数、branch/loop frame、closure/effect fn、call-modes、name totality、capability各witness；重生receipt并SHA-256固定每个 `.wasm`；迁移incumbent专属CI gate | [#2755](https://github.com/almide/almide/issues/2755), [#2756](https://github.com/almide/almide/issues/2756), [#2757](https://github.com/almide/almide/issues/2757), [#2758](https://github.com/almide/almide/issues/2758), [#2759](https://github.com/almide/almide/issues/2759), [#2760](https://github.com/almide/almide/issues/2760), [#2753](https://github.com/almide/almide/issues/2753) |
| 0.70 | **gate release——删除incumbent wasm emitter**（one-leg router、撤除 `ALMIDE_WASM_INCUMBENT`、删除 `render_wasm*`） | [#2761](https://github.com/almide/almide/issues/2761), [#2739](https://github.com/almide/almide/issues/2739), [#1696](https://github.com/almide/almide/issues/1696), [#1584](https://github.com/almide/almide/issues/1584) |

**Gate 0.70**：只剩一条 wasm 路径 / C-SAFE、C-PROVEN receipt 来自默认路径的 build / certified wasm_cross 通过 grow-only 棘轮覆盖全部 / ADR-0020 serving 契约在两个路径均成立。

<a id="07x--native-の性能と表現"></a>

## 0.7x — native 性能与表示

消除 wasm 胜过 native 的部分（Map / JSON / Value 为 1.5〜2.3 倍，#2157），让 native 路径的“数字说明”齐备。表示裁定（ADR-0018）排在最前，是因为其后的优化都以该表示为对象。

| 版本 | 大功能 | Issue |
|---|---|---|
| 0.71 | **`Value` 表示**——实施ADR-0018（`Value::Str` = `AlmideStr`，2026-09-08已裁定），消除占decode 68%的字符串复制 | [#1679](https://github.com/almide/almide/issues/1679) |
| 0.72 | **native Map / Value不逊于wasm双胞胎**——包括循环内累积Map时只有native呈二次的问题 | [#2157](https://github.com/almide/almide/issues/2157), [#2393](https://github.com/almide/almide/issues/2393) |
| 0.73 | **native `fan` 并行性**——`fan.map` 顺序、effect callback逐个运行的问题 | [#2044](https://github.com/almide/almide/issues/2044), [#2594](https://github.com/almide/almide/issues/2594) |
| 0.74 | **统一fusion**——作为self-hosted HOF主体特殊化重做fusion，Rust target也融合 `|>` 链；让推荐写法成为最快写法 | [#2289](https://github.com/almide/almide/issues/2289), [#2045](https://github.com/almide/almide/issues/2045), [#2331](https://github.com/almide/almide/issues/2331) |
| 0.75 | **bytes/数值workload**——`string.to_bytes` 为 `Vec<i64>`（lexer 84%耗在malloc/free）、死亡BLAS路径 | [#2078](https://github.com/almide/almide/issues/2078), [#2512](https://github.com/almide/almide/issues/2512) |
| 0.76 | **大小与启动**——native hello 468 KB比较测量、main前0.166 ms、小wasm大部分不是program的问题 | [#2092](https://github.com/almide/almide/issues/2092), [#2162](https://github.com/almide/almide/issues/2162), [#2312](https://github.com/almide/almide/issues/2312) |
| 0.77 | **perf仪表全覆盖**——runtime ratchet至13/13 program、`almide profile`、fuzz perf-class finding及下游4–5%退化 | [#2142](https://github.com/almide/almide/issues/2142), [#2598](https://github.com/almide/almide/issues/2598), [#2302](https://github.com/almide/almide/issues/2302), [#2415](https://github.com/almide/almide/issues/2415) |
| 0.78 | **native Rust生产者统一**（`codegen` 与 `mir/render_native`）——Critical profile native路径也使用它。**Mob** | [#1480](https://github.com/almide/almide/issues/1480) |
| 0.79 | **debug路径决策行**——测rustc是否仍卡编辑循环，决定cranelift（#1005）与wasm DWARF（#1315）开工/不开工；与0.45一样先测再决定，不写代码可能正确 | [#1005](https://github.com/almide/almide/issues/1005), [#1315](https://github.com/almide/almide/issues/1315) |
| 0.80 | gate release——固定native性能审计 | — |

**Gate 0.80**：runtime 棘轮监视 13/13 个程序 / 相同程序中 native 不输给 wasm 对应版本 / 相对手写 Rust 的胜利声明（#1330）由棘轮持续维持。

<a id="08x--エージェント編集ループと-msr-の公開"></a>

## 0.8x — agent 编辑循环与公开 MSR

将 v0.64.0 已发布的 `almide survive` / `repair` 诊断扩展为 agent 使用的整套工具，并用**数字**说明效果。MSR 实测安排在这里（顺序依据 3）。

| 版本 | 大功能 | Issue |
|---|---|---|
| 0.81 | **`almide query`**——hover / definition / references / inlay hints成为CLI子命令 | [#2148](https://github.com/almide/almide/issues/2148) |
| 0.82 | **agent-connect与生成型code action**——语言指南随binary打包，按MSR顺序做missing pattern、label补全、decoder生成、pipe转换 | [#2153](https://github.com/almide/almide/issues/2153), [#2640](https://github.com/almide/almide/issues/2640) |
| 0.83 | **增量基础**——消费 `.almdi` interface及early cutoff、模块级cache、从IR确定entry point | [#2151](https://github.com/almide/almide/issues/2151), [#1003](https://github.com/almide/almide/issues/1003), [#2372](https://github.com/almide/almide/issues/2372) |
| 0.84 | **实测survive A/B**——dojo#58/#59，胜利条件=平均尝试数≤0.8×control；有数字后关闭#2147 | [#2147](https://github.com/almide/almide/issues/2147) |
| 0.85 | **完成契约相对生存arc**——契约保持修改库（dojo#3）的饱和verdict、resource oracle用live-allocation计数器 | [#1998](https://github.com/almide/almide/issues/1998), [#2581](https://github.com/almide/almide/issues/2581) |
| 0.86 | **protocol可作DDD port**——generic protocol、protocol-as-type（#1998 arc末尾）；决定是否用protocol折叠stdlib 119个 `_str` 双胞胎（**Mob**） | [#1589](https://github.com/almide/almide/issues/1589), [#1460](https://github.com/almide/almide/issues/1460) |
| 0.87 | **当前MSR测量**——更新147天旧thesis数字；frontier模型行手动测量（CI不放API key） | [#1963](https://github.com/almide/almide/issues/1963) |
| 0.88 | **公开MSR harness**——其他语言也能同条件运行、第三方可复现；README新鲜度ratchet | [#2146](https://github.com/almide/almide/issues/2146) |
| 0.89 | **补开发体验缺口**——fuzzing/coverage/benchmark标准入口、不重编译的REPL | [#1490](https://github.com/almide/almide/issues/1490) |
| 0.90 | gate release——固定agent编辑循环审计 | — |

**Gate 0.90**：MSR 可由第三方复现，且不超过 90 天 / survive 的效果由受控 A/B 数字呈现（输赢都公开）/ agent 能通过 CLI、MCP、LSP 的同一表面使用 query、survive、repair。

<a id="09x--エコシステム表面凍結10-エンドゲーム"></a>

## 0.9x — 生态、表面冻结与 1.0 收尾

建立外部编写的包能够走到发布的路径，将破坏性表面修改集中到一处后再冻结。

| 版本 | 大功能 | Issue |
|---|---|---|
| 0.91 | **package发布路径**——publish路径、MVS所选版写入lock、从interface artifact生成API docs | [#2332](https://github.com/almide/almide/issues/2332), [#2532](https://github.com/almide/almide/issues/2532), [#2597](https://github.com/almide/almide/issues/2597) |
| 0.92 | **进程与加密**——子进程在自己的group启动、非阻塞poll、整树停止的handle；`process` 变WIT resource host capability；纯Almide AEAD / Ed25519 / HMAC | [#2587](https://github.com/almide/almide/issues/2587), [#2589](https://github.com/almide/almide/issues/2589), [#2596](https://github.com/almide/almide/issues/2596) |
| 0.93 | **Component Model剩余**——host边界determinism、env/process p3、wasi:http@0.3 client、P3 handler export与stock `kv` binding | [#1628](https://github.com/almide/almide/issues/1628), [#1710](https://github.com/almide/almide/issues/1710), [#2702](https://github.com/almide/almide/issues/2702) |
| 0.94 | **已接受ADR剩余**——ADR-0005（operator desugar、移除 `unwrap_or`、正式化 `?.`）、ADR-0021 docs | [#1107](https://github.com/almide/almide/issues/1107), [#2728](https://github.com/almide/almide/issues/2728) |
| 0.95 | **edition行（破坏性变更最后窗口）**——`fan {}` 变并行record literal、非ASCII identifier（替代类型/值大小写分割）、源码声明所有权保证（fip/fbip形）；各项**Mob** | [#1388](https://github.com/almide/almide/issues/1388), [#1456](https://github.com/almide/almide/issues/1456), [#1479](https://github.com/almide/almide/issues/1479) |
| 0.96 | **规格冻结**——语法/operator/stdlib边界最终审计；ALS已规范化（#530），这里声明冻结并使edition政策生效 | — |
| 0.97 | **版本与方言承诺**——将 `proofs/dialect-epochs.toml` 及interface diff（`scripts/check-interface-diff.sh`）文档化并作为1.x兼容承诺生效 | — |
| 0.98 | **已知偏差零审计**——contract ledger `flagged-for-revision` 0（当前0/369）、wall 0、claim-drift 0、fuzz nightly full-budget连续green | [#924](https://github.com/almide/almide/issues/924) |
| 0.99 | RC硬化——全gate green下冻结，只剩1.0发布 | — |

**Gate 1.0**：下方“1.0 的定义”全部成立。

<a id="10-以降"></a>

## 1.0 之后

| Issue | 内容 | 不纳入 1.0 的理由 |
|---|---|---|
| [#1331](https://github.com/almide/almide/issues/1331) | `fan` → WGSL：一份源码生成 CPU / GPU 路径，输出相同 | 增加新的后端。对 1.0 定义中的任何项目都不起作用，冻结后添加也不会破坏兼容性 |

<a id="パッチ枠行を持たない-chore"></a>

## 补丁栏（不占独立行的 chore）

| Issue | 内容 |
|---|---|
| [#2618](https://github.com/almide/almide/issues/2618) | 已盖章台账计数的漂移 — 由发布流程步骤 7 的 `gen-ledger-counts.sh` 消除 |

---

<a id="10-の定義"></a>

## 1.0 的定义

同时列出截至 2026-09-27 的达成状态。

| 项目 | 条件 | 状态 |
|---|---|---|
| **规范** | ALS 具有规范地位（#530），almide-interp 是规范语义（#564），规范已冻结 | 规范化 ✅（08-27），冻结在 0.96 |
| **正确性** | fuzz 始终全绿，hole-hunt findings 为 0，contract ledger flagged 为 0，**默认路径**的每次构建都有 translation-validation 证书 | hole-hunt ✅（#912），flagged 0 ✅，证书仅覆盖 incumbent → 0.69–0.70，fuzz 连续全绿天数为 0 → 0.98 |
| **速度** | 编辑循环不依赖规模，与 Rust 的差距有实测和棘轮管理 | 规模无关 ✅（#1334），棘轮覆盖 7/13 个程序 → 0.77。是否移除 debug 路径的 rustc，在 0.79 决定 |
| **信任** | critical profile + qualification dossier 可以作为产品交付，reference app 提供证据 | ✅（#567 / #571 / #574 / #776）。组织方面（如签署法人）在 repo 之外（[flight-organization](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-organization.md)） |
| **数字** | README 公开 build-speed / runtime-perf / safety 三方面的实测数字 | ✅（#999），带 90 天新鲜度门槛 |
| **指标** | MSR 可由第三方复现 — dojo harness 公开，其他语言也可在相同条件下测量 | 0.88（#2146） |

---

<a id="出荷記録041064"></a>

## 发布记录（0.41〜0.64）

旧台账各行与实际 tag 的对应。**行与版本并非 1:1**（见下方 2026-08-01 记录）。

| 版本 | 日期 | 主要内容 |
|---|---|---|
| v0.41–v0.44 | 07-31〜08-01 | 测量仪器（fuzz nightly、perf scoreboard）、单一 driver（#925）、concurrency 立场及 fan 家族（#1000） |
| v0.45–v0.49 | 08-01 | rtlib 化、query 基础设施、模块缓存经测量决定不启动；完成 10k 行 dogfood（#1001） |
| v0.50 | 08-01 | **门槛** — 三方面数字公开、派生并纳入棘轮（#999） |
| v0.51–v0.53 | 08-02〜03 | 统一 effect 表面规则、JSON interop 决定（#1062）、平台 conformance（Windows、Wasm 3.0） |
| v0.54–v0.56 | 08-06 | 错误表面的裁定与发布 — `-> T!`、移除 auto-`?`、`T?`、移除 try_ 家族 |
| v0.57–v0.59 | 08-11〜27 | 整数域清理（218 个单元）、3-way oracle 87%、通过 protocol 实现静态 DI、`@bounded` profile |
| v0.60 | 08-30 | **structural 路径成为默认**（greenfield Stage 2） |
| v0.61–v0.62 | 08-30〜09-08 | patch train、绑定剩余部分的模式、可执行 docs、wasm 路径的 leak 修复 |
| v0.63 | 09-24 | 单一分析决定借用和 clone，并与输出 IR 对照（#2186 / #2231） |
| v0.64 | 09-27 | 应用前检查编辑（`almide survive`、#2147）、诊断中的 repair、http layer |

旧台账“研究产生的 issue 分配（2026-08-13）”和“各轴路线图产生的 issue 分配”中的 23 项，除 #1315 / #1331 / #1388 外均已关闭。剩余 3 项分配在上方 0.79 / 1.0 之后 / 0.95。旧台账全文保留于 git 历史（`git show 8de6d7473:docs/roadmap/ROAD_TO_1_0.md`）。

<a id="台帳の完全性"></a>

## 台账完整性

- **版本行 35 / 35** — 0.65〜0.99 的每个 minor 都有一行（32 项大功能 + 3 次门槛发布）
- **88 / 88 个 open issue 已分配**（2026-09-27）— 版本行中 86 个，1.0 之后 1 个，补丁栏 1 个
- Issue 栏为“—”的行（门槛发布 0.80 / 0.90、0.96 / 0.97 / 0.99），开工时创建 issue，并在同一个 PR 中补上链接
- 台账与 issue 列表不一致就是债务 — 新 issue 在同一 PR 中分配到这里，关闭后 issue 链接显示 closed，进展便可见

<a id="release-order-deviation-2026-08-01--recorded-so-the-ladder-can-be-read-honestly"></a>

## 发布顺序偏离，2026-08-01 — 留下记录，以便诚实阅读阶梯

**阶梯规则是一个 Unit 对应一次发布，并按顺序进行。** 0.4x decade 期间违反了这条规则，本注记是纠正，而非悄悄改号。

经过：v0.45.0 发布，`Cargo.toml` 升至 0.46.0，随后 **0.47、0.48、0.49 和 0.50** 各 Unit 的工作和记录全部在 **v0.46.0 打 tag 之前进入 `develop`**。因此，源码树在一个尚未发布的版本号下承载了五个 Unit 的改动。

直说原因：Unit 0.45、0.47 和 0.48 通过*测量*而非实现得出结论 — 各自认定“触发条件不成立，数字如下” — 因而产出文档而非构件，而文档让人觉得无需发布。其实需要。发布使一行结论可引用并具有日期；“我们不构建它”这样的测量结论，恰恰需要一个后人可以指向的固定点。

**纠正**：从 **v0.46.0** 起恢复按顺序发布，每行一次，tag 存在前不得称该行已发布。某次发布包含提前合入的记录时，release notes 明说，而不假装顺序一直正确。

**为下次明确规则**：Unit 不是写好 `construction.md` 就算完成，而是 tag 存在才完成。在此前开始下一个 Unit 就造成了此次问题；只有两者保持耦合，阶梯才可审计。

<a id="merged-past-unverified-ci-2026-08-01--the-mechanism-that-should-have-stopped-it"></a>

## 越过未经验证的 CI 合并，2026-08-01 — 本应阻止它的机制

**v0.47.0、v0.48.0 和 v0.49.0 在 CI 从未完成的提交上打了 tag。** 不是失败，而是 **CANCELLED**：每次尚在运行就被下一次合并取代。记录于此，是因为缺少的是机制，而非意愿。

**经过。** 前两个发布 PR 都在明确轮询检查至全绿后才合并。每次发布轮询约 50 分钟，于是剩下三个改用 `gh pr merge --merge --auto`，以为自动合并会等待检查通过。`main` 要求 pull request，但**没有 required status checks**，所以 `--merge` 立即执行，`--auto` 没有起作用。命令返回 `MERGED`，发布随之继续。

**用一个 flag 代替了验证。** 这就是全部故障。五次发布的意愿相同，差别仅在强制执行机制。

**损害经过测量，而非推断**：`git diff v0.49.0 v0.50.0` 限定在 `crates/ src/ stdlib/ runtime/ spec/ tests/ tools/ .github/` 后为**空** — 所有差异都是文档 — 且 v0.50.0（`c75f2ee8`）在 `main` 上全绿。因此这三个 tag 得到了传递性验证，实质上没有未经验证的交付物。每份 release note 现在都说明这一点，而非让他人发现空缺。

**不撤回，原因很重要。** CLAUDE.md 的发布删除流程用于 BROKEN 发布。这些发布并未损坏；它们缺少自身提交上的完整运行，属于流程缺陷。删除会破坏固定到这些 tag 的用户；单独移除 0.49 又会恢复本阶梯原本要避免的 0.48 → 0.50 空缺。

<a id="the-fix-is-a-mechanism-not-a-resolution"></a>

### 修复靠机制，而非决心

**`main` 尚未配置 required status checks，应该配置。** 配置后，无论操作者怎么想，GitHub 都会拒绝 `--merge`：

```bash
gh api -X PUT repos/almide/almide/branches/main/protection --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "Test Rust", "Test WASM", "Emit & Format",
      "Cross-Target (Rust vs WASM)",
      "Coq proofs + axiom audit + PCC gate",
      "WASM host-arch determinism"
    ]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null,
  "required_linear_history": false,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
```

`"strict": true` 还要求分支合并前保持最新，这能抓住问题的另一半：其中两个 PR 打开期间 `develop` 已前进，因此那些检查被取消是有值得揭示的原因的。

这与仓库已经用于其他一切的纪律相同 — contract ledger、棘轮、只减计数。**依赖操作者记住的规则，不算规则。** 配置前，下方发布流程是后备措施，而且严格说来更弱。

<a id="until-then-the-release-procedure-has-one-added-step"></a>

### 在此之前：发布流程新增一步

在“合并”和“打 tag”之间：通过阅读结论，**确认 PR 的每个检查均达到 `SUCCESS`**，而不是相信合并 flag。

```bash
gh pr view <N> --json statusCheckRollup \
  --jq '[.statusCheckRollup[]|select(.conclusion!="SUCCESS" and .conclusion!="SKIPPED")|{name,conclusion,status}]'
```

只有空输出才允许打 tag。`CANCELLED` 算未经验证。
