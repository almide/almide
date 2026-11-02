<a id="where-the-string-workload-gap-goes-1004"></a>

# 字符串工作负载的差距在哪里（#1004）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](string-gap-1004.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](string-gap-1004.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](string-gap-1004.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md)的非规范历史参考快照，固定提交：35297a612fb29e3f78f6587aba253aa91d51f466。不自动同步。未在此独立重新验证或重跑测量、因果归因、实现描述及拟议工作。以正本为准。这些带日期且限定工作负载的主张不构成当前或普遍性能保证；未执行引用命令或拟议变更。

**一句话回答：** 差距的四分之三来自 `string.split` 的返回类型（`Vec<String>`——Almide 没有借用字符串类型），八分之一来自 `string.len` 的字符计数语义；所有被怀疑的因素——RcCow 表示、rlib crate 边界、中间列表——合计不到五分之一。与遵守相同两项要求的 Rust 参考实现相比，编译器输出在 **1.04–1.12x** 以内。

测量于 2026-08-13，M4 Pro（14 核），`--release`。下列每个数字都来自本目录中的差分实验，没有推断值。

---

<a id="the-workload-and-the-two-references"></a>

## 工作负载与两个参考实现

`strchurn/strchurn.almd` 原样采用 #1004 工作负载：N 个整数 → `int.to_string` → `list.join` → `string.split` → `string.len` → `list.sum`。

它有**两个** Rust 参考实现，正是区分它们才使归因成为可能：

| 参考实现 | 做什么 | 作用 |
|---|---|---|
| `rust-ref/strchurn.rs` | `split` 收集拥有的 `String`；长度为 `chars().count()` | 同形态／同语义——**有意义的比较** |
| `rust-ref/strchurn_idiomatic.rs` | `split` 产出借用的 `&str`；长度为 `s.len()` | 人们会写出的代码——API 契约的代价 |

此行由 `check-perf-ratio.sh` 报告，而不作为固定基准，遵循 commit 95b9b6d59 为 `listbuild` 家族确立的政策：成本来自分配的工作负载，首先比较 allocator，其次才是 codegen；almide/rust 比值并不能抵消机器差异（同一 commit 的 listbuild 在 M4 Pro 为 1.58，在 ubuntu-latest runner 为 0.91）。与 listbuild 不同，此处两边分配相同，因此该行或许可以固定为基准——但应先取得第二种架构的读数。

两项要求都是真实的，不是为 benchmark 人为设置的劣势：`almide_rt_string_split` 是 `s.split(sep).map(|x| x.to_string()).collect()`，因为 Almide 的 `List[String]` 元素类型是拥有的字符串；`almide_rt_string_len` 是 `s.chars().count()`，因为 `string.len` 定义为字符数。工作负载全是 ASCII，因此三个程序打印逐字节相同的输出，`bench.py` 在计时前验证这一点。

<a id="headline"></a>

## 代表结果

N=4M，取 21 次交错运行的最小值，再取三个此类批次的中位数：

| 二进制 | ms | 相对交付版 |
|---|---:|---:|
| `almide build --release`（交付的 rlib 路径） | 228.0 | 1.00 |
| 同一程序，整体构建（`ALMIDE_NO_RTLIB=1`） | 218.0 | 0.96 |
| 同形态 Rust 参考实现 | 205.3 | 0.90 |
| 惯用 Rust 参考实现 | 111.5 | 0.49 |

**相比人们会写的 Rust 为 2.04x；相比做相同工作的 Rust 为 1.11x。** issue 最初的 1.7x 在其 N=1M 下准确复现（89.7ms 对 53.8ms = 1.67x）——issue 所说的“手写 Rust”是惯用版本。

<a id="the-gap-attributed"></a>

## 差距归因

方法是一个**阶梯**，收录于 `strchurn/ladder.py`：取编译器自身的 `--target rust` 输出，每级仅替换 `__almide_main` 中的一项成本，保持所有 runtime 函数逐字节相同，再用与参考实现完全相同的 rustc flag 编译所有级别。阶梯两端闭合：`v3_fused_sum` 级落到同形态参考实现附近（211.7 对 205.3ms），末级落到惯用参考实现（111.5 对 111.5ms），因此差值解释的是整个差距，而非仅讲述其中一部分。

| # | 移除的成本 | ms | 占比 |
|---|---|---:|---:|
| 1 | `string.split` 返回 N 个**拥有的 `String`**（每片 malloc + memcpy + free，结果容器也胖 3x） | 69.4 | **59.6%** |
| 2 | split 结果的**容器本身**（无大小 hint 的 `collect()` → 翻倍增长） | 17.6 | **15.1%** |
| 3 | `string.len` = `chars().count()`——O(n) UTF-8 扫描，无 ASCII 快速路径 | 13.2 | **11.3%** |
| 4 | **rlib crate 边界**（runtime 无法内联到用户代码） | 11.0 | **9.4%** |
| 5 | 三个列表流水线中间因素**合计**——每元素 `Rc<dyn Fn>`、`list.range` 的 `Vec<i64>`，以及未融合 `list.map`→`list.sum` 的 `Vec<i64>` | 5.3 | 4.6% |
| 6 | 残余 codegen（末级对惯用参考实现） | 0.0 | 0% |
| | **总计** | **116.5** | **100%** |

第 1+2 行是同一根因——`string.split` 的返回类型——合占差距的 **74.7%**。

第 5 行有意作为一个类别报告。第 1–4 行在各批次间复现误差在几个百分点内；第 5 行内部三个级别则不然。四个批次中，各自差值先为 6.4 / −2.7 / 1.6 ms，随后为 2.6 / 11.2 / −8.3 ms——也就是每项都不高于共享机器 ±3–8 ms 的噪声底，而总和稳定在约 5 ms。在这个 N 下把一个 5 ms 类别拆成三份，是算术而不是测量。这些成本真正可测的地方是下面的发现 (a)，在那里值 6.6x。

<a id="the-counter-hypothesis-the-issue-names-refuted"></a>

## issue 点名的反向假设：已驳倒

> “`ALMIDE_NO_RTLIB=1`（整体 LTO 构建）没有缩小差距——仍是 0.05s。所以这是 runtime 值表示的问题。”

前半句**得到确认且更精确**，后半句则**错误**。

- 消除 crate 边界使交付二进制从 228.0 降到 218.0 ms，而惯用参考实现仍为 111.5。因此边界占**运行时间的 4.4%／差距的 9.4%**——确实存在（`cargo_build.rs` 注释估计“2–5%，在字符串／列表密集循环中最高约 10%”是准确的），却远不足以解释全部差距。issue 最初的测试用 10 ms 计时分辨率看不到 4%；它报告“无效果”，从中得出的结论在两个方向上都过强。
- issue 标题提到的表示**完全没有参与**。`RcCow<T>` 仅用于 `Bytes` 与 `Matrix`（`codegen/templates/rust.toml` 的 `[type_bytes]` / `[type_matrix]`）；Almide `String` 按机器字一一对应地 lowering 为 Rust `String`。此路径没有引用计数操作——在输出的 `__almide_main` 中 grep `RcCow`，不会出现。**issue 标题的前提是错的。** 成本不在字符串如何表示，而在 stdlib signature 强制产生多少字符串。

<a id="two-micro-optimizations-both-measured-both-rejected"></a>

## 两个微优化：均已测量，均未采用

记录在此，避免他人重新推导：

- **在 `almide_rt_string_split` 中预留容量。** 消除翻倍增长（第 2 行）需要一次计数遍历。4M 片段测量：当前 `collect()` **99.4 ms**，`Vec::with_capacity(matches.count()+1)` + `extend` **115.1 ms**，仅计数遍历 **25.6 ms**。遍历成本高于所消除的增长成本。**净亏损——不要合入。**
- **`almide_rt_string_len` 的 ASCII 快速路径**（`if s.is_ascii() { s.len() } else { s.chars().count() }`，一般情况下也精确）。对 4M 短字符串：`chars().count()` **10.7 ms** → 快速路径 **8.4 ms**。收益 2.3 ms = **差距的 2%**，代价是在热 primitive 中增加第二种扫描形态。作为舍入误差量级的项目暂存在工作清单。

<a id="two-findings-outside-the-string-row"></a>

## 字符串行之外的两个发现

**(a) `IterChain` 在 Rust 目标上从不触发。** 流水线的融合 iterator 节点是死的：`list.map` / `filter` / `fold` 无论在 pipe、直接调用，还是以 `list.range` 为输入，都输出针对已实体化 `Vec` 的 `almide_rt_list_*(Vec, Rc<dyn
Fn>)`。本测试集任一 benchmark 输出的 `__almide_main` 中都没有 `.into_iter().map(…)`。有效的是 egg pass 的组合器融合：`range |> map |> fold` 变成单个 `almide_rt_list_fold`，map 合成进 reducer。因此 #1004 背景所赞许的融合在组合器层面真实存在，在 iterator 层面不存在。`crates/almide-codegen/src/pass_stdlib_lowering.rs:426` 将 `try_lower_to_iter_chain` 描述为“单次调用形态的 fallback”的注释已经过时——它从不触发，因为 `ResolveCallsPass` 已经改写了它匹配的 `CallTarget::Module`。

在没有分配掩盖成本的廉价 lambda 主体上，这值多少（30M 元素，输出形态的模型，5 次最小值）：

| 形态 | ms |
|---|---:|
| `map` + `sum`、`Rc<dyn Fn>`、两个实体化 `Vec`（输出形态） | 36.1 |
| 同样形态，单态闭包 | 37.9 |
| egg 融合后的 `range` → `map` → `fold`——一个实体化 `Vec` | 31.5 |
| 完全融合的 Rust iterator chain | **4.8** |

**6.6x，而且 boxed closure 不是原因**——它在此约值 0。全部成本在于 `list.range` 是一个实际写入再读回的 240 MB `Vec`，而非 `impl Iterator`。

**(b) `listbuild` 行的约 1.6x 不是 #1004。** `perf/README.md` 和 `scripts/perf-ratio-baseline.txt` 都曾将三个 listbuild 行中的残余实体化成本称为“#1004 追踪的实体化成本”。不是，而且不是实体化。在输出 Rust 中，仅将 `almide_rt_libm_sin`/`_cos`——跨目标逐字节一致契约要求的确定性 **software libm**——换成平台版本：

| 二进制（2^23） | ms |
|---|---:|
| 输出的交付形态 | 194.5 |
| 输出形态，平台 `sin`/`cos` | **105.5** |
| 手写 Rust 参考实现（用平台 `sin`/`cos`） | 123.9 |

该行整个 1.57x 来自超越函数确定性；使用平台 libm 时，编译器输出反而以 1.17x **胜过**手写参考实现。被指责的嫌疑项是无辜的：同形态 A/B 将 `list.repeat` 零填充 + 带边界检查的索引写入测为 **75.5 ms**，而 `Vec::with_capacity` + push 为 **90.6 ms**——输出形态更快；`ALMIDE_NO_RTLIB=1` 仅使该行变化 0.6 ms。这次变更修正了两个注释。

<a id="verdict-and-work-list"></a>

## 结论与工作清单

**接受字符串数值，并与分解结果一同发布。** 相对同等工作的 Rust 为 1.11x 并不丢人；相对 `&str` Rust 的 1.9x 是拥有字符串的 stdlib 的代价，是应根据自身利弊讨论的语言设计决定，而非悄悄修复的 codegen 缺陷。

按类别列出下一步，从最大开始：

| 类别 | 占比 | 下一步 |
|---|---:|---|
| `split` → 拥有的 `String`（第 1+2 行） | 75% | 只有借用／slice 字符串类型，或 `string.split_iter` 风格的 consumer 融合形态能改变它。这是语言层面的事，需要独立 issue 与 MSR 论证（第二种字符串类型会增加编写成本）。在此之前，它是已文档化并接受的契约。 |
| `string.len` = `chars().count()` | 11% | 保持语义。若某次 profile 表明值得，可选用 2% 的 ASCII 快速路径（上文已测）。 |
| rlib crate 边界 | 9% | `cargo_build.rs` 中的既定计划是给热的非泛型 runtime 函数（`string_len`、`int_to_string`、`string_split`）加 `#[inline]`；必须对照 `check-embedded-size.sh` 落地，因此尚未实施。收益有限但真实，约占运行时间 4%。 |
| 每元素 `Rc<dyn Fn>` | 5.5% | 不值得单独做一个 pass——廉价主体上测得约 0。若下项落地，它会附带免费消失。 |
| 列表中间项（第 6+7 行，以及发现 (a)） | 此处约 1.5%，**别处 6.6x** | **本文最大的真实收益，不在字符串行。** 恢复 `IterChain`（或将 `list.range`+组合器 lowering 为惰性 Rust iterator），使组合器链不再实体化。需要独立 issue；`pass_stdlib_lowering.rs:426` 的过时注释应一同消失。 |
| listbuild 的 1.6x（发现 (b)） | — | 不是 #1004 类别。若某天认为 software libm 成本过高，可调整的是受跨目标契约约束的 native-only 快速路径——这是契约讨论，而非 codegen 讨论。 |

<a id="reproducing"></a>

## 复现

```bash
cargo build --release

# the row, on the harness (native vs both references)
ALMIDE_BIN=target/release/almide \
  python3 research/benchmark/perf/bench.py --legs native,rust --bench strchurn --runs 7

# the attribution table above, rebuilt from scratch
ALMIDE_BIN=target/release/almide \
  python3 research/benchmark/perf/strchurn/ladder.py --runs 21

# the ratchet (prints strchurn's ratio in the reported, un-anchored group)
bash scripts/check-perf-ratio.sh
```

代码注释译文：在 harness 上测量该行（native 对两个参考实现）；从头重建上面的归因表；ratchet 将 strchurn 比值打印在仅报告、未固定基准的组中。

`ladder.py` 输出程序，每级去掉一项成本，验证各级仍打印相同输出，交错计时，并打印差值及占比。运行三次：表中第 1–4 行可复现，第 5 行内部拆分不可复现（见表下注记）。它还打印两端闭合级别离各自参考实现有多远——若不接近零，差值便不再解释差距，表也不再是测量。

复现 rlib 边界行时，应将同一程序按两种方式构建到新的项目目录（构建缓存按生成的 Rust 而非 `ALMIDE_NO_RTLIB` 键控，因此复用同一目录会静默返回另一种构建的二进制）：

```bash
ALMIDE_RUN_PROJECT_DIR=$PWD/.a  almide build .../strchurn.almd --release -o /tmp/rlib
ALMIDE_NO_RTLIB=1 ALMIDE_RUN_PROJECT_DIR=$PWD/.b \
  almide build .../strchurn.almd --release -o /tmp/mono
```

---

原文注意事项：结论使用 1.9x，而 N=4M 代表结果为 2.04x、N=1M 示例为 1.67x；原文未调和这些数字。工作清单的占比（5.5% 与约 1.5%）及“第 6+7 行”引用也与归因表不一致：归因表将中间项合并在第 5 行，第 6 行是零残余，没有第 7 行。这里保留这些历史不一致，不修复，也不视为独立测量。拒绝测得的容量预留改动、暂存 ASCII 选项、提出 iterator／字符串类型／inline 改动，是不同状态；没有将提案改写为已实现。runtime 表示和过时注释的主张属于固定原文时代，不是新的代码审计。
