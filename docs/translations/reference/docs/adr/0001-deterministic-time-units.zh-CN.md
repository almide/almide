<a id="adr-0001-deterministic-budgets-are-written-in-time-units"></a>

# ADR-0001：确定性预算用时间单位书写

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0001-deterministic-time-units.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0001-deterministic-time-units.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0001-deterministic-time-units.zh-CN.md)

> 非规范性历史读者快照，未改动的固定原本为权威。不自动同步，未独立重跑benchmark或证明，不保证当前语法/特性。保留八月决定及后来被替代的fan形式、计划校准gate、单位无关证明主张；不代表今日所有实现/host均满足。原文张力不擅改：“不到2秒”与所示0.514 total，54倍比较使用所引user时间而不重算user+system，相对成本冻结/纠错与绝对值可变并存，以及声称12字符差。原文本身说Stage 1才产生首次校准实测。oracle一致以固定ω为条件，不是实时墙钟运行相等。代码fence和引用保持原语言及字节。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)

- **状态**：已接受
- **日期**：2026-08-01
- **决定范围**：`fan.bounded` / `fan.race` 计算量预算的表层写法，以及确定性时钟的单位定义
- **相关**：[async-inception.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/async-inception.md)（章程）、[logical-time-async.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/logical-time-async.md)（语义）、[fan-v2.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/fan-v2.md)（语法）
- **批准**：2026-08-01的两项需求讨论（“能用传统ms书写”×“完备性”）批准当前形式。考虑后拒绝单一Duration类型方案（见替代方案），并在S8补充墙钟侧完备性机制。

<a id="前提--時間は-1-つではない"></a>

## 前提 — “时间”不只一种

本ADR决定“用时间单位写预算”。此前需要明确：**即便同叫秒，测量对象也不同**。混淆此点就无法理解后续讨论。

一次运行至少可得到两种不同时间。

```
プログラム開始 ──────────────────────────────────────────── 終了
              [計算]   [ネット待ち]   [計算]   [ディスク待ち]  [計算]
               30ms      500ms        20ms      200ms        10ms

  壁時計時間 = 760ms   ← 部屋の時計で測った経過。待ち時間も込み
  CPU 時間   =  60ms   ← CPU が実際にこのコードを実行していた分だけ
```

可在本地验证。用同一 `time` 测量只睡2秒的进程和不停计算不到2秒的进程，得到如下结果（实测）：

```
$ time python3 -c "import time; time.sleep(2)"
0.04s user   0.02s system   2% cpu   2.173 total     ← 壁時計 2.17s / CPU 0.04s

$ time python3 -c "x=0
for i in range(8000000): x+=i"
0.39s user   0.03s system  80% cpu   0.514 total     ← 壁時計 0.51s / CPU 0.39s
```

上例**墙钟时间是CPU时间的54倍**。睡眠时CPU时间不前进。Cloudflare的出租车比喻很贴切：司机停下加油吃点心时计价器仍走，那是墙钟；**真正行驶的部分**才是CPU时间。

必须区分，因为墙钟**会因代码之外的因素变化**：API慢的一天、繁忙机器、旁边其他进程。这也是Cloudflare按CPU时间收费的理由：“CPU time is more predictable and **under your control**… **purely a function of the logic and processing of inputs on outputs to your Worker**”。

Almide的**确定性时钟沿这个方向再走一步**。

| | 包含等待 | 依赖机器速度 | (程序, 输入) 的函数 |
|---|---|---|---|
| 墙钟时间 | **包含** | 是 | ✗ |
| CPU时间 | 不包含 | **是**（CPU更快则更短） | ✗ |
| **确定性时钟**（本设计） | 不包含 | **否** | **✓** |

CPU时间仍依赖机器。同样代码在更快CPU上更短，不是“每台机器同值”。确定性时钟最后一步是**停止实测**：冻结的抽象机器成本表（CM-1）定义“此op相当于多少纳秒”，把执行的op相加。因为不测量，快机器、慢机器、native、wasm都**必然同值**。

所以 `compute.ms(100)` 不是“等待墙钟100毫秒”，而是**“让它做100毫秒计算量的工作”**。快机器可能墙钟40ms就结束，慢机器可能需300ms。仍然**在所有机器的同一位置截断**，这就是“确定性”。

“单位是秒，却不是墙钟”本身不新。POSIX `ulimit -t 60`（CPU时间60秒后杀进程）已运行50年。最准确的定位是本设计再加上**机器无关性**。

<a id="用語集"></a>

## 术语表

| 术语 | 含义 | 参考 |
|---|---|---|
| **墙钟**（wall clock） | host真实时间，`Date.now()` / `Instant::now()` 返回的量。依赖host速度、负载、调度器，**不是**(程序, 输入)的函数 | 通用术语 |
| **逻辑时钟**（logical clock） | 独立于物理时间的进展尺度。**注意术语冲突**：分布式Lamport逻辑时钟计数happens-before偏序，与本ADR中程序自身执行推进的确定性计数器不同 | [Lamport 1978](https://lamport.azurewebsites.net/pubs/time-clocks.pdf)（同名异义） |
| **确定性时钟**（deterministic clock） | 本设计的逻辑时钟，按CM-1成本表在每charge site推进。值为(程序, 输入)函数，在native/wasm/全部host一致。与墙钟成对，**fan head声明读哪一种** | [logical-time-async.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/logical-time-async.md) |
| **charge site** | 确定性时钟推进的**唯一位置**，如共享MIR的basic block入口。fuel构造在此读计数器，oracle构造在同处读环境（统一中断点原则） | [logical-time-implementation.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/logical-time-implementation.md) |
| **lockstep语义** | 描述race胜者选择的图景：所有分支以同一步幅推进确定性时钟，先完成者胜，同刻按源顺序。实现使用等价刻画“(消耗, index)字典序最小”。**两者一致**正是“无物理时间的race”的内容 | [logical-time-async.md §确定性事件规则](https://github.com/almide/almide/blob/develop/docs/roadmap/active/logical-time-async.md)、[Decisive.lean `Ev.prec`](https://github.com/almide/almide/blob/develop/crates/almide-race-belt/AlmideRaceBelt/Decisive.lean) |
| **版本化抽象成本表**（CM-1） | 为每MIR op分配确定性时钟步长。是契约台账中的**语义对象**，常量变更视为semantic change（版本提升）。EVM gas schedule是同构先例 | [Yellow Paper](https://ethereum.github.io/yellowpaper/paper.pdf) Appendix G |
| **EVM**（Ethereum Virtual Machine） | Ethereum执行环境。所有节点对同一状态转移使用同一gas schedule，得到同一out-of-gas决定以维持共识。**确定性计算量上限最大规模的生产实例** | [Yellow Paper](https://ethereum.github.io/yellowpaper/paper.pdf) / [ethereum.org: gas](https://ethereum.org/developers/docs/gas/) |
| **race belt的7个定理** | `crates/almide-race-belt/` 的Lean 4定理（0 sorry，由CI `lean-proofs` kernel-check）：决定事件唯一性、子集稳定性、剪枝cap不能隐藏决定事件与trap可见窗口、合流。**全部与单位无关**（事件时间为 `Nat`），所以本ADR单位变更不改一个字符 | [Decisive.lean](https://github.com/almide/almide/blob/develop/crates/almide-race-belt/AlmideRaceBelt/Decisive.lean) |
| **fuel** | 实现确定性时钟的**机制侧**术语（内部计数器、`--fuel-probe`、Wasmtime / EVM谱系），不进入表层词汇 | [Wasmtime: fuel vs epoch](https://docs.wasmtime.dev/examples-interrupting-wasm.html) |

<a id="context"></a>

## 背景

需要确定确定性计算量预算（`fan.bounded` / `fan.race`）的表层写法，设计依次经过三个方案。

1. **`fuel:`** — 名称显露Wasmtime / EVM谱系。拒绝：机制隐喻泄漏表层，fuel是燃料不是单位。
2. **`ticks:`** — “标签=单位名”与 `ms:` 一致，正是lockstep语义的单位。到这里逻辑通顺。
3. 然而 `ticks: 100_000` 有**无法写出的数字**这一缺陷。

第三项是本ADR起点。tick绝对值依赖CM-1（版本化抽象成本表），人和LLM事前都没直觉。预算还耦合body实现，重构 `optimal_plan` 后 `100_000` 就过时。**把编辑会腐化的魔法数字设为必填参数，几乎违背宣称modification survival的语言自身目标。**

预算错误**两个方向都静默**：过小总回退，过大实际无界，两者诊断都抓不到。

<a id="decision"></a>

## 决定

**确定性计算量预算用时间单位书写。类型承载单位，使用现有语法的函数调用构造，不添加新字面量语法。**

```almide
fan.bounded(compute.ms(100)) { optimal_plan(g) } ?? greedy_plan(g)   // 決定的時計
fan.race { exact(p); heuristic(p) } ?? fallback(p)                    // 予算なしが基本形
fan.race(compute.ms(500)) { search_a(p); search_b(p) } ?? none        // 発散ガードは任意
fan.timeout(duration.s(5)) { http.get(url) } ?? cached                // 壁時計（oracle 層）
```

同时确定以下内容。

<a id="d1-論理時計の単位は時間である"></a>

### D1. 逻辑时钟的单位是时间

CM-1给每MIR op分配“**冻结Almide抽象机器上的耗时**”。程序消耗是总和，即一个持续时间，是(程序, 输入)函数；每host同值。**所数的东西确定（fuel侧），名称是时间。** 两者可分，证据见D6。

<a id="d2-型で分ける単位ではなく"></a>

### D2. 用类型区分，而非单位

确定性时钟量与墙钟量是**不同类型**（类型名确定为 `Compute` / `Duration`，S1），均共享ms/s单位。调用点由head区分，但经变量或配置记录混入，只能靠类型阻止。是类似Go `time.Duration` 的**包装类型**，现有语法可表示（单case variant `type Compute = Compute(Int)`，或像 `fan` 一样为编译器已知类型）。实现形式由PR确定，**两种都不需新语言语法**。

**裸整数是类型错误**。`fan.bounded(100)` 以“expected Compute, found Int”不能编译。接收无单位整数的API造成千倍事故的实例很多，从Go `time.Sleep(10)`（10纳秒）到Jenkins“300秒→3.5天”。

<a id="d3-構築は現行構文の関数呼び出し--新しいリテラル構文は追加しない"></a>

### D3. 用现有函数调用语法构造，不加新字面量语法

单位由module函数名承载。**module指明时钟，函数指明单位。**

```almide
compute.ms(100)     compute.s(2)      compute.us(500)     // 決定的時計 → Compute
duration.s(5)       duration.ms(300)  duration.min(3)     // 壁時計 → Duration
compute.ms(n)                                             // 変数もそのまま
```

单位名集合为 `ns` / `us` / `ms` / `s` / `min` / `h`（只用ASCII，不用 `µs`；分钟用 `min`，因为 `m` 有歧义）。不加天及以上单位（与Go无Day同理）。把此集合**作为完备性matrix放入gate**；手工维护表层会漂移，LLM会编造 `msec`、`5m`。

**不采用字面量后缀（`100ms`）。** 初版曾决定用它，但给lexer添加数字后缀需要**新增语言语法**。Almide现无此机制，仅为 `fan` 表层扩展全语言词法规则不划算。函数形式只增加字符数，单位显式性、类型安全、可gate性都相同。

未来即便添加 `100ms`，**这些函数就是脱糖目标**（`100ms` ≡ `compute.ms(100)`）。所以当前决定不封死未来语法糖，条件为下文D3-F。

<a id="d4-抽象機械は凍結する"></a>

### D4. 冻结抽象机器

会修正模型错误（某op相对成本错了），**不跟随硬件进化对整体重新缩放**。10年后与墙钟比例可能漂移数倍，但与今天机器间差异同量级，换来“程序含义永远不变”。

<a id="d5-校正はゲートで機械検査し対応幅を宣言する"></a>

### D5. 用gate机械检查校准并声明范围

CI测量确定性ms与实测墙钟ms的比例，检查落在声明区间内。记录区间外的工作负载类别。报告**总是并列显示两者**：

```
this region: 0.42ms deterministic (≈0.31ms wall here)
```

<a id="d6-比のみが契約絶対値は非契約を初日に宣言する"></a>

### D6. 首日即声明“只有比值是契约，绝对值非契约”

op间相对成本是契约，绝对值不是。CM版本提升可能改变绝对值。

<a id="仕様--時間指定の完全な表面normative"></a>

## 规格 — 时间指定的完整表层（规范性）

将D1–D6落成实现PR可直接转写的完整表层规格。**完备性遵循repo matrix原则**：完整枚举存在单元，缺失单元声明为带理由与重开条件的有意省略，并用可执行gate检查整个表面。

<a id="s1-型"></a>

### S1. 类型

| 类型 | 含义 | 构造module | derive |
|---|---|---|---|
| `Compute` | 确定性时钟量（前提表第三行） | `compute` | `Eq, Ord, Repr` |
| `Duration` | 墙钟量 | `duration` | `Eq, Ord, Repr` |

- **表示**：内部单位纳秒，一个 `Int`（i64），上限≈292年。与Go同上限，但调研事故源不是上限而是**wraparound**（Swift `sleep(nanoseconds: .max)` 即返bug）及**D×D**（Go #64420）。前者用饱和运算（S3）消除，后者用类型错误（S3）消除。
- 两类型具有**相同运算表层**（S2/S3 matrix按类型各复制，gate检查双方）。
- 编译器已知类型或stdlib `.almd` 类型由实现PR自由选；本节已确定可观测规格，两种不改变它。

<a id="s2-構築--2-時計--6-単位の完全-matrix"></a>

### S2. 构造 — 2时钟 × 6单位完整matrix

| 单位 | `compute.*` | `duration.*` |
|---|---|---|
| `ns` | `compute.ns(n)` | `duration.ns(n)` |
| `us` | `compute.us(n)` | `duration.us(n)` |
| `ms` | `compute.ms(n)` | `duration.ms(n)` |
| `s` | `compute.s(n)` | `duration.s(n)` |
| `min` | `compute.min(n)` | `duration.min(n)` |
| `h` | `compute.h(n)` | `duration.h(n)` |

12单元全部存在，均由gate检查。规则：

- 参数仅 `Int`（`Float` 版有意省略，S7）。
- **负值确定性trap**（台账常量消息，以两目标观测一致fixture固定）。构造后值不变量≥0；0有效。
- 构造乘法溢出（如 `compute.h(大値)`）**饱和**。
- **UFCS形式 `n.ms()` 是歧义错误**：接收者 `Int` 同时匹配 `compute.ms`、`duration.ms`，不许靠推断选择时钟；静默默认违背本ADR存在理由。诊断指明两候选，要求限定形式。
- 单位名集合与CLI（S4）**共享单一来源**（`scripts/lib/contract-classes.txt` 方式）。

<a id="s3-演算-matrix"></a>

### S3. 运算matrix

`T` 分别为 `Compute` / `Duration`（每类型相同表面）：

| 运算 | 判定 | 理由 |
|---|---|---|
| `T + T → T`（饱和） | **有** — 组合预算/期限 | |
| `T - T → T`（0处饱和） | **有** — 计算剩余量 | 不为负（不变量≥0） |
| `T * Int → T` / `Int * T → T`（饱和） | **有** — 非对称缩放 | Go #20757教训：“用数缩放，不用duration缩放” |
| `T * T` | **类型错误** | Go #64420静默10⁹倍事故。Go无法用类型表达，需专门linter；我们以类型消除 |
| `T / Int` | 有意省略（S7） | 推迟舍入语义决定 |
| `==` / `<` 等（同类型内） | **有**（`Ord`） | |
| `Compute ⊕ Duration`（所有运算） | **类型错误** | 时钟混合，正是Ada分类型原因 |
| `Int ⊕ T`（缩放除外） | **类型错误** | 阻断裸整数（D2） |
| `Compute ↔ Duration` 转换函数 | **不存在** | 两时钟无语义桥梁。唯一桥是tooling并列显示（D5），是报告不是值 |

<a id="s4-消費者-matrix--どこに何の型が現れるか"></a>

### S4. 消费者matrix — 哪里出现什么类型

| 表层 | 接收类型 | 时钟 | 数量 |
|---|---|---|---|
| `fan.bounded(c) { body }` | `Compute` | 确定性 | 必须1 |
| `fan.race { arms }` / `fan.race(c) { arms }` | `Compute` | 确定性 | 可选0..1（发散guard） |
| `fan.race(xs, f)` / `fan.race(c, xs, f)` | `Compute` | 确定性 | 可选0..1 |
| `fan.timeout(d) { body }`（Stage 4，oracle层） | `Duration` | 墙钟（S8） | 必须1 |
| effect表层期限（http/process等；形状留待未来设计，只在此固定类型） | `Duration` | 墙钟（S8） | — |
| CLI `almide run/test --budget 500ms` | `Compute`（CLI字符串用相同6单位的后缀；CLI不是语言语法，因此允许后缀） | 确定性 | 可选 |
| 报告（`--time-report`） | 两者并列：`0.42ms deterministic (≈0.31ms wall here)` | 两者 | D5 |

时钟列与契约台账声明1:1对应，由S6-6 gate检查表面。

错误放置的诊断连同文案用fixture固定（`expected Compute, found Duration` 加head引导提示，与fan-v2-examples/diagnostics.almd一致）。

<a id="s5-端の意味論"></a>

### S5. 边界语义

- **零预算**：check-then-charge，所以charge前完成的表达式成功：`fan.bounded(compute.ms(0)) { 42 }` → `Ok(42)`。踏到首次charge的表达式耗尽。
- **负值**：构造时trap（S2）使负值不能到达消费侧。
- **饱和上限**：最大 `Compute` 有限（≈292年）但无法实际执行如此大的量，区别于race“无预算=n无限”；无预算是无参数，上限则是值。

<a id="s6-実行可能ゲート実装-pr-と同一-pr-で-land"></a>

### S6. 可执行gate（与实现同PR落地）

1. 12构造单元的存在及类型签名（S2 matrix表面检查）。
2. 负值trap的双目标观测一致fixture（`spec/wasm_cross`）。
3. 每类类型错误各一诊断fixture：裸整数/混合时钟/`T * T`/UFCS歧义。
4. CLI单位名集合=语言单位名集合（检查从单一来源生成）。
5. 饱和边界fixture（溢出构造、`T - T` 的0饱和）。
6. **时钟声明表面检查**：全部接收 `Compute` / `Duration` 的表层与S4时钟列（=台账声明）全集一致；CI拒绝无时钟声明的时间消费表层。

<a id="s7-意図的省略の台帳再開条件つき"></a>

### S7. 有意省略台账（附重开条件）

| 省略 | 理由 | 重开条件 |
|---|---|---|
| `d`（日）及以上单位 | 对预算/期限无意义，与Go无Day同理 | 记录真实需求 |
| `Float` 构造器（`duration.s(1.5)`） | Go #20757的float误用类别；用 `ms` 时整数足够 | dojo观测到整数换算书写错误 |
| `T / Int` | 推迟舍入语义（截断/最近） | 真实需求 |
| `min` / `max` helper | 嵌套min-cap由语义隐式承担（EIP-150式） | 显式预算组合需求 |
| accessor（如 `in_ms(c)` 读取） | `Repr` 足够显示 | 把预算用于计算的实例 |
| `Codec` derive | 无跨序列化边界用例 | 用例出现 |
| 字面量后缀 `100ms` | D3所述，lexer扩展交易不成立；脱糖目标已定 | D3-F（dojo实测） |

<a id="s8-壁時計側の完全性--timeout-が従来の-ms-で書けてなお完全である理由"></a>

### S8. 墙钟侧完备性 — timeout为何能用传统ms且仍完备

`fan.timeout(duration.ms(5000))` 和effect表层期限可像通用语言惯例用ms书写。以下三层使其不违背确定层完备性。用墙钟决定胜者或结果的构造不能进入确定层，这本身是T9定理，不是偏好。

1. **时钟声明matrix（S4时钟列）**：所有消费时间量的表层在契约台账声明自己读的时钟。无声明者被gate（S6-6）拒绝。台账总能回答“此ms是哪种时钟”，这是共享单位仍不混合的根据。
2. **timeout事件属于ω**：到期作为“环境在哪个charge site令期限到期”的oracle输入（统一中断点原则：oracle构造在与确定层相同site读取环境）。契约类别为oracle-relative（R_Ω），主张成为相对形式“对∀ω观测一致”。
3. **record/replay是可执行证据**：以记录的ω replay逐字节重现观测；native记录、wasm replay也按定义成立。**不主张**实时运行跨目标一致；这是oracle层诚实的边界，主张的是“固定ω则一致”。

一句话：**墙钟不是非确定性，而是声明的输入。** 输入可记录，记录则可重现，重现便进入契约与gate的范围。传统ms接口与完备性在这一结构上共存。

<a id="rationale"></a>

## 理由

<a id="決定的だったのは計算予算は実質すでに時間であるという発見"></a>

### 关键发现：“计算预算实质上已经是时间”

EIP-2929 Motivation，逐字引用：

> "Generally, the main function of gas costs of opcodes is to be **an estimate of the
> time needed to process that opcode**, the goal being for the gas limit to correspond
> to a limit on the time needed to process a block."

“gas刻意避开时间”的普遍理解被一手资料否定。gas是时间估计，只有单位无量纲。ewasm更直接，将换算固定到具名2014 Haswell CPU（“1 second of CPU execution equals to 10 million gas”）。Solana内部用30 CU/µs，eBPF一百万指令上限源于“典型x86上0.1秒”。

甚至Go pprof返回的“cpu / nanoseconds”，实际是**样本数×10ms常量**（`b.period = 1e9 / hz`、`hz = 100`），并未读时钟。计数×校准常数称为纳秒，早已生产运行，没人说是谎言。

所以问题不是“能否让计算量用时间写”，而是**“把本来就是时间估计的东西称为时间，还是继续披上抽象单位名称”**。

<a id="抽象単位に留まる理由が我々には無い"></a>

### 我们没有留在抽象单位的理由

其他系统选择无量纲可归为两个原因，都不适用。

- **gas是货币单位。** 乘gas price收费，单位不叫gas则经济体系不成立。Almide无价格维度。
- **EVM以重新定价为前提。** 给不断修订的实测对应关系贴ms，每次修订都会破坏单位含义。这是**重新缩放时**的论点，D4冻结作答。冻结先例为gem5（`1 Tick == 1 ps`，一旦固定再改就panic）和POSIX（`CLOCKS_PER_SEC` 冻结为10^6，标准承认真实分辨率不必达微秒）。

<a id="抽象単位では直感が育たないことが全系で実証されている"></a>

### 各系统都证明抽象单位无法培养直觉

> **调研结论：“未找到任何一个用户成功对数值形成直觉的系统。”**

- **EVM**：`eth_estimateGas` 用二分搜索。63/64规则使所需gas非单调（gas used ≠ 所需gas limit）；MetaMask自动设置，估计失败回退block上限95%，破坏UX。
- **NEAR**：200 Tgas时期，Aurora的27.4万交易中**9.2%因到达上限失败**（失败部分占gas使用量26%）。300 Tgas后为1.0%。参考FT实现附带实际消耗数倍的30 TGas，**超量附带成为行业标准规避法**。
- **eBPF**：“4k limit was confusing to users”“加代码verifier能过，删代码反而不过”。
- **Earth Engine EECU**：“极难估计”。
- **F\***：`--fuel` 要求默认0工作，非零须记录；把无法调节的抽象单位固定为0。

所有系统解法相同：运行测量的估计器、安全余量、大默认值。**逃到抽象单位并不解决问题。**

<a id="時間単位を表面に出した先例は存在する"></a>

### 暴露时间单位已有先例

- **Cloudflare Workers**：配置key就是 `"limits": { "cpu_ms": 300000 }`。墙钟侧“No limit”（HTTP duration无上限），只限制CPU时间。理由与我们逐字一致：“CPU time is more predictable and **under your control**… **purely a function of the logic and processing of inputs on outputs to your Worker**”。
- **NEAR**：“we define 10^15 gas to be executed in at most 1s… 1 Tgas corresponds to **1ms execution time**”。Runtime Parameter Estimator还按“1ms = 1 Tgas”规则**机械检查**现有参数（D5先例）。
- **F\* `--z3rlimit`**：称其确定、机器无关，同时说n直觉上计数 **mythical powerful laptop** 上允许的秒数；“correspondence is quite good, **it's not perfect**”（D5诚实性的形式）。
- **Ada**：`type CPU_Time is private`，单位为秒，却与 `Real_Time.Time` 是**不同类型**（D2先例）。
- **gem5**：`simSeconds` / `simTicks` / `hostSeconds` 各以不同名称并列（D5形式）。

<a id="msr-の観点では時間単位が正しさに効く"></a>

### 从MSR角度，时间单位影响“正确性”

LLM从未见过tick预算。`ticks: 100_000` 可能错六个数量级。`100ms` 有先验分布。预算错误双向都静默，所以**数字可猜测是正确性问题，不是便利性问题**。

<a id="単位の変更が意味論の変更でないことは機械検査で担保されている"></a>

### 机械检查保证单位变化不是语义变化

Lean `Ev { time : Nat }` 和穷举模型 `u64` 都**完全不依赖单位**。把tick读成ns，[race belt](https://github.com/almide/almide/tree/develop/crates/almide-race-belt)的7定理及74,898配置合流gate不改一个字符。Wasmtime具名的轴（fuel=确定 / epoch=墙钟非确定）是“**数什么**”，不是“叫什么”。其他系统没分开是因为无理由分开，我们有能分开的证据。

<a id="我々に有利な構造的事実"></a>

### 对我们有利的结构事实

“无法估计”的真因不是抽象单位，而是**成本状态依赖和非单调**（EVM 63/64规则、eBPF verifier搜索）。**Almide成本单调且不依赖状态**，这些病态从开始就不存在。

<a id="alternatives"></a>

## 替代方案

| 方案 | 拒绝理由 |
|---|---|
| **`fuel:`** | 机制隐喻泄漏表层，fuel不是单位 |
| **`ticks:`** | 写不出的数字（本ADR背景），对抽象单位形成直觉的系统为零 |
| **`compute_ms:` 等限定标签** | Cloudflare用 `cpu_ms` 限定因配置同时有墙钟。Almide head声明时钟（`bounded` / `timeout`），会重复显示；类型（D2）更强地承担同一功能 |
| **Postgres式折中**（默认无量纲，特定环境可选ms） | Postgres成本常量是**管理员调节的配置值**才可两种共存，不能搬到源码预算；同一量两个拼写是共存债务 |
| **用物理机器定义参考机**（F\* / ewasm） | 会老化。ewasm明确“CPU持续改善，每3年调整”，调整破坏既有程序含义（EIP-1884破坏假设2300 gas stipend的 `transfer()`；Aragon函数1759→2359 gas越阈值）。采用D4冻结抽象机器 |
| **用户可扩展字面量后缀**（C++ UDL） | Google C++ Style Guide连标准库 `100ms` 都禁止，原因全是**可扩展性的后果**（不能namespace限定、必须 `using`、第三方 `_ms`）。不建扩展机制 |
| **语言拥有的封闭字面量后缀**（CSS式 `100ms`） | **暂缓，并非拒绝**。CSS反证封闭集合可行，设计健全，但给lexer加数字后缀是**新增语言语法**；仅为 `fan` 表层扩词法不划算。意义与函数形式相同，未来加入也脱糖到它（D3） |
| **仅构造器函数**（Rust / Swift） | **采用**（D3）。Kotlin 1.5废弃简洁形式转向静态factory、1.6恢复，证明人偏爱简洁。但Kotlin简洁形式基于已有**扩展属性**，无需新词法。Almide没此机制，购买相同简洁的价格不同 |
| **裸整数+文档**（Go） | Go `time.Sleep(10)` 是10纳秒；实事故hudl/fargo #56，staticcheck有SA1004。`Duration × Duration` 通过类型检查的问题（#64420）以*not planned*关闭，需专用linter。Go团队自身在#20757承认Go Duration比Rust/Java/C#/Python更危险。Swift、Zig最近都从裸整数API迁移到类型化Duration |
| **单一Duration类型，消费语法声明时钟**（`ulimit -t` / Cloudflare `cpu_ms`） | **考虑后拒绝（2026-08-01讨论）**。也考虑了bounded/race分支pure无I/O等待，计算时间与墙钟在校准范围一致，调用点混合实害小的论点。拒绝因config/变量混入时钟（`cfg.timeout` 传给 `bounded`）只能靠类型阻止；该防火墙正是二类型存在理由（Ada先例、D2）。用户选择防火墙而非短词汇 |

<a id="前回の却下を訂正する"></a>

### 修正上一次拒绝

[ticks-interface-audit.md](https://github.com/almide/almide/blob/develop/docs/roadmap/active/ticks-interface-audit.md)曾将“时间样单位换算tick（`budget: 100.ms`）”作为**反向重演误读**而拒绝。这是**混淆严重程度**。`fan.timeout(1000)` 事故是“依赖墙钟=破坏确定性”，让逻辑时间用时间单位书写则**完全不破坏确定性**，仅墙钟对应关系偏移。上次把两者放同类是错误，本ADR取代那一节。

<a id="consequences"></a>

## 后果

**所得**

- 数字可写；人和LLM对 `100ms` 都有先验。
- 一个单位体系，确定性时钟和墙钟共享ms/s词汇，由head和类型区分。
- parser不再需要标签机制。`fan.bounded(<expr>) { body }` 足够，不需head-args标签解析，fan-v2表层缩小。
- **新增语言语法为零**。只加类型与stdlib module，不动lexer/parser。
- 成本表有外部锚点。tick表“无法出错”即无法校准；时间表对参考机器**可错→可修**。CM修订不再任意漂移，而是趋向真值；校准可用gate机械检查（D5）。
- 裸整数成为类型错误，单位混淆事故类别（Gearman 17分钟timeout，Jenkins 300秒→3.5天）被语法消除。

**所付**

- **标牌与实际的幅度**。gem5虽是cycle-accurate simulator，对真实硬件runtime平均绝对误差13–17%（ARM server MAPE 26–30%）。我们的加权op计数不模拟cache、分支预测、内存层次，部分负载可偏差数倍。D5区间声明与双值报告诚实处理“ms”标牌后的范围。
- **可能误读**。把它读为“100ms返回”的用户，在慢机器看见墙钟300ms。但**所有host上程序行为相同**，破坏的仅是墙钟延迟期望，正确性与确定性无损；严重程度与 `fan.timeout(1000)` 非确定性事故相差两个数量级。
- **长度**。`compute.ms(100)` 比 `100ms` 长，最常见调用点相差12字符。
- **新增两类型**（`Compute` / `Duration`）。触及Vocabulary Economy，以Ada先例及D2事故预防换取。

**不变**

- Lean 7定理、74,898配置合流gate、T1–T9，全部与单位无关。
- 语义：lockstep、确定性事件规则、trap可见窗口、嵌套min-cap。
- `fan {}` / `fan.map` / `fan.any` / `fan.settle` 表层。

<a id="falsifier"></a>

## 可证伪条件

- **若校准gate实测对应范围过宽**（超过声明区间，参考5倍），撤下ms标牌，回到无量纲名。gem5为13–30%，目前不能保证我们在数倍以内。**Stage 1 probe才给出首次实测。**
- 若误读成为观测到的事故类别（把 `fan.bounded` 预算当墙钟的代码造成实害），把D2类型名改为自解释，或回到限定标签。
- **D3-F**：若函数形式实测降低MSR（dojo中 `compute.ms(100)` 写错显著），加入字面量后缀 `100ms` 作为语法糖。脱糖目标已是函数形式，不改语义。
- 若出现无法避免跟随硬件演化的情况（放弃D4冻结），取代整个ADR，因为冻结是ms写法前提。

- 若负值trap实际过严（记录到合法计算构造参数降至负的模式），改为0饱和构造，S5不变量仍保留。
- 若UFCS歧义错误降低MSR（频繁遗漏限定），考虑短限定形式；隐式选择时钟不在选项内。

<a id="references"></a>

## 参考资料

**一手资料**

- [EIP-2929](https://eips.ethereum.org/EIPS/eip-2929) — gas成本是“an estimate of the time needed”
- [EIP-150](https://eips.ethereum.org/EIPS/eip-150)、[EIP-1884](https://eips.ethereum.org/EIPS/eip-1884) — 重新定价及破坏既有契约
- [ewasm: determining wasm gas costs](https://github.com/ewasm/design/blob/master/determining_wasm_gas_costs.md) — 固定2014 Haswell，每3年调整
- [NEAR nomicon: Gas](https://nomicon.io/architecture/gas/index.html) — “1 Tgas = 1ms”
- [NEAR parameter definition](https://near.github.io/nearcore/architecture/gas/parameter_definition.html) — estimator机械检查
- [NEPs discussion #305](https://github.com/near/NEPs/discussions/305) — 9.2%失败
- [Cloudflare Workers limits](https://developers.cloudflare.com/workers/platform/limits/) — `cpu_ms`
- [Cloudflare pricing blog](https://blog.cloudflare.com/workers-pricing-scale-to-zero) — “purely a function of the logic”
- [F\* rlimits](https://github.com/FStarLang/FStar/wiki/rlimits:-Machine-Independent-Resource-Limits-for-Deterministic-Execution) — mythical powerful laptop
- [Wasmtime Store::set_fuel](https://docs.wasmtime.dev/api/wasmtime/struct.Store.html) / [Interrupting Execution](https://docs.wasmtime.dev/examples-interrupting-wasm.html) — fuel与epoch轴
- [gem5 core.cc](https://raw.githubusercontent.com/gem5/gem5/stable/src/sim/core.cc) — 冻结 `1 Tick == 1 ps`
- [Gutierrez et al., ISPASS 2014](https://tnm.engin.umich.edu/wp-content/uploads/sites/353/2017/12/2014.03.Sources-of-error-in-full-system-simulation.pdf) — gem5的13–17%误差
- [Ada RM D.14](https://ada-lang.io/docs/arm/AA-D/AA-D.14/) — `type CPU_Time is private`
- [POSIX clock()](https://pubs.opengroup.org/onlinepubs/9699919799/functions/clock.html) — 冻结 `CLOCKS_PER_SEC`
- [PostgreSQL planner cost constants](https://www.postgresql.org/docs/current/runtime-config-query.html) — “arbitrary scale, only relative values matter”
- [Solana block_cost_limits.rs](https://github.com/solana-labs/solana/blob/master/cost-model/src/block_cost_limits.rs) — 30 CU/µs
- [Linux commit c04c0d2](https://github.com/torvalds/linux/commit/c04c0d2b968ac45d6ef020316808ef6c82325a82) — eBPF“1M instructions in 1/10 of a second”

**语言的Duration设计**

- [Go time](https://pkg.go.dev/time)、[golang/go#20757](https://github.com/golang/go/issues/20757)、[#64420](https://github.com/golang/go/issues/64420)、[staticcheck SA1004](https://github.com/dominikh/go-tools)、[hudl/fargo#56](https://github.com/hudl/fargo/issues/56)
- [Kotlin 1.6发布说明](https://kotlinlang.org/docs/whatsnew16.html) — 恢复扩展属性
- [JetBrains/kotlin#4119](https://github.com/JetBrains/kotlin/pull/4119) — 1.5废弃
- [Rust pre-RFC: custom suffixes](https://internals.rust-lang.org/t/pre-rfc-custom-suffixes-for-integer-and-float-literals/8029)
- [Google C++ Style Guide](https://google.github.io/styleguide/cppguide.html) — 禁UDL及理由
- [CSS Values 4 §time](https://www.w3.org/TR/css-values-4/#time) — 封闭内建后缀
- [SE-0329](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0329-clock-instant-duration.md)、[Zig lib/std/Io.zig](https://github.com/ziglang/zig/blob/master/lib/std/Io.zig)

**内部**

- [race belt](https://github.com/almide/almide/tree/develop/crates/almide-race-belt) — 7个单位无关定理
- [logical-time-race spike](https://github.com/almide/almide/tree/develop/research/spike/logical-time-race) — 74,898配置合流gate


### 保留的日文图示与代码注释说明

第一幅图从程序开始到结束依次为：计算30ms、网络等待500ms、计算20ms、磁盘等待200ms、计算10ms。墙钟共760ms，包含等待；CPU时间为实际执行代码的60ms。引用shell注释分别标墙钟/CPU为2.17s/0.04s及0.51s/0.39s，沿用原文数字而不重新计算。决定示例的注释分别表示确定性时钟、无预算的race基本形式、可选发散guard、oracle层墙钟。构造示例注释区分确定性Compute与墙钟Duration，并说明变量可原样使用。保留参数大値意为较大的值。
