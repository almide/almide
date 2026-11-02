历史描述：原始飞行子集提案；规范性有界配置的权威是 ALS §B，而非历史上的 @flight 草案。
<a id="almide-flight-profile--original-subset-proposal"></a>

# Almide 飞行配置 — 原始子集提案

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flight-subset-spec.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-subset-spec.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flight-subset-spec.zh-CN.md)

> 非规范性固定原文历史读者快照。未改动的固定原本是翻译依据；当前有界配置的规范权威是 ALS §B，而非历史 @flight 设计。未自动同步，也未独立验证证明、测量、实现状态或标准映射。下文飞行级/航空品质、certified、全球罕见、O(1)、WCET及比较措辞是原分析/提案记录，不表示航空认证、工具鉴定、当局接受或新保证。计数、旧源码行号、提议 gate 和后续修订各自保留日期/状态。分配次数本身不能确立字节上限或物理 WCET，技术证明资产不等于适航应用资格。此处未独立验证原文的 MISRA/Ravenscar/DO-178C 比较及对各标准义务的主张。命令为复现参考，不是执行指令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md)

当前规范性有界配置是 [ALS §B](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md)，使用 `@bounded` 和 critical 配置检查。
本路线图中的 `@flight`、IN/OUT 表及旧 lowering 行号是原始设计，不是第二份规范性语言规格，
也不是已获认证飞行用途的证据。不要借文档清理实施其政策差异。
下文逐循环证书/WCET 想法，凡尚未由当前 ALS、源码和 receipt 确立的部分，仍是提案。

> **目标**：规范化 flight-grade Almide 代码的**语言子集**——类似 SPARK/Ravenscar 或 MISRA C。
> 目标是让每次构建的证书**通过证明机械强制**子集成员资格，而非靠评审。
> 这是 [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) 阶梯 **G-F1** 的核心。
> 两个关键支点（[flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md) / [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)）
> 留下的开放问题在这里定案。
> **相关**：[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §3.5 / [flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)。

---

<a id="1-設計核一行"></a>

## 1. 设计核心（一行）

> **飞行子集通过额外 gate，从“已经能够 lowering 的内容”中选择严格子集（不是另一个编译器）。
> 非飞行代码 lowering 为健全的 model-one-iteration / `Opaque` 形式，只是不符合飞行准入。
> 在 `@flight` 函数的闭合调用图上，通过两层（lowering 拒绝边界 + 已证明的 checker 性质）
> 强制成员资格：证书携带每个 `@flight` 函数属于配置内的证明。这是差异点：不是评审，而是每次构建的证明。**

---

<a id="2-原設計の特徴分類表in--out--restricted"></a>

## 2. 原始设计的特性分类表（IN / OUT / RESTRICTED）

图例——**IN**：飞行代码中允许。**OUT**：禁止（拒绝）。**RESTRICTED**：仅允许显式形式。
Enforcement 是设计时的 gate/未实现备注；当前是否允许，以 ALS §B 为准。

<a id="21-制御フロー"></a>

### 2.1 控制流

| 特性 | 分类 | 依据 | 强制措施 |
|---|---|---|---|
| 计数循环 `for i in lit..end` | **IN** | 唯一有静态迭代次数的循环 → WCET、分配上限由构造保证。`n` 是良基排名 | `try_lower_scalar_for_range`（`lower/control.rs:766`）已经隔离。证明后盾**尚未实现**（`CountedLoopStart{bound}` + `NoAllocInLoop.v`/`AllocBound.v`） |
| 无界 `while` | **OUT** | 没有静态迭代上限 ⇒ 没有 WCET。`LoopBreakUnless{cond}` 是任意运行时 Bool（`lib.rs:255`） | `try_lower_scalar_while`（`control.rs:603`）会为执行 lowering，但它不是计数循环。飞行 gate 拒绝非计数循环**尚未实现** |
| `for x in <heap容器>` | **OUT** | 迭代次数 = 运行时长度，不是静态值 | 飞行 gate 拒绝非 `CountedLoop` 迭代**尚未实现** |
| 递归（自身/互相，`CallFn`） | **OUT** | 无界栈 + 无界工作量，没有迭代度量 | 调用图**无环性**检查（§3 (c)）**尚未实现**（机制在 `reachable_caps` 的 `visited` 中） |
| `if`/`match`（scalar/Unit 结果） | **IN** | 有界：只执行一个分支，标记所有权为零 | 可执行 `IfThen/Else/EndIf`（`control.rs:175,267`）。checker 是平坦 fold |
| `if`/`match`（heap 结果） | **RESTRICTED→IN** | 一个分支分配并 move-out（`"im"`），有界 | `try_lower_heap_result_if`（`control.rs:671`），分支仅限字符串字面量/嵌套 if/直接调用 |
| payload-bind / guard / arm-reassign match | **OUT** | 平坦 fold 看不到依赖路径的重新绑定（潜在 UAF） | 已 **WALLED**（`control.rs:319,330`） |
| 对已物化 `Option[Int]` 的 `match` | **RESTRICTED→IN** | tag=长度，scalar payload 按值复制，有界 | `try_lower_variant_match`（`control.rs:307`） |
| 提前返回 `Try`（`?`）/`Unwrap`（`!`） | **RESTRICTED** | 提前退出是控制路径。仅在不跳过仍存活的逐迭代 heap drop 时允许 | `lower/mod.rs:247`。**计数循环体内 OUT** 才安全（teardown 提前路径的 free 尚未证明） |
| `break`/`continue` | **OUT** | 破坏精确迭代界限 + 跨 heap frame 跳过逐迭代 drop（wasm 泄漏） | counted-range 已拒绝（`control.rs:777`），heap frame 使用 `wall_break_over_heap_frame`（`:868`）。§3 (b) |
| `guard … else`（发散） | **OUT**（暂定） | 条件性提前退出，失败路径未建模 | 仅 effect-capture（`lower/mod.rs:401`）。建模前 OUT |
| `fan.*`（并发） | **OUT** | 非确定性调度，无界 | 不在 MIR 子集中（lowering 时拒绝） |

<a id="22-メモリ"></a>

### 2.2 内存

| 特性 | 分类 | 依据 | 强制措施 |
|---|---|---|---|
| 循环外动态分配 | **IN** | 以原始固定页分配器为前提的提案。另行检查当前 size-class/free-list 和 heap cap 条件，不从分配次数独自推导字节上限或物理 WCET | `Op::Alloc`（`lib.rs:162`）。通过 `AllocBound` 加界**尚未实现** |
| 循环内分配 | **OUT** | 不乘以迭代次数，就会破坏 high-water-mark | 在 `CountedLoopStart/End` 之间拒绝 `is_allocating` **尚未实现** |
| `Init::DynStr`/`DynList`（运行时长度） | **OUT**（字节有界飞行） | 长度是运行时 `ValueId`（`lib.rs:137,151`）→ 不限定 `len`，字节就无界 | 证明 `len` 有界前拒绝**尚未实现**（次数标准允许，字节标准不允许） |
| static-`Init` heap（`IntList`/`Str` 字面量、`OptSome`） | **IN** | 从 `Init` 静态知道大小 → 可计算字节上限 | 带 static `Init` 的 `Op::Alloc` |
| `xs[i]=v` 等原地变更（COW） | **IN** | 有界，`MakeUnique` 仅共享时 clone | `lower_place_mutation`（`lower/mod.rs:419`）。`ListPush`（增长）RESTRICTED 为**仅限循环外** |
| 无界数据结构（Map/Set 增长、循环内 push） | **OUT** | 增长 ⇒ 无界内存。`ListPush` 可能重新分配 | 循环内 push = alloc-in-loop ⇒ OUT **尚未实现** |

<a id="23-関数"></a>

### 2.3 函数

| 特性 | 分类 | 依据 | 强制措施 |
|---|---|---|---|
| 顶层 `fn` | **IN** | 调用图单位，有界函数体 | `MirFunction`（`lib.rs:393`） |
| `effect fn` | **RESTRICTED→IN** | 声明 capability（`Stdout`）并验证 `reachable ⊆ declared`。I/O 仅经过已声明的有界 capability | `lower/mod.rs:109` + `CapabilityBound.v`（`certificate.rs:127`） |
| closure / lambda | **OUT** | 捕获的所有权未跟踪，间接 callee 破坏静态调用图 | `is_higher_order` 拒绝边界（`lower/mod.rs:621`） |
| 高阶函数（`list.map(f)` 等） | **OUT** | closure 参数有未建模 capability + 间接 dispatch | `is_higher_order` 拒绝边界（`lower/mod.rs:621`） |
| 函数指针 / `FnRef` | **OUT** | 间接调用 ⇒ acyclicity/caps/WCET 失去静态 callee | `is_higher_order`（`lower/mod.rs:629`） |
| 直接用户调用 `f(x)`（`CallFn`） | **IN** | 静态 callee 名 ⇒ 显式调用图 ⇒ 可判定无环性 + 传递 caps | `Op::CallFn`（`lib.rs:214`）。受递归 gate 限制 |
| pure stdlib 调用 | **RESTRICTED→IN** | 仅限 `PURE_MODULES` 且一阶 | `purity.rs::PURE_MODULES` + `is_higher_order` |

<a id="24-型"></a>

### 2.4 类型

| 特性 | 分类 | 依据 | 强制措施 |
|---|---|---|---|
| scalar Int/UInt/Float/Bool | **IN** | Copy、无 refcount、固定宽度、WCET-flat | `Repr::Scalar{width}`（`lib.rs:75`） |
| 整数运算 Add/Sub/Mul/Div/Mod/cmp | **IN** | 有界常数时间。Div/Mod 遇 0 trap（全域、有定义） | `IntOp`（`lib.rs:283`） |
| **浮点运算** | **OUT**（暂定） | **MIR 中不存在 `FloatOp`**（只有 `IntOp`）。Float 是 `Repr`，但 lowering 后没有运算。飞行 IEEE-754 还需要确定性规则 | `lib.rs` 中没有。需要 fixed-point 或确定性固定的 float，**尚未实现**（§5） |
| record | **RESTRICTED→IN** | 有界布局，字段访问按容器粒度 borrow | `Repr::Ptr{layout}`。布局注册表当前是 placeholder，**尚未实现** |
| enum/variant | **RESTRICTED** | `Option`（len-as-tag）允许，一般 payload-bind 需要布局砖块 | `Init::OptSome`，一般 variant match 已 WALLED（`control.rs:343`） |
| `Option`/`Result` | **RESTRICTED→IN** | `Option[Int]` 是已证明的物化形式，`Result` 传播=`Try` | `Init::OptSome` + `try_lower_variant_match` |
| `Repr::Boxed`（递归类型） | **RESTRICTED** | box read 是 Borrow。递归**类型**允许，但递归**遍历**是递归（OUT） | `Repr::Boxed`（`lib.rs:81`） |
| generics | **RESTRICTED→IN（mono 后）** | 单态化在 MIR 前消去，每个实例都是具体 `MirFunction` | `mono/` + `repr_of` 拒绝 `Ty::Unknown`（`lower/mod.rs:67`） |
| `where` 约束 / protocol（`any P`） | **OUT / RESTRICTED** | `any P` = 存在类型 = 动态 dispatch（间接 callee）⇒ OUT。`where` 是编译期，mono 消去 ⇒ IN | `any P` OUT，`where` IN |

<a id="25-stdlibflight-safe-面"></a>

### 2.5 stdlib（flight-safe 表面）

| 组 | 分类 | 强制措施 |
|---|---|---|
| pure 模块（`int`/`float`/`string`/`list`/`map`/`set`/`math`/`option`/`result`/`value`/`json`/`bytes`/`regex`/`matrix`…）且一阶 | **IN** | `purity.rs::PURE_MODULES` + 一阶 |
| pure 模块的高阶成员（`list.map`/`filter`…） | **OUT** | `is_higher_order` 拒绝边界（与 purity 分开） |
| effectful 模块（`env`/`fs`/`http`/`io`/`net`/`process`/`random`/`zlib`） | **OUT**（通常）。仅 `io` print 为 RESTRICTED | `purity.rs`。仅 `Stdout`（`PrintInt/List/Str`）是已建模 capability |
| impure-plain（`datetime`/`args`/`mem`/`testing`） | **OUT** | 无 `effect` 关键字也到达 host（时钟/非确定性/abort）——飞行中必须禁止 |
| 无界分配的 pure 成员（`string.repeat`/循环内 concat/运行时长度 `with_capacity`） | **RESTRICTED** | 受 no-alloc-in-loop + DynStr 拒绝边界限制，**尚未实现** |

---

<a id="3-キーストーン-open-question-の決着"></a>

## 3. 关键支点开放问题的定案

- **(a) Dup-in-loop = IN**（alloc-count 配置）。循环体的 `Dup`（`'a'`）递增已有对象的 refcount，
  即无新 heap、O(1) 常数时间 → 不破坏 WCET 或 high-water-mark。
  禁止它会使值语义语言的“循环内 heap 读取”全灭。最小证明砖块
  `NoAllocInLoop.v` 已按 `is_allocating`（仅 `'i'`）定义 → 无需新字母表。
  zero-heap-traffic 配置保留为需要确定性 RC 时序时的**更严格可选子层级**（非默认）。
- **(b) break/continue = OUT**（精确界限）。允许提前退出会使 `AllocBound` 弱化为 `≤ B` 上界。
  精确界限更强，证明也已写好（每次迭代恰好消耗一次 `n`）。lowering 已拒绝
  （`control.rs:777,868`）；允许需要新 teardown，违反“最便宜的拒绝边界”。
  也与 MISRA single-exit 一致。`≤ B` 变体是未来放宽（Brick 2），不在规范范围。
- **(c) 递归 = 通过调用图无环性拒绝**。飞行模块调用图（节点=`MirFunction`，边=每个 `Op::CallFn`）
  必须是 **DAG**。检测器已存在：`reachable_caps`（`certificate.rs:168-188`）中 `visited` 的提前返回
  命中环。飞行 gate 使用同一 DFS，反转判定——**active-stack 上 `visited.insert` 失败则 REJECT**
  （back-edge=递归）。指真正的回边，而非菱形去重。unknown/cross-file callee 保守拒绝
  （不放行隐藏的递归边）。bounded recursion 仍是待办（§5）。
- **(d) 嵌套计数循环 = IN，上限 = 迭代次数之积**。Coq 的 `lrun` 已递归处理 `LLoop n body`
  （模型支持嵌套）。总和 = `Σ_loops bound_L × 本体 alloc`，嵌套时内层 bound 乘外层迭代次数，
  得到**乘积**上限。剩余工作在 emission 端（`scalar_loop_depth` 是平坦的，
  `try_lower_scalar_for_range` 一次一个范围）——Coq 端已准备好。

---

<a id="4-強制アーキテクチャ"></a>

## 4. 强制架构

**声明 = 每函数 `@flight` 属性（模块也提供语法糖）。** 不采用编译标志（`--flight`），
否则连导入的非飞行 stdlib 也会卷入；飞行代码是大型程序内的**内核**。
整个模块作为审计单位是正确的，但**强制的原子单位是函数**（acyclicity/caps fold 按函数）。
**决定**：`fn`/`effect fn` 上的 `@flight` 属性。`@flight` 函数只能调用 `@flight` 函数（以及 pure stdlib），
因为 acyclicity + caps fold 要求整个 reachable 集合都在配置内
（与 `reachable_caps` 已要求的条件相同）。飞行边界 = **闭合调用图**，这正是 WCET/capability 证明的要求。

**两层强制**（与 §2.3 / [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2 一致）：
1. **lowering 拒绝边界（不受信、最便宜）**：lowering `@flight` 函数时，在已有值语义拒绝边界之外拒绝：
   非计数循环 / `CountedLoopStart/End` 之间的 `is_allocating` / `Init::DynStr|DynList`
   （字节飞行）/ closure、HOF、`FnRef`（已有）/ 闭合环的 `CallFn` 边 / 非 `PURE`、
   非 `@flight` callee。拒绝只是不授予 flight-certified（其他仍正常编译）。
2. **已证明的 checker 性质（有约束力、每次构建重验）**：拒绝边界是建议，约束来自 kernel-proven checker
   对 witness 的重验。在已有 3 个 witness（ownership/name/caps）上加两个（`NoAllocInLoop`/`AllocBound`），
   再加调用图无环性 witness。checker 保持平坦 fold、有界大小
   （编译器计算 `B`/trip 并放入 witness；checker 只 fold + 比较，不走 CFG）。
   **证书为每个 `@flight` 函数携带“属于配置内的证明”——这就是差异点。**

---

<a id="5-misra--ravenscar--do-178c-マッピングサブセット--強み"></a>

## 5. MISRA / Ravenscar / DO-178C 映射（子集是优势）

OUT 列表各项都是 DO-178C C-level 编码标准（MISRA C / Ada-SPARK 的 Ravenscar）
**通过评审强制**的纪律。飞行配置**通过证明机械强制同样的纪律**：

| Almide OUT/RESTRICTED | DO-178C C 标准中的对应项 | 差异 |
|---|---|---|
| 禁止递归 | MISRA C:2012 Rule 17.2 / Ravenscar | 对方=评审，Almide=**调用图无环性、proof-carried** |
| 无界 while / 非计数循环 OUT | DO-178C WCET 义务 / MISRA Dir 4.1 / Ravenscar 静态迭代 | 对方=手动 WCET+评审，Almide=**迭代次数 → AllocBound 证明** |
| alloc-in-loop OUT / 分配有界 | MISRA 21.3（no malloc）/ SPARK no-dynamic-alloc | 对方=全部禁 heap 或评审，Almide=O(1) 分配器 + **静态 high-water-mark 证明**（更宽容且已证明） |
| closure/函数指针/`any P` OUT | MISRA 函数指针限制 / Ravenscar 动态 dispatch 禁止 | `is_higher_order` 拒绝边界 + 闭合调用图 |
| 禁止 break/continue | MISRA 15.x（single exit） | exact-bound 要求、lowering 拒绝边界 |
| 时钟/random/声明外 I/O OUT | DO-178C resource determinism | **capability-bound 证明**（`used ⊆ declared`） |

飞行工程师读这份 OUT 列表，会看到**熟悉的编码标准**。新意不在规则，
而在于**每条规则由每次构建重跑 kernel-proven checker 来 discharge**（`make verify` 杀手级演示）。

---

<a id="6-正直な残務言語自体に欠けるもの"></a>

## 6. 坦诚的剩余工作（语言自身缺少什么）

飞行代码需要、但 Almide 还**不能表达/证明**的内容：

1. **bounded recursion**——配置全面禁止递归（§3 c，`CallFn` 环没有迭代度量）。
   实际飞行代码可能需要静态深度递归（固定深度树遍历）。缺少 `@bound(n)` 递归和调用图环的 Coq 度量。
2. **fixed-point / 确定性 float**——MIR 完全没有 float 算术 op（仅 `IntOp`）。DAL-A 控制律需要
   fixed-point 或确定性固定的 IEEE-754（rounding/NaN/Inf/no-FMA）。缺少飞行 float op 集或 fixed-point 类型。
3. **静态大小数组 / 有界集合**——全部 heap 容器动态大小，`DynStr`/`DynList` 是运行时长度。
   缺少 `[T; N]`（编译期 N）——**字节级 WCET 的 gate 待办**（没有它，`DynStr`/`DynList` 仍被拒绝）。
   布局注册表当前为 `PLACEHOLDER_LAYOUT`。
4. **字节级内存上限**（而非对象计数上限）——RC 证书丢弃字节大小（`lib.rs:130`）。
   当前只能证明对象计数 high-water-mark。缺少携带大小的 `LAlloc(size)` 精化（依赖 3）。
5. **`≤ B` 上界提前退出**——为精确界限而禁止（§3 b）。依赖数据的提前退出（仍然 WCET 有界）当前 OUT。
   缺少 `AllocBound` 的 `≤ B` 放宽（证明支持，配置不允许）。
6. **真实布局 / 可读类型**——record/payload enum/统一 `Vec<i64>` 都未有布局（`PLACEHOLDER_LAYOUT`）。
   Rust render 全用 `Vec<i64>`，非 review-grade。不满足 DO-178C“源码可读”。缺少布局注册表砖块和
   `debug_name` side-table。
7. **功能正确性可追踪性**（本配置范围外，但是真正飞行缺口）——配置证明*安全*
   （mem/name/caps/bound），不证明控制律是否*正确*。支柱③=零。任何子集都无法提供，
   这是应用的责任，加上 Almide 缺少的追踪机制。

**总结**：验证主干领先于 gate——证明和检测器大体已存在，飞行配置主要是
**把现有 fold 接到 reject 判定 + 增加两个 Subset 形状的 Coq 性质**
（与关键支点“扩展而非重做”一致）。

保留的日文代码字面量说明：“heap容器”指堆容器，“本体 alloc”指循环体内的分配。
