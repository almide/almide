<a id="cross-target-completeness-the-lid"></a>

# 跨目标完整性（封盖）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](cross-target-completeness.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](cross-target-completeness.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](cross-target-completeness.zh-CN.md)

> 非规范读者快照，以固定原文为准。不自动同步，未独立执行或验证当前情况。此历史目录及带日期的比较调查，不代表当前缺陷、保证或经独立验证的竞争主张。下文“Active”“实时”“已经闭合”保留原文时代用语；开篇的证据边界适用于这些历史段落。不执行运行指示。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/cross-target-completeness.md)

原文描述元数据：历史跨目标清理策略、带日期目录，以及差分证据的限制


<a id="historical-drain-queue-and-present-evidence-boundary"></a>

## 历史清理队列与当前证据边界

下面的 2026 年 6 月目录不是当前缺陷队列。它统计的 v0 运行时注册表已退役；原生 base64、自托管算法以及解释器／模糊测试器现在已经存在。当前操作可用性及例外范围记录在[目标可用性](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/target-availability.toml)和[契约台账](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml)中。

“四道封印”描述的是保证策略，并非证明所有可能的差异都不可能存在。有限语料／模糊测试的一致性无法确立这一全称命题，已记录的宿主观察及资源限制仍有范围约束。语言比较调查及其唯一性主张保留为作者带日期的调查，而不是重新验证过的竞争性主张。以下是历史测量与拟议的清理工作。

> Native(Rust) 与 WASM 对每个程序都必须产生**字节完全相同的 (stdout, stderr, exit)**。
> 门禁已存在，且**没有沿用既有情况的例外**；本路线图描述从“发现的都已修复”走向“结构上不可能出现差异”的方法，以及仍被列举出来的问题的实时清理队列。

状态：**Active**——清理阶段。门禁、注册表及棘轮均已运行。

<a id="verified-positioning-2026-06-05-audit-11-languages-surveyed"></a>

## 已验证的定位（2026-06-05 审计，调查 11 种语言）

没有其他多目标语言提出这一基准主张。行业的三种做法都更弱：免责声明并转交责任（Gleam 的“by design”、Haxe、Dart），没有字节差分的语义层共享测试（Scala.js、Kotlin MP），或以单目标规避（Elm；ReScript 放弃了原生后端）。架构最接近的 MoonBit（直接生成 wasm + Perceus 系 RC）完全没有提出等价性主张。

> **“Almide 是唯一通过 CI 门禁要求两个后端产生字节完全相同的 (stdout, stderr, exit-code) 输出的语言——每项差异都是被跟踪、不断缩减的债务，而不是被接受的必然事实。”**

<a id="current-state-2026-06-06"></a>

## 当前状态（2026-06-06）

| 机制 | 状态 |
|---|---|
| 跨目标门禁（`tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec`） | 65+ 个语料文件，逐字节比较，**0 @xt-allow** |
| 已记录的差异列表 | **N = 0**（最后一项 `fan.timeout` 已在 0.29.0 从语言中移除，C-006 转为 active） |
| oracle 配对注册表——随 v0 emitter 退役（#782）；`scripts/check-rt-oracle-registry.sh` 为退役遗迹 | 由 spec/wasm_cross 和 interp 三方 oracle 取代（v0 最终统计：76 已验证／42 沿用既有情况） |
| 按构造导出的表（生成时从 Rust std 导出的 Σ-probe，以及全标量 CI 锁定） | 大小写折叠、空白、UTF-8 分类、Unicode 属性（Alphabetic/Alphanumeric/Uppercase/Lowercase） |
| CI 中的 Lean 内核检查 | 41 条定理（Perceus RC／closure env／heap）——运行 `lake build`，而非仅 grep sorry |
| 其他棘轮 | fmt 往返性质（不跳过）、不受宿主架构影响的确定性生成、快照套件 |

最初的 8 簇目录（46 个错误）：**全部清理完成**（#363–#385）。
沿用既有情况的清扫目录（48 个未验证例程中的 47 项实际差异）：**13 项已清理，下面列出 34 项**。

<a id="the-lid-four-seals-each-finite"></a>

## 封盖：四道封印，每道都有有限边界

新的差异要存在，就必须逃过以下所有机制：

1. **例程封印**——将 `grandfathered` 清理至 **0**：每个 wasm 运行时例程都属于 (a) 按构造导出（表）、(b) 穷尽差分测试（小定义域），或 (c) N 百万次模糊差分测试（大定义域）。#382 门禁已阻止未注册的新例程；计数达到 0 时，加入禁止新增既有例外的 CI 断言。
2. **组合封印**——例程相同并不能封住组合（fan.map 内联 lambda 陷阱是 CHECKER 的错误）。组成部分：ConcretizeTypes 的**强制** AllTypesConcrete 后置条件（Unknown→i32 静默误编译类别变为构建错误），然后是在约 30 节点 IR 核心上的**参考解释器**（可执行规范；三方差分定位哪个后端出现差异）加上**每夜生成式模糊测试**（类型引导，每夜约 44k–175k 个程序，自动最小化、自动提升为语料）。只有模糊测试器不再产出新差异后，才可以宣称“零差异”。
3. **未来代码封印**——已经闭合：跨目标门禁、oracle 配对门禁、Lean 内核检查及 fmt 往返测试在每个 PR 上运行。
4. **内核封印**（长期）——用 Almide 自托管算法类标准库，让两个目标运行同一源码（关掉“两份实现漂移”的源头），将证明范围缩小到约 30 个 IR 操作 + RC + 布局；证明这些部分（逐操作定理）。**此处按决定暂停自托管（2026-06-05），在获准启动前仅保留为路线图。** 参见 `research/spike/v1-mir/`（原 `research/selfhost/` 探针已移除）。

<a id="live-drain-queue-all-enumerated-coordinates-known"></a>

## 实时清理队列（全部列举、位置已知）

<a id="pr-d--encodings--json-path-13-divergences-next-up"></a>

### PR-D——编码与 json 路径（约 13 项差异，下一步）
- **base64 在两个目标上都坏了**：原生构建失败（E0425——`almide_rt_base64_encode/decode` 从未存在），wasm 返回空结果／拒绝有效输入。从头实现原生版本，修复 wasm。
- `int.from_hex`：`0X` 前缀、跳过 `'_'`、重复 `0x` 的语义存在差异。
- `bytes.read_f16_le`：wasm 将 ±inf 表示为 ±f32::MAX（有限值！）。
- `hex.decode` 的错误消息细节（原生版本附带位置／长度后缀）。
- `json.get/set/remove_path`：wasm 缺少负索引规范化；`set_path` 对对象使用 Index 时读取堆垃圾；越界 Index **trap 134**，而原生为不会失败的无操作；以上情况还会在嵌套中传播。

<a id="pr-b--wasm-regex-engine-port-11-divergences-biggest-chip"></a>

### PR-B——移植 wasm 正则引擎（11 项差异，最大块）
wasm 引擎存在结构性问题：按字节处理（多字节搜索文本 → U+FFFD 垃圾），把 `|` 当作字面量，转义原子仅有 `\d\w\s`（`\.` `\n` `\D` `\W` `\S` 及转义字面量均无法匹配），没有字符类转义（`[\d]` = `{'\', 'd'}`），捕获返回错误的组。
修复方案是将原生手写引擎（`runtime/rs` 中基于标量的 `rx_parse_*`）忠实移植到 wasm 生成端，并加入差分模糊门禁（随机模式 × 包含多字节的搜索文本）。
考虑为回溯引擎（带 fuel 上限）建立 Lean **终止性定理**——穷尽测试和模糊测试都无法确立这一性质。

<a id="pr-c--math-determinism-16-divergences-needs-a-contract-decision"></a>

### PR-C——数学确定性（16 项差异，需要契约决定）
wasm 多项式近似与原生 libm 处处有差异（sin(π) 相差 8 个数量级）；无论契约如何，明确的错误包括：`log(0)`=NaN（原生为 -inf），`exp(-745)`=0（原生为次正规数），`fpow(-2, 0.5)`=1.414（abs 错误；原生为 NaN），`fpow(0,-1)`=0（原生为 inf），`fpow(2, inf)` **trap 134**，`to_fixed(x, ≥19)` **trap 134**，`to_fixed` 舍入错误（0.05@1dp → 0.0 对 0.1）。
**契约建议（待确认）**：原生 libm 本身也依赖平台（macOS 与 glibc 的末位 ulp），因此将确定性的纯 Rust libm（musl 移植的 `libm` crate）纳入原生运行时，再将同一算法移植到 wasm；这样也能获得原生跨平台确定性。先例：Java `StrictMath`/fdlibm。

<a id="small-items-queued"></a>

### 小项（已排队）
- 用 enum 名构造记录 variant：原生拒绝（E0574），但 **wasm 接受并运行**；前端应在两边都拒绝。
- 常量折叠后的浮点 inf/NaN 生成非法 Rust 标识符 `inff64`/`NaNf64`（E0425 原生构建失败 = ICE 泄漏类别）。
- 定宽整数记录字段（`M { a: 5 }`，其中 `a: Int8`）：原生 E0308，wasm 将下一字段读成垃圾（#368 类别，构造位置的残留问题）。
- wasm 上递归泛型类型表示（自引用 `Tree[T]`）：T 负载生成错误（不 panic；没有规范测试覆盖它）。
- 名为 `Box` 的用户类型与需要 Box 间接引用的递归 enum 在原生端冲突（既存问题）。

<a id="order-of-work"></a>

## 工作顺序

PR-D → PR-B → PR-C（先作契约决定）→ 小项 → AllTypesConcrete 强制后置条件 → 参考解释器 → 每夜模糊测试器。每项落地时都附带：`spec/wasm_cross/` 内的语料文件、引用新测试的注册表 grandfathered→verified 切换，以及声明验证类别（`exhaustive` / `fuzz-N` / `lean` / `by-construction`）。

<a id="related"></a>

## 相关资料

- [correctness-guarantee-gaps.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md)——白盒分层证明轴（WasmIR、ANF、closure env、Perceus 符合性）
- [determinism-belt.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md)——通过构造实现编译器输出确定性
- [closure-cross-target-completeness.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/closure-cross-target-completeness.md)——闭包维度（大部分已清理）
- [llm-first-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md)——字节确定性为何服务于 MSR
