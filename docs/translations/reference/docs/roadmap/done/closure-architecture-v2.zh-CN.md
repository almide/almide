说明：闭包架构 v2——一个身份、一个捕获集，提升即lowering；将闭包表示与内联优化分离
历史完成元数据：2026-06-02
<a id="closure-architecture-v2"></a>

# 闭包架构 v2

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](closure-architecture-v2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](closure-architecture-v2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](closure-architecture-v2.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)

> 原文注意事项：这是已完成的历史设计与实现日志，不是当前编译器审计，也不是恢复 branch 的指令。早期要求和后续订正并存：local lambda 的 SCC 递归处理变为不需要；auto-? 失败被诊断为 frontend 调用解析，而非 Perceus；尽管早期目标是退役，最终 clone predicate 仍保留；尽管早期理想是全部 lift，mutable capture 仍保留 raw Lambda。P0–P6 完成不等于解决另列的类型／解析／存储缺口。原文关于闭合 Perceus 到 binary 证明链及证明实际发出代码的说法保留为历史主张：模型的 kernel 检查 RC 定理、人工实现对应、有限跨目标测试与任意可执行物正确性各不相同。runtime allowlist 注释仍为所述信任边界，并保留后续 user-HOF 推断的限定。测试总数和日期未独立重跑或更新。

> **已完成的设计记录。** P0–P6 已完成；最终实现分支于 2026-06-02 通过 [#332](https://github.com/almide/almide/pull/332) 合并。于 2026-10-31 移至 `done/`。下文的问题描述、阶段表、命令、测试数量和证明主张记录的是当时的实现，而非当前编译器行为，也不是待恢复的工作。

> **一个身份、一个捕获集，提升即lowering。** 闭包是一个规范值，具有程序范围唯一的身份，以及只计算一次的显式捕获集；*内联*是一项独立的、经逃逸证明的重写——绝不是表示层面的捷径。这对应于闭包领域的 `Verified` / `Canonical` 输出门禁：通过构造保证正确，而不是靠猜测。

<a id="original-problem-historical"></a>

## 最初的问题（历史记录）

设计时，同一个源代码 lambda `(x) => f(x)` 会根据使用点语法、捕获分析、可变性和目标，以多达 **五种**不同方式表示：

1. 内联的迭代器适配器回调（`IterStep`/`IterCollector`，Rust）；
2. 残留的原始 `IrExprKind::Lambda` 值；
3. 提升后的 `__closure_N` 函数 + `ClosureCreate` + `EnvLoad` 三件套（WASM，不可变捕获）；
4. 使用堆单元的可变捕获原始 `Lambda`（WASM）；
5. eta 展开的包装器 / `FnRef`（命名函数）。

捕获**不**存储在 `Lambda` 节点上；它们由**三个**独立的自由变量分析重新推导（闭包转换中的 `FreeVarCollector`、`emit_wasm` 中的 `ClosureScanner`，以及 Rust 隐式的 `move`），供给**两个**近乎重复的 WASM 环境构建器（`emit_lambda_closure` 与 `emit_closure_create`）。`Ty::Fn { params, ret }` 是*唯一*的可调用类型——不携带闭包／捕获／调用约定标签，所以所有操作层面的差别，都隐含在所选的*节点种类*和*流水线分支*中。

概念完整性的缺陷：**不存在规范的“函数值”。** 表示方式是从 lambda 的形状*猜出来*的（`free.is_empty()` ⟹“它会在 HOF 调用点被内联”），而该属性实际上并不在这个 pass 的可见范围内。

<a id="the-verified-bug-this-produces"></a>

### 由此产生的已验证错误

`ClosureConversionPass` 将无捕获 lambda 保留为原始形式（`pass_closure_conversion.rs` 的 `free.is_empty()` 分支）。输出时通过 `lambda_id` 将原始 lambda 与预扫描的 `LambdaInfo` 关联——但 `lambda_id` **在每个模块重置为 0**（`LowerCtx::new`），而模块 lambda **从未注册**（WASM 预扫描只遍历 `program.functions`）。结果如下，已在 0.23.14 上用一个 8 行、双模块程序复现：

```almide
// src/lib.almd
pub fn neg() -> (Int) -> Int = (n) => 0 - n
// main.almd
import self.lib
fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)
fn add() -> (Int) -> Int = (n) => n + 1000
effect fn main() -> Unit = {
  println(int.to_string(apply(add(), 5)))     // 1005  (correct on both)
  println(int.to_string(apply(lib.neg(), 5))) // want -5; WASM printed 1005
}
```

`lib.neg()(5)` 在 WASM 上返回 `1005`（main 中的 `add` 函数体）——静默的错误结果。主程序中没有 lambda 的变体会输出无效 WASM（`unknown table 0: table index out of bounds`）。原生目标结果正确，因为 Rust 目标使用原生闭包，从不经过 WASM 的 ID 匹配路径。

<a id="the-principle-separate-three-conflated-concerns"></a>

## 原则：分离三个被混为一谈的关注点

| 关注点 | 当前（有缺陷） | v2 |
|---|---|---|
| **身份**——是*哪个*闭包 | 每模块的 `lambda_id`，跨模块冲突 | 程序范围唯一的 `ClosureId` |
| **捕获集**——捕获了*什么* | 由 3 个分析重新推导，可能不一致 | 一个共享分析，附在节点上，包含显式 `mode` + 精确 `Ty` |
| **表示**——*如何*布局和调用 | 根据捕获是否为空猜测 → 5 种形状 | `Lambda` 值节点在共享流水线中始终是规范形式；*提升*属于目标lowering；*内联*则是证明无需提升 |

思路反转：**当前从 lambda 的形状猜测表示；v2 从使用方式证明表示**（逃逸分析在每个调用点作决定），而身份和捕获只计算一次，供所有目标共享。

<a id="the-boundary-shared-semantic-core-vs-target-lowering"></a>

## 边界：共享语义核心与目标lowering

这次重设计最重要的单一决定（统一的*已提升* `Closure{code,env}` 节点抽象层级不对——它是 WASM lowering形式；强行让它经过 Rust，会迫使反向重建原生闭包，妨碍 `rustc`，而 WGSL 根本无法表示它）：

**共享部分（位于 `almide-ir`，在流水线分叉前对函数*和*模块全体计算一次）：**

- `ClosureId(u32)`——程序范围唯一的身份，替代 `lambda_id: Option<u32>`。
- 附在节点上的捕获集：
  `Lambda { params, body, id: ClosureId, captures: Vec<Capture>, escape: EscapeVerdict }`，
  `Capture { var, ty: Ty /* Almide owned type */, mode: ByVal | ByMutCell }`。
  无捕获 ⟹ `captures: []`。不存在 `free.is_empty()` 旁路。
- 以 `ClosureId` 为键、覆盖整个程序的 `ClosureTable`（函数 + 模块）。
- 一个共享的自由变量 + 逃逸分析填充以上全部内容。

`Lambda`（携带 id + captures + escape）**在整个共享流水线中保持为规范闭包值形式。**不存在共享的已提升 `Closure{code,env}` 节点。

**目标lowering（各自读取共享捕获集 + 逃逸判定）：**

- **Rust**——从不提升。输出原生 `move |params| body`，由 `captures` 确定性地驱动 `CaptureClonePass`（删除 `needs_clone_type` 白名单）。`ParamBorrow` 物化（`to_vec`/`to_owned`）是 Rust 表示的派生。逃逸所需的存储形式 → `Rc<dyn Fn>`。让 `rustc` 优化。
- **WASM**——仅用于 WASM 的 `ClosureLowering` pass **只提升经过 `InlineClosures` 后仍保留的闭包**（会逃逸的闭包），变为 `__closure_N` + `Ty::EnvPtr` 环境 + `EnvLoad`，通过 `ClosureTable` **按名称**注册到函数表（与顺序无关，模块安全）。一个环境构建器；一个 `valtype_and_load(ty, mode)` 辅助函数，集中处理 ValType 选择、可变单元作为 i32，以及窄整数符号扩展。`call_indirect` 类型索引经由 `ClosureId` 取自已提升函数注册的 `closure_type_idx`——**单一事实来源**。
- **WGSL**——*无闭包*目标。在 `@gpu` 函数体内无条件运行 `InlineClosures`；任何残余逃逸闭包 → 硬性编译错误。（这从另一侧验证了该主张：WGSL 中的“内联或报错”是强制要求，而不是可选方案。）

优雅之处在于：**共享部分 = 身份 + 捕获集 + 逃逸判定；提升、符号扩展、`to_vec`/`to_owned`、`Rc<dyn Fn>`、`call_indirect` 以及“内联或报错”，都是从这一份共享事实按目标派生的结果。**两个后端都无需扭曲自身。

<a id="inlineclosures-prove-the-use"></a>

## InlineClosures：“证明使用方式”

一个在提升**之前**运行的识别 pass。只有**双边**证明均成立时，闭包才可内联：

- **(a) 使用点**：闭包是*匿名的*（不是任何 `Bind` 的右侧），*仅使用一次*，且不逃出其作用域；**并且**
- **(b) 接收方**：被调用者位于精心维护的*仅消费*组合子白名单中，这些组合子在 stdlib `.almd` 源码中带有 `@fn_arg_consumed(idx)`（stdlib/defs TOML 流水线已移除），并且已对照运行时实现验证一次——因此闭包不能*通过*调用逃逸（否则，某个 `fold` 的 reducer 若将函数捕获进累加器，就会让它逃逸）。

两者都满足时，将 `Call(combinator, [.., literal-closure])` 重写为 `InlinedHOF` 节点（WASM：拼接函数体，无分配 / 无 `call_indirect`；Rust：`IterChain` 适配器）。Egg 融合组合 `InlinedHOF`。**绑定过的闭包绝不内联** ⟹“只计算一次”在字面上成立。WASM 输出器的快速路径依据 `InlinedHOF`，而不是原始 `Lambda` 的形状。

<a id="the-hard-correctness-pieces-designed-in-not-glossed"></a>

## 正确性的难点（纳入设计，而非含糊带过）

- **递归。** 自递归／相互递归的*局部*闭包不能将自己捕获到自己的环境中（循环／尚未分配的值）。预处理 pass 对 `let` 绑定的 lambda 组计算 SCC；将自身／同组引用重写为 `ClosureId` 代码符号，并将提升后函数自身的 `__env` 参数穿入递归调用（标准的“递归闭包传递自身环境”方式）。后置条件：没有 `env` 捕获正在构造的闭包。
- **可变捕获是*生命周期*错误，而非表示错误。** `ByMutCell` 必要但单独使用并不充分（当前代码在捕获的局部变量不在 `var_map` 时，会静默地向环境存入 `0`）。修复：在变量*绑定*处放置 `CellAlloc` 节点；将该变量的**每一次**读写——包括外层函数，而不只是提升后的函数体——重定向到 `CellRead`/`CellWrite`；单元（`Ty::Cell(T)`，一个单槽位堆记录）受 RC 跟踪，使 Perceus 在闭包逃逸后仍保持其存活。删除存零回退 → 改为后置条件错误。
- **穿过环境的 Perceus RC。** 当前仅当 `ClosureCreate` 是 `VDecl` 的值时才触发捕获 inc——因此，作为调用参数或尾部创建的闭包（恰好是已验证错误的形状）**没有** inc → 少计数。修复：在*每一种*语法位置（声明值、调用参数、尾部、列表元素），都将 inc/dec 绑定到 **Closure 节点**；在值物化处对每个堆环境捕获 inc，在 Closure 自身 drop 时逐个 dec。`Ty::EnvPtr` 属于堆，**且**是借用而非拥有；由 EnvLoad 绑定的捕获是结构性借用（无 inc/dec）。这给 L5 Lean 证明一个清晰的切入点——由于两端绑定到同一节点，每条路径上都有 `inc_count(cap) == dec_count(cap)`——**从而闭合“Perceus → 二进制”的证明链。**
- **类型身份，单一事实来源。** `Ty::EnvPtr` 是*真正的封闭标量变体*（不是 `Ty::String` 的别名——否则 Perceus 会对环境块执行 `RcDec`，破坏堆）；当前 `ty_to_valtype = I32`，为未来 GC 后端（`(ref $env)` + `call_ref`）按目标参数化。`call_indirect` 类型经由 `ClosureId` 取自 `closure_type_idx`，而不是在每个位置从 `callee.ty` 重新推导（结构性 `register_type` 去重是潜在的第二处冲突——无主程序 lambda 的“table OOB”变体）。**保留一个 `Ty::Fn`**——将可调用类型分叉，会让问题扩散到统一化及每个 `Type[Fn]` 容器；约定保存在节点上，通过“转换后每个可调用值都是 Closure；各位置始终输出闭包 ABI，绝不探查 `.ty`”来强制执行。

<a id="phasing-every-boundary-perf-neutral-and-invariant-satisfiable"></a>

## 分阶段（每个边界都保持性能中立且不变量可满足）

红队规则：**全覆盖必须与其依赖方原子性落地。** 当 egg 融合（在转换前运行，重新发出原始 lambda）和 WASM 内联快速路径（`is_inline_lambda` 匹配原始 `Lambda`）仍需要原始 lambda 时，“转换后零原始 Lambda”是*不可满足*的。绝不能把这些拆到不同版本发布。

| 阶段 | 内容 | 性质 |
|---|---|---|
| **P0** | 程序范围唯一的闭包身份（`GlobalizeClosureIdsPass`）+ 在 WASM 扫描中注册模块 lambda | **修复已验证错误，性能中立**（快速路径不变）。将 8 行复现作为 native==wasm 回归测试 |
| **P1** | 共享核心：`ClosureId` + 覆盖函数与模块的一个自由变量／逃逸分析；将 `captures`/`escape` 附在节点上；`ClosureTable` | 表示不变；Rust/WASM *读取*共享集合；断言旧分析一致 |
| **P2** | `InlineClosures`（`InlinedHOF`）+ **原子性全覆盖**：让 egg 使用带身份的闭包；重写 3 条 WASM 快速路径，以 `InlinedHOF` 为依据；对所有残留闭包进行 WASM `ClosureLowering`；删除 `lambda_id`/`param_id`/计数器解析器及一个环境构建器；`Ty::EnvPtr`；经 `emit_load_at` 的 `EnvLoad`；经 `ClosureId` 的 `closure_type_idx` | “零原始 Lambda”终于可满足；`unreachable` 变为后置条件错误 |
| **P3** | `ByMutCell`（`CellAlloc`/`CellRead`/`CellWrite`）+ SCC 递归 + 穿过环境的 Perceus RC | 三个正确性阻碍，由 `PerceusVerify` 验证 |
| **P4** | Rust 完整读取共享捕获集；退役 `needs_clone_type` + 第三个（隐式 move）分析 | 一个分析，两个目标 |
| **P5** | Lean：证明检查环境上的 `inc == dec` | 证明链闭合 |

<a id="p3-ground-truth-measured-on-develop-post-p2ba"></a>

## P3 的实际情况（在 P2b/A 之后的 develop 上测量）

针对实际语言，对红队的三个“阻碍”进行行为探测后，**修正了问题框架**——只有一个是现实存在的错误：

- **递归并不构成问题。** 自引用的*局部* lambda（`let fact = (n) => ... fact(n-1)`）被检查器拒绝（`E002 undefined function 'fact'`）：递归使用顶层 `fn`（命名 `FnRef`，不是闭包值）。没有需要修复的内容；无需 SCC pass。
- **不可变捕获已经正常工作**，包括堆捕获。`let prefix = "hi-"; (n) => prefix + n` 在**两个**目标上都返回正确结果，且 RC 正确——“穿过环境的 RC”这一担忧，在不可变情形下实际上已经得到处理。
- **可变捕获是唯一真正的错误**，而且很严重：
  - **Rust 目标——静默错误结果。** `var total = 0; let add = (x) => { total = total + x }; add(5); add(10); total` 返回 **`0`**（应为 `15`）。输出器生成 `let add = move |x| { total = total + x }`：`move` 闭包捕获 `total` 的一个**副本**（它是 `Copy`），所以外层 `total` 从不改变。（*静默*错误答案——对于强调书写准确性的语言，这是最恶劣的一类。）*会逃逸*的可变闭包（`make_counter`）则完全无法编译（`rustc E0525`，将 FnMut 当作 Fn）。
  - **WASM 目标——逃逸触发陷阱。** 堆单元机制对*不逃逸*的可变捕获是正确的（返回 `15`），但**逃出**其帧的可变闭包（`make_counter`）会**在运行时触发陷阱**。

<a id="the-fix-feature-sized-not-a-patch"></a>

### 修复（功能规模，不是小补丁）

被修改且被捕获的变量必须是**共享可变状态**，而不是移动／克隆的副本——这与 WASM 单元机制已经实现的结论相同，现在将其应用于 Rust，并修正逃逸：

- **Rust**：将被捕获且被修改的 `Mutability::Var` 变量lowering为 `Rc<Cell<T>>`（`Copy`）/ `Rc<RefCell<T>>`（`!Copy`）：声明 `Rc::new(Cell::new(init))`，每次读取 `→ .get()`，每次写入 `→ .set(v)`，闭包捕获 `Rc::clone`（共享单元）。需要将检测集（`shared_mut_vars`）传入 walker（Bind/Var/Assign）和 `CaptureClonePass`（克隆 `Rc`，而不是值）。
- **WASM**：保留堆单元，但修复逃逸——即使在闭包构建位置，单元变量不是直接局部变量，也必须将被捕获单元的指针存入环境（`var_map` 未命中时存入 `0` 的情况）。

这才是“ByMutCell”的真正内容——贯穿两个后端的新共享单元表示，而不是一行改动。下方阶段行中的 SCC 递归和广泛的穿环境 RC 工作*并不需要*（递归不是问题，不可变 RC 已经正常工作）。

**架构复杂性（实施中发现）。** Rust 修复不仅是 walker 代码生成——它需要调整流水线顺序。捕获分类（哪些变量被修改且被捕获 → `shared_mut`）目前位于 walker 的注解阶段，即*最后*阶段。但（早得多的）`CaptureClonePass` 才是将捕获 lambda 包装成 `{ let __cap = v.clone(); move |..| ..__cap.. }` 的地方，而且它会*跳过* `Copy` 捕获（`Int` 在 `move` 前无需预克隆）。一旦 `Int` 计数器变为 `Rc<Cell<i64>>`，它就**不再是 `Copy`**，所以现在*需要*这层克隆包装——否则 `move` 会消费唯一的 `Rc`，闭包之后便无法再使用变量。因此 `CaptureClonePass` 必须包装 `shared_mut` 捕获，但它在识别这些捕获的分类*之前*运行——形成循环依赖。干净的解决方案是**尽早（在 `CaptureClonePass` 之前）只计算一次“被修改且被捕获”集合**，让 `CaptureClonePass`（包装并 `Rc::clone` 单元，同时将 `__cap` 标记为 `shared_mut`）和 walker（Bind → `Rc::new(Cell::new())`，读 → `.get()`，写 → `.set()`，拥有上下文 → `.get()`）共同消费它。这种单一共享分析才是正确结构（对应 P1 的“一个分析，两个消费者”）；这也说明它是一项需要集中处理的功能，而不是会话收尾时的小补丁——只贯通部分路径的实现能编译，却会在某些上下文中静默误行为，恰好重现正在修复的故障模式。

基础工作已落在 `closure-v2-p3` 分支（未合并）：`shared_mut_vars` 注解 + 辅助函数，以及捕获分类器中的 Copy／非 Copy 划分。剩余 walker 代码生成 + `CaptureClonePass` 顺序调整将完成它。

<a id="honest-limits"></a>

## 如实说明限制

- **P0 是定点修复，不是重设计。** 它使 `lambda_id` 在程序范围唯一，并注册模块 lambda；`free.is_empty()` 表示旁路及并行的原始 Lambda 输出路径直到 P2 才会消失。它不改善可变捕获生命周期（P3），也不消除重复分析（P1/P4）。
- **逃逸白名单（`@fn_arg_consumed`）由人工维护，并对照每个运行时实现验证一次**——这是维护义务，不是推导出的属性。错误注解可能将逃逸闭包内联；应将其视为受信任基础。*对于用户高阶函数，已由 #2288 取代*：函数类型参数的逃逸根据其出现位置，在决定 `&T` 的同一不动点中推导（`fn_param_borrow`，docs/specs/codegen.md“函数类型参数默认借用，除非其可调用值逃逸”）；不逃逸槽位是 `&dyn Fn`，证书检查器的 C5 检查该判定；白名单现在只描述运行时对应实现自身的槽位（#2289 将这些也退役至消融用途）。
- **GC 迁移是局部的，但不是零成本**：`Ty::EnvPtr` 和 Closure 布局按目标参数化，因此 `gc` 后端只替换 `emit_wasm` + `ty_to_valtype`——但 `call_ref`/typed-funcref 带来其自身需要设计的类型身份模型。
- **L3 确定性交互**：`GlobalizeClosureIdsPass` 在 `CanonicalizePass` 之前运行，只修改 `lambda_id`（不改函数顺序），因此不影响 `Canonical` 证书；闭包表槽位按扫描顺序分配，与这些 ID 无关，所以输出字节保持跨宿主确定性。

<a id="implementation-progress-log-historical"></a>

## 实现进度日志（历史记录）

<a id="done--in-flight"></a>

### 已完成／进行中

| 阶段 | 内容 | 状态 |
|---|---|---|
| P0 | 程序范围唯一的闭包 ID + 注册模块 lambda | **已合并**（`#329`） |
| P1 | `almide-ir::free_vars` 中一个共享自由变量分析 | **已合并**（`#330`） |
| P2b/A | 值 lambda → `ClosureCreate`；只有 `map/filter/fold` 内联参数保留原始形式；有捕获的组合子 lambda 现在也内联 | **已合并**（`#331`） |
| P3（Rust） | 被修改且被捕获的 Copy 局部变量 → 共享 `Rc<Cell<T>>`（修复本应 `15` 却静默返回 `0` 的问题） | **PR `#332`**，分支 `closure-v2-p3`，已验证（cargo test + spec 240/240） |
| P3（WASM） | 按名称调用 auto-`?` 闭包绑定 → `lower_call_target` 错误解析（在 wasm 上触发陷阱） | **已修复**，分支 `closure-v2-p3`，已验证（spec 240/240 + cargo test + 2 项回归） |
| P4 | Rust 的两个私有自由变量扫描（`pass_capture_clone`、walker 的 `CaptureCollector`）→ 共享 `almide_ir::free_vars` | **已完成**（`7bdc31f3`），分支 `closure-v2-p3`，已验证（spec 240/240 + cargo test）；−218 LOC |
| P5 | Lean——证明检查环境 RC 契约（捕获上的 `inc == dec`），将闭包纳入无泄漏证明 | **已完成**（`f6c55727`），`AlmidePerceusBelt/ClosureRc.lean`，`lake build` 通过，无 `sorry` |
| P6 | 非 Copy 可变捕获 → 共享单元（`SharedMut`/堆单元），使闭包的修改传播——此前两个目标都静默出错 | **已完成**（`f680b807`），分支 `closure-v2-p3`，已验证（spec 240/240 + cargo test + 2 项跨目标回归） |

P3 位于分支 `closure-v2-p3`（Rust：`22a7d87a` 基础工作 + `232a4fac` 修复；WASM：下文的调用目标修复）。实施使用从 `origin/develop`（或 `origin/closure-v2-p3`）新建的 worktree，以避开主检出目录中无关的未提交更改。这是历史工作流说明，不是恢复执行的指令。

<a id="fixed--p3-wasm-auto--closure-binding-called-by-name-was-drops-the-block"></a>

### 已修复——P3（WASM）：按名称调用 auto-`?` 闭包绑定（原称：“释放了块”）

返回闭包的 `effect fn`，当绑定使用 auto-`?` 时，会在 WASM 上触发陷阱：

```almide
effect fn make_adder_e(n: Int) -> (Int) -> Int = (x) => x + n
effect fn main() -> Unit = {
  let add5 = make_adder_e(5)        // auto-`?` → TRAPPED on wasm
  // let add5 = make_adder_e(5)!    // explicit `!` → always WORKED (15)
  println(int.to_string(add5(10)))
}
```

**真正根因（前端，*不是* Perceus 或 `Try` 输出——本文先前的假设错误）。** `lower_call_target`（`crates/almide-frontend/src/lower/calls_target.rs`）通过读取 `add5` 的 **`var_table` 快照类型**并检查 `matches!(.., Ty::Fn)`，决定 `add5(10)` 使用 `Named` 还是 `Computed`。在 effect fn 中，effect-`?` 解包重写（`auto_try`）在lowering*之后*运行，所以在调用位置，`add5` 的存储类型仍滞后为 `Result[(Int)->Int, String]`——不是 `Ty::Fn`。检查失败，调用落入 `Named { add5 }`，而该函数不存在。

这一个错误解析产生了先前被误归因的*两个*症状：
- **WASM 输出 `unreachable`**：`Named { add5 }` 调用没有目标 → 未解析调用桩。
- **Perceus 过早 `rc_dec`**：`Named` 调用不是变量引用，所以 `add5` 看起来在绑定后未使用；Perceus *正确地*释放了它。Perceus 从未出错——它收到的是一个没有提及该变量的调用。（通过比较 `?` 与 `!` 形式逐 pass 的 IR dump 确认：经过 `StackBalance` 时一致；区别只在 `lower_call_target` 输出——`Named{add5}` 与 `Computed{var2}`——而且从最初的 dump 就存在，即源于前端。）

**修复。** `lower_call_target` 现在根据**被调用者在使用点的类型**判断可调用性（`ctx.expr_ty(callee)`——检查器已经用 auto-`?` 将其解包为函数类型），并以存储类型及剥除 `Result` 后的形式作回退。解析为绑定的局部名称始终*通过变量*调用（`Computed`）；输出阶段随后分派闭包，Perceus 看见使用并在其后释放。无需修改 Perceus、`Try` 输出或任何代码生成 pass。`var_table` 类型仍滞后，但使用点类型是权威来源，绕开了这个问题——值类型的 auto-`?` 绑定此前已正常工作，也是同样原因（其输出读取由 `auto_try` 修正的 `Bind.ty`）。

已验证：`?` 和 `!` 复现 → 在 wasm + 原生上均为 `15`；修复后的 `main` IR 与 `!` 形式一致（尾部提升的 `__perceus_ret`，使用后的 `rc_dec`）；边界情况（两个闭包、同一表达式多次调用、闭包传给 HOF）跨目标一致；spec 240/240；完整 `cargo test`；在 `tests/wasm_runtime_test_parts/p2.rs` 中新增 2 项回归（`wasm_effect_fn_returns_closure_auto_try_binding`、`_used_twice`）。

<a id="done--p4-one-free-var-analysis-for-both-targets"></a>

### 已完成——P4：两个目标共用一个自由变量分析

现在，每个消费者的捕获集都由同一个函数 `almide_ir::free_vars::free_vars` 计算。P4 之前，Rust 目标有**两个**与它存在细微偏差的私有重实现：
- `pass_capture_clone::collect_free_vars`（供给 `__cap` 克隆包装），以及
- walker 的 `CaptureCollector`（lambda 深度／局部变量栈遍历；供给 `RcCow` 存储分类）。

两者均已删除；两个调用点现在都使用 `free_vars`。共享分析严格更准确——它跟踪*所有*绑定器（块 `let`，包括解构、match 分支模式、for-in 变量、嵌套 lambda），并通过 `walk_expr` 对新 IR 节点默认安全；手写扫描则会静默遗漏其中一些。它返回按 VarId 排序的集合，所以 `__cap` 绑定现在按确定顺序输出。净减 −218 LOC。

仍然特定于目标的是对这一个集合的*投影*，这是正确的分工，不是重复：Rust 筛选值得克隆的捕获（`needs_clone_type` 跳过 `Copy`，`SHARED_MUT` 强制包括 `Rc<Cell>` 类型）并分类 `RcCow`；WASM 将每个捕获都存入环境。“退役 `needs_clone_type`”实际实现为退役给它供给数据的重复*扫描*——谓词本身是一行 Rust 克隆策略，而不是分析。已验证：spec 240/240、完整 `cargo test`，以及棘手场景下 native==wasm（可变 `Copy` 捕获 → 共享单元、一个变量被两个闭包捕获、嵌套闭包捕获外层的外层变量）。

<a id="done--p5-the-env-rc-contract-certified-in-lean"></a>

### 已完成——P5：经 Lean 证明检查的环境 RC 契约

`AlmidePerceusBelt/ClosureRc.lean`（经 `lake build` 内核验证，无 `sorry`/`axiom`）。编译器将闭包生命周期lowering为普通 `RcInc`/`RcDec` IR 节点（`PerceusPass` + `pass_perceus.rs` 中的规则 6）：创建时对每个被捕获堆变量 inc，并分配闭包对象；drop 时先 dec 对象，再 dec 每个捕获。`closureScope` 仅使用现有 `inc`/`dec`/`vdecl` 的 `FnBody` 节点，精确建模这种lowering——没有公理化内容——契约是*推导*出来的，而非假设：

- `closure_env_rc_balanced`——闭包对捕获集中的每个变量 inc 和 dec 相同次数（`inc == dec`）；对每个捕获引用计数的净贡献为 0。
- `closure_preserves_isFreed`——闭包保持每个借用捕获的释放收支不变（捕获值无泄漏、无双重释放）。
- `closure_obj_freed`——闭包对象自身被释放（drop 时的 `dec cv`）。
- `closureScope_allHeapFreed`——只要其后续计算满足 `allHeapFreed`，闭包也满足，从而将它纳入与普通堆变量相同的端到端无泄漏定理（`perceus_all_heap_freed`）。

建模*实际* inc/dec lowering（而非加入一个公理化的 Closure 构造器）是更强的结果：它证明编译器实际输出的代码在 RC 上正确。这闭合了闭包的 Perceus→二进制证明链——此前唯一未纳入证明的路径。

<a id="done--p6-non-copy-mutable-captures-the-silent-wrong-gap-closed"></a>

### 已完成——P6：非 Copy 可变捕获（静默错误缺口已封闭）

在针对完整性做红队验证时暴露：`var acc: List = []; let f = () => list.push(acc, 1);
f(); f(); list.len(acc)` 在两个目标上均返回 **0**——闭包的修改丢失了。P3 只覆盖 Copy 标量（`Rc<Cell>` / 堆单元）；非 Copy 捕获经过 Rust 的 `RcCow`（写时复制——共享写入会克隆，丢掉修改），而 WASM 按值捕获列表。修复将被捕获且被修改的非 Copy 变量变为*共享*单元（Rust 上 `SharedMut<T>` = `Rc<RefCell<T>>`；WASM 上使用现有堆单元）：

- **检测**依据*通过闭包进行的修改*，而不是 IR 的 `Mutability` 标志——只通过方法（`list.push`）修改的 `var` 被记录为 `Let`（从未重新赋值）。Rust 的 `CaptureClonePass` 和 WASM 的 `ClosureConversionPass` 都扫描对捕获变量的赋值 / `&mut` 借用 / 原地修改器（`list.push` 等）。
- **Rust**：`SharedMut` 仿照 `Cell` 的 `.get()/.set()`，所以读取／赋值的lowering保持不变；只有构造器（`SharedMut::new`）及原地修改（`borrow_mut()`）按类型分支；这些变量不再使用 RcCow。共享读取借用一个拥有所有权的快照（`&x.get()`），因此即使单元是块局部变量，在尾部位置也安全。
- **WASM**：被修改的捕获必须保留为*原始* lambda（堆单元路径），而不能提升为环境会丢失单元的 `ClosureCreate`；`shared_mut_vars`（在转换前、函数体仍使用捕获名称时计算）预先填充 `mutable_captures`，让单元贯穿环境，并让 `list.push` 的重新分配将新指针写回其中。

已验证 native==wasm：通过捕获修改（`2`）、嵌套闭包（`3`）、Copy 仍正常（`20`）、只读捕获不变（`40`/`104`）；spec 240/240；完整 `cargo test`；两项新增跨目标回归。Lean 的 P5 证明无需修改就已覆盖这些单元：共享单元是普通的被捕获堆对象（捕获时 `Rc::clone`，drop 时 `dec`），所以 `closure_env_rc_balanced` 证明其 `inc == dec`——内容修改与 RC 正交。

<a id="status-closure-architecture-v2-complete"></a>

### 状态：闭包架构 v2 已完成

P0–P6 全部落在分支 `closure-v2-p3`（P0/P1/P2b 通过 #329/#330/#331 合并；P3-Rust、P3-WASM、P4、P5、P6 是该分支上的后续提交）。没有剩余阶段——闭包正确（包括两个目标上所有类型的可变捕获），使用一个捕获分析，且 RC 已在 Lean 中证明。

捕获*单元*架构已经完成。后续对抗性跨目标全面检查发现了闭包*类型化／解析／存储*方面的相邻差异（WASM 上的字符串修改器、元组内闭包、列表索引调用、带类型参数、Map 内闭包、非 Copy 元素的 IndexAssign）——与捕获单元正交。这些问题在[闭包代码生成跨目标缺口](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-codegen-cross-target-gaps.md)中单独跟踪（5 项通过 #334 修复；2 项深层问题已定位根因，仍未关闭）。

保留代码与诊断的说明：首例文件为 src/lib.almd 与 main.almd；1005 在两目标都正确，下一调用期望 -5，但 WASM 输出 1005。诊断 unknown table 0 表示 table index 越界。第二例中自动 ? 在 wasm 上 trap，注释掉的显式 ! 则始终成功并得到 15。捕获类型的 inline 注释 Almide owned type 表示 Almide 所有类型。
