<a id="native-nested-ctorliteral-pattern-at-a-boxd-recursive-variant-field"></a>

# native：Box 包装的（递归 variant）字段中的嵌套 ctor／字面量模式

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](native-boxed-pattern-lowering.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](native-boxed-pattern-lowering.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](native-boxed-pattern-lowering.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/native-boxed-pattern-lowering.md)
> 应优先按开头的已实现更新理解，而非下方保留的历史“准备实现”诊断。旧失败、修复方案和低风险判断是历史，不是新复现或重新实现指令。


> 原文描述：嵌套 boxed-pattern 的历史诊断；已实现的 native 降低处理及回归引用

<a id="implemented-mechanism-original-diagnosis-below"></a>

## 已实现的机制；以下是原始诊断

此处描述的 native 嵌套 box 模式机制已经实现在 [statements_box_patterns.rs](../../../../../../crates/almide-codegen/src/walker/statements_box_patterns.rs) 和 [pass_box_deref.rs](../../../../../../crates/almide-codegen/src/pass_box_deref.rs)，随后还有嵌套模式／捕获改进（#757/#3174）。[嵌套 variant 夹具](../../../../../../spec/wasm_cross/nested_variant_tag_box.almd) 记录了回归用例。下方旧的“准备实现”状态及编译失败属于最初 #610 的诊断，不是新复现，也不是要求重新实现该 pass。

状态：active — 已分析，准备实现（#610）
负责人：compiler / codegen
范围：仅 NATIVE（Rust）目标。wasm 已正确（control.rs #607 将嵌套 boxed 模式重构为标签守卫＋嵌套 match）。

<a id="symptom"></a>

## 症状

```almide
type Tree = | Leaf(Int) | Node(Tree, Tree)
fn sum(t: Tree) -> Int = match t {
  Node(Leaf(a), Leaf(b)) => a + b,   // nested ctor at a Box'd field
  Node(l, r)             => sum(l) + sum(r),
  Leaf(n)                => n,
}
```

- wasm：正确（`6`）。
- native：生成的 Rust 无法编译。两个相互关联的缺陷都位于这一行：
  `Tree::Node(Tree::Leaf(a), Tree::Leaf(b)) => ((*a) + (*b))`。

  - **(a) E0614 多余解引用** — `a:Int` 并未被 box 包装，却使用 `(*a)`/`(*b)`。`pass_box_deref.rs::collect_bind_vars`（约第 450 行）无条件递归进入嵌套 `Constructor` 的参数，将内部绑定（`a`,`b`）标为解引用变量；但只有直接 Box 包装的字段位置才应解引用。修复：`collect_bind_vars` 只能标记直接的 `IrPattern::Bind`，删除对 `Constructor { args }` 的递归（boxed 字段中的嵌套 ctor 由缺陷 (b) 处理，内部绑定是解引用后的值，而非 box 指针）。
  - **(b) E0308 box 未匹配** — 将 `Tree::Leaf(a)` 直接与 `Box<Tree>`（Node 的字段）进行模式匹配。稳定版 Rust 没有 `box` 模式（生成 Rust 使用稳定版，不使用 `#![feature(box_patterns)]`），因此 boxed 字段中的嵌套 ctor／字面量不能直接匹配。

(a) 与 (b) 必须同时落地：只修 (a) 会移除遮蔽问题的解引用并留下 E0308；只修 (b) 则留下 E0614。

<a id="fix-for-b-mirror-pass_pattern_literal_guardrs-for-ctors"></a>

## (b) 的修复：对 ctor 仿照 `pass_pattern_literal_guard.rs`

`pass_pattern_literal_guard.rs` 已解决嵌套 STRING 字面量的类似问题（仅 Rust 的 pass）：将载荷内嵌套的 `Literal{LitStr}` 替换为新的 `Bind __lit_N`，并把 `__lit_N == "..."` 以 AND 加入分支守卫。守卫为假时继续后续分支，与 wasm 的先标签后值检查完全相同。`IrMatchArm` 有 `guard: Option<IrExpr>` 字段，walker 将其渲染为 `pattern if guard`。

针对 #610，扩展该 pass（或添加同级 pass）：对于处于类型满足 `ty_contains_any_recursive` 的字段上的 `IrPattern::Constructor`（或非 Int/Bool/Float 的 `Literal`），进行下述改写。该谓词与 Box 渲染和缺陷 (a) 的位置相同，复用 `walker::ty_contains_any_recursive` 加 cycle set。

1. 将嵌套模式 `Leaf(a)` 替换为新的 `Bind __box_N`。
2. 向分支以 AND 添加标签守卫：`match *__box_N { Leaf(_) => true, _ => false }`（Bool 类型的 `IrExpr` Match）。守卫为假时继续后续分支，保持与 wasm 逐字节相同的可达性。
3. 在原本体执行前，于分支本体中重新绑定内部变量，例如前置 `let Leaf(a) = *__box_N else { unreachable!() }`（守卫已证明形状），或使用等价的内部 `match *__box_N { Leaf(a) => …}`。内部绑定是已经解引用的值，因此缺陷 (a) 中不再递归的改动，正是防止在使用位置再次解引用的关键。

在 `tests/crossmod_matrix_test.rs`（或 `spec/wasm_cross/` 夹具）中固定上述 `Tree`/`sum` 形式，native 和 wasm 都必须输出 `6`；门禁已经逐字节交叉检查 wasm。

<a id="why-deferred-from-the-v0276-batch"></a>

## 为什么从 v0.27.6 批次延期

问题仅限 native（跨目标字节门禁不受影响，wasm 正确），而缺陷 (b) 是新的 match 重构 pass（标签守卫 IrExpr 生成＋内部绑定提取），值得集中实现并专门运行测试，不宜混入多项修复发布。触发范围窄（boxed 字段的嵌套 ctor／字面量，当前不能编译的形式），因此对当前通过程序的回归风险低。
