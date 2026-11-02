<a id="adr-0011-the-execution-substrate-is-a-free-variable--and-one-arms-output-is-the-last-hole-in-the-determinism-claim"></a>

# ADR-0011：执行基底是自由变量 — 一个 arm 的输出是确定性主张的最后缺口

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0011-execution-substrate-is-a-free-variable.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0011-execution-substrate-is-a-free-variable.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0011-execution-substrate-is-a-free-variable.zh-CN.md)

> 非规范性历史读者快照，未修改固定原本为权威来源。不自动同步，未独立执行、重跑测量或认证当前安全性／等价性。状态仍为Proposed并附带有日期的D2a补充；预期收益和gate不视为已实现。原文两年时间线、旧timeout撤除与后来oracle-tier形式及旧路径保留，不与当前release统一。runtime支持和唯一性是原文当时主张，此处未重新核实。测量限于列明硬件、版本、重复次数及spike进程启动条件。D1 trap flush推理和广泛无条件等价措辞仍是设计主张，不是普遍证明；保留F1–F6限制。develop链接是原文可变引用，本读者版自身基于固定pin。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md)

- **状态**：提案，**2026-10-04 补充 D2**（§D2a：wasm 计算并行以实例为单位，不共享内存 — #3003 裁定）。
- **日期**：2026-08-07
- **决定范围**：`fan` 的执行基底（实际并发运行什么）、各 target 的选择、将基底作为自由变量的前提，以及纠正 C-004 EXCEPTION 的分类。
- **相关**：[async-inception.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/async-inception.md)（观测宪章）、[execution-inception.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/execution-inception.md)（本 ADR 宪章）、[concurrency-stance.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/concurrency-stance.md)（#1000）、[ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)（确定性时钟）。
- **对台账的影响**：C-004 EXCEPTION 的**分类错误**（理由 R1）。C-006 的“sole stdlib surface”描述也因此不准确。

<a id="context"></a>

## 背景

<a id="実測--例外条項を再現した"></a>

### 实测 — 复现例外条款

契约台账 C-004 有如下例外：

> EXCEPTION：`fan { }` block arm 与 `fan.settle` thunk 内副作用交错在 native 按墙钟发生（两者均使用真实线程），wasm 则顺序执行；只固定其 RESULT（tuple / list）顺序。

这读起来是“native 与 wasm 的输出交错不同”。**确实运行了它**（2026-08-07，almide 0.56.0 / wasmtime 47.0.2 / 14 核 / arm64 macOS）。

```almide
effect fn arm(tag: String, extra: Int) -> Int = {
  let v = spin(6000000 + extra, 1)   // extra は env.args() 由来 — 定数畳み込み不能
  println("${tag}")
  v
}

effect fn main() -> Unit = {
  let n = list.len(env.args()!)
  let (a, b, c, d) = fan { arm("A", n)  arm("B", n)  arm("C", n)  arm("D", n) }
  println("sum=${a + b + c + d}")
}
```

```
native × 10 回 — 出力順:
   2  ACDB
   1  DCAB / DBCA / CBAD / BDCA / BCAD / BACD / ADCB / ACBD
   → 10 回で 9 通り

wasm × 3 回 — 出力順:
   3  ABCD                                    ← 常に arm 順
```

如条款所述，native 与 wasm 不一致。但同时出现了**条款未描述的事实**。

<a id="見つかった事実--native-は自分自身とも食い違う"></a>

### 发现 — Native 也与自身不一致

10 次出现 9 种顺序。这不是 target 间差异，而是**同一二进制、同一输入，不同运行的观测改变**。台账已为此性质命名。C-006 写道：

> 比跨 target 分歧更糟：deadline 是否触发取决于机器负载，因此即使**两次 native 运行之间**，结果也不是 program + inputs 的函数 — **这是唯一违反该性质的 stdlib 表面。**

这正是 0.29.0 删除 `fan.timeout` 的理由。C-006 说它是**唯一**的表面。上述实测显示，该表述当时已不准确。在 `fan { }` 中放入两个打印 arm，当时仍在交付同样的性质。

遗漏不在“存在例外”，而在**例外分类**。C-004 将现象写入 cross-target 栏。跨 target 分歧是台账日常处理的熟悉类别，其中也有 `env.os` 这样的设计使然。因此条款不显眼。实际上它是 determinism hole，正是 0.29.0 清理所针对的类别。

这是第二次低估 C-004 EXCEPTION。条款末尾说：

> 直到 #915 审计，这里一直少算了 block：它在 native 为每个 arm 创建 scoped thread，与 settle 属于同一交错类别。

#915 纠正了**范围**低估：不仅 `fan.settle`，也包括 `fan {}`。本 ADR 纠正的是**类别**低估。

<a id="第-3-のオラクルは-native-の側にいない"></a>

### 第三个 Oracle 不站在 Native 一边

`crates/almide-interp/src/eval.rs:148` — 参考解释器的 fan 是顺序的。

> Fan block：按源代码顺序逐个求值表达式 — 两个 backend 都退化到的确定性模式。

三路 oracle（native / wasm / interp）中，**wasm 与 interp 顺序执行且一致，只有 native 偏离**。这是二比一，而且不必争论哪个正确：顺序侧就是 #1000 定义的观测，“与按 list 顺序逐个求值严格相同”。

<a id="基体は誰も決めていない"></a>

### 没有人决定基底

为何如此？**执行基底不是设计产物，而是实现便利留下的结果。**

| target | 基底 | 来源 |
|---|---|---|
| native | OS 线程（`std::thread::scope`） | `codegen/templates/rust.toml` `[fan_expr]` / `[fan_effect]` |
| wasm | 完全顺序（arm 内联展开） | `crates/almide-mir/src/lower/desugar_fan.rs` |
| interp | 完全顺序 | `crates/almide-interp/src/eval.rs` |

宪章没有写过“native 使用 OS 线程，wasm 顺序运行”。native 用 `std::thread::scope`，只是因为 Rust 有它；wasm 顺序，只因 wasip1 没有线程。**基底处于宪章空白地带，空白泄漏进了观测。**

泄漏有两个方向。向下（基底 → 观测）是上述例外，向上（基底 → 表面）也已实测：`spec/wasm_cross/fan_race_mapper.almd` 在 wasm 正常结束，**native 却遇到 wall**（`op "Prim Handle" in main — outside the rung subset`，exit 1）。宪章§3 宣称 head × form 矩阵所有格子已确定，但从 native 看有空格。

（比较全部 23 个 `spec/wasm_cross/{fan_*,fuel_*}.almd` 在两个 target 上的表现，除这一项之外，22 项 stdout 和 exit code 均逐字节一致。向上泄漏一项；向下为零，因为 fixture 避开了两个打印 arm 同时存在的构成 — 这种回避本身就是条款的自白。）

<a id="decision"></a>

## 决定

**执行基底是与观测分离的自由变量。它只是性能旋钮，不得改变观测的任何一位。wasm 并发取自 WASI 0.3 native async，不采用共享内存和 atomics。**

<a id="d1-rung-1arm-単位の出力トランザクションは前提条件であり改善ではない"></a>

### D1. Rung 1（逐 Arm 输出事务）是前提，不是改进

`fan` 每个 arm 的 stdout / stderr 进入独立缓冲区，join 时按 **arm 顺序** flush。宪章§4 支柱5 已将其设计为 Rung 1；本 ADR 将其从“以后到来的改进”提升为**讨论基底本身的资格**。

理由有两个，按重要程度排列：

1. **这是 determinism hole，不是性能问题**（背景）。对相同性质，作出与 0.29.0 对 `fan.timeout` 相同的判断。
2. 次序不能颠倒。没有 Rung 1 就并行化基底，会将“仅 native 非确定”**恶化**为“全部 target 非确定”。

Rung 1 必须解决三个设计问题，都由“与顺序运行一致”这一准则决定。

- **stdout 与 stderr 的相对顺序**：分开缓冲会丢失 arm 内 `println` → `eprintln` 的顺序。顺序执行时两者在 fd 层交错，所以 **arm 内记录为一条时间线**，flush 时分发到相应 fd。
- **trap 时 flush**：C-200 固定“发生 trap 的 sibling 走统一 abort，不等待 in-flight sibling”。顺序执行时 trap 之前 arm 的输出已出现，因此**先按 arm 顺序 flush 已完成 arm 的缓冲区，再 abort**。C-200 fixture 测得“empty stdout”是因为没有打印 arm，不与条款冲突（需增加 fixture）。
- **缓冲区上限**：打印 1GB 的 arm 缓冲 1GB，这是顺序执行没有的成本。按 arm 顺序的**第一个 arm 可以立即 flush**，因为前面没有其他 arm，与顺序执行一致。因此最坏情况降为“除第一个 arm 外全部输出的总量”。若实际构成仍不够，触发 F4。

<a id="d2-wasm-の並行性は-wasi-03-async-から取る"></a>

### D2. Wasm 并发来自 WASI 0.3 Async

以 `async func` / `stream<T>` / `future<T>` 为基底，已吸收进 Component Model 的 Canonical ABI，`wasi:io` 已撤除。不需要共享内存或 atomics。

<a id="d2a-補足2026-10-043003-wasm-の計算並列はインスタンス単位で取りメモリは共有しない"></a>

### D2a. 补充（2026-10-04、#3003）：Wasm 计算并行以实例为单位，不共享内存

D2 提供的是**并发 I/O**。持续占用 CPU 的 `fan` chunk 不会因 Component Model async 而加速：它是单实例内协作并发，计算 subtask 会运行到结束。使用 fannkuchredux 实测（n=10、10 chunk，所有方式输出 `73196` / `Pfannkuchen(10) = 38` 一致，wasmtime 47.0.2、arm64 macOS 14 核、交替运行5次取最小值）：

| 执行方式 | 墙钟时间 |
|---|---:|
| native，线程运行 `fan`（#2044） | 34.7 ms |
| native，将 `fan` 改写为顺序 | 178.6 ms |
| wasm，一个实例（当时状态） | 199.9 ms |
| wasm，p3 component（async ABI） | 202.8 ms |
| wasm，每 chunk 独立实例，顺序 | 249.8 ms |
| wasm，每 chunk 独立实例，10 路并行 | **47.9 ms** |

（最后两行是 spike：通过10个 `wasmtime run` 进程同时启动预编译模块，包含10次进程启动成本。）

决定：

1. **Wasm 计算并行的唯一手段为：在 OS 线程上运行同一模块的不同实例。** 每个实例仍是当前单线程模块，拥有自己的 heap。不共享线性内存，不 emit atomics 或 shared memory — C-210 的 `FORBIDDEN = ["relaxed", "atomic", "shared"]`、所有权证书、region allocator 都保持成立（D3 不受损）。
2. 从**纯、标量输入输出的 chunk**开始（与 native #2044 线程拆分同一判定：无 effect、无可变 capture；元素、结果、capture 为标量或标量 tuple）。一个 chunk 成为一个 export，host 在 N 个实例调用该 export，按 list 顺序收集结果。较大数据（list、string）的 copy-in/out 留待后段（#3003 stage 2）。
3. 只有**提供此能力的 host** 才能并行（当时为嵌入式 host）。产物只有一个；不提供的 host（普通 wasmtime、其他 WASI runtime、`to_wasi` / p2 / p3 形式）仍按原样顺序运行。host 选择基底，观测与顺序执行相同（ADR-0024、C-321）。
4. 并行失败（子实例 trap、abort、exit）时，丢弃结果并顺序重跑。chunk 是纯的，故重跑除时间外不可观测。abort 文本与 exit code 原样来自顺序执行。

否决项：

- **wasi-threads + 共享内存**：如 D3；还必须将 allocator（bump pointer、free list、region window 保存/恢复）与 RC 全部改为 atomics 或锁，违反 C-210，甚至让单线程程序也更慢、更大。速度上限与本方案相同（计算并行的工作量相同），没有收益。2026-10-04 再确认 wasmtime 47.0.2 仍在运行时拒绝 `-S threads=y`。
- **用 p3 async subtask 并行计算**：如上表，不会加速（202.8 ms）。D2 仍是重叠 I/O 的路线（`fan.map(fs.read_text)` 预取，b973a1dd4）。

此补充的可证伪条件为下文 F6。

<a id="d3-wasi-threads--shared-everything-threads-は採らない"></a>

### D3. 不采用 Wasi-Threads / Shared-Everything-Threads

前者是2023-08撤回的 legacy proposal，wasmtime 47.0.2 **在运行时拒绝** `-S threads=y`。后者未在任何 WASI host runtime 实现（R2）。

因此，`crates/almide-mir/tests/deterministic_profile_test.rs` 中的 `FORBIDDEN = ["relaxed", "atomic", "shared"]` **原样保留**。不修订 C-210。

<a id="d4-native-の-os-スレッドは維持する"></a>

### D4. 保留 Native OS 线程

若基底差异不影响观测，可按 target 选择最快基底，这就是自由变量的含义。为匹配 wasm 而将 native 顺序化，是为观测约束基底，因果颠倒。#1000 明确允许并行执行作为性能事项：“执行可以并行，但可观测行为与顺序求值严格相同”。

<a id="d5-基体差分ゲートを新設する"></a>

### D5. 新增基底差分 Gate

将含打印 arm 的程序在全部基底（native 顺序 / native 线程 / wasm 顺序 / wasm async / interp）上**各运行 N 次**，机械检查 stdout、stderr、exit code 一致。必须跨运行重复：本缺陷一次运行不可见，10次出现9种顺序才暴露。

<a id="d6-c-004-と-c-006-を訂正する"></a>

### D6. 纠正 C-004 和 C-006

在 Rung 1 落地的 PR 中删除 C-004 EXCEPTION（ratchet 下降），并为 C-006 的“the sole stdlib surface violating that property”补充：**`fan {}` / `fan.settle` 的输出交错自 0.24.0 起作为同类别第二例存在，由本 PR 关闭**。台账主张不仅在变正确时，也应在发现原先不准确时修正。

<a id="rationale"></a>

## 理由

<a id="r1-例外条項つきの等価性はこの-repo-の運用と整合しない"></a>

### R1. 带例外条款的等价性不符合本仓库运作方式

契约台账一直将 `flagged-for-revision` ratchet 保持为零。C-006 为此自豪：“这退役了台账最后一条 flagged-for-revision；ratchet 为零，因此等价主张没有等待修复的分歧条款。”

同一段承认一个例外：`env.os` / `env.temp_dir` 的平台报告（C-189）。此 carve-out 是**设计使然**，不是待修问题。

C-004 EXCEPTION 两者皆非：不在 ratchet 上，也不是设计 carve-out。**自 0.24.0 起未经分类存在了两年。** 如果正确分类，本应在 0.29.0 清理时一同消失。

<a id="r2-共有メモリで並列にするは-2026-に選択肢として存在しない実測"></a>

### R2. 2026 年不存在“共享内存实现并行”这一可选项（实测）

concurrency-stance.md 用“wasm32 没有线程”解释 wasm 顺序性。截至2026-08，该前提不再直接成立：threads proposal 本身已标准化，wasmtime 接受 `-W threads` / `-W shared-everything-threads` feature flag。若拒绝理由改变，应怀疑是否可恢复（ADR README 规则2）。对此提出怀疑，并以实机排除。

```
$ wasmtime --version
wasmtime 47.0.2 (90fed3c6a 2026-07-21)

$ wasmtime run -S threads=y t.wasm
Error: the `-Sthreads` flag is no longer supported        ← ヘルプには載っているが拒否される

$ wasmtime run -W shared-everything-threads=y t.wasm
hi                                                        ← flag は通るが提案自体が未実装
```

`-S help` 仍显示 `threads[=y|n] -- Enable support for WASI threading imports
(experimental). Implies preview2=false.` 只读帮助会得出“虽 experimental 但可用”。**实际运行却被拒绝。**

第一手信息也一致：wasi-threads 于2023-08撤回，为 shared-everything-threads 让路，仅为只支持 preview1 的引擎保留 legacy。shared-everything-threads 处于早期，尚无 WASI host runtime 实现。

**没有可与之权衡的对手。** C-210 与并行性的取舍在2026年的格局中并不存在。

<a id="r3-wasi-03-は-atomics-なしに並行-io-をくれる"></a>

### R3. WASI 0.3 无需 Atomics 即提供并发 I/O

WASI 0.3.0 于2026-06-11发布，为 Component Model 引入 native async。`wasi:io` 撤除、功能吸收进 Canonical ABI；0.2 的 `start-foo` / `finish-foo` / `subscribe` 三步消失。支持 **Wasmtime 43+**，本地为47.0.2。

关键是这不是指令集问题。并发位于 component 边界的 ABI，不共享线性内存。不 emit atomics 或 shared memory，故 C-210 deterministic-profile 适配保持。“卖掉确定性换并行”是错误二选一，2026 wasm 不卖也能买。

<a id="r4-憲章-6-の-go-敗北行は決定性を売らずに埋まる"></a>

### R4. 宪章§6 相对 Go 的落后项无需牺牲确定性即可补上

> | Go | 生产 async 成熟度 | **oracle 层尚未实现，无法编写真实并发 I/O race** |

oracle 层需要将环境响应序列 ω 作为声明输入的契约类别（B1 已落地），以及**真正能够重叠 I/O 的基底**。后者就是 D2；宪章归为“时间问题”的落后，具体所需的时间内容就在这里。

<a id="r5-基体非依存性はこの言語だけが機械検査で言える"></a>

### R5. 只有本语言能通过机械检查宣称基底独立性

改变 Go、Rust、JS 的并行度会改变输出交错。这不是缺陷，而是选择了观测依赖调度的设计。Almide 用两年将观测固定于逻辑时间。**这项投资的回报是基底自由。**

但当时还不能如此宣称 — 上述实测正是反例。D1 是换取这句话所需的工程。

<a id="alternatives"></a>

## 替代方案

| 方案 | 判定 | 理由 |
|---|---|---|
| 采用 wasi-threads 并修订 C-210 | **否决** | 功能不存在，wasmtime 47.0.2 运行时拒绝（R2，2026-08-07实测） |
| 等待 shared-everything-threads | **远期搁置** | host runtime 未实现；若实现出现且不需 emit atomics，则重新评估 D3（F3） |
| 顺序化 native 消除例外 | **否决** | 观测一致但放弃基底自由，为观测约束基底颠倒因果（D4）；#1000 明确允许并行 |
| 将 EXCEPTION 正式化为 C-189 式设计 carve-out | **否决** | `env.os` carve-out 限于**报告平台的函数**，其目的本就是差异；输出交错不以差异为目的，这只会永久保留 determinism hole |
| 推迟 Rung 1，先引入 D2 | **否决** | 从“仅 native”恶化为“所有 target”（D1） |
| 保持现状 | **否决** | C-006 继续不准确，宪章 claim 2 固定带有例外条款 |

<a id="consequences"></a>

## 后果

<a id="得るもの"></a>

### 收益

- **关闭 determinism hole。** “观测是（program，inputs）的函数”连同输出交错都成为真。
- C-004 EXCEPTION 归零，等价主张不再有例外条款（D6）。
- 宪章 claim 2（native ⇄ wasm 观测等价）从有例外变为无条件。
- 新的第六句话：**切换基底不改变观测（有机械检查）**。
- 基底差分 gate（D5）结构性检测 `fan_race_mapper` native wall 这类“基底决定表面”的问题。

<a id="払うもの"></a>

### 代价

- **Arm 输出不再实时出现。** 第一个 arm 可立即 flush（D1），第二个起需等前面 arm 完成。与顺序运行一致，按定义正确；但带进度显示的长 arm 体验会改变（F4）。
- 缓冲内存成本，最坏为“除第一个 arm 外所有输出总量”（D1）。
- **Wasm backend component 化成为前置工程。** 当时 emit 的是 `wasi_snapshot_preview1` core module（`render_wasm_p3.rs`）。WASI 0.3 async 位于 Component Model 之上，迁移不小。本 ADR 将 D2 采纳为**方向决定**，开始时间交由 execution-inception.md。
- Gate 时间增加：D5 运行全部基底 × N 次的笛卡尔积。

<a id="falsifier"></a>

## 可证伪条件

- **F1**：若切换基底改变观测且 Rung 1 无法封堵，移除该基底。若不能守住 D1，撤回 D2 和 D4，所有 target 顺序运行（为保观测放弃基底自由，不反过来）。
- **F2**：若实测 WASI 0.3 async 开销超过顺序执行，仅撤回 D2，wasm 保持顺序。D1 / D3 / D5 不论基底数量都保留。
- **F3**：若 shared-everything-threads 有 host runtime 实现，且无需推翻 C-210 FORBIDDEN 即可使用，则重新评估 D3；否则维持。
- **F4**：若实际用户投诉逐 arm 缓冲破坏长 arm 进度流，**不提供 opt-out**（这会让观测重新依赖基底，使本 ADR 失效）。重新设计 flush 边界粒度，或在 `fan` 外的语法处理进度。
- **F5**：若 D5 即使重复 N 次也无法复现缺陷（例如 CI 单核），则 gate 通过而 hole 仍在。改为**直接指定基底**（强制顺序基底的 flag）检查，而非依靠重复次数。

- **F6**（D2a）：若在目标范围（纯、标量输入输出 chunk）内实测出子实例启动和结果回收成本使逐实例并行慢于顺序，则将该形态恢复顺序（缩小判定范围）。只要有一个构成因是否并行而改变观测（stdout、stderr、exit code），就撤回 D2a，全部 wasm 恢复顺序（与 F1 相同次序）。

<a id="references"></a>

## 参考资料

- [WASI 0.3 · WASI.dev](https://wasi.dev/releases/wasi-p3) — 0.3.0 内容（native async、`async func` / `stream<T>` / `future<T>`）。
- [Bytecode Alliance — WASI 0.3 Launched](https://bytecodealliance.org/articles/WASI-0.3) — 2026-06-11发布、Wasmtime 43+、撤除 `wasi:io`。
- [WebAssembly/wasi-threads](https://github.com/WebAssembly/wasi-threads) — legacy proposal，2023-08撤回。
- [WebAssembly/shared-everything-threads](https://github.com/WebAssembly/shared-everything-threads) — 后继 draft，host runtime 未实现。
- [Roadmap · WASI.dev](https://wasi.dev/roadmap) — 0.3.x release train 与 WASI 1.0（2026年末至2027年初）。
- 本仓库（2026-08-07实测，almide 0.56.0 / wasmtime 47.0.2 / 14核 arm64 macOS）：四 arm `fan {}`，native 10次9种顺序与 wasm 3次1种顺序；23个 `spec/wasm_cross/{fan_*,fuel_*}.almd` 的双 target 比较；`codegen/templates/rust.toml` `[fan_expr]`、`crates/almide-interp/src/eval.rs:148`、`crates/almide-mir/src/render_wasm_p3.rs`。
