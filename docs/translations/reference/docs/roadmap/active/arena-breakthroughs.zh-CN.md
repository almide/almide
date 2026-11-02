说明：全轴制胜突破点台账——基于邻近竞技场实测与对 18 个邻近编译器机制的阅读，固定落后轴、成为世界第一所需突破，以及获胜条件、门禁和顺序
<a id="全軸制覇への突破点--アリーナと近隣比較から導いた設計指針"></a>

# 全轴制胜的突破点——由竞技场与邻近比较得出的设计指引

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](arena-breakthroughs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](arena-breakthroughs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](arena-breakthroughs.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/arena-breakthroughs.md)

> 原文历史说明：排名、对手机制、星数、标准/证明描述及因果解释都是原文带日期的调查主张，不是独立验证或当前世界领先证据。此处未复现外部 almide-references 结果及 file:line 引用。并存保留 2026-10-31 导航修正与旧无/未建项主张、09-22 Dojo 行与测量已过5个月的表述、10月重测与9月原表。default/debug 与 release 参数、不同 host/hardware、median 7/9 与 mean 21、own-host 与 stock-WASI、verified 与 wasm-opt、基础与 stdlib 链接 workload 属于不同范围。原文 wasm 4/4 最小与 Float 打印落败矛盾；T6 表的2.5 KB与后文机制≤2,000 B不同，T7 的 < 与后文不高于也不同。均保留，不暗中统一。提案获胜条件、机械检查结果与有限比较不确立普遍安全、资格化或当前完成。历史命令及建项指示是非执行资料。

> **定位**：[ALL_AXES.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ALL_AXES.md)（6 轴获胜条件，2026-08-13）的**后续增量**。轴的定义及“退出的轴”以该文为准；本文只写 **2026-09-13 时点的实测排名**与改变排名**必须突破的点**。台账的 decade 分配见 [ROAD_TO_1_0.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/ROAD_TO_1_0.md)。
>
> **来源**（全部实测，附日期）：`../almide-references/RESEARCH-neighbours.md` §7–§16（2026-09-06～09-11 全 field 竞技场，15 个工具链）；同处 `RESEARCH-championship.md`（2026-08-16，深入阅读 9 个编译器——本文于 **2026-09-13 重验全部 18 项**）；深入阅读邻近 8 种语言的克隆（2026-09-13，file:line 引用见文末附录）；stdlib 链接场景的追加测量（2026-09-13，§2.4）。
>
> **本文与 ALL_AXES 共享两项原则**：“无法用数字写出获胜条件的轴，不着手”，“只有 CI 门禁或棘轮守护该主张，才可以写已经获胜”。本文另加一项原则：**将落败与未测量列在同一张表中**。未测量不是获胜。

---

<a id="0-一枚の結論原調査時点"></a>

## 0. 一页结论（原始调查时点）

**当前导航（2026-10-31 核对）**：下方排名、邻近比较、课题编号属于 2026-09-13 的调查记录（明示的重测为 10 月），不是今天的世界排名。不声称仅凭本仓库就复现确认了外部 `almide-references` 的结果。T2 的 `survive` / `apply` 在 `src/cli/mod.rs` 有实现入口，T3 的结构化 repair 与 mechanical gate 存在于 `src/diagnostic.rs` / `tests/diagnostic_coverage_test.rs`。T8 的分发 attestation / SHA256SUMS 也存在于 `.github/workflows/release.yml`，因此下文“没有”描述的是原调查时点。仅凭这些不能声称各 T 的数字目标（如 Dojo 20%）已经达成，也不将 `query`、独立版本 verifier 等其他提案一概视为完成。

**原调查将证据、性能、大小评为优势，但未证明包括调查外对象在内的无竞争首位。LLM writability 未测量意味着排名未知，而非首位。** 星数（34）为该类别最低，这是症状，不是原因。

将世界第一定义为“所有轴均处于第三方可复现的首位”，突破点可收敛为 9 个：致命（落败则论题不成立）3 个、确立优势 4 个、防御 2 个。

| # | 突破点 | 轴 | 类型 | 今日排名 | 获胜条件（数字） |
|---|---|---|---|---|---|
| **T1** | 让 MSR 成为竞技场——公开 harness、具有区分力的任务集、跨语言同条件 | D | **致命** | 声称首位，**测量已过 5 个月** | 第三方在相同条件下运行 5 种语言，Almide 居首，该数字附棘轮显示于 README |
| **T2** | 存活增量 API——应用编辑前返回“此编辑能否存活” | D/E | **致命** | 无人拥有（邻近 1 家在契约层拥有） | `almide survive`（LSP/MCP/CLI）返回 check+test+contract 差分，Dojo 每 1 轮循环尝试数减少 20% |
| **T3** | 结构化诊断→修复——修复字段必需，机械应用 4/73 → 全 mechanical | D/E | **致命** | 第 2（rustc 之后），机械修复 **4 code** | 声明 `## Fix-it verdict: mechanical` 的全部 code 发出 `MachineApplicable`，含 silent 用例的修复棘轮为绿 |
| **T4** | stdlib 使用自身抽象机制——通过 protocol 退役 123 个 `_str` 孪生项 | D/F | 确立优势 | **第 10/10**（唯一 0 项） | `_str` 孪生棘轮 123 → 0，stdlib 中 `protocol` 声明 ≥ 5 |
| **T5** | 推荐习惯写法 = 最快——全面覆盖写法门禁、启用 fusion、fan 真并行、Map 线性扫描 | B | 确立优势 | 基础测试居首，但**推荐形式慢 1.4–4.3×，stdlib 链接后的 native 在 Map 输 3.7× / JSON 输 2.2×**（§2.4） | perf 全 13 行 idiomatic/imperative ≤ 1.15，CI 裁决 wasm 棘轮，stdlib 链接 3 行受棘轮保护 |
| **T6** | 为大小建立叙事——native 488 KB（输 7–9×）、float printer 3.5 KB、无大小棘轮 | B | 确立优势 | wasm 首位 / **native 中游** | `--release-small` 使 native hello < 200 KB，CI 裁决 wasm 大小棘轮，float 打印 ≤ 2.5 KB |
| **T7** | 增量——无人读取 `.almdi`，native 134 ms 输给 Go 的 92 | A | 确立优势 | **第 10/10**（唯一 0） | 未改模块重新 check 产生 0 IR，native 编辑循环 p95 < Go 同规模构建 |
| **T8** | 信任可移植性——验证器独立二进制、分发签名、抑制不静默、intrinsic 有权限 | C | 防御 | 首位，但**无导出格式** | `almide-verify` 以独立版本裁决证书，一行安装含 attestation，798 个 intrinsic 声明有权限门禁 |
| **T9** | 生态非对称——agent 编写、门禁放行、人批准的公开路径 | F | 防御 | issue **0 项**，docs 在 on-hold 陈旧 | 10 个外部 package 走通该路径（与 ALL_AXES 轴 F 相同） |

T1–T3 是同一件事的三个阶段：**将“LLM 能写”变成第三方可测的数字，并在语言中内置每次编辑都能推动该数字的机制**。没有 T1，T2/T3 只是主张；没有 T2/T3，T1 会被其他语言追上（事实上，邻近一种语言已点名移植 MSR harness：附录 A-5）。

---

<a id="1-今日の順位表2026-09-13全て実測"></a>

## 1. 今日排名表（2026-09-13，全部实测）

<a id="11-アリーナ1314m4-pro出力照合済みmedian-9"></a>

### 1.1 竞技场（§13/§14，M4 Pro，已核对输出，median 9）

> **注（2026-10-12）**：本表 Almide native 列测量的是不带 `--release` 的 `almide build`，所以是 opt-level=1、无 LTO 的值（`src/cli/cargo_build.rs` 的 rlib 快路径）。比较对象 Zig 使用 ReleaseFast，Rust 使用 `-O`。用 `--release` 重测的数值见 §1.1a。

**native，ms**（越低越好）

| | hello | fib(35) | binarytrees d19×12 | spectralnorm n=1200 |
|---|---:|---:|---:|---:|
| **Almide（命令式）** | 2.3 | **15.4** | **51.4** | 39.3 |
| **Almide（推荐习惯写法）** | — | — | 49.7 | **62.9** |
| Rust | 2.0 | 18.3 | 193.7 | 38.8 |
| MoonBit | 2.0 | 19.1 | 62.2 | **38.0** |
| Zig | 2.2 | 19.7 | 66.5 | 38.1 |
| Go | 2.7 | 23.2 | 63.4 | 54.0 |
| Jacquard（经 C） | 1.9 | 50.7 | 61.8 | — |
| Aver（经 Rust） | 2.1 | 79.1 | 287.9 | 836.2 |
| 手写 Rust arena（对照） | | | **31.3** | |

解读：只有 binarytrees 是**机制差异**（region window，#1991），其他排名处于噪声范围。**用推荐写法编写 spectralnorm 会输给 Go**（§15，#2098 已于 09-12 解决——重测是 T5 的首项工作）。

**stock-WASI，ms / bytes**

| | fib | binarytrees | spectralnorm | hello B (+opt) | fib B | btrees B | snorm B |
|---|---:|---:|---:|---:|---:|---:|---:|
| **Almide** | 37.0 | **67.1** | **82.3** | 1,337 (**407**) | **1,144** | **1,924** | 8,585 |
| MoonBit | 36.9 | 195.3 | 93.5 | 2,522 (2,106) | 3,797 | 3,826 | 11,198 |
| Zig（wasm，§11） | **41–45** | 124–126 | — | 46,640 | 47,651 | 48,132 | — |
| Wado `-Os`（own host） | — | — | — | 2,013 | 2,524 | 2,648 | **7,185** |
| vibe（own host） | — | — | 无法完成（heap 4.4 GiB） | 519 | **823** | 2,164 | — |

**native 二进制，KB**：Swift 51（动态链接）· **Jacquard 53** · MoonBit 196–219 · Zig 387–404 · Rust 455–475 · **Almide 457–495** · Aver 467–557 · Go 2,373。

**编译，ms（确定排除缓存的 1 文件）**：Aver→Rust source 6 · **Almide→Rust source 23** · **Almide→wasm 33** · vibe→wasm 56 · rustc -O 59 · MoonBit→wasm 61 · go build 92 · **Almide→native 134** · swiftc 135 · Wado 183 · MoonBit→native 304 · Jacquard 682 · Gleam 728 · Grain 1,918 · Zig 3,612。

<a id="11a-対-zig-再計測2026-10-12gcealmide---release"></a>

### 1.1a 对 Zig 重测（2026-10-12，GCE，Almide `--release`）

在 develop `3763aea`（含 #3592 fusion 与 #3597 region handle）测量。Almide native 使用 `--release`（opt 3、fat LTO、codegen-units=1），Zig 0.17.0 使用 ReleaseFast，Rust 1.99 使用 `-O`，wasm 使用 wasmtime 49。hyperfine 21 次运行的 mean，全部单元格输出已核对。x86 为 c4-standard-8（Xeon 8581C），ARM 为 c4a-standard-8（Neoverse-V2）。

**native，ms**

| | x86 Almide | x86 Zig | x86 Rust | ARM Almide | ARM Zig | ARM Rust |
|---|---:|---:|---:|---:|---:|---:|
| hello | 0.41 | **0.22** | 0.40 | 0.37 | **0.30** | — |
| fib(35) | 19.80 | **18.92** | 21.26 | 17.64 | **17.25** | 17.86 |
| binarytrees | **61.3** | 212.9 | 232.6 | **66.4** | 232.7 | 224.3 |
| spectralnorm 命令式 | **48.40** | 59.49 | 59.77 | 50.14 | **49.81** | 50.16 |
| spectralnorm 习惯写法 C / D / E | 58.7 / 58.9 / 59.8 | | | 76.9 / 76.9 / 76.9 | | |

**stock-WASI（wasmtime），ms**

| | x86 Almide | x86 Zig | ARM Almide | ARM Zig |
|---|---:|---:|---:|---:|
| hello | **2.53** | 3.64 | | |
| fib | **31.79** | 32.72 | **43.98** | 49.94 |
| binarytrees | **73.8** | 137.1 | **89.7** | 173.4 |
| spectralnorm 命令式 | 67.21 | **63.82** | 91.05 | **85.84** |
| spectralnorm 习惯写法 C / D / E | 73.5 / 108.7 / 111.8 | | 106.0 / 130.1 / 131.0 | |

**strip 后大小（x86）**：native 为 Almide 293–337 KB、Zig 546–565 KB、Rust 337–363 KB。wasm（`wasm-tools strip`）为 Almide hello 325 B / fib 495 B / binarytrees 1,741 B / spectralnorm 8,289 B，Zig 67–84 KB。

解读：
- binarytrees 上 Zig 慢，是因为 std 的 `ArenaAllocator` 消耗了 80% 时间。不是语言速度差，而是所用分配器差异。
- ARM 上 `--release` 时，spectralnorm 三种习惯写法比命令式慢 1.53×。原因是 LLVM 循环向量化：通用 aarch64 成本模型将 i64 乘法向量化为逐 lane 搬移的形式。opt-level=1 不出现此差异（#3604）。

<a id="12-工学的性質championship-18-項目2026-09-13-再検証"></a>

### 1.2 工程性质（championship 18 项，2026-09-13 重验）

| 项目 | 08-16 排名 | 09-13 状态 | 证据 |
|---|---|---|---|
| 机械检查的自证明 | 第 1/10，无竞争 | 保持 | Coq 25 文件 0 Admitted，Lean 3 belt 0 sorry |
| compile-speed 门禁 | 第 1 | 保持 | `check-edit-loop-scale.sh` 斜率 1.13 |
| 经验证的交付产物 | 第 1（结构上） | 保持 | 明示 wasm-opt 在验证包络外 |
| fmt 的 AST 保持验证 | 第 1 | 保持 | `verify_format` 始终 on |
| MCP / agent 表面 | 第 1（0/9 拥有） | 保持，但邻近已追上（附录 A） | `almide mcp` 5 工具 |
| 诊断纪律 | 第 2 | 保持，**机械修复 4 code / 73** | `with_machine_fix` 非测试调用 6 处 |
| `list.sort` O(n²) | 第 10 | **已解决** | 6 文件全部 bottom-up mergesort |
| 模式语言（or/as/rest） | 第 10 | **已解决** | `ast.rs:110-136` `As` `Or` `List{rest}` |
| MVS | 第 8（仅文档） | **已解决** | `project_fetch.rs:245-313` 选择 semver 最大值 |
| lock 格式 | 手写逐行格式 | **已解决** | TOML，带引号 |
| snapshot / `test --json` | 未内置 | **已解决** | `snapshot.rs`、`test_report.rs:223` |
| ablation 路径 | 无 | **已解决** | `VICTORY_ABLATION_FLOOR`、`ALMIDE_REGION_OFF` |
| **增量** | **第 10（唯一 0）** | **未着手** | `read_almdi` 调用 **0** |
| **stdlib 使用 protocol** | **第 10** | **倒退**：`_str` 孪生项 119 → **123** | 棘轮冻结，退役 2 方案均 stage 0 |
| LSP 广度 | 第 9 | 部分解决：13 方法，INCREMENTAL sync，有 references/rename | code action **1 种**（Gleam ≈ 60） |
| doc generator | 薄 8× | 仅 `--check`，46 文件手写 | #1469 |
| intrinsic 边界标记 | 无 | 仅 link 门禁，**无声明权限门禁**（798 声明） | `check-intrinsic-boundary.sh` |
| native 大小配置 | — | `strip`/`panic=abort`/`opt-level=z` **均无** | `cargo_build.rs:38-45` |

<a id="13-テーゼの軸llm-writability"></a>

### 1.3 论题之轴（LLM writability）

| 指标 | 值 | 新鲜度 | 问题 |
|---|---|---|---|
| Dojo MSR | 65%（25/38，Llama 3.3 70B）· 44%（17/38，Llama 3.1 8B） | 2026-09-22 | 两次运行均由 harness 标记 `comparable`（全部 38 任务抵达模型＝率是点而非区间），manifest 将固定 seed `20260922`、temperature 0 记录为实测值（`almide-dojo@8af34bc`，无需 CI 或密钥）。Sonnet 行因“不在 CI 放 Anthropic 密钥”的裁决而手动测量，目前不存在 |
| MiniGit 同一模型 | 20/20，233 LOC（5 语言中最小），**573 s / $3.19** | 2026-07-15 | Rust 297 s / $1.45，Ruby 73 s / $0.36——**贵 8.9×，慢 7.8×** |
| 邻近反应 | vibe 点名移植 MSR harness | 2026-09 | 并未垄断指标，现在正是建立竞技场之时 |
| 星 | almide 34 · vibe 35 · aver 60 · wado 116 · jacquard 119 · hexa 196 · vera 413 · moonbit/core 1,205 · gleam 21,907 | 2026-09-13 | 类别最低 |

573 s / $3.19 正是“仅靠编译器循环，让模型编写无训练数据的语言”的费用。**降低这一费用的机制是 T2/T3，展示费用已降低的数字是 T1。**

---

<a id="2-原調査時点の敗北と未計測の台帳"></a>

## 2. 原调查时点的落败与未测量台账

“赢了”的另一面。每行附原因、位置、负责 issue；没有 issue 的行在 §4 建项。

<a id="21-実測で負けている行"></a>

### 2.1 实测落败的行

| 行 | 对手 | 差距 | 原因（实测） | issue |
|---|---|---|---|---|
| native binarytrees | 手写 Rust arena 31.3 ms | 1.6× | 推断 arena 只是回卷区域 1 次，手写采用 `Vec<u32>` 索引 | region-inference.md Phase 3（on-hold）；#1997 `scoped:` |
| native spectralnorm（推荐形式） | Go 54.0 | 1.16× | 捕获 list 的逐 closure clone + `enumerate` 元组实体化（#2098，已解决）。10-12 的 `--release` 重测为 x86 1.21×、ARM 1.53×（均相对自身命令式，§1.1a） | T5；#3604 |
| native spectralnorm 推荐形式（aarch64 `--release`） | 自身命令式 50.1 ms | 1.53× | LLVM 循环向量化导致恶化，关闭向量化为 53–54 ms，`target-cpu=native` 为 52.9 ms | #3604 |
| wasm fib | ~~Zig 41–45 ms~~ | — | 10-12 重测 Almide 更快（x86 31.8 对 32.7，ARM 44.0 对 49.9，§1.1a），留此行以展示经过 | #2150 |
| wasm spectralnorm bytes | Wado `-Os` 7,185 | 1.19× | **float printer 增加 +3,531 B**（§16），math 使用 `f64.sqrt` 无额外成本 | #2099 |
| wasm fib bytes（own-host 级） | vibe 823 | 1.39× | 级别不同（own host import），但差距内容尚未分解 | #2150（级别差分解属于 T6） |
| native 二进制 | Jacquard 53 KB | 7–9× | 源自 backend（bare rustc hello 466 KB），未配置 `strip`/`panic=abort`/`opt-level=z` | #2092（P-low → 提升） |
| native 编译 | Go 92 ms | 1.46× | 路径中有 rustc，答案是 0.6x（cranelift） | #1005 |
| Rust source 生成 | Aver 6 ms | 3.8× | 23 ms 中 frontend 之外的分项未测量 | #2151 |
| agent 费用 | Ruby $0.36 / 73 s | 8.9× / 7.8× | 用编译器循环弥补训练数据 0 | #1963，T2/T3 |
| 星 | gleam 21,907 | 640× | 缺少分发和叙事（§3 T9） | T9 |

<a id="22-実測で未計測の行勝ちではない"></a>

### 2.2 实际尚未测量的行（不是获胜）

| 行 | 状态 | 谁来填补 |
|---|---|---|
| stdlib 链接场景（Map / String / JSON）的速度与大小 | §16 明示“双向均未测量”。棘轮只看 13 行中 7 行（#2142），无大小棘轮（#2141） | **§2.4 已完成首次测量** → #2154 #2155 #2156 #2157，T5/T6 |
| 跨语言同条件 MSR | 仅测量自己 | T1 |
| effect 声明对 MSR 的影响 | dojo#2 应成为文献首个数字，但尚未实施 | T1 |
| 与其他语言的**编译时间**比较（相同 LOC） | 轴 A 获胜条件仅相对自身 | T7 |
| wasm perf 缺口（fft list-write 悬崖 ~3,500×，mandelbrot ~130×） | perf README 写“需先修复”——**无 issue** | T5 |
| `AlmideMap` 线性扫描 | perf README 写“Not yet covered”，map-data-structure-roadmap 将 HAMT 视为 research | T5 |
| 推荐习惯写法是否在全部 13 行与命令式同速 | `IDIOM_CEILING=1.15` 仅用于 listbuild 1 行 | T5 |
| 4 程序中 3 个小于 2.7 KB＝基础测量 | “小”是对 4 文件的观察 | T6 |

<a id="23-近隣が持っていて-almide-に無い機構付録-a-の要約"></a>

### 2.3 邻近拥有、Almide 没有的机制（附录 A 摘要）

| 机制 | 拥有者 | Almide 状态 | 吸收目标 |
|---|---|---|---|
| 推测编辑的**证明增量**（应用前 newly_broken/newly_fixed） | 1 家（LSP 自定义方法） | 无 | **T2** |
| 验证与应用融合为一次调用（显式 `force`） | 同上 | 无 | T2 |
| 修复棘轮（`diag.grep` + **silent 用例出现诊断即 FAIL**） | 1 家（移植 Almide MSR 的一方） | `tests/diagnostics` 仅正例 | **T3** |
| 诊断必需结构字段（cause / 仅一个 next-step / contrast） | 1 家（224 code） | hint 是必需 String，修复是散文 | T3 |
| JSON 诊断含 `Repair{primary, alternatives, example}` | 1 家（经 Rust 的直接竞争者） | `MachineApplicable` 4 code | T3 |
| 无参数 `explain --diagnostic` **列出全部 code**（mnemonic + 状态） | MoonBit | `almide explain E0xx` 只查单项 | T3 |
| 通过 CLI 子命令提供 LSP 功能（hover/definition/references/inlay-hints） | 1 家 | 仅 LSP | T2 |
| 通过 `include_str!` 将语言指南嵌入二进制，在拥有的标记内幂等写出 | 1 家 | `llms.txt` 在仓库侧 | T9 |
| 不静默且有期限的抑制（每次编译重新告知 + commit trailer） | 1 家 | 尚无抑制语法本身（这是好事，但预先决定引入时的形式） | T8 |
| 独立版本的证书验证二进制 | 1 家 | 验证器与编译器捆绑 | T8 |
| 一行 install 含 attestation / SHA256SUMS | 1 家 | 无 | T8 |
| build 时 capability const-fold + DCE（未授权项从二进制消失） | 1 家 | 仅 check 时 deny-all | T8 |
| 公开接口文件（`.mbti` / package-interface JSON） | MoonBit、Gleam | 有 `almide compile --json`，**未作为 diff 对象提交** | T7/T9 |
| 生成类 code action（missing patterns / decoder 生成 / 补 labels） | Gleam（≈ 60） | 1 种 | T3 |
| mutation score 作为公开 badge | 1 家 | 有 `check-mutation-gate.sh`，无公开值 | T8 |
| hosted registry（CLI 提供账户 lifecycle） | MoonBit、Gleam、Wado（OCI） | 仅 git deps | T9 |

<a id="24-stdlib-連結レジームの初回計測2026-09-13"></a>

### 2.4 stdlib 链接场景首次测量（2026-09-13）

用 Almide、MoonBit、Rust 编写 3 个程序（wordfreq＝用 Map 统计 2M 词取 top 10 / strchurn＝20 万字符串的插值→split→trim→to_upper→join / jsonround＝5 万记录的 stringify→parse→Σ），确认全 26 单元格 stdout 字节一致后测量（M4 Pro，median 7，almide 0.62.0，wasmtime 47）。语料与 runner 位于 `../almide-references/bench-sources/stdlib-regime/`，解读见 RESEARCH-neighbours §17。

**native，ms**

| | Almide 推荐形式 | Almide 命令式 | MoonBit | Rust |
|---|---:|---:|---:|---:|
| wordfreq | 507 | 404 | **136** | **79** |
| strchurn | **166** | — | 216 | **82** |
| jsonround | 307 | — | **137** | **129** |

**stock-WASI，ms / bytes（verified → `--wasm-opt`）**

| | Almide | MoonBit（→ `wasm-opt -Oz`） |
|---|---:|---:|
| wordfreq 推荐形式（`group_by`） | **22,016** · 7,472 → 5,647 B | 262 · 16,132 → 11,380 B |
| wordfreq 命令式（`var` Map） | **201** · 8,210 → 6,239 B | （同一单元格） |
| strchurn | **166** · 28,153 → 25,713 B | 726 · 22,010 → 14,564 B |
| jsonround | **132** · 25,575 → 17,704 B | 477 · 71,395 → 51,590 B |

**解读（4 点，全部已建 issue）**：
1. **native 与 §13 反转**：链接 stdlib 后，Map 输 MoonBit 3.7×、JSON 输 2.2×，对 Rust 则全行输 2–6×。仅 strchurn 获胜。“首位圈”说的是基础程序和 allocation 行。
2. **wasm 反而优势明确**：比 MoonBit 快 1.3× / 4.4× / 3.6×，bytes 为 0.5–1.3×。“小”在链接场景仍成立，“快”是新事实。
3. **wasm 优势也是对 native 的控诉**：同一程序在 wasmtime 比 native 快 1.5–2.3×（wordfreq 404 → 201，jsonround 307 → 132）。无 Map 对照循环在 native 为 163 ms，所以 native 的 `map.get_or` + 索引赋值执行 2M 次需 ~240 ms；结构路径的 Map 完整程序只需 201 ms。→ **#2157**（native Map 索引、Value 表示 #1679）。
4. **推荐写法在 wasm 是陷阱**：`list.group_by` 在结构路径每元素 ~11 µs（22 s 对 201 ms，对 N 线性，输出相同）→ **#2156**。元组 sort key `(0 - c, w)`（count 降序、word 升序的唯一写法）**在结构路径受阻，在 incumbent 误编译**：`almide check --target wasm` 显示 "No errors found"，build 显示 "verified"，wasmtime 在 `list.sort_by_rc` 触发 `indirect call type mismatch` trap，而 native 打印答案 → **#2154（I-divergence，release blocker）**；wall 与 `list.group_by_x` 未链接为 **#2155**。

**现状（2026-10-20 追加，上表与解读保留为 09-13 记录）**：
- 解读 1 的 wordfreq 已解决。native wordfreq 相对 Rust 为 0.97（`scripts/perf-ratio-baseline.txt`，#3507 后重新锚定：GCE c3-standard-8 为 0.98，c4a-standard-8 为 0.92）。jsonround、strchurn 对 Rust，以及与 MoonBit 的比较尚未重测。
- 解读 3 已解决。同程序 native 时间 ÷ wasm 时间：wordfreq 0.46、wordfreq-group 0.39、jsonround 0.69（GCE c4-standard-8，2026-10-19，`--quick --runs 9`），native 更快。`check-perf-ratio.sh` 以 1.0 为上限在每次 CI 监视（执行机器的波动见 #3873）。

**对 T5/T6 的结论**：将这 3 程序纳入 `research/benchmark/perf/` 棘轮行（正是 #2142 指出的未监视 6 行），大小阶梯（T6）的 String / Map / JSON 阶段由这 3 程序填补。“全轴压倒性领先”在 native 侧的剩余债务是 **Map 与 Value 数据结构，而非 codegen**。

---

<a id="3-原調査からの設計指針実装状態は-0-の現行ナビゲーションも参照"></a>

## 3. 原调查的设计指引（实现状态也参见 §0 当前导航）

每项包括：**为何致命 / 机制 / 获胜条件 / 门禁 / 依赖 / 建项**。已有 roadmap、issue 覆盖的部分仅给指针，正文只写本文新决定的内容。

<a id="t1-msr-を土俵にする致命"></a>

### T1. 让 MSR 成为竞技场（致命）

**为何致命。** 使命是“LLM 最能准确编写的语言”，但该指标只有自己在 5 个月前测过。如今邻近已移植 harness，**定义者的位置转移给“先公开的一方”**。ROAD_TO_1_0 的 0.59 行 Issue 栏仍是 `—`（无人负责）。

**机制。**
1. **具有区分力的任务集**（#1963）：MiniGit 在 5 语言均 20/20，已饱和。任务不是“首次实现”，而应是**“对既有代码进行 N 次规格修改后的存活”**，难度要能区分语言正确率。难度指标为“最强模型下 60–80% 存活”。
2. **跨语言同条件**：将 Dojo runner 语言插件化（Almide / Rust / Go / TypeScript / Zig / Gleam / MoonBit）。Zig std 从 0.15→0.16 改变，导致依训练数据编写失败（§7 实见），可实证 MSR 能区分“训练数据量”与“语言稳定性”。
3. **第三方复现**：一个命令 `git clone && make msr`，manifest 记录模型、prompt、温度、seed，结果 JSON 提交到 dojo。
4. 将 **effect A/B**（dojo#2）作为相同 harness 的一种条件同梱——文献首个数字。
5. **README 数字是棘轮**：在 `check-readme-numbers.sh` 添加 msr 行，新鲜度超过 90 天即红。

**获胜条件。** 存在第三方在相同条件运行至少 5 语言的表格，Almide 居首，README 数字在 90 天内。

**门禁。** Dojo PR gate 在本仓库 CI 运行 subset（CLAUDE.md “The bridge”）。

**依赖。** T2/T3 负责推动数字。顺序是 T1 任务集 → T3 → T2 → T1 重测。

**建项。** #1963 / #1617 对应任务集与重测。**公开 harness（跨语言同条件、第三方复现）**尚未建项 → #2146。

<a id="t2-生存デルタ-api致命"></a>

### T2. 存活增量 API（致命）

**为何致命。** 扼杀 MSR 的失败是“未经验证便应用的编辑”。邻近一种语言在 LSP 有 `speculativeEdit`（应用前证明增量）和 `proposeEdit`（验证→应用融合），这是 **MSR 的 API 化**。Almide 没有这种形式，而是分别调用 `almide check` 与 `almide test`。

**机制。**
1. `almide survive <file> --with <patch-or-full-text> --json`（CLI）：**在内存中**对当前源码树应用编辑，以 `{unchanged, newly_broken, newly_fixed, removed}` 返回 `check`（诊断差分）、`test`（通过/失败差分）、契约（`// @contract:` fixture 差分）。不写入。
2. 同一函数接到 **LSP 自定义方法** `almide/survive` 与 **MCP 工具** `almide_survive`。MCP 5 工具均采用子进程读取现有 CLI JSON 的方针（docs/mcp.md），所以先做 CLI。
3. `almide apply --if-survives`（显式 `force`，非 boolean 失败关闭）：一次完成验证与应用。
4. `almide query hover|definition|references|inlay-hints --line --column --json`：通过 CLI 暴露已有 LSP handler。**agent 编辑前了解类型**，由诊断失败变为预防失败。

**获胜条件。** 使用 `survive` 时 Dojo 每任务平均尝试次数比不使用时少 20%（同一模型，A/B）。

**门禁。** `survive` JSON schema 含 `schema_version`；fixture 将 3 个已知编辑的预期增量固定为 golden。

**依赖。** T3 结构化诊断构成 `newly_broken` 内容。

**建项。** 全部尚未建项 → #2147、#2148。

<a id="t3-診断--修復の構造化致命"></a>

### T3. 结构化诊断→修复（致命）

**为何致命。** ALL_AXES 轴 D 引用文献给出的反馈质量排序是：复合 63.6% > 测试失败 57.9% > 最小 53.1% > **原始编译器错误 49.2%**。Almide 通过类型要求 hint（`hint: String`），但**修复是散文**，73 code 中机械应用仅 4 个（E013 E031 E049 E052）。声明 `## Fix-it verdict: mechanical` 的 doc 有 6 个，但 gate 只是 soft-report。

**机制。**
1. **将修复字段 schema 化**：`Diagnostic { hint, repair: Option<Repair{primary, alternatives, example}> }`。`mechanical` verdict 的 code，若 `repair.primary` 不是 `MachineApplicable`，就使 `diagnostic_coverage_test.rs` **hard fail**（去掉当前 `:234` 的 soft 行为）。
2. **修复棘轮（双向）**：除 `tests/diagnostics/<code>/repair.grep`（诊断应含的词）外，增加 `tests/diagnostics/silent/*.almd`——**本应通过编译**的程序。出现新诊断即 FAIL。成族覆盖“被静默接受的错误”（#2097 族、vera 右→左槽、jacquard W0301 族）。
3. `almide explain --list --json`：以 `code | mnemonic | severity | since | fix-it verdict` 列出全 code。`since` 为版本（`_since` 表）。agent 无需先知道 code 即可定位。
4. 将**生成类 code action**从 1 种扩充。Gleam ≈ 60 项中，按 MSR 效果排序：`Add missing patterns`（与穷尽性检查器同源）、`Fill labels`、`Generate decoder`、`Convert to pipe`。是否将 on-hold/lsp-code-actions.md 返回 active，由此行决定。

**获胜条件。** 声明 mechanical 的 code 100% 可机械应用，silent 用例 ≥ 30 个且绿灯，`explain --list` 行数 = `with_code` 的 distinct 数（drift 门禁）。

**门禁。** 为 `check-diagnostic-code-coverage.sh` 添加 repair/silent。

**建项。** #2097 为单个实例。schema 化、silent 棘轮、`explain --list` 尚未建项 → #2149。

<a id="t4-stdlib-が自分の抽象機構を使う優勢確定"></a>

### T4. stdlib 使用自身抽象机制（确立优势）

**原因。** championship 称之为“codebase 最大的 MSR 缺陷”。添加 `list.chunk_by` 的模型必须猜测 5 个表示后缀该写哪一个。数字从 119 倒退到 **123**，退役 2 方案（selfhost-link-v2.md、protocol-any-existentials.md）均 stage 0。

**机制。** 本文不新增机制，而是**决定顺序和期限**：
1. 将 selfhost-link-v2 stage 1（parity harness＝反证器）与 **T5 写法门禁扩展放在同一 PR 群中落地**（两者都测量“同一 workload 的不同写法给出相同输出”，可共享仪器）。
2. 在 `scripts/str-twin-baseline.txt` 旁写明收缩计划：`_str` 棘轮 baseline **每季度 −25%**（棘轮不只是冻结，还强制下降）。
3. stdlib 首次出现 `protocol`，从 `Encode`/`Decode`/`Numeric` 的**文档化**开始（protocol-any pre-step）。

**获胜条件。** 123 → 0。stdlib 中 `protocol` 声明 ≥ 5。

**建项。** 由 #1460 / #1589 承担，在 #1460 评论追加收缩计划。

<a id="t5-推奨イディオム--最速優勢確定"></a>

### T5. 推荐习惯写法 = 最快（确立优势）

**原因。** §15 教训：“风格指南是**什么也不测的承诺**”。CLAUDE.md 推荐写法慢 1.4–4.3×，而使用它的语料遵循指南，因而无人察觉。#2098 已解决，但门禁仅覆盖 listbuild 1 行。

**机制。**
1. **全面覆盖写法门禁**：perf 13 行中所有有意义的行均提供 `imperative` 与 `idiomatic` 两种写法，逐行应用 `IDIOM_CEILING=1.15`。将 §15 spectralnorm 4 写法原样常设于 `spectralnorm/`。
2. **CI 裁决 wasm 棘轮**（#2143）：`RATIO_VERDICT=0` 的理由（跨引擎比值不能抵消 hardware）是正确的，因此只裁决 **wasm/wasm 同次运行内比值**（idiomatic/imperative、fan/sequential、region on/off），与轴 A 无量纲化相同。
3. **棘轮覆盖 7/13 → 13/13**（#2142）：优先 Map / JSON / String 的 6 行，将 §2.4 三程序移入 perf suite。
4. **启用 fusion**（#2045，0.56 行）和 **fan 真并行**（#2044）——已有 issue。先 fusion（决定推荐 `|>` 链速度的是它）。
5. **`AlmideMap` 线性扫描**：未建项。native `Vec<(K,V)>` lookup 在保持 insertion-ordered 的同时加索引（compact-ordered-dict 的 hash 索引侧）。wasm 已为 O(1)。§2.4 wordfreq 行是判定器。
6. **wasm perf 两个悬崖**（fft list-write ~3,500×、mandelbrot ~130×）：未建项，是棘轮前提。
7. **wasm fib 调用成本**（输 Zig 1.2–1.3×）：未建项，通过 disasm 分解结构路径 call 形式。
8. **stdlib 链接场景**（§2.4）：`group_by` 的 wasm 常数（#2156）、native Map/Value（#2157）、元组 sort key 的 wall 与 divergence（#2155 / #2154；后者是 release blocker，所以先做）。

**获胜条件。** 全 13 行 idiomatic/imperative ≤ 1.15，3 种 wasm 同次运行内比值可以让 CI 变红，stdlib 链接 3 行受棘轮保护，2 个 wasm 悬崖 ≤ 3×。

**建项。** #2045 #2044 #2142 #2143 已有。Map 线性扫描、wasm 悬崖、wasm fib call → #2150。链接场景 → #2154–#2157。

<a id="t6-サイズに物語を持つ優勢確定"></a>

### T6. 为大小建立叙事（确立优势）

**原因。** wasm 4/4 最小，但 **4 程序中 3 个小于 2.7 KB**，只测了“底座”。唯一打印 Float 的行落败（§16）。native 为 488 KB，输给 53 KB 邻近 7–9×，emit profile 没有 `strip`/`panic=abort`/`opt-level=z` 任何一项（`cargo_build.rs:38-45`）。

**机制。**
1. **`almide build --release-small`**（或 `[profile.small]`）：`opt-level="z"`、`lto="fat"`、`codegen-units=1`、`panic="abort"`、`strip=true`。bare rustc hello 为 466 KB，故**先测量** backend 底座能降多少（满足 #2092 的“measured against anything”）。目标 hello < 200 KB（MoonBit 级）。53 KB 级涉及 libc 动态链接或 `no_std` runtime，与 flight-subset（`--profile critical`）的 `no_std` 化汇合；此处不以此为目标。
2. **wasm 大小棘轮**（#2141）：将 `research/benchmark/perf/wasm-size/` 的 5 程序替换为“逐步引入 stdlib 的阶梯”——hello / Int 打印 / Float 打印 / String 操作 / List / Map / JSON / regex。每阶的 verified 与 `--wasm-opt` 均 shrink-only。
3. **float printer**（#2099）：3,531 B → ≤ 2,000 B。自行以 wasm 编写 Ryu/Grisu shortest-roundtrip，将 Dragon4 移往 `--exact` 路径。邻近同项为 +4,326 B，故 −28% 即可胜过。
4. **RC helper 无条件同梱**已由 #1962 解决。`hello` 剩余 251 B 不纳入叙事（放弃）。

**获胜条件。** native hello < 200 KB（small profile）、大小阶梯 8 阶受棘轮保护、spectralnorm wasm ≤ 7,000 B（低于 Wado `-Os` 7,185）。

**建项。** #2092（P-low → P-high）、#2141、#2099。small profile 追加至 #2092。

<a id="t7-インクリメンタル優勢確定"></a>

### T7. 增量（确立优势）

**原因。** 9/9 参考编译器都有逐模块重建，只有 Almide 为 0。`.almdi` 已写出，但 **`read_almdi` 调用方为 0**。另一方面轴 A 规模无关性已达成（斜率 1.13），即 **check 已够快，慢的是包含 rustc 的 native 134 ms**，输给 Go 的 92 ms。

**机制。**
1. **粗粒度足够**（维持 ALL_AXES 判断：不采用 salsa 级）。编写 `.almdi` 消费者：近邻的四部分谓词（config digest 一致 ∧ source 比 artifact 旧 ∧ 所有 import 的 interface digest 一致 ∧ artifact 可读）+ **interface digest 相同则不写**（不改变 mtime、阻断下游工作的 15 行 early cutoff）。测试约 26 个 in-memory unit test，而非启动进程。
2. **native 134 ms** 只有在 0.6x（cranelift，#1005）才能消除。本文只决定顺序：**T7-1（`.almdi` 消费）在 0.5x 内落地，cranelift 保持 0.6x**。
3. 用 `--timings` 输出 **Rust source 生成 23 ms 的分项**（与 Aver 6 ms 的差 17 ms，尚不知属于 frontend 还是 render）。
4. 将**同 LOC 跨语言编译时间**列为竞技场常设行（让 §13 表能由 `bench-sources/` runner 复现，并纳入 `check-readme-numbers.sh` 新鲜度检查）。

**获胜条件。** 未改模块重新 `check` 不产生任何 IR（以 `--timings` 确认），native 编辑循环 p95 不高于 Go 同规模 `go build`。

**建项。** #1003 为逐模块 cache，#1005 为 cranelift。`.almdi` 消费者和 `--timings` 作为 #1003 子项 → #2151。

<a id="t8-信頼の可搬性防衛"></a>

### T8. 信任可移植性（防御）

**原因。** 证据轴无竞争，但**没有导出形式**：验证器捆绑编译器，分发无签名，798 个 `@intrinsic` 声明也可由用户代码编写（绕过 capability 叙事）。直接竞争者将验证器做成**独立版本的另一二进制**，`cert verify` 仅为子进程 shim。

**机制。**
1. **`almide-verify`**：将 PCC checker（1,400 行）分成独立 crate、版本、二进制。`almide verify` 仅 shim，无链接 fallback。这样 trust-layer.md L3 的“第三方执行 `make verify`”也能适用于二进制分发。
2. **一行 install 含 attestation**：release.yml 提供 `SHA256SUMS.txt` + GitHub artifact attestation，README 一行中包含 `gh attestation verify --repo almide/almide`。
3. **intrinsic 声明权限**：`@intrinsic` 仅在 `stdlib/` 与 `runtime/` 下 module 合法，其他地方报新 E-code（hint：“intrinsic 放在 stdlib，用户代码用 effect fn 包装”）。`check-intrinsic-boundary.sh` 是 link 侧，此为声明侧。
4. **预先决定抑制形式**：Almide 没有抑制语法（这是好事）。未来若引入相当于 `@allow` 的机制，先将 4 条件写成 ADR：**不静默**（每次编译重告知）、**有期限**（`until=`）、**必须给理由**、**commit trailer 中 ack**。
5. **公开 mutation score**：将 `check-mutation-gate.sh` 结果刻印于 `proofs/`，成为 README badge。
6. **capability 的 build 时 DCE**：用 `check-wasi-pins.sh` 断言 `--profile critical --allow IO` 拒绝的 capability import 从 wasm **消失**（目前仅 check 时 deny）。

**获胜条件。** `almide-verify` 独立发布、README 一行含 attestation 验证、用户代码声明 `@intrinsic` 报 E-code、mutation badge、DCE 断言门禁。

**建项。** 全部未建项 → #2152。

<a id="t9-エコシステムの非対称防衛"></a>

### T9. 生态非对称（防御）

**原因。** ALL_AXES 轴 F 获胜条件（agent 编写、门禁放行、人批准 → 外部 10 个 package）有 **0 issue**，on-hold 两文已陈旧。34 星是缺少分发的症状。邻近具备“语言指南内嵌二进制并幂等写出”“每提交 benchmark 公开至 Pages”“skills 独立仓库”。

**机制。**
1. **`almide agent-connect`**：用 `include_str!` 将 `llms.txt` 与 CHEATSHEET 嵌入二进制，写到 `.claude/skills/almide/SKILL.md` 与 `AGENTS.md` 的**标记区间**。标记不一致则点名拒绝，不猜测；重跑无差分。**编译器与 prompt 版本错位在结构上变得不可能**。
2. **公开接口文件**：将 `almide compile --json` 输出以 `almide.interface.json` 提交，作为 `check-interface-diff.sh` 输入（把发布时已使用的索引转为逐 PR diff 对象）。
3. **agent 编写 package 的路径**：`almide/pkg-template` + Dojo MSR gate + 人批准 = publish。registry **先用 GitHub org + tags**（MVS 使用 git ref 足够），外部 10 个 package 之后再 hosted registry。
4. **公开每提交 benchmark**：将 `research/benchmark/perf/results/*.json` 绘于 Pages，让外界看见“数字变化”。

**获胜条件。** 与 ALL_AXES 轴 F 相同（10 个外部 package 走通路径）。中间指标：使用 `agent-connect` 的外部仓库数。

**建项。** `agent-connect` 与 package 路径未建项 → #2153。

---

<a id="4-起票2026-09-13"></a>

## 4. 建项（2026-09-13）

与本文同时建立的 issue（2026-09-13）。

| § | 题目 | 类型 | 突破点 |
|---|---|---|---|
| 4-1 #2146 | 公开 MSR harness：5 语言同条件运行、第三方可复现、README 新鲜度棘轮 | enhancement, mob | T1 |
| 4-2 #2147 | `almide survive`：在 CLI、LSP、MCP 上提供建议编辑应用前的存活增量（check + test + contract） | enhancement | T2 |
| 4-3 #2148 | `almide query`：基于现有 LSP handler，以 CLI 子命令提供 hover / definition / references / inlay hints | enhancement | T2 |
| 4-4 #2149 | 修复作为 schema 字段、含 silent 用例的双向修复棘轮，以及 `explain --list` | enhancement | T3 |
| 4-5 #2150 | native Map 为线性扫描，两处 wasm perf 悬崖无 issue，wasm fib 承担其他比较对象没有的调用成本 | enhancement | T5 |
| 4-6 #2151 | 消费 `.almdi` 接口产物：四部分新鲜度谓词与 digest 不变时 early cutoff | enhancement | T7 |
| 4-7 #2152 | 可移植信任：独立版本验证器二进制、install 行 attestation、`@intrinsic` 权限门禁 | enhancement, proof | T8 |
| 4-8 #2153 | `almide agent-connect`：将语言指南随二进制交付，在拥有的标记下幂等写出 | enhancement | T9 |

由 §2.4 测量建项（同日）：

| 题目 | 类型 | 突破点 |
|---|---|---|
| #2154 元组 sort key 在 incumbent 保持 "verified" 却触发 `indirect call type mismatch` trap，native 输出答案 | **I-divergence**, P-high | T5（release blocker） |
| #2155 推荐 word-count 形式在 stock WASI 无法 build：元组 key wall + `list.group_by_x` 未链接 | enhancement | T5 |
| #2156 `list.group_by` 在结构路径每 1 元素 ~11 µs（命令式的 110×） | enhancement, P-high | T5 |
| #2157 native Map 与 Value 比 wasm 孪生实现慢（同程序在 wasmtime 快 1.5–2.3×） | enhancement, P-high | T5 |

已有 issue 追加：#2092（P-low → P-high、small profile 测量步骤）、#1460（收缩计划）。

---

<a id="5-順序--何を先になぜ"></a>

## 5. 顺序——先做什么，为什么

```
T1 課題集(#1963) ─┐
T3 修復スキーマ+silent ratchet ─┼─→ T2 survive API ─→ T1 公開ハーネス再計測   … 致命の 3 つは一本の鎖
                                 │
T5 綴りゲート全面化 ─→ T4 stage 1(同じ器具) ─→ fusion(#2045) ─→ fan(#2044)
T6 size 梯子 + small profile ─→ float printer(#2099)
T7 .almdi 消費(0.5x) ────────────────────────→ cranelift(0.6x, 据え置き)
T8 / T9 は並走可（コンパイラ本体を触らない）
```

保留图示的译文：T1 任务集（#1963）与 T3 修复 schema + silent 棘轮共同通向 T2 survive API，再通向 T1 公开 harness 重测；3 个致命项是同一条链。T5 全面覆盖写法门禁 → T4 stage 1（同一仪器）→ fusion（#2045）→ fan（#2044）。T6 大小阶梯 + small profile → float printer（#2099）。T7 .almdi 消费（0.5x）→ cranelift（0.6x，保持不变）。T8 / T9 可并行推进（不改编译器本体）。

**致命链优先**。原因见 §1.3 一行：573 s / $3.19 是其他语言的 4–9 倍，不降低它，“LLM 最能准确编写”就没有数字支撑。性能、大小、编辑循环已在首位圈，保持不退即可（有棘轮）。

---

<a id="6-降りる軸の再入条件"></a>

## 6. 退出轴的重入条件

ALL_AXES 的“明确退出的轴”表未给重入条件，此处补上。

| 退出的轴 | 考虑重入的测量 |
|---|---|
| zero-shot / pass@1 | Dojo 出现 pass@1 与 MSR 排名**一致**的任务集时（＝pass@1 成为 MSR 的廉价代理） |
| 通用标量 codegen 超过 LLVM | 竞技场噪声范围（fib / spectralnorm）内，**相同机制导致**的落败在至少 2 机型复现时 |
| 采用 MLIR | `--target wgsl`（#1331）在 1 个真实 workload 给出与 CPU 相同输出，且需要下一 backend 时 |
| salsa 级查询引擎 | `check-edit-loop-scale.sh` 斜率超过 1.5，或 T7-1 粗粒度 cache 无法将 dogfood 全构建降至 2–3 s 以下时 |
| 以 stack switching 为前提的并发 | WASI 0.3 async stable，且 fan 真并行（#2044）只加速 native 时 |
| 巨大 stdlib | T9 外部 package 路径已通过 10 个，并证明能在此吸收 stdlib 新增需求的 8 成时 |

---

<a id="7-この文書の維持"></a>

## 7. 维护本文

- 所有数字附日期。更新 §1 表时必须并列来源（RESEARCH-neighbours §编号 / BENCHMARKS.md / dojo）。
- 只有 §3 各项的**门禁**已能使 CI 变红，才可写突破点“已达成”。
- ALL_AXES 获胜条件变更时，在同一 PR 更新本文表 0，反向也一样。
- 使用 `bench-sources/` runner 重测邻近。**未写明排除 cache 的 compile 时间不可信**（§13 陷阱）。

---

<a id="付録-a-近隣機構の引用2026-09-13-深読みr--almide-references"></a>

## 附录 A. 邻近机制引用（2026-09-13 深读，`R = ../almide-references`）

本文目的不是点名批评邻近，故仅以 file:line 留下 **Almide 所吸收机制的位置**。星数和 tip 日期来自 2026-09-13 的 `gh api`。

**A-1. 直接思想竞争者（经 Rust、Lean 证书）**——`R/aver/`
- 通过 `include_str!` 内嵌指南，`agent-connect` 在拥有的标记下幂等写出：`src/main/agent_connect.rs:29-46, 380-395`
- 诊断包含 `Repair{primary, alternatives, example}` / `conflict` / `related` / `intent`：`src/diagnostics/model.rs:63-105`
- 128 行 slug 目录（Fires when / Repair 列）：`docs/diagnostics-slugs.md`
- NDJSON + `schema_version` + 末尾 summary：`docs/diagnostics-schema.md:25-37`
- `context --budget 10kb --json`（带 token 预算的结构图）：`docs/cli.md:218-247`
- `audit --hostile`（敌对 world 单列 slug）：`docs/cli.md:210-217`
- 独立版本证书验证二进制，本体为 shim：`docs/cli.md:330-372`、`aver-cert/src/verifier.rs`
- `--explain-passes --json`（逐函数 alloc-free / TCO 可由 CI 断言）：`docs/cli.md:315-325`
- 整数除法是返回 `Result` 的函数，不是运算符：`docs/diagnostics-slugs.md:25`

**A-2. 在契约层将 MSR API 化的语言（无变量名）**——`R/vera/`
- `speculativeEdit`（应用前证明增量）：`LSP_SERVER.md:123-153`
- `proposeEdit`（验证→应用融合、显式 `force`、失败关闭）：`LSP_SERVER.md:155-177`
- `strengthenContract`（契约变更的调用侧审计）：`LSP_SERVER.md:179-193`
- `builtins|effects|errors --json` 直接读取注册表（文档数字是 length）：`vera/introspect.py:1-16`
- 全 158 code 附 `since` 版本：`vera/_since.py`
- 公开 Lark 文法 + “文法 ≡ 规格散文 ≡ editor 文法” CI 门禁：`scripts/check_grammar_alignment.py:1-30`
- 公开 mutation score 83.3%：`mutation.json`、`MUTATION.md:1-22`
- 右→左槽编号产生可通过类型检查的错误（§13 实见）——消除变量名也消除了名称不匹配这一检测器

**A-3. 面向“模型写、人审”的体制（OCaml、经 C）**——`R/jacquard/`
- 224 code，`(domain, code)` 不复用、不重新编号，测试所有 emit code 均在目录中：`docs/errors.md:1-6`
- 诊断是结构体：summary / `Cause:` / **仅一个** `Next step:` / `Contrast:`：`docs/errors.md:7-22`
- W0301（跨值种类的歧义绑定告知）：`docs/errors.md:104`
- `--allow net` 运行时 capability，设计上拒绝 `fs`/`eval` grant：`README.md:48-53, 164-166`
- 规范结构 hash + 语义 diff + content-addressed 测试 cache：`bin/main.ml:2332, 2373`、`docs/warp-testing.md:298-336`
- 多重 fault 模拟（2ⁿ 路径）与 relational case：`docs/warp-testing.md:207-297`
- 53 KB native 二进制（§14）

**A-4. 仅 Wasm GC + Component Model + WASI 0.3**——`R/wado/`
- LSP 功能接入 CLI（`query diagnostics|references|definition|hover|inlay-hints`）：`wado-cli/src/query.rs:9-49`
- install 一行含 `SHA256SUMS.txt` + `gh attestation verify`：`README.md:29-45`
- publish 至 OCI artifact，仅 workspace-root：`wado-cli/src/publish.rs:1-26`
- 诊断 reason chain 设计文档引用文献（type-error ablation × agent）：`docs/wep-2026-06-02-diagnostic-reason-chains.md:1-40`
- 每提交 benchmark 至 Pages：`README.md:169-176`
- 已调查 Almide：`docs/research-language-survey-almide.md`

**A-5. 自托管（effect row），移植 Almide MSR 的一方**——`R/vibe/`
- MSR harness 移植（点名）：`eval/msr/README.md:1-12`
- 每轮以 8 维量规进行 AI 语言 review：`eval/lang-review/rubric.md`
- **双向修复棘轮**（`diag.grep` + `silent` 出现诊断即 FAIL）：`eval/lang-review/run_repair.sh:1-16`、`repair/09_silent_builtin_arity`
- 对“哪个编译器回答”陷阱 hard fail：`run_repair.sh:24-40`
- capability flag 在 build 时 const-fold + DCE：`README.md:100-108`
- 可执行书籍（输出嵌回 markdown）：`README.md:61-66`
- P0 = silently wrong > P1 = crash：`AGENTS.md:23-29`

**A-6. 强制引用、自托管（停滞于 2026-07-19）**——`R/hexa/`
- `@grace(code, until=, reason=)` + 每次编译重新告知 HX9000 + `Acked-grace:` trailer：`README.md:109, 333`
- 文法、陷阱、变更记录统一为一个 JSONL SoT：`doc/grammar.jsonl`、`CHANGELOG.jsonl`
- gen3 ≡ gen4 字节一致的自托管不动点门禁：`README.md:24`

**A-7. 主流小群体的前 2 名**——MoonBit（`~/.moon`）、`R/gleam/`
- `moon explain --diagnostic`（无参数时以 172 行列出全部 code，mnemonic + 状态）、`--attribute`
- `moon test -u --limit 256`、`--test-failure-json`、`moon check --patch-file`、`moon info`（`.mbti`）、`moon prove`
- Gleam：≈ 60 code action（`Add missing patterns` / `Generate dynamic decoder` / `Fill labels` / `Convert to pipe`）：`language-server/src/code_action.rs`
- Gleam：`export package-interface --out x.json`：`compiler-cli/src/lib.rs:695-700`
- Gleam：穷尽性检查器与 code action 同源：`compiler-core/src/exhaustiveness/`
- Gleam：CI 编译 `test-community-packages/`（生态破坏的回归门禁）

<a id="付録-b-08-16-サーベイの再検証で見つけた注記"></a>

## 附录 B. 重验 08-16 调查发现的注记

- `stdlib/list_sort_by_str_key.almd:3,104` 与 `list_sortby_float.almd:4` 注释仍为“insertion sort”（实现为 mergesort，#1459）。只改注释。
- 所有 workflow 均无 `cargo fmt --check`（有 clippy 棘轮）。`.almd` 侧严格，Rust 侧无门禁，与主流相反。
- `check-pass-isolated.sh` 点名以 rustc 的 `test-mir-pass` 为范本，却选择**输出一致性**（有意为之，`:23-28`）。没有 MIR golden。
- ALL_AXES 轴 E 的 #1312/#1313/#1314 全部 closed，ROAD_TO_1_0 0.4x 表的 #928/#999/#1334/#917 也 closed。两文在本 PR 加注。
