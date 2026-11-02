历史描述：单一语义架构：所有权和布局在 MIR 中只决定一次，renderer 仅重放决定。
<a id="almide-v1-mir-を唯一の真とする単一意味論アーキテクチャ"></a>

# Almide v1：以 MIR 为唯一真相的单一语义架构

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-mir-architecture.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-mir-architecture.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-mir-architecture.zh-CN.md)

> 非规范性固定原文历史架构读者版。固定原文仍为正本且不变；不自动同步，未实现、重跑测试/证明或独立测量。6月13日五形spike结果是有限实验，不证明全部程序或当前renderer正确。结构性不可能、自动正确、唯一道路、规模及资格鉴定的表述是原设计论证和目标；本译文不独立确立这些主张，也不声称航空认证。旧walker/emit和计划MIR/interp规范地位是历史描述，不覆盖当前规格。保留同错输出一致与独立语义oracle的区别、退役gate前必须替代oracle，以及尚待完成的形式化/self-host/资格鉴定工作。原文行/节引用和设计张力不擅自修正。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md)

状态：active — Phase 0 决策门槛通过（5/5，§8.1）。采用**完全 greenfield + 双 oracle**实现（§6），语言表面按 **mission-first**（§10）确定（2026-06-13）。手术前的两张网（语义法则 oracle / compound-drop churn）已铺设且全绿。
Owner: compiler
取代 / 包含：#529（WasmIR）、#643/#591/#610（cross-target emit drift 类），以及 wasm-ownership-emit-mechanization.md 的提案部分
相关：flight-grade #563–#586、trust-layer

---

<a id="0-なぜ-v1-か--病巣は意味論の実装が2つあること"></a>

## 0. 为什么是 v1 — 病灶是“语义有两个实现”

当前 Almide 从高层 IR 有**两条独立 lowering**：

- `walker/` → 惯用 Rust（native）。将所有权委托给 Rust。
- `emit_wasm/` → 手写 wasm（约 136 个 runtime routine + emit 层手工放置 inc/dec）。

native 是“oracle”，因为它**委托**给 Rust 成熟、经验证的所有权/内存系统。wasm 则**手工重新实现**。约 72% 的 cross-target bug 是“wasm 偏离 native oracle”，#643/#591/#610 是其中所有权/内存版本。

庞大的验证群（byte gate / rt-oracle registry / churn / interp 第三裁判 / Lean）**全是事后检测两个实现漂移的装置**。有两个根本限制：

1. **原则上无法达到“由构造保证正确”。** 因为承诺永远同步两个手写实现，上限是“通过全面检测实现正确”。完备性路线图变成零敲碎打修补，原因就在这里。
2. **检测本身也有漏洞。** #643 就是证据 — 观察（`__println_int`）调用 `__alloc`，扰动 heap 复用、掩盖 corruption 的 heisenbug，byte gate 和 `almide test` 都漏掉了。

→ **v1 目标是从“靠检测正确”转向“靠构造正确”。** 因此将语义实现变成**一个**。

---

<a id="1-v1-アーキテクチャ"></a>

## 1. v1 架构

```
.almd
  → Core IR            型付き・脱糖・ターゲット中立(今の高 IR を整理したもの)
  → MIR                所有権(Perceus)と レイアウトを明示。小ステップ意味論を持つ。 ← 唯一の真
      ├ → wasm         主経路・検証済み・主権的(MIR は wasm 整合 → ほぼ自明に下る)
      └ → Rust         perf と Ferrocene 資格化の踏み台(MIR を Rust idiom に描くだけ)
  runtime              Almide 自身で書き、同じ Core→MIR→target を通る(self-host)
```

**不变量（v1 宪法）：**

> **所有权和布局只在 MIR 中决定一次。renderer（Rust/wasm）仅重现决定，绝不重新决定。**

这句话使 #643 类在结构上不可能发生。当前 #643 的原因是“emit_wasm 手工布置所有权，偏离 MIR（或等价分析）” — 因 renderer 重新决定而发生。宪法禁止它。

---

<a id="2-mir-の設計中核"></a>

## 2. MIR 设计（核心）

<a id="21-明示するもの"></a>

### 2.1 显式表达什么

1. **Repr（值表示）**：每个值为 `Scalar{width}` / `Ptr{layout}` / `Boxed{layout}` 之一。字段偏移、variant tag 放置、recursive 类型 box 化 — 当前由 layout registry 和 box_deref 分散决定的东西，变成 **MIR 的属性**。
2. **所有权操作**：对值显式标注 Perceus `dup`（inc）和 `drop`（dec）。逐使用点区分 **consume（move）/ borrow / dup**。
3. **分配/构造**：alloc + 字段 store 成为显式节点。

**设计纪律：永久核心中的量是 ValueObject（用类型排除 magic number）。** 分散的原始常数是 v0 痛苦的原因之一。v1 永久 MIR 核心以**值对象**而非裸数字保存量，让 `Repr::Scalar { width: 4 }` 这样的 magic number 成为**编译错误**：
- 宽度 = `ScalarWidth` enum（`Word`/`Double`…，`.bytes()` 是唯一数值化点，即“Word=4”只在一处）。不能写裸 `4`。
- `LayoutId` 字段 private。不可临时构造 `LayoutId(0)`，只能用 `PLACEHOLDER_LAYOUT` 或 registry 发行。
- 未来 layout offset 也用 `ByteOffset` 等 ValueObject。

→ **类型系统本身就是门槛**（始终开启、零维护，不是事后检测而是结构性不可能）。唯有字符串模板 bootstrap WAT（§4.1）无法受类型保护，此处以“不增加手写 routine”的机器门槛（`handwritten_wasm_runtime_does_not_grow`）替代。

<a id="22-正準モデル--perceusこれが鍵"></a>

### 2.2 规范模型 = Perceus（关键）

**Perceus RC 是更一般的模型。affine/move 只是“计数静态已知为 1 的 RC”的特例。** 因此 MIR 若采用 Perceus 所有权（显式 dup/drop/borrow + 显式 Repr），两目标都成为**机械翻译**：

| MIR（Perceus） | wasm renderer | Rust renderer |
|---|---|---|
| `dup v` | `__rc_inc(v)` | `v.clone()` |
| 最后一次 consume | 传递指针 | move（所有权转移） |
| `borrow v` | 传递指针 | `&v` / `&mut v` |
| `drop v`（此后不使用） | `__rc_dec(v)` | scope 结束时 Drop |
| 可变别名 + 修改（AliasCow） | COW（`__cow_check`） | 修改前 `.clone()` |
| closure capture | dup capture | 向 move-closure clone-in |

也就是说，当前**不同 pass**所做的 — `pass_perceus`（wasm inc/dec）、`pass_clone`/CloneInsertion（Rust clone）、`pass_borrow_inference`（Rust &）、`pass_capture_clone`、`pass_box_deref` — 是**同一所有权事实的不同表示**。v1 让**一个 Core→MIR pass 决定一次**，两个 renderer 仅按上表描绘。

**所有权的决策单位是“每个绑定/参数的极性”**（Phase 0 门槛确认，§8.1-①）。即使相同源码表达式，该绑定/参数若为 **consume**，就是 move（Rust）/ptr 转移（RC）；若为 **borrow**，则是 `.clone()`（Rust）/dup（RC）。这**不是 renderer 重新决定，而是 MIR 对每个绑定、每个参数决定一次的事实**。因此“`alias 返却` 时 move payload”（所保留日文指 alias 返回）和“borrow 参数时 clone”，只是同一规范形式的不同极性。

**规范形式的已知边缘（Phase 1 处理）**：只有“**被共享、跨多个 call 修改并返回**”的值不属于 move/borrow 二选一，需要显式共享可变性（MIR MakeUnique / 相当于 `Rc<RefCell>`）。这是 AliasCow 行（§8.1-④）的推广，read-only capture 和简单 alias **未到此边缘**（门槛中可用零 Rc 表达）。v1 将此边缘做成 **MIR 显式节点**，不让 renderer 隐式引入。

**与 Lean 资产一致**：已有 Perceus 健全性、ClosureRc 证明。针对 MIR 小步语义重新陈述后，“语言所有权 = 形式化证明的 Perceus”即可作用于**两个目标**（当前仅 wasm）。

<a id="23-形式の錨"></a>

### 2.3 形式化锚点

给 MIR 小步操作语义（自己的 IR，因而可以）。使之与**机器验证的 wasm 形式规格一致**：MIR→wasm 忠实性可针对 wasm spec 证明。Rust 没有完整形式语义，故 Rust renderer 忠实性由**翻译验证 + 迁移期间 byte gate**确保（下方 Phase）。

**完备性核心 — unify 放弃差分独立性**：变为一个实现后，跨目标差分测试（native↔wasm 一致）所依赖的**独立性本身**消失。Core→MIR 决定若错，**所有目标会同样出错**，一致门槛对此失明（双方 agree）。Phase 0 alias_cow（§8.1-③）已实证。因此通过在**每一步归约放置独立于受检对象的 oracle**达到完备性。尤其 **Core→MIR 正确性的 oracle 不是跨目标差分** — 它会令双方 agree — 而是**源码层语义法则（值语义、所有权不变量）的 property test + 翻译验证（#570）**。interp“忠实执行错误 MIR，也会同意错误结果”，因此用于 MIR→target 忠实性，对 Core→MIR bug 无力。详情及双层 oracle 结构见 [v1-phase1-mir-core.md §5.1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md)。

---

<a id="3-忠実なレンダラの契約"></a>

## 3. 忠实 renderer 的契约

renderer R **忠实**，是指对任意 MIR 程序 P，R(P) 的观察行为与 MIR 语义 ⟦P⟧ 一致。具体规则：

- **renderer 不决定所有权/布局。** 仅将 MIR 的 `dup/drop/borrow/Repr` 转成 idiom。重新推导就是 **bug**（作为禁止 #643 重演的 lint/assert 实现）。
- **可吸收语法（SYNTACTIC）差异，不可做语义（SEMANTIC）决定**（Phase 0 门槛 §8.1-②）。例如 Rust 无 box-pattern，boxed field 的 nested ctor 表达为 tag-guard + `&**deref` — 这是**解决目标语法差异**，不是重推所有权/布局。边界清楚：**改变** `dup/drop/borrow/Repr/MakeUnique` 就是语义重新决定 = bug；其他写法（deref 插入、match 展开、临时变量）是 **renderer 自由**。
- wasm renderer：针对 wasm spec 证明忠实性（长期）。
- Rust renderer：通过翻译验证检查忠实性。迁移中现有 byte gate 是保证“新 MIR-wasm == 旧 emit_wasm”的 oracle。
- **忠实性违规的观察特征依形状而异**（§8.1-③）。重新决定所有权在 #643 表现为 RC double-free，在 AliasCow 则是**值损坏（RC 仍平衡）**。故迁移门槛**只有**“无 leak/double-free”不足，还要同时要求 native↔wasm **值等价**（现有 byte gate 满足）。

---

<a id="4-self-host-runtime"></a>

## 4. Self-host runtime

`runtime/rs`（native）与 emit_wasm 约 136 个 routine（wasm）的双重实现，是所有权以外漂移（约 72% 的主体：`string.lines`/regex/libm/json…）的来源。v1 **用 Almide 编写 runtime** → 通过同一 Core→MIR→target 到达全部目标。整个漂移类消失，也成为 dogfooding（旧 `research/selfhost/`，现 `research/spike/v1-mir/` 的方向）。仅最小 alloc/RC primitive 为 MIR 内建，其余 Almide 编写。逐阶段 bootstrap（Phase 3）。

<a id="41-wasm-emit-の理想形--手書き-wasm-表面を極小化して証明可能にする"></a>

### 4.1 wasm emit 理想形 = 最小化手写 wasm 表面以便证明

**v0 wasm emitter 如地狱般的根因 = 两个巨大手写表面**：① emit 层逐操作手工决定所有权/布局（drift、#643、is_heap 双份、tag 两处）；② 手写约 136 个 wasm routine，与 native `runtime/rs` 双重维护。多到无法保持正确。

**理想形原则：不再受苦的唯一道路是缩小 emitter。**

- **MIR 掌握全部决定 → renderer 从不决定**（§3）→ drift 和 #643 结构性消失。
- **手写 wasm = 仅 MIR primitive 集合的映射**（约 20：alloc / offset load、store / scalar 运算 / 控制流 block、loop、if、br / call / rc_inc、dec、cow）。全域、穷尽、**零决定**。这是唯一“受苦表面”，足够小，能**写一次并针对 wasm 形式规格证明忠实性**（20-op 可证明，136-routine 不可 → 最终形 V/faithfulness 的前提）。
- **其余全部用 Almide 编写，走相同 Core→MIR→wasm**。list/string/map/formatting/RC 本体，即 136 个 routine，变成 **Almide 代码**，一份源码自动使两个目标正确。**零手写 wasm runtime。**
- **边界 = “MIR primitive op”与“Call to self-hosted runtime fn”**。`Push`/`IndexSet`/`Print` 这类操作不烧进特殊 MIR op，而变为对 Almide runtime 函数的 `Call`。MIR op 集合缩到仅 primitive。

> ⚠ 注意 bootstrap 捷径：为先跑通路径，`almide-mir` wasm renderer（`render_wasm.rs`）暂时用 WAT 手写 list_copy/itoa/print，并将 `Push`/`IndexSet`/`Print` 烧成 op。**这正是 v0 陷阱** — 仅为证明“能跑”的临时脚手架。通过 (a) 替为 runtime fn Call，(b) op 集合缩为 primitive，(c) runtime Almide 化，向理想形收敛。纪律是不增加手写 WAT。

---

<a id="5-flight-grade-が-v1-から落ちてくる"></a>

## 5. Flight-grade 从 v1 设计中自然导出

| flight-grade 目标 | v1 中的实现 |
|---|---|
| #529 WasmIR（构造保证结构不变量） | **MIR 本身** |
| #563 ALS / #530 规范规格 / #564 interp 成为规范 | **MIR 小步语义 = 规范；interp = 执行它的参考** |
| #570 逐构建翻译验证证书（已验证 checker） | 分别验证 Core→MIR 和 MIR→target **各一条**。MIR 有形式规格就能编写 checker |
| #572 可读 Rust 的逐行追踪 | 单一 pipeline 的 provenance（.almd→Core→MIR→Rust） |
| #573 Ferrocene / #574 KCG 资格鉴定包 | **有规格 MIR → 有规格 Rust** = 可资格鉴定 code generator 的形式。Rust→机器由 Ferrocene 负责 |
| #575 DO-333 形式信用 / #576 Lean 进入 runtime | 针对 MIR 语义证明 Perceus 健全性。runtime 是 Almide 代码，走已验证 pipeline |
| #567 Critical subset / #568 静态内存 / #569 WCET | 定义为 **MIR 子语言/属性**。单一有规格 lowering 使 WCET 分析容易数个数量级 |

当前结构让 flight-grade 成为**另一座山**，v1 让 **flight-grade 从设计自然导出**。

---

<a id="6-完全グリーンフィールド--二重オラクル2026-06-13-確定"></a>

## 6. 完全 greenfield + 双 oracle（2026-06-13 确定）

**确定**：v1 **完全 greenfield**（包括 lexer/parser，全部从零，以独立 crate 群全新构建）。最初 §9 以丢失 270 fixture 的知识、second-system risk 为由拒绝从零重写，但这**过于粗糙** — second-system risk 的本体是**隐性知识丢失**，而 Almide 的知识不在实现代码中，而结晶为**可执行外部规格**（270 spec fixture / wasm_cross byte-gate / 100 contracts / 137 rt-oracle / 参考 interp / 语义法则 oracle / Lean / CHEATSHEET、STDLIB-SPEC）。知识在 **fixture 和 contract** 中，不丢弃就不会崩塌。

**安全“从零”的唯一形式 = 带双 oracle 的 fresh parallel build：**

- 将新 pipeline **作为新 crate 并行构建**，**旧实现完全不碰，在证明 parity 前保留为差分 oracle**。
- 新实现由 **(a) 全部外部规格（fixture/contract/interp/语义法则 oracle）** + **(b) 与旧实现差分** + **(c) §2.3 语义法则/形式 oracle** **三重**裁决。
- **逐 fixture 从已达到 parity 的部分切换**。失败时丢弃新路径，旧的仍完好。
- **不可协商约束（完备性核心）**：fixture 单独不完整（R1）。旧实现是供给**差分独立性**的第二实现，**parity 前不可删除**（否则暴露 §2.3 盲点）。

> 对改变表面的功能（range/protocol/where，§10），旧实现只对**不变语义**可作差分 oracle；改变的表面建立新 spec。**语义法则 oracle 与表面无关，因此全面有效。**

**删除列表（v1 终态消失项 = 选择双实现带来的债务）：**

- emit_wasm 的 emit 层所有权（手放 inc/dec）→ MIR dup/drop 替代
- `pass_perceus`/`pass_clone`/`pass_borrow_inference`/`pass_capture_clone`/`pass_box_deref` → 合并为**一条 Core→MIR 所有权 + 布局 pass**
- byte-identity gate（wasm_cross）→ renderer 忠实性建立后不再需要（作为迁移 oracle 用尽后退役）
- rt-oracle registry + churn（作为漂移检测）→ runtime 单一化后不再需要
- `runtime/rs` ↔ emit_wasm routine 双重维护 → 单一 self-host runtime

**⚠ 退役顺序的硬不变量**（完备性生命线）：上方“byte gate 退役”**顺序至关重要**。只有**上位独立 oracle（interp 规范化 + 语义法则 property test + wasm-spec 忠实性证明）先替代之后**，才可移除差分 oracle（与旧 emit 差分 / native↔wasm 一致）。否则退役瞬间暴露 §2.3 盲点 — unify 后两目标因 Core→MIR 决策 bug **同样出错**，检测层永久漏掉。**“要删差分，先建立更高层独立 oracle。”** 详见 [v1-phase1-mir-core.md §5.3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md)。

→ **代码净减少。** v1 不是“增加功能”，而是“折叠债务，将正确性转化为构造”。

---

<a id="7-phase-計画"></a>

## 7. Phase 计划

- **Phase 0 — spike（1–2 周，§8）**：在少数所有权棘手形状上实证“单一 MIR → 两 renderer 由构造一致”，通过决策门槛（RC 和 borrow 能否共用一个规范形式）。
- **Phase 1 — MIR core + 两 renderer（主体）** → 实现设计见 [v1-phase1-mir-core.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md)（精读 6 组件后已绘图，5 pass + emit_typed_rc_dec→1 pass 对照表、canonical cut、去风险 5 阶段、接地风险台账 R1–R12）。要点：扩展至完整语言；从 MIR 描绘 wasm（把 emit_wasm 的 layout/runtime 知识用于 dumb renderer）；用现有 byte gate + corpus 作迁移 oracle，确认旧行为一致，同时删除旧 emit 所有权和双重所有权 pass；然后 Rust renderer 改为消费 MIR。**关键实测：高层 IR 已是“半 MIR”**（RcInc/RcDec 已是 IR 节点，LayoutRegistry 已是零 magic number 的 Repr 表）。
- **Phase 2 — MIR 形式化 + interp 规范化**：小步语义，interp = 参考（#563/#564）。在 Lean 中针对 MIR 证明 Perceus 健全性。
- **Phase 3 — runtime self-host**：用 Almide 编写 runtime，经 MIR 到全部目标。删除 rt-oracle/churn-as-drift。
- **Phase 4 — flight-grade 交付物**：Critical subset（#567）、静态内存（#568）、WCET（#569）作为 MIR 属性；逐构建翻译验证证书（#570）；KCG 包（#574）。

**规模（诚实估计）**：spike + Phase 1 占第一个季度（验证这场赌注），v1 完整形态需要**年级时间**。期间 feature/issue 工作几乎冻结。最大成本是**验证尾部**（每次所有权变化都需让全部 corpus 重新全绿）。

---

<a id="8-phase-0-スパイク仕様まずここ"></a>

## 8. Phase 0 spike 规格（先做这里）

**目的**：在所有权最棘手的形状中，推翻或证明 §2.2 核心论点 — “以 Perceus 为规范，从单一所有权决定使两目标由构造一致”。

**五种目标形状**（当前跨目标分歧/可能分歧）：

1. **alias 返回** — `fn first(o: Option[String]) = match o { some(s) => s, none => "" }`（返回值借用参数内部）
2. **`list.get(xs, i) ?? d`**（#643 Some-box leak 核心）
3. **boxed variant 的 nested pattern** — `match t { Node(Leaf(a), Leaf(b)) => a + b, ... }`（#610）
4. **closure capture** — 可变 capture + 返回闭包
5. **可变别名 + 修改（AliasCow）** — `var b = a; mutate(b)` 中 `a` 不变

**最小 MIR（spike 范围）**：`Repr ∈ {Scalar(w), Ptr(layout), Boxed(layout)}`，节点 `Alloc/Store/Load/Dup/Drop/Borrow/Consume/Call`，函数边界所有权（每参数 own/borrow）。

**单一 pass**：一条 Core→MIR 基于 Perceus 的所有权解析（以现有 `pass_perceus` 为起点，推广到 Rust 决策也由同一事实导出）。

**两 renderer**：薄层 MIR→Rust（§2.2 右列）、MIR→wasm（§2.2 中列）。断言**不重新决定所有权**。

**忠实性检查**：在两目标构建并执行五种形状，检查 (a) 观察一致，(b) #643 消失，(c) renderer 原样输出 MIR dup/drop 数量（不重新决定）。

**决策门槛（通过/失败标准）：**
- **通过** → RC 与 borrow 干净地落在同一规范形式（Perceus）。进入 Phase 1。
- **失败（例如五形中某个 Rust renderer 无法翻译为惯用 move/borrow，不得不输出显式 Rc，破坏“可读 Rust（#572）”）** → 明确记录事实，再决定 (i) 修改规范形式，(ii) Rust 允许显式 Rc、牺牲可读性，或 (iii) Rust renderer 专用于资格鉴定，并重设计。**可以低成本撤退。**

<a id="81-決定ゲート結果--合格2026-06-13-55"></a>

### 8.1 决策门槛结果 — **通过（2026-06-13，5/5）**

五形均以“单一 MIR 决定 → (A) 惯用 Rust 与 (B) 手写 RC（wasm 语义）两种渲染 → `rustc --edition 2021 -O` 构建、运行、比较”验证。**所有形状 A=B 由构造一致，只有重新决定所有权的 buggy variant 被捕获。conditional / fail / escape hatch 为 0。** 证据与重跑位于 `research/spike/v1-mir/`（各形为自包含 meta-harness，`./run-gate.sh` 整体复验），完整记录在 `research/spike/v1-mir/GATE.md`。

| # | 形状 | 一个决定 | A=B | RC clean | 捕获 buggy | hatch | verdict |
|---|---|---|---|---|---|---|---|
| 1 | alias_return | 最后 consume = payload ptr 转移，**只释放 shell** | ✓ | ✓ | double-free | none | **PASS** |
| 2 | list_get_643 | alias-inc + scope-dec，循环 heap temp 每迭代 drop | ✓ | ✓ | leak/divergence | none | **PASS** |
| 3 | boxed_pattern_610 | boxed field **跨 box 借用**，Leaf payload 为 Scalar/Copy | ✓ | ✓ | double-free | none | **PASS** |
| 4 | closure_capture | capture = Dup 进 env + closure-drop 时 Drop，每次 call 借用 | ✓ | ✓ | rustc reject / double-free | none | **PASS** |
| 5 | alias_cow | 修改可能共享的 ref 前先 **MakeUnique** | ✓ | ✓ | 值损坏（两种 idiom 相同） | none | **PASS** |

门槛不只报告 PASS，还揭示规范形式四个**边界**（已反映于 §2.2/§3）：

1. **所有权极性是 per-binding/per-parameter MIR fact**（alias_return）。同样源码 consume→move、borrow→clone/dup。不是重新决定，而是每绑定/参数决定一次（§2.2）。
2. **可吸收 SYNTACTIC 差异，不可做 SEMANTIC 决定**（boxed_pattern_610）。无 box-pattern 是语法差异 → tag-guard + `&**deref`；不重新决定所有权/布局（§3）。
3. **buggy 的观察特征因形状而异**（alias_cow vs 643）。#643=RC double-free；AliasCow=wrong-output（RC 平衡但值损坏）。→ Phase 1 不仅检测 leak，也需要**值等价**（§3、§6 迁移 oracle）。
4. **shared + mutated-across-calls + returned capture 是 Rc<RefCell> 区域边界**（closure_capture）。read-only capture 可零 Rc 渲染，但此边缘需在 Phase 1 作为 MIR 共享可变性处理（§2.2 已知边缘）。

---

<a id="9-却下した代替案"></a>

## 9. 否决的替代方案

- **持续加强检测（现状+）**：原则上达不到 by-construction，检测有洞（#643 heisenbug）。与熵作战。
- **Rust 为唯一真相，wasm 成为 rustc 目标**：建在没有形式规格的 Rust 语义之上，无法作为 DO-333/Lean 基础，也失去主权。（首次讨论提出，但违背形式验证北极星而否决。）
- **wasm 为唯一真相，完全抛弃 Rust**：失去 Ferrocene 资格鉴定捷径和 native 性能/成熟度。→ 改为“MIR 是真相，Rust 是踏板 renderer”（§1）。
- ~~**字面上从零重写**：丢失 270 fixture 知识、second-system risk~~ → **此否决于 2026-06-13 推翻（§6）**。知识结晶于**可执行外部规格**（fixture/contract/interp/语义法则 oracle），而非实现代码。保留它，就不会发生 second-system risk 的核心（隐性知识丢失）。v1 采用**完全 greenfield + 双 oracle（旧实现保留到 parity）**。**但“先删旧，再仅靠 fixture 验证的盲目 rewrite”仍被否决** — parity 前会丢失差分独立性（§2.3）。
- **blind greenfield（立刻删旧，仅用 fixture acceptance）**：fixture 不完整（R1），丢失旧实现这个第二实现的差分独立性，Core→MIR 决策 bug 成为盲点。→ 改为“保留旧到 parity 的双 oracle”（§6）。

---

<a id="10-v1-言語表層の決定--mission-first2026-06-13-確定"></a>

## 10. v1 语言表面决定 — mission-first（2026-06-13 确定）

通过 greenfield 演进表面，但**判断标准是 Almide 唯一指标 = LLM 能最准确编写（MSR）**。“参考 Swift”确定为**采用 Swift 的清晰，不采用吞噬 MSR 的复杂性**。

| 功能 | v1 决定 | 理由 | 依据 |
|---|---|---|---|
| **range** | Rust `0..5`/`0..=5` → **Swift `0..<5`/`0...5`** | `<` 在写法上表明“小于”，消除 off-by-one 歧义 = 直接利好 MSR。增加 `...`/`..<` token（小） | — |
| **where（generic 约束）** | **采用** `type Set[T] where T: Hash + Eq` / `fn max[T](..) where T: Ord` | 约束**改善**错误（MSR+）。与 test-where 同一关键字：“body 有效的前提” | [type-where-constraints.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/type-where-constraints.md) |
| **protocol（内部模型）** | **可采用 Swift 风格干净 witness 模型**（编译器内部） | 整理内部表示不增加表面复杂性 | — |
| **protocol（用户表面）** | **维持现状**：声明式 nominal conformance + `any P`（Go 接口值易用性） | 沿用 `protocol-any-existentials.md` 的有意设计 | [protocol-any-existentials.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/protocol-any-existentials.md) |
| **associated types / conditional / retroactive conformance** | **仅保留语法，v1 core 延期** | 同路线图以 MSR 明确否决（“人和 LLM 都读不懂错误”）。**greenfield 和解路线**：把“从一开始就做好可读错误的 AT”保留为未来重新加权 mission 时的选项 | 同上“否决”节 |
| **HKT（高阶 kind 的 where）** | **仅保留语法，v1 core 延期** | 同路线图以“complexity eats MSR”否决。**Swift 也无 HKT**（无法参考，属 Scala/Haskell 领域）。三功能中复杂度最大、MSR 最少 | 同上 |
| **`if let`**（Optional 绑定） | **采用** `if let x = opt { … }`（在 some-arm 绑定 unwrap 值） | 常见“unwrap 后使用”比 match 更清楚。Swift/Rust 先验使 LLM 准确编写。**frontend 脱糖** = `match opt { some(x) => …, none => … }` | — |
| **`guard let … else`**（提前退出绑定） | **采用** `guard let x = opt else { return … }`（else 必须 diverge：return/throw/exit） | 保持 golden path 平坦（减少嵌套=减少 LLM 错误=MSR+）。采用 Swift 可读性。**frontend 脱糖** = checker 强制 else-must-diverge 的 match。else 不退出的路径报 E-诊断 | — |

**一句话决定**：range Swift 化 / 采用 where 约束 / **采用 `if let`、`guard let … else`（Optional 绑定、frontend 脱糖、guard else 强制 diverge）** / protocol 仅内部 Swift 风格，表面保持声明式 nominal + `any P` / **associated types、HKT 保留语法，v1 core 延期**。贯彻 MSR 优先。

`if let`/`guard let` 是 **frontend 脱糖**（MIR 只看落为 match 之后），因此没有新增所有权/布局决定，不影响 §3 renderer 契约。实现放在 v1 frontend 构建时（或现 frontend 临时扩展）。guard 的 **else-must-diverge** 由类型/控制流检查强制（与 Swift 相同纪律，可用 `Never` 表示）。

保留的日文架构图释义：.almd 进入带类型、脱糖、目标中立的 Core IR（整理当时高层 IR），再进入显式表达 Perceus 所有权和布局、具有小步语义且为唯一真相的 MIR。图将 wasm 分支描述为主路径、已验证、具有主权，因 MIR 与 wasm 对齐而几乎自然下降；Rust 是性能及 Ferrocene 资格鉴定的踏板，仅将 MIR 描绘为 Rust idiom。runtime 用 Almide 自身编写，走同一 Core→MIR→target 路径（self-host）。这些是历史图的架构主张，不是此处独立验证。
