<a id="region-based-memory-management"></a>

# 基于区域的内存管理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](region-inference.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](region-inference.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](region-inference.zh-CN.md)

> 固定原文的非规范读者副本，不会自动同步。历史实现与未实现的搁置提案保持区分；当前实现陈述限定于原文时点，未在此独立验证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/region-inference.md)

> **状态**：完整区域推断仍是搁置中的提案。下面的 Phase 1+2 记录 v0.23.4 的实现，并非当前 Wasm 分配器。结构式后端现在使用引用计数和按大小类别划分的空闲列表（`crates/almide-wasm/src/alloc_inline.rs`）；不要从旧 bump-restore 设计的局限推断当前存在泄漏。

<a id="historical-implementation-v0234"></a>

## 历史实现（v0.23.4）

<a id="phase-1-function-level-region-v0234"></a>

### Phase 1：函数级区域（v0.23.4）

返回 Unit 的函数和 lambda 在入口／出口自动保存、恢复堆指针。内部所有临时分配都在函数返回时回收。

**覆盖**：基准测试回调、事件处理器、请求处理器。

<a id="phase-2a-loop-iteration-region-v0234"></a>

### Phase 2a：循环迭代区域（v0.23.4）

若 while 循环体不向外层作用域变量赋予堆值，则自动为每次迭代建立作用域。临时分配逐次迭代回收。

**覆盖**：`while i < N { let _r = list.map(...); i = i + 1 }`

<a id="phase-2b-block-liveness-region-v0234"></a>

### Phase 2b：块内活跃性区域（v0.23.4）

对块内每个堆类型 let 绑定，编译器找出最后一次引用，并在其后插入堆恢复操作。分配一旦不再使用便被回收。

**覆盖**：`let step1 = map(...); let step2 = filter(step1, ...); // step1 freed here`

<a id="phase-3-full-region-inference-future"></a>

## Phase 3：完整区域推断（未来）

<a id="cases-motivating-the-original-proposal"></a>

### 推动原提案的场景

这些描述的是仅靠 bump 恢复区域的局限，并非当前缺陷清单。提出新分配器前，必须根据当前所有权／drop 实现评估嵌套值释放和可变值替换。

1. **嵌套堆结构**：`List[List[Int]]` — bump 恢复无法单独释放内部列表（恢复会释放标记之上的全部内容）。
2. **可变变量替换**：`var s = "hello"; s = "world"` — 仅靠该 bump 恢复无法回收旧 "hello" 分配（恢复会使 "world" 失效）；这不是对当前 RC 行为的主张。
3. **闭包捕获**：闭包捕获的值可能比捕获它的作用域活得更久。区域推断必须跟踪捕获值的生命周期。
4. **跨函数区域多态**：`fn make_list() -> List[Int]` — 返回列表的区域取决于调用方上下文。

<a id="design-approach-mlkit-style-region-inference"></a>

### 设计方向：MLKit 式区域推断

- **区域类型注解**：每种堆类型带有隐式区域变量。`List[Int]@r1` 表示“在区域 r1 中分配的列表”。
- **区域推断**：用区域约束扩展 Hindley-Milner 合一。区域变量由推断得到，而非由用户书写。
- **区域多态**：`fn map[A, B, r1, r2](xs: List[A]@r1, f: A -> B) -> List[B]@r2`。输出区域 r2 由调用点决定。
- **区域效果**：函数声明在哪些区域分配。`fn f() -r1-> T` 表示“f 在区域 r1 中分配”。
- **Letregion**：`letregion r in { ... }` 创建在块退出时释放的区域。编译器通过推断自动插入这些结构。

<a id="implementation-scope"></a>

### 实现范围

- **前端**：在 almide-frontend 类型检查器中推断区域变量
- **IR**：在 IrExpr 类型上附加区域注解
- **代码生成**：感知区域的分配（多个 bump 指针，每个活跃区域一个）
- **运行时**：区域栈（push/pop 区域，而非单一 heap_ptr）

<a id="references"></a>

### 参考

- Tofte & Talpin (1997): "Region-Based Memory Management"
- MLKit compiler: https://github.com/melsman/mlkit
- Cyclone language: Region-based safe C

<a id="original-motivation"></a>

### 原始动机

v0.23.4 计划在 Phase 1+2 优先处理这些模式：
- 短生命周期进程（Phase 1）
- 处理循环（Phase 2a）
- 流水线转换（Phase 2b）

Phase 3 原本针对具有复杂状态、长时间运行的服务器。重启本提案需要通过测量证明当前 RC 和空闲列表回收之外的需求；仅凭历史区域计划不能确立这种需求。
