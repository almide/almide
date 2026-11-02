<a id="phase-0-決定ゲート--結果記録"></a>

# Phase 0 决策门禁 — 结果记录

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](GATE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](GATE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](GATE.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/v1-mir/GATE.md)
> 五形态实验是历史证据，不是对全部程序的一般定理。原文表把 list_get_643 描述为 leak/divergence，而发现3称 #643 为 RC double-free；保留此原文不一致，不杜撰结果调和。

状态：**PASSED — 5/5 形态，0 conditional，0 fail，0 escape hatch（2026-06-13）**
规格：[docs/roadmap/active/v1-mir-architecture.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) §8
重跑：`research/spike/v1-mir/run-gate.sh`（用 rustc 1.95.0 验证）

<a id="ゲートが問うたこと"></a>

## 门禁提出的问题

用所有权最棘手的五种形态，推翻或证明 §2.2 的核心论点——

> **Perceus RC 是一般模型，affine/move 只是“静态已知计数为 1 的 RC”的特殊形式。**

每种形态在最小 MIR 中只有**一个 Perceus 所有权决策**，分别呈现为 **(A) 惯用 Rust（move/borrow/clone）** 和 **(B) 引用计数（用手写 RC 模拟 wasm 语义）**。通过条件：

1. A 与 B **按构造一致**（输出相同）。
2. 忠实 RC 不发生 leak / double-free。
3. **重新决定所有权**的 renderer（buggy variant）**被捕获**。

每种形态是自包含 meta-harness：构造 MIR 决策，以两种方式呈现，用 `rustc --edition 2021 -O` 编译双方、执行、比较，仅 PASS 时 exit 0。

<a id="結果"></a>

## 结果

| # | shape | 一个决策 | A=B | RC clean | 捕获 buggy | escape hatch | verdict |
|---|---|---|---|---|---|---|---|
| 1 | alias_return | 最后 consume = 转移 payload ptr，**仅释放壳** | ✓ | ✓ | double-free | none | **PASS** |
| 2 | list_get_643 | alias-inc + scope-dec，对迭代堆 temp 逐迭代 drop | ✓ | ✓ | leak/divergence | none | **PASS** |
| 3 | boxed_pattern_610 | boxed field **跨 box borrow**，Leaf payload 为 Scalar/Copy | ✓ | ✓ | double-free | none | **PASS** |
| 4 | closure_capture | capture = Dup 到 env + closure-drop 时 Drop，每次 call 借用 | ✓ | ✓ | rustc reject / double-free | none | **PASS** |
| 5 | alias_cow | 修改可能共享的 ref 时 **MakeUnique 先行**（clone \| cow_check） | ✓ | ✓ | 值破坏（两种惯用表达相同） | none | **PASS** |

形态文件：`src/main.rs`（#2，cargo entry）+ `shapes/{alias_return,boxed_pattern_610,closure_capture,alias_cow}.rs`。

<a id="正準形を精緻化する-4-つの正直な発見"></a>

## 细化规范形的四个诚实发现

门禁不只给出“PASS”，还揭示 §2.2 规范形的四个**边界**。这不是 escape hatch（规范形破裂），而是明确规范形**一次决定什么**（已反映到 design doc §2.2/§3）。

1. **所有权极性是 per-binding/per-parameter 的 MIR fact**（alias_return）。相同源码在 consume 时成为 move，borrow 时成为 clone/dup。这不是 renderer 重新决定，而是 MIR **每个绑定／参数决定一次**的事实。规范形 §2.2 的“最后 consume → move”是此极性为 consume 时的形态。

2. **可吸收 SYNTACTIC 差异，不可作 SEMANTIC 决策**（boxed_pattern_610）。Rust 没有 box-pattern（`box Leaf(a)` 为 unstable），因此 boxed field 的 nested ctor 呈现为 tag-guard + `&**deref` nested-match。这是**语法差异**，不是所有权／布局决策。renderer 可解决 target 语法差异，但**绝不能重新决定**所有权／布局。

3. **buggy 的观察特征因形态而异**（alias_cow vs 643）。即使同属“renderer 重新决定所有权”，#643 表现为 **RC double-free**，AliasCow 则表现为 **wrong-output**（违反 value-semantics，RC **仍然平衡**，两种惯用表达同样破坏 `a`）。→ Phase 1 验证不能只检测 leak/double-free，还需要**值等价**。门禁诚实地按这些特征构造，不伪造 double-free。

4. **shared + mutated-across-calls + returned 的 capture 是 Rc<RefCell> 区域的边界**（closure_capture scope note）。本门禁的 read-only capture-called-twice 可呈现为普通 owned-String move-closure（Fn），Rc/RefCell 为零。但“共享、跨多次 call 修改、并被返回”的 capture 属于 alias_cow 形态，这就是显式 Rc 的起点。Phase 1 将其作为 MIR MakeUnique／共享可变处理（规范形已知边缘）。

<a id="結論"></a>

## 结论

五种形态全部实现**一个 Perceus 所有权决策忠实呈现为两种惯用表达且一致**，仅重新决定所有权的 buggy 被捕获。RC 与 Rust move/borrow **共享一个规范形**。按 §8 的通过分支，**进入 Phase 1（MIR core + 两个 renderer）**。上述四点作为 Phase 1 MIR 设计输入：per-binding 极性、语法与语义分层、值等价门禁、共享可变边缘。
