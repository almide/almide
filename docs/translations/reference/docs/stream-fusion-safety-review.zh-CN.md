<a id="stream-fusion-safety-review-2026-09-09"></a>

# Stream fusion 安全性审查（2026-09-09）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](stream-fusion-safety-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](stream-fusion-safety-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](stream-fusion-safety-review.zh-CN.md)

> 固定原文的非规范性历史阅读资料。下述测试、分支状态、测量及修复均为带日期报告，不是新执行的验证或普遍正确性保证。保留包括故意发散示例在内的原代码，未执行。不自动同步，以原本为准。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stream-fusion-safety-review.md)

PR #2053 修复 #2054 中重复执行 map 回调效果的问题。更新到当时的 develop 时，也保留了与 StreamFusion 并列的 DecodeErrFrame pass；唯一的文本合并冲突位于流水线文档。

<a id="reference-and-decision"></a>

## 参考与决定

固定的 Gleam checkout `../almide-references/gleam`（`19bf207ebb7d953ea1391f041da48c214ee1440a`）在 `compiler-core/src/javascript/decision.rs::assign_subject` 中解释，非变量匹配对象为何必须只求值一次、绑定后再复用。这支持保留可观察的求值行为，而非假定任意表达式都能安全地重复、重排或省略。Almide 既有的 `crates/almide-mir/src/purity.rs` 还区分有宿主效果的模块、不纯的普通函数（包括分配器状态）及高阶调用。这些区别也适用于融合。

审查发现，原融合终止性分析对纯度与全定义性都使用最大不动点。递归循环可能没有外部副作用，但这不是它会终止的证据。以下被接受的程序：

```almide
fn diverge(n: Int) -> Int = if n == 0 then 0 else diverge(n) + 1
fn main() -> Unit = {
  let xs = [1] |> list.map((x) => diverge(x)) |> list.take(0)
  println(int.to_string(list.len(xs)))
}
```

过去生成单个 `.map(...).take(0).collect()` 链，递归调用因此消失。记录中的修复会生成 `list_take(map(...).collect(), 0)`：原本的立即求值 map 必须先完成，再执行 take。没有测试实际执行该发散程序；检查的是生成形态。单元测试拒绝将直接递归、相互递归及其调用者认定为全定义，而允许无环的全定义调用链。

全定义性现在从已证明的叶函数增长（最小不动点）；纯度仍使用最大不动点。运行时调用还必须属于已知且不在有副作用集合内的标准库模块；函数参数要求其被调用函数具有证明；可变借用被拒绝。任意 InlineRust 不能仅因没有可疑词汇就获得纯度：只允许编译器精确的带编号数组占位符形式。测试涵盖不透明 Rust、有副作用／未知运行时模块、回调引用及可变借用。

表达式证明划分为互不相交的字面量、运算符、控制、聚合、包装及调用情况。`Some(false)` 停止分派；未知情况不返回证明。Codopsy 2.2.0 在未修改规则时测得融合 pass 为 **100/A**；codegen crate 原有汇总仍为 **89/B**。

<a id="validation-of-the-updated-branch"></a>

## 更新分支的验证

- `almide test` 与 `almide test --target rust` 均为 438/438 文件。
- Codegen crate：56 项单元测试、10 项集成测试通过。
- Nanopass／codegen snapshot／端到端 render 门禁：34／13／2 项通过。
- 对全部 codegen target 执行 Clippy：通过；没有警告指向改动的融合文件。原有无关警告仍存在。
- Pass 列表、文件规范及 `git diff --check`：通过。

<a id="module-names-must-not-establish-root-function-purity"></a>

## 模块名不得作为根函数纯度的依据

进一步端到端审计发现，模块函数的裸名称被加入与根函数相同的证明集合。存在纯函数 `helper.observe` 与会打印的根函数 `observe` 时，`[1, 2] |> list.map((n) => observe(n)) |>
list.take(0)` 错误地抑制了两次打印。原二进制打印 `0, 0`；禁用融合后打印 `0, 1, 2, 0`。修复后模块函数仅提供限定别名，避免跨命名空间的纯度与全定义性证明。`tests/stream_fusion_module_purity_test.rs` 通过实际导入模块固定可观察输出。
