说明：历史 MIR Phase 1 实现设计，以及 IR 与 MIR oracle 的区别。
<a id="almide-v1-phase-1-mir-コア--二レンダラ--実装設計"></a>

# Almide v1 Phase 1：MIR 核心 + 双渲染器——实现设计

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-phase1-mir-core.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-phase1-mir-core.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-phase1-mir-core.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md)

> 原文注记：历史设计与分阶段进展，不是当前未启动迁移或普遍证明。六组件/六决策措辞与§1.1七行并存；§3.4不存在；§5.1将实际首个数据行 Core→MIR 称为第二行，均不猜测修正。开头 IR/MIR 解释器区别限制后文“解释器失明”主张。Phase 0、32+ 案例和149/270/200k有限测试是历史观察，非新执行或整个编译器忠实性证明。结构不可能及 flight-grade 是原目标/主张，不是外部认证。borrow/clone、展平 drop、共享可变设计保留但不宣称最终实现。装箱判定坐标未配对 backtick 原样保留。未执行引用 CI 修改或指令。


<a id="historical-mir-design-and-oracle-boundary"></a>

## 历史 MIR 设计与 oracle 边界

这是 2026 年 6 月 Phase 0 原型之后的实现设计，不是当前编译器尚未开始的计划。v0 Perceus/layout/emit 源码坐标属于当时快照。当前发布的 Wasm 路线是 [structural](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)。

§5.1 所谓解释器看不到 Core→MIR 错误，针对的是解释同一 MIR 的解释器，不是所有解释器。仓库的 [IR 解释器](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-interp/src/lib.rs) 执行更早的 IR，提供独立差分观察；它也有覆盖与信任限制。结果一致或 Phase 0 示例都不能证明整个编译器忠实。

状态：active——设计优先。Phase 0 决策门禁通过（5/5）后的主体设计；实施前的评审对象。
负责人：compiler
上级：[v1-mir-architecture.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md)（宪法、规范形、阶段计划）
依据：精读六个组件后对现有编译器所有权/布局决策作出的实测映射（§1 的所有 file:line 都是已验证锚点）。

---

<a id="0-このドキュメントの位置づけ"></a>

## 0. 本文定位

[v1-mir-architecture.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) 规定**做什么、为什么**（宪法：“所有权和布局只在 MIR 决定一次；渲染器仅重现”）。本文是 Phase 1 的实现设计，以现有代码实测地图确定**在哪里、按何顺序、怎样验证**。

**Phase 0 已确定的内容**（§8.1、`research/spike/v1-mir/GATE.md`）：五种所有权棘手形态全部实现“单一 Perceus 决策→两种惯用表达构造性一致”，逃生口 0。也识别出规范形的四个边界：逐绑定极性、语法与语义、值等价验证、共享可变边缘。**Phase 1 将此论点扩展至全语言，借现有 oracle 实证并消除债务。**

---

<a id="1-現状マップ--意味論の実装が2つあるの実体"></a>

## 1. 现状地图——“语义有两份实现”的实质

精读确定的最重要事实：**高层 IR 已是半个 MIR**。

- `IrStmtKind::RcInc/RcDec` **已是 IR 节点**（crates/almide-ir/src/lib.rs:723）；Perceus 在 IR 层放置 dup/drop。§2.2 的“dup→__rc_inc / drop→__rc_dec”就是 v0 的 `emit_wasm/statements.rs:329/358`（#782 已删除，见 git 历史）。
- `engine/layout.rs` 的 `LayoutRegistry`（:77）**已经就是 §2.1 的 Repr 表**：STRING/LIST/SWISS_MAP/SET/VARIANT/OPTION/RESULT 具有字段偏移（Fixed/AfterDynamic）、MemType 宽度和 header_size，**此模块外魔数为 0**（`list_layout.rs` 是薄访问器）。

→ Phase 1 不是从零建新 IR，而是**把已集中化事实提升为 MIR 值，把分散决策合并到一个 pass**。

<a id="11-5つの所有権パス--レンダラ再決定--1つの事実"></a>

### 1.1 五个所有权 pass + 渲染器重新决策 → 一个事实

| 当前 pass/位置 | 目标 | 决定的事实 | MIR 承载者 | 出处（已验证） |
|---|---|---|---|---|
| **PerceusPass** | 仅 wasm | fresh/alias 极性→RcInc/RcDec 放置 | `Dup`/`Drop` 节点（已有 IR）+ 逐绑定极性 | pass_perceus.rs:728 `yields_borrowed_alias`（全覆盖，无通配分支），:612 `is_heap_type` |
| **BorrowInferencePass** | 仅 Rust | 参数极性 {Own/Ref/RefStr/RefSlice/RefMut} + 调用侧插入 Borrow | 函数边界逐参数极性 {own/borrow/borrow_mut}（Ref 系列是 Repr 导出的**拼写**） | pass_borrow_inference.rs:497 is_heap_type，:516 check_needs_ownership，:371 is_derive_fn(@derived) |
| **CloneInsertionPass** | 仅 Rust | 最后使用→move，否则 Clone（Rust 的 dup） | 逐使用极性 `Dup`（非最后）/ `Consume`（最后=move） | pass_clone.rs:141 split_clone_ids，:223 last-use move gate |
| **CaptureClonePass** | 仅 Rust | 捕获极性：Dup-into-env / 值复制 / 共享单元 | closure env 每个字段的极性 + 共享可变节点 | pass_capture_clone.rs:100 detect_shared_mut，:448 wrap_lambda_with_clones |
| **AliasCowPass** | 仅 wasm | needs_cow（可变别名+修改）→`__cow_check` | `MakeUnique` 节点 | pass_alias_cow.rs，annotations.rs:85 needs_cow |
| **BoxDerefPass** | 仅 Rust | 递归类型哪些字段装箱→插入 Deref | `Repr::Boxed{layout}`（装箱是 MIR 事实，`&**deref` 是拼写） | pass_box_deref.rs:132 find_recursive_enums，:18 Rust-only |
| **emit_typed_rc_dec** ⚠ | 仅 wasm，**渲染器内** | 复合类型递归 drop 调度（哪些子指针被拥有、释放顺序） | **展平的 `Drop` 节点序列**（§3.4） | emit_wasm/statements.rs:734-939，:724 is_heap_type（`Mirrors pass_perceus::is_heap_type`） |

<a id="12-二実装病が具体的に見える-drift-座標phase-1-が殺す対象"></a>

### 1.2 具体可见“双实现病”的漂移坐标（Phase 1 消除目标）

精读发现的**同一事实重复实现**，即违宪的实质：

1. **两份 `is_heap_type`**：`pass_perceus.rs:612` 与 `emit_wasm/statements.rs:724`；后者注释明确写“Mirrors pass_perceus::is_heap_type”。人工同步。
2. **两份共享可变检测**：`detect_shared_mut`（Rust，捕获时）与 `detect_mutated_captures`（wasm，ClosureConversion 时），只能靠构造一致。
3. **两处枚举 variant tag**：`mod.rs:1338` 和 `equality.rs:179`（都是 `enumerate()` 索引）；顺序漂移会静默破坏 eq。
4. **三处递归判定**：find_recursive_enums / walker annotation / wasm 自然的指针处理。
5. **emit_typed_rc_dec 重遍历 Ty**，**在渲染器内重新决定 drop 调度**——§643 类问题的核心。对 `PerceusVerify`（只看 IR 的 RcInc/RcDec）和 byte gate（`verify_rc_balance` 只看 `wasm_dec ≥ ir_dec`，不能捕获过计数，mod.rs:1858）都**不可见**。

> §643 作为 heisenbug 逃过检测并非偶然。**部分所有权决策本就不在两种验证器射程内，而在渲染器手写逻辑中**。Phase 1 将其提升为 MIR 节点，纳入射程。

---

<a id="2-mir-コアの定義"></a>

## 2. MIR 核心定义

<a id="21-値モデル-repr"></a>

### 2.1 值模型：`Repr`

每个 MIR 值/绑定附带 `Repr`，将 `LayoutRegistry` + `byte_size`（values.rs:58）+ `find_recursive_enums` 提升为**值的性质**：

```
Repr =
  | Scalar { width }                  // Int=8, narrow=1/2/4, Bool=4, Float=8 — RC 不要、Dup/Drop 無
  | Ptr    { layout: LayoutId }       // String/List/Map/Set/Record — RC 管理、layout は offset/header/stride を持つ
  | Boxed  { layout: LayoutId }       // 再帰サイクル member field — Rust:Box<T> / wasm:bare ptr
```

代码注释译文：Scalar 不需要 RC，也没有 Dup/Drop；Ptr 由 RC 管理，layout 带偏移、头部、步长；Boxed 表示递归环的成员字段，Rust 拼成 Box<T>，wasm 为裸指针。具体数值宽度原样保留。

- `layout: LayoutId` 指向 `LayoutRegistry` 的 `MemLayout`（header_size、有序 MemField 的 FieldOffset::Fixed/AfterDynamic、MemType、elem_stride）。**字段偏移变成 MIR 查询**，消除 walker 的 `byte_size` 累加及 wasm 的 `4+foff` 字面量。
- variant 布局包含 `tag_value`（case 声明顺序索引，在**一处**决定，消除 mod.rs:1338 和 equality.rs:179 双重计算）、`tag_offset`（当时固定为 0）及 `payload_layout`。将来的 niche/tagless 优化只改**一处**。
- `clone_free`（top_let_storage.rs:67）⇔ `Repr::Scalar`。#531 的四张投影表合为一个 `Repr`。

**硬性前提**：Repr 必须完全具体化。`is_heap_type` 的 `Ty::Unknown ⇒ heap`（pass_perceus.rs:620）和 `byte_size` 的 Unknown→4 回退（values.rs:75，#525）是**静默误编译温床**。→ 把 `AllTypesConcrete` 门禁（当时 pipeline 后的 `assert_types_concretized`）移到 **Core→MIR 之前**（§4）。

<a id="22-所有権ノード集合"></a>

### 2.2 所有权节点集合

将 Phase 0 原型节点集合扩展到全语言：

```
Alloc { repr }                 // ヒープ確保(+ field store)
Store / Load { offset }        // layout 経由のフィールド読み書き
Dup    v                       // +1 owned ref(Perceus dup) — wasm:__rc_inc / Rust:.clone()
Drop   v                       // -1 ref(以後未使用) — wasm:__rc_dec / Rust:スコープ末 Drop
Borrow v  (mut?)               // 消費せず参照 — wasm:ptr透過 / Rust:&v / &mut v
Consume v                      // 最後の使用=所有権移動 — wasm:ptr転送(inc無) / Rust:move
MakeUnique v                   // 共有され得る可変への変更前(§2.5)
Call ...
```

代码注释译文：Alloc 分配堆并存储字段；Store/Load 按布局读写字段；Dup 增加一个拥有引用（Perceus dup），wasm 用 __rc_inc，Rust 用 .clone()；Drop 减一个引用，之后不再使用，wasm 用 __rc_dec，Rust 在作用域末 Drop；Borrow 不消费而引用，wasm 透明传指针，Rust 为 &v / &mut v；Consume 是最后使用并转移所有权，wasm 传指针不递增，Rust move；MakeUnique 位于可能共享的可变值修改之前（§2.5）。

**不变量**：`Dup`/`Drop`/`Borrow`/`Consume`/`MakeUnique`/`Repr` 是 **MIR 一次决定的语义事实**。渲染器改变其**数量或极性**，就是重新决定所有权，也就是错误（§3 lint）。

<a id="23-per-binding--per-parameter-極性gate-finding-1"></a>

### 2.3 逐绑定 / 逐参数极性（gate finding #1）

所有权决策单位是绑定/参数的极性：

- **函数边界**：每个参数有 `own | borrow | borrow_mut`。`ParamBorrow` 的 Ref/RefStr/RefSlice **不是 MIR 事实**；由 borrow × 参数 Repr **导出拼写**（`Ptr{String-layout}` 的 borrow → `&str`；`Ptr{List-layout}` 的 borrow → `&[T]`）。不动点签名传播（调用者极性依赖被调用者，pass_borrow_inference.rs:95）作为 **Core→MIR 内的数据流**只执行一次。
- **每个使用点**：`Dup`（clone 数 >1）/ `Consume`（最后使用=move）/ `Borrow`。pass_clone 的 `eligible` last-use==0 move 表示“计数静态为 1→Perceus 仿射特例”；`always` 类（top-let/static/loop-bump）表示“计数静态 >1，必须 Dup”。

> 同一源码绑定，consume 时 move，borrow 时 clone/dup。这**不是渲染器重新决策，而是 MIR 对每个绑定/参数决定一次**。

<a id="24-平坦化された-drop-schedule最大の獲物"></a>

### 2.4 展平 drop 调度（最大收益）

`emit_typed_rc_dec`（statements.rs:734-939）在 emit 时重遍历 Ty，**在渲染器内重新决定** List/Set 元素、Map 的 key/val、Result/Option payload、record 字段、closure env 的递归 drop。改为 **Core→MIR 对每次 drop 一次决定完整所有权树，并 lowering 为平坦 `Drop` 序列**：每个拥有的子指针对应一个 Drop，含释放顺序。

结果：
- wasm RcDec 分支只有**一行**（`call __rc_dec`），Rust 在作用域末 Drop；**两边都不重遍历 Ty**。
- 重复的 `is_heap_type`（坐标①）**消失**。
- §643 类“渲染器重新决定 drop”变得**结构上不可能**。

⚠ **最高价值也是最高风险**：递归 drop（Map-entry/closure-env/nested-Named）的 **corpus 覆盖最薄**（§5 先补专门 fixture）。

<a id="25-共有可変ノード-makeuniquegate-finding-4-の縁"></a>

### 2.5 共享可变节点 `MakeUnique`（gate finding #4 的边缘）

**重要实测**：共享可变逃生口**当时已存在于 production**：`detect_shared_mut` 的非 Copy 分支产生 `SharedMut<T> = Rc<RefCell<T>>`（lib.rs:481），Copy 分支产生 `Rc<Cell<T>>`。Phase 0 门禁说“只读捕获 Rc 为零”是因为**形态不同**，不矛盾。

→ MIR 的 `MakeUnique` / 共享可变必须持有**区分 Cell（Copy，不经借用修改）与 RefCell（非 Copy，可被 `list.push` 等原地 `&mut` 借用）的 Repr 事实**。`needs_cow`（wasm cow_check）和 Rust 修改时 clone **统一到此节点**；alias_cow.rs 已实证“一次决策”。

---

<a id="3-単一-coremir-パスと忠実なレンダラ契約"></a>

## 3. 单一 Core→MIR pass 与忠实渲染器契约

<a id="31-1つのパスが全部決める"></a>

### 3.1 一个 pass 决定全部

把 §1.1 六行（五个 pass + emit_typed_rc_dec）统一为**一个 Core→MIR 所有权+布局 pass**。输出 MIR：附 Repr 的值、§2.2 节点、逐参数/使用极性。PerceusPass（is_heap_type × yields_borrowed_alias）已是规范 fresh/alias 分类器，**从这里出发**，Rust 的 clone/borrow 也由同一事实导出。

<a id="32-忠実性契約を操作可能な-lintにする"></a>

### 3.2 把忠实性契约变成可操作 lint

由实现强制遵守 §3 宪法：

> **任何渲染器都不得自行计算 `is_heap_type` / `yields_borrowed_alias` / last-use / 递归判定，只能读取 MIR 事实。计算就是 #643 重新决策错误。**

- **可吸收语法（SYNTACTIC）差异**（gate finding #2）：无 box-pattern→tag guard + `&**deref`（v1 的 BoxDeref 是**渲染器工作**而非 Core→MIR pass）、选择 `&str` 或 `&[T]`、`__hoist` 临时量、ANF lifting、match 展开、IterChain fusion。
- **禁止语义（SEMANTIC）决策**：改变 `Dup/Drop/Borrow/Consume/Repr/MakeUnique` 的数量或极性就是错误。
- 实现：从渲染器**删除这些谓词的调用**，改为读取 MIR 字段。CI grep 禁止残留调用（忠实性 lint）。

---

<a id="4-パイプライン挿入点canonical-cut"></a>

## 4. 流水线插入点（canonical cut）

精读发现**不对称**：Rust 路线（main.rs:484）**不调用** `optimize_program`/DCE（只 ConstFold），wasm 路线（build.rs:294）在 mono **之前**调用。五个所有权 pass 在 mono/DCE/ir_link **之后**的 nanopass Pipeline 中运行。

→ Core→MIR 放在**两目标共用的同一切点**：mono + ir_link + DCE 之后、任何所有权 pass 之前。前提：
1. **前移 `AllTypesConcrete`**（Repr 要求具体类型，§2.1）；当时在 pipeline 后 assert。
2. 消除编译顺序不对称：两路线将相同 Core IR 交给 Core→MIR。
3. 硬顺序约束（BorrowInsertion→CloneInsertion / ANF→Perceus / StackBalance→Perceus / Canonicalize 最后）作为**一个 pass 内的阶段**重现；ANF/堆可见性是正确 Drop 放置的前提。

---

<a id="5-移行戦略--オラクル駆動の脱リスク順序"></a>

## 5. 迁移策略——oracle 驱动的降风险顺序

**不重写。以终态为北极星，借已有 byte gate + corpus 作为 oracle 收敛并剥离旧实现**（§6）。按依赖和“尽早低成本检验押注”排序：

| 阶段 | 内容 | 风险 | Oracle | 顺序理由 |
|---|---|---|---|---|
| **1. Repr-on-values** | LayoutRegistry+byte_size 提升为 MIR 值；前移 AllTypesConcrete；行为不变，仅附带 Repr | 低（已集中化，魔数 0） | 字节一致（行为不变） | drop 展平前提，最机械 |
| **2. 展平 drop 调度** | emit_typed_rc_dec→平坦 Drop 节点，wasm RcDec 一行化，消除重复 is_heap_type | **高**（corpus 最薄、§643 核心） | **先加专门 compound-drop fixture**（Map-entry/closure-env/nested-Named）+ byte gate + corpus 重放 | 真正检验押注，可早期证伪 |
| **3. 统一极性** | 一 pass 为两目标生成逐使用 Dup/Consume 和逐参数 own/borrow；pass_clone/borrow_inference/perceus 重构为读 MIR | 中（两实现保守性不同） | **必须值等价**（byte gate），类型不匹配另以编译错误捕获 | 渲染器机械化核心 |
| **4. 统一 capture+MakeUnique** | detect_shared_mut/detect_mutated_captures→一个分析；needs_cow + 修改时 clone→一个 MakeUnique（区分 Cell/RefCell Repr） | 中（边缘、双检测器漂移） | 值等价 + 宿主确定性（BTreeSet 顺序） | 显式节点化 finding #4 边缘 |
| **5. Rust 渲染器 MIR 化** | Rust 也消费 MIR，两渲染器均机械化 | 中 | 全 corpus 字节/值一致 | §7“接着切换 Rust 渲染器” |

<a id="51-検証-oracle-の二層構造完全性の核心--必読"></a>

### 5.1 验证 oracle 的两层结构（完整性核心，必读）

**统一会放弃差分测试依赖的独立性本身**。byte gate（native↔wasm 一致）有效，是因两个**独立实现**不容易偶然一致。v1 两渲染器来自**同一个 MIR**；**Core→MIR 决策错时全部目标同样错，native↔wasm 一致门禁在这里失明**（两边 agree）。这不是假设：Phase 0 alias_cow（§8.1-③）已实证，省略 MakeUnique 后两种表达**同样**破坏 `a`，RC 仍平衡；共享决策错误对差分测试不可见。

→ 完整性**绝不能只依赖 byte gate**。每个归约步骤都要有**独立于被验证对象的 oracle**：

| 步骤 | 保持性质 | 独立 oracle | byte gate 能覆盖吗？ |
|---|---|---|---|
| **Core→MIR（决策）** | 保存源码语义 | **语义定律 property test + 翻译验证（#570）** | ❌ **不能（本最前沿）** |
| MIR→wasm | 忠于 MIR 小步语义 | interp 一致 + wasm-spec 忠实性证明 | △（迁移期旧 emit 是独立版） |
| MIR→Rust | 忠于 MIR 小步语义 | 迁移期旧 emit 差分，迁移后 interp | △ |

**第二行 oracle（Core→MIR 正确性）是本阶段承重的新工作**。byte gate / interp 用于渲染器忠实性，对 Core→MIR 错误无能为力。具体措施：
- **语义定律 property test**：把**定律**而非 fixture 写为 proptest。值语义“`b=a; mutate(b)` 时 `a` 不变”、所有权不变量（dup/drop 平衡、drop 后无 use-after-free、Consume 后不再引用原变量）作为**源码层真理**作性质检查。不需第二实现，从**上方**抓共享决策错误。
- **interp 能抓渲染器错误，不能抓 Core→MIR 错误**：忠实执行错误 MIR 会**一致**地产生错误结果。因此 interp 用于 MIR→target，Core→MIR 需要 property+proof。
- **翻译验证证明（#570）**：证明 Core→MIR 所有权决策保存语义。core 已有 Perceus 健全性 Lean，针对 MIR 小步语义重述（Phase 2 正式展开，Phase 1 先用 property test）。

<a id="52-renderer-忠実性ゲートbyte-gate-群--既存資産"></a>

### 5.2 渲染器忠实性门禁（byte gate 群，已有资产）

- **值等价（byte gate）是承重项**，限渲染器层。只看泄漏/RC 平衡**不够**：AliasCow 回归在 RC 平衡时仍 wrong-output（alias_cow.rs 已实证）。只叠加 Perceus-belt 式 RC 验证器会放过值破坏。
- 以 `wasm_cross_target_spec`（tests/wasm_runtime_cross_target.rs，两目标比较 exit/stdout/stderr 三元组）验证 **`new MIR-wasm == old emit_wasm`**。迁移期以**冻结的旧 emit_wasm**为基准重放更广 corpus；旧 emit 仅在迁移期间作为**独立版**（§5.3）。`@xt-allow` 棘轮只减不增。
- **宿主确定性**（check-host-determinism.sh）：MIR pass 若在 drop/cow 顺序加入 HashMap 遍历，值门禁可过而宿主门禁会失败。保持 BTreeSet 顺序。
- **heisenbug 对策**：corpus 重放必要但历史上不足（§643），同时用定向 drop-order fixture。
- 新旧两边用**相同 `ALMIDE_WASM_FREES`**，不要比较泄漏 body 与 RC body 造成虚假差异。

<a id="53-退役順序の不変条件危険な落とし穴"></a>

### 5.3 退役顺序不变量（危险陷阱）

上级 §6 写“忠实性建立后退役 byte gate”，但**顺序至关重要**。只有**更高层独立 oracle（interp + property laws + wasm-spec 忠实性）先完成替代**，才可删除差分 oracle（与旧 emit 差分 / native↔wasm 一致）。否则退役瞬间会暴露 Core→MIR 决策错误盲区。将**“删除差分前先建立高层独立 oracle”**作为迁移硬性不变量。

---

<a id="6-リスク台帳接地済み"></a>

## 6. 有依据的风险台账

| # | 风险 | 出处 | 门禁/缓解 |
|---|---|---|---|
| R1 | emit_typed_rc_dec 的 Map-entry/closure-env/nested-Named drop 覆盖最薄；即使通过全部 149 个 wasm_cross fixture，未测复合 drop 仍漂移 | statements.rs:734-939；wasm_cross=149（270 是更广 corpus） | §5 阶段2前加专门 fixture，MIR 化使其可被检查 |
| R2 | 名称字符串所有权通道 `__tco_`/`__br_`/`__perceus_*`（donate/share）随命名变化静默反转，曾有 7→55MB churn 回归 | pass_perceus.rs:309-315 | MIR 用**显式事实**表示 move/dup，不依赖名称 |
| R3 | TCO 在 BorrowInsertion 后**剥离**参数 borrow 注解，变为 owned loop var | pass_tco.rs:1142 | loop 转换=owned 在极性决定**之前**；byte gate 以 E0308 捕获，是值等价不足的例子 |
| R4 | @intrinsic/@inline_rust 模板拼写 borrow，与 MIR 决定的 borrow 冲突成 `&*&*` | pass_borrow_inference.rs:308 | runtime 自举（Phase 3）前接缝脆弱，迁移期模板侧固定 Own 隔离 |
| R5 | 丢失 is_derive_fn/mono 排除（全 Own）会让跨模块 owned-arg 调用 E0308 | pass_borrow_inference.rs:371（@derived，#647） | MIR 带“边界 fixed-own”属性 |
| R6 | 人工维护的 in-place-mutation-counts-as-use 对称性失同步会导致 use-after-move（E0382） | pass_clone.rs:116 | MIR drop/dup 放置重现同一记账 |
| R7 | 模块 top-let 是 static，不能 move（E0507）→总是 clone | pass_clone.rs:156 module_origin | Repr/极性标记 borrow-only/Dup-always |
| R8 | 递归判定过度包含在 Rust 中无害（多余 Box），但驱动 wasm alloc/RC 会尺寸错误 | walker/helpers.rs:39 | 两渲染器统一 Boxed 含义；byte gate |
| R9 | variant_alloc_size 的 4+max_payload padding（wasm）与 rustc enum layout（native）不对称，天真 Repr 破坏 mem_eq | equality.rs:1272 | 值等价门禁 |
| R10 | mutable_captures 用 plain rc_dec；把 cell 指针当 object 遍历会递减垃圾引用→wasm trap | statements.rs:360-368 | MIR 记录“cell，plain drop”事实 |
| R11 | 阶段2–5切换期旧 emit_typed_rc_dec 与新 MIR Drop 共存，两份活跃 is_heap_type 漂移 | statements.rs:724 | 按 fixture 原子切换，持续 assert 两路径字节一致 |
| R12 | 一个极性错误扩散至 corpus，验证尾部为 §7 最大成本 | — | 每阶段完整 corpus 重新变绿，阶段2尽早判定押注 |
| **R13** | **统一消除差分独立性→Core→MIR 错误使全部目标同样错，native↔wasm 一致门禁失明**；alias_cow §8.1-③ 是预警 | §5.1 | **property-law oracle（值语义/所有权不变量）作为一等门禁**；退役顺序不变量（§5.3）；byte gate 仅管渲染器忠实性 |

---

<a id="7-phase-1-完了の定義exit-criteria"></a>

## 7. Phase 1 完成定义（退出条件）

1. 单一 Core→MIR pass 生成 §1.1 全部六个决策，**两渲染器不再调用 `is_heap_type`/`yields_borrowed_alias`/last-use/递归判定**，忠实性 lint 在 CI 变绿。
2. 删除 emit_typed_rc_dec，wasm drop 变成平坦 Drop 节点的一行渲染。
3. 坐标①–⑤重复实现统一为单一事实。
4. 全 corpus（native↔wasm）**值等价**变绿，宿主确定性绿，`@xt-allow` 未增加。
5. **Core→MIR 决策的 property-law oracle 绿**（值语义/所有权不变量 proptest 化，§5.1），作为从上方捕获 byte gate 无法覆盖之共享决策错误的一等门禁运行。
6. `PerceusVerify`（Lean 证明检查）对新 MIR Dup/Drop 节点仍有效，flight-grade Lean 资产不脱离。
7. §643 / #591 / #610 **结构性**解决而非单点补丁，并以回归 fixture 固定。
8. 差分 oracle（旧 emit 差分 / byte gate）**暂不退役**。只有 Phase 2 以后，更高独立 oracle（规范 interp + property laws + wasm-spec 忠实性）在 §5.3 不变量下替代后才退役。

→ 达成后进入 Phase 2：MIR 形式化、interp 规范化、Lean 转到 MIR 语义。未达成则可通过**阶段2廉价的早期证伪**选择 §8 失败分支：重审规范形、允许 Rust 显式 Rc、或仅用于工具资格化。

---

<a id="8-実装状況と次の一手"></a>

## 8. 实现状态与下一步

<a id="80-段0完了-意味法則-property-law-oracle-"></a>

### 8.0 阶段0（完成）：语义定律 property-law oracle ✅

§5.1 独立 oracle 已在**手术前**铺设、验证、提交（`tests/semantic_laws_test.rs`）。generator 用普通 Rust 独立计算值语义正确答案，在 native+wasm 执行相同操作的 Almide，检查**两者均符合独立模型**（`native==expected AND wasm==expected`，严格强于 `native==wasm`，统一后仍有效）。模型自身以已知 alias_cow（C-033）形态对照真实编译器自检。当时编译器上 32+ 案例、3 测试全绿，可用 `ALMIDE_SEMLAW_CASES` 深挖。这是从上方监控阶段1以后所有所有权修改的网。

<a id="81-段1repr-on-valuesの-grounding実測"></a>

### 8.1 阶段1（Repr-on-values）的实测依据

- **已有 `Repr` 种子**：`crates/almide-ir/src/wasm_repr.rs` 中的 `WasmRepr{I32/I64/F64/Void}` 用于 mono ABI 兼容判定，但只有**粗略四类，无布局**；§2.1 的 `Repr{Scalar{width}/Ptr{LayoutId}/Boxed{LayoutId}}` 更丰富。
- **Repr 跨 crate**：enum 本体（scalar width）可放 almide-ir，但 `LayoutId` 指向的 `LayoutRegistry`（`emit_wasm/engine/layout.rs:77`）和装箱属性（`pass_box_deref.rs:132
  find_recursive_enums)は codegen 側。→ 段1 は almide-ir(Repr 型)+ codegen(layout 解決・
  stamp)に跨る一体変更。LayoutRegistry の所属(codegen→共有クレートへ昇格?)が最初の設計判断。
- **再設計衝突なし**: `codegen-ideal-form.md` **已完成**（commit 8ea2d234，crates/CLAUDE.md 的 active/ 引用陈旧）；var-table/dispatch 统一也完成。MIR 工作建立在已有统一之上。

原文未闭合 inline 跨段译文：find_recursive_enums 在 codegen；阶段1是一体变更，跨 almide-ir（Repr 类型）及 codegen（布局解析、标记）。LayoutRegistry 归属、是否由 codegen 提升到共享 crate，是首个设计决策。后续项目标题表示“无重设计冲突”。

<a id="82-段1-の着手手順"></a>

### 8.2 阶段1启动步骤

1. 前移 `AllTypesConcrete` 并消除编译顺序不对称（§4）的 PoC，确认行为不变、字节一致。← **下一步，手术起点**。
2. 加 MIR 值模型（`Repr` enum + LayoutId），mono 后标记 Repr，吸收 `WasmRepr`。
3. ~~阶段2专门 compound-drop fixture~~ ✅ **完成**：`spec/churn/{map_entry,closure_env,nested_named}_churn.almd` 用 200k churn 固定 emit_typed_rc_dec 三分支（Map-entry/closure-env/nested-Named drop）。`check-frees-churn.sh` 检查 native==wasm + exit0 + no-trap，六个 fixture 全绿。避开 `??`（UnwrapOr）与 #643 分离。已先为手术最薄区域铺网。

阶段1跨 almide-ir+codegen 一体实现，需要多个会话。**手术前两张网均已铺设且绿**：§8.0 语义定律 oracle 从上方观察值语义，§8.2-3 compound-drop churn 覆盖最薄区域 drop 调度。每个增量都确认两张网保持绿再推进。
