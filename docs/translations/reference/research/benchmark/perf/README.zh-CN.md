# 性能套件 — Native 与 WASM 运行时成绩表（#917）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/README.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 本译文是固定源版本的非规范性快照（修订：35297a612fb29e3f78f6587aba253aa91d51f466），不会自动同步。测量结果与主张未经独立重新验证；以英文原文为准。
>
> 原文注意：正文同时记录历史测量、尚未重新测量的归因，以及验证和门禁的覆盖缺口。请勿把数字泛化到所有环境或当前实现。

`bench.py` 中的套件包含计算内核、缩小版 One Billion Row Challenge、列表构建形态、字符串反复处理以及其他工作负载行。每行声明它运行哪些分支及其验证模式；并非每种工作负载都在每个目标上运行。已定义的地方包含手写 Rust 参考实现。此目录是 [docs/project/BENCHMARKS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BENCHMARKS.md) 中数字的来源；凡不是 `bench.py` 产出的数字，都不发布。

## 布局

| 路径 | 用途 |
|---|---|
| `<bench>/<bench>.almd` | 基准源码（Almide） |
| `rust-ref/*.rs` | 手写 Rust 参考实现，使用与 native 分支完全一致的标志编译 |
| `bench.py` | 构建 + 验证 + 计时测试框架；写入 `results/` |
| `results/<date>-<label>.json` | 带日期的原始结果（已提交） |
| `native/` | README 的二进制大小/CLI 测量（`measure.sh`，独立于成绩表） |
| `string-gap-1004.md` | #1004 的归因：字符串行的差距实际花在哪里 |
| `strchurn/ladder.py` | 生成该归因的差分阶梯（可重跑） |

## 方法

- **分支**：`native` = `almide build --release`（cargo release 配置：opt-level=3、LTO、1 CGU）；`wasm` = `almide build --target wasm`，由 wasmtime CLI 执行；`rust` = `rust-ref/*.rs`，通过 `rustc -C opt-level=3 -C lto=yes -C codegen-units=1 -C overflow-checks=no` 编译。
- **先正确，再谈速度**：任何计时开始前，基准的每个变体都必须在小工作负载上产生逐字节一致的 stdout（fft 只比较第 1 行；第 2 行是它自行报告的时间）。算错答案的条目不能算成绩表条目。
- **计时**：测量整个进程的墙钟时间，stdout 重定向到 /dev/null，预热一次，然后交错执行 N 轮（变体 A、B、C、A、B、C…），让漂移均等影响每个变体。主要数字为中位数；JSON 保留每次运行。
- **安静的机器**：关闭浏览器，不要并行构建编译器；机器上有其他重负载时的测量不可信。Apple Silicon：安静机器上的墙钟测量会落到 P 核；不尝试绑核。
- **主张需要时，参考实现采用相同形态**：`nbody_unrolled.rs` 复现 nbody.almd 完全展开的标量局部变量，以隔离代码生成开销；`nbody.rs` 是人们实际会写的典型天体数组形态。Almide 当前优于后者（边界检查）；该比较仅报告，不设门禁。
- `fannkuchredux` 和 `mandelbrot` 使用 `fan` 编写，自 #1330 起拥有普通的顺序 Rust 参考实现（`rust-ref/fannkuchredux.rs`、`rust-ref/mandelbrot.rs`：单线程，无 `unsafe`，无 SIMD）。这不是普遍意义上同等并行度的比较：native `fan` 可以在线程上运行满足条件的回调（`pass_rust_lowering_fan.rs`）。`fannkuchredux` 现为 VICTORY 行，通过 `ALMIDE_FAN_SEQUENTIAL` 消融检查。`mandelbrot` 仍为 REPORTED；其返回 Bytes 的回调不满足该 native Send-safe 路由的条件。此前 ~1.0 的读数描述的是早期实现，并不表示如今 native fan 没有并行性。
  `binarytrees`（#1991）和 `treealloc`（#2028）有同形态的 `Box` 参考实现（`rust-ref/binarytrees.rs`、`rust-ref/treealloc.rs`：单线程，每个节点一个 `Box`、一次释放，无 arena，无 `unsafe`）。两者都低于 1，因为 native 上 `check(make(d))` 在区域窗口内执行；它们是 `check-perf-ratio.sh`（#1330）的 VICTORY 行，针对主张本身及 `ALMIDE_REGION_OFF=1` 消融设置门禁，而非围绕一个依赖分配器的数字设 ±区间（M4 Pro 上为 0.32，CI runner 上为 0.61；类似 listbuild）。包含消融和两种规模的声明位于 `docs/project/BENCHMARKS.md`。
- `onebrc` 是缩小版 One Billion Row Challenge（`station;temp` 行 → 按站点排序的 min/mean/max）；这是唯一一行热循环为文件 I/O、`string.split` 和 map 更新而不是算术的工作负载。温度全程使用十分之一度的整数，因此输出逐字节一致，不涉及浮点格式化。wasm 分支被排除：`wasmtime run` 不预打开目录，所以该分支无法访问文件。这一行是流式行处理家族（`fs.for_each_line` / `fs.fold_lines`，C-220）的诞生地，也是它的监测点。
  原来的急切式 `fs.read_lines` 形态测得 RSS 约为文件大小的 ~4×（2026-08-08，M4 Pro：126 MB / 10 M 行文件对应 505 MB，632 MB / 50 M 对应 2.2 GB；同形态 Rust 参考实现完全相同，因此瓶颈在 API 形态而非代码生成），外推到官方 1 B 行文件约为 ~50 GB。聚合阶段如今在两个分支上都采用流式处理，RSS 在各个规模都保持 1 MB。首个流式版本付出了时间回退的代价（50 M 行为 33.4 s，急切式为 15.6 s），追查发现 fold 累加器的 Map 每行都被克隆：clone pass 跨互斥 match 分支统计语法上的使用次数，因此分支尾部永远不能成为最后一次使用（#1143）。完整的性能攻坚记录（2026-08-08，50 M 行 / 632 MB，M4 Pro）如下，每个阶段与上一阶段逐字节一致：
  急切式 15.6 s / 2.2 GB → 首个流式版本 33.4 s / 1 MB（fold 累加器的 Map 每行克隆；clone pass 跨互斥 match 分支统计语法使用，分支尾部因此无法成为最后一次使用；#1143）→ 兄弟分支扣除修复 + 消耗式 `map.set` 15.2 s → `string.split_once` + `map.upsert`（一次查找，无 Vec，无键克隆）8.2 s → `fs.fold_lines_chunked`（运行时线程上的范围 worker，由调用者合并部分结果）**8 个 worker 时 1.27 s / 12 个时 0.95 s，而同形态单线程 Rust 参考实现为 2.58 s**。朴素 fold 提交通过结构战胜了手写顺序 Rust，在 native 上重演了 binarytrees 的做法。扩展性确实存在（1w 8.5 s → 2w 4.6 s → 12w 0.95 s），RSS 保持 2 MB。
  必须坦诚说明：参考实现刻意使用单线程（手工并行化的 Rust 会再次胜出）；每核差距（~3.3×）曾被归因于逐行分配相关操作（split_once ×2 + strip_prefix + 每行一个 upsert 闭包）以及 `AlmideMap` 查找，**而该查找测量时是线性扫描，现在已不是**。自 #2150 起，超过 `ALMIDE_MAP_INDEX_THRESHOLD` = 16 且键可哈希的 map 会带有槽表索引（`runtime/rs/src/map.rs:12`-`31`），此后尚未重新测量这一行，因此该归因中的查找部分未经验证；闭包捕获的 Map（`for_each_line` + `var stats`）仍会在每次通过 `SharedMut` 单元读取时克隆。聚合应该用 `fold_lines`，CHEATSHEET 也是如此指导的。在性能攻坚继续期间，这里的墙钟比率仅报告，不设门禁。
- `fft-wasm` 行存在，是因为 wasm 分支过去会在频繁的 `data[i] = x` 列表写入上崩塌（2^18 时约 ~3 个数量级）：每次存储都会复制整个块（每次写入调用 `$block_copy`，每次存储 O(n)）。RC-5 的写时复制判定（#1729）让存储变为 O(1)，在 #2150 重新测量前，断崖就已消失：develop 的嵌入式宿主上，2^18 时 4.1× / 2^22 时 4.7×（2026-09-24）。剩下的是每次存储都执行的判定本身：一次调用、一次堆边界比较和一次引用计数加载，每个蝶形执行四次。#2150 对于循环仅逐元素读写的列表，在每次进入循环时只执行一次（`crates/almide-wasm/src/cow_hoist.rs`），该行读数变为 2^18 时 2.7× / 2^22 时 2.9×。剩余成本来自每次访问：带中止帧的 i64 边界比较，以及每次加载和存储都折返为 i32 的 i64 索引运算。`perf_ratchet.rs` 的 R5（存储的渐近复杂度类别）和 R6（存储/读取对等性）防止这两种形态回归。
- **mandelbrot 的 wasm“巨坑”（4000 时约 ~130×）未能复现**：#2150 在 develop 上重测时，在 `wasmtime run`（本测试框架的 wasm 分支）下，4000 时为 native 的 1.13×。`almide bench --target wasm` 测得 2.05×，全部差异都来自嵌入式宿主的 30 s epoch 看门狗；epoch 中断让 wasmtime 在每个循环头检查 epoch，而逃逸时间循环每次迭代只有几条指令（同一模块使用 `wasmtime run -W
  timeout=60s`：636 → 1,199 ms）。看门狗属于测试框架设施，因此基准运行器如今不带它运行（`run_wasm_unbounded`），读数为 1.13×；`perf_ratchet.rs` R7 固定了运行器不再付这项开销的状态。
- **`fft` 测量变换，而非输入**（#1338）。该行过去在循环中用 `data = data + [x]` 构建 8.4M 元素输入，让该行约 ~8% 的墙钟时间花在列表构建上，却作为代码生成结果发布：其报告的 1.27x 包含部分准备成本。如今改为预分配并按索引写入，与参考实现的 `Vec::with_capacity` + push 形态相同，符合上文 `nbody_unrolled.rs` 所述公平性规则。2026-08-13 重新锚定为 1.18x。变换在此次修改前后逐位一致，已在计时外通过整个数组的位置加权校验和于 2^12 / 2^16 / 2^20 验证；该行自身的 `line1` 验证模式仅比较 `size: 2^k = n`，不含计算值，所以测试框架无法发现此行的变换损坏。这个缺口值得补上，但这里没有补，因为在计时路径加入校验和遍历，会恰好重新引入 #1338 所针对的“基准在测内核以外的东西”问题。
- **`listbuild` 是三行、一个工作负载**（#1337）：同一个含 2^23 个元素的交错 `Float` 数组，分别通过 (a) 预分配 + 索引写入、(b) `var` + `for` + `data = data + [x]`、(c) `list.range |> list.flat_map` 构建。三份源码仅构建循环不同，算术相同、校验和消费逻辑相同，所以行间差距就是形态的成本，且三者在计时前都会验证为逐字节一致。这个家族存在，是因为 (c) 是 CLAUDE.md 和 docs/CHEATSHEET.md 推荐的形态，却是三者中最慢的：耗时为它被文档建议替代的追加循环的 1.67x，全部来自 `flat_map` 中间列表的每元素一次堆分配。这首先是 MSR 问题，然后才是性能问题；惯用法文档是引导生成代码的上下文材料，因此“推荐”和“快”必须指向同一种形态。三者如今相差在 2% 以内。
  `check-perf-ratio.sh` 对此家族设置门禁的是行间关系（`IDIOM_CEILING`），而非每行相对 Rust 的比率：这里的绝对比率被发现强烈依赖架构，同一提交在 M4 Pro 上为 1.58，在 ubuntu-latest CI runner 上为 0.91，因为该工作负载的成本是分配而非算术，两个分配器行为不同。因此这是“比率能抵消机器差异”不成立的一个家族；各行像 onebrc 一样仅报告，承担门禁的是关系：本地 1.018x，CI 1.045x。
  **残余 ~1.6x 究竟是什么，已于 2026-08-13 测量：既不是物化，也不是 #1004 的表示方式问题，而是确定性软件 libm。** 仅将生成 Rust 中跨目标逐字节一致契约所要求的 `almide_rt_libm_sin`/`_cos` 换为平台版本，就能让该行从 194.5 ms 降到 105.5 ms，而参考实现为 123.9 ms；去掉超越函数的确定性要求，生成代码以 1.17x 战胜手写参考实现。构建形态是无辜的：`list.repeat` + 带边界检查的索引写入测得 75.5 ms，而 `Vec::with_capacity` + push 为 90.6 ms。证据：[string-gap-1004.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md)。
- **`strchurn` 是字符串行**（#1004）：N 个整数 → `int.to_string` → `list.join` → `string.split` → `string.len` → `list.sum`，其对 Rust 的 1.7x 促成了该 issue。它有两份参考实现。有意义的比较是相对 `rust-ref/strchurn.rs`，后者遵守 Almide 程序无法回避的两项标准库义务：`split` 收集拥有所有权的 `String`（`List[String]` 的元素类型拥有所有权），长度使用 `chars().count()`（`string.len` 计算字符数）；在 M4 Pro 上为 1.12x。`rust-ref/strchurn_idiomatic.rs` 是同一程序，但使用借用的 `&str` 片段和按字节计的 `len()`，运行快 1.9x；全部差异都来自 API 契约，而非代码生成。
  与 listbuild 家族一样，该行为 REPORTED，不作锚定：差值的 75% 是 N 个拥有所有权的 `String` 的 malloc/memcpy/free，因此它首先比较分配器，其次才是代码生成，一个架构的数字不应成为门禁（但与 listbuild 不同，这里两侧分配完全相同，因此第二种架构的读数很可能使其升级）。完整归因——75% 来自 `split` 的返回类型，11% 来自 `string.len` 的语义，9% 来自 rlib 边界，5% 来自列表中间值，而 issue 标题指责的 `RcCow` 根本不在这条路径上——见 [string-gap-1004.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md)；`strchurn/ladder.py` 可从头重建。
- **`wordfreq` 是 Map 行**（#2150、#2157）：从 5 000 词的词汇表抽取 N 次，在 `Map[String, Int]` 中计数，按计数降序 / 单词升序取前 10 个；这是所有词频统计、group-by 和直方图程序共有的按键聚合形态。像 listbuild 一样，一个工作负载有两种写法：`wordfreq` 是命令式 `var` Map + `m[w] = (map.get(m, w) ?? 0) + 1` 循环，`wordfreq-group` 则是 CHEATSHEET 的 `list.group_by` + `map.map`；词汇、抽样、排序和输出完全相同，因此差距代表推荐惯用法的成本。参考实现（`rust-ref/wordfreq.rs`）同形态、同语义：使用 `HashMap<String, i64>`（hashbrown + SipHash），每次抽样从词汇表克隆一个拥有所有权的键（Almide 的 `let w = vocab[i]` 拥有所有权），每次抽样调用一次 `entry`。
  两行也都运行 wasm 分支，`check-perf-ratio.sh` 对它们的 wasm 关系设置门禁（`wordfreq-wasm-idiom`，上限 1.15×）：#2156 之前，结构化分支的 `group_by` 每遇到一个新键就复制扩展累加器，导致查找固定在线性 `$scan_str` 路径；每个元素遍历每个分组键，在 5 000 词词汇表上每元素约 ~11 µs，是命令式写法的 110×（2M 次抽样：22 s 对 201 ms）。如今它通过 `$map_reserve` 原地扩容，并像 `m[w] = …` 窗口一样走索引路径，wasm 上惯用法行约为命令式行的 ~0.8×。
  自 #2150 起，`wordfreq` 的 native/Rust 比率已在 `check-perf-ratio.sh` 的 PAIRS 中锚定（4M quick 工作负载的基线为 1.90）；它先比较分配器和哈希器，再比较代码生成，所以在获得两类机器的读数之前一直仅报告，而这些读数相符：五次通过的 runner 运行在 2M 时为 1.66-1.75，M4 Pro 在 2M 时为 1.77、4M 时为 1.97。`wordfreq-group` 继续仅报告。
  自 #2150 起，native `AlmideMap` 是紧凑有序字典（按插入顺序的条目向量 + 使用开放寻址、存储位置的槽表；每个条目缓存完整哈希，每次操作一次 multiply-fold 哈希），读侧（`map.get` / `get_or` / `contains`、`set.contains`）借用键，普通变量键写入 `m[w] = …` 则移动键；M4 Pro 上 2M 次抽样循环的 Map 成本从 ~93 ms 降到 ~55 ms，低于同一程序结构化 wasm 分支的 ~73 ms。
- **`jsonround` 是 Value 行**（#2157）：将 N 个四字段记录（一个嵌套对象、一个双字符串数组）构建成一个 `Value`，经 `json.stringify`、`json.parse` 往返后，求嵌套 `meta.score` 的和。仅 native 和 wasm；设置门禁的是两分支关系，不是 Rust 比率。#2157 发现它作为 native 二进制比在 wasmtime 下慢 1.3–2.5×：`value.array` / `value.object` 会从传入列表克隆每个元素（每个元素都是整棵子树，因此文档每嵌套一层就复制一次），`value.as_array` 从解析后的文档克隆每个记录，`json.stringify` 则用每节点一个 `String`、每容器一次 `format!` + `join` 自底向上拼装文本。这三个操作现已声明为 `@reuse`，并有 `_owned` 运行时对应版本，因此最后一次使用的源会移动到结果中，文本写入单个缓冲区。`check-perf-ratio.sh` 在此行和两条 wordfreq 行上维持 native ≤ wasm（NATIVE <= WASM 块）。
- **`regexcycle-8` / `regexcycle-32` 是规则表行**（#4392）：依次将 N 个 `Regex` 值应用到四行文本的每一行，两个 N 下匹配数相同。没有 Rust 参考实现；门禁约束行间关系（`check-perf-ratio.sh`，REGEX CYCLE，两分支上限均为 1.5x）。0.69.0-rc1 将已编译模式保存在 16 项 LRU 列表中，因此超过 16 的循环会在每次匹配时重新编译每个模式：撤销修复时为 3.27x，保留修复时为 1.03x（GCE n2-standard-4）。native 缓存如今在哈希索引下保存 1024 个模式，满时随机淘汰，因此周期比这更长时，仍能保留大部分已编译模式。

## 运行

```bash
python3 research/benchmark/perf/bench.py                 # full suite, all legs
python3 research/benchmark/perf/bench.py --runs 7 --label m4pro
python3 research/benchmark/perf/bench.py --legs native,rust --bench nbody
ALMIDE_BIN=target/release/almide python3 research/benchmark/perf/bench.py
```

需要 `almide`（或 `ALMIDE_BIN`）、`rustc`，以及 wasm 分支所需的 `wasmtime` CLI。

## CI 棘轮

`scripts/check-perf-ratio.sh`（CI 的 `perf-ratchet` 作业）在 `--quick` 工作负载上，对每个基准的 almide-native / 手写 Rust 比率，依据 `scripts/perf-ratio-baseline.txt` 设置门禁。看比率，不看绝对时间；比率抵消 runner 差异。持续回退或改善，就在同一修改中移动基线。完整策略见脚本头部。共有三类行：PAIRS（锚定在 ±区间）、REPORTED（打印，依赖分配器）、VICTORY（#1330：Almide 比普通 Rust 快的行，门禁检查 `ratio < 1.0`，以及针对该行所归因优化的同源消融）。

## 尚未覆盖

- **列表组合子的惰性：已不再是缺口，但仍没有测试行。** 在 2026-08-13（[string-gap-1004.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/perf/string-gap-1004.md) 的发现 (a)），`list.range` → `map` → `fold` 在 30M 个元素时构建了一个实际占用 240 MB 的 `Vec`，耗时为融合 Rust 迭代器链的 6.6x。该链如今在 native 分支被编译为计数迭代器：`(0i64..n).map(…).filter(…)`，没有范围 `Vec`（#2045 的融合工作和 #3824 的计数 fold 头部）。2026-10-20 在 M4 Pro 上重测（develop 5d52899e2、`--release`、`n` 从 argv 读取，使用 `% 1000` 的循环体使 LLVM 无法将循环化为闭式，9 次交错运行）：Almide 中位数为 18.0 ms，最大 RSS 为 1.5 MB，手写 Rust 迭代器链为 14.0 ms，因此是 1.29x，而非 6.6x。目前还没有基准行对这一形态设置门禁。
