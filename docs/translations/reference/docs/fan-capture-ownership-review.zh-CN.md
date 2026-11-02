<a id="fan-sibling-capture-ownership-2061"></a>

# Fan 兄弟分支的捕获所有权（#2061）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-capture-ownership-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-capture-ownership-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](fan-capture-ownership-review.zh-CN.md)

> 非规范、固定原文的历史读者快照。原文仍为权威来源，不替换机器输入或可执行 gate。不自动同步。记录的测试结果、数量、实现状态及证明主张仅限原文语境，并未在此独立重跑或确立为当前保证。 保留 Option 分支尚不支持、仓库整体 A 尚未达成这两项明确限制，不将其写成成功。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/fan-capture-ownership-review.md)

`fan { list.sum(rows), list.len(rows) }` 通过类型检查，却发射两个共享同一 Rust `rows` 绑定的 `move` 闭包，导致 E0382。两个调用都只是借用，但第一次 spawn 就 move 了该绑定。

CaptureClone 现在将 fan 分支视为隐式闭包，在整个 Fan 表达式外准备拥有所有权的只读捕获。每个分支获得不同的 VarId 和发射名称，因此每次 clone 都先于所有 spawn。原值在 fan 表达式后仍可用。借用的 slice 和 string 参数使用与显式 lambda 相同的拥有式物化。分支直接修改的捕获保留现有处理；此更改不引入线程间共享可变状态。

现有 capture-binding 实现由 lambda 和 fan lowering 共用。CloneInsertion 仍统计现有 Clone 节点内部的使用，但不再在其下插入额外 clone。调用参数所有权逻辑和捕获绑定构造现在分属不同模块，在不改变阈值的前提下，让各源文件低于 Codopsy 的 800 行限制。

<a id="reference-research"></a>

## 参考实现研究

克隆于提交 `0d31508599a7814a7044e9a7a871e3dc5f037753` 的 Rust 参考实现现在包含 `library/std/src/thread/scoped.rs`：`Scope::spawn` 接收具有 `FnOnce + Send` 约束的闭包值（201–206 行）。支持作用域生命周期不意味着两个 move 闭包可以消耗一个拥有的值。捕获准备必须在将闭包值传给 `spawn` 之前完成。现有 Gleam 参考实现的 `javascript/decision.rs::assign_subject` 同样在重用前绑定计算得到的 subject，以保持求值。两个仓库均在 `../almide-references`。

<a id="regression-coverage"></a>

## 回归覆盖

`tests/fan_sibling_capture_test.rs` 在 native 和结构化 WASM 上执行 List、String、Map、记录捕获，借用参数，fan 后使用以及嵌套 fan 表达式。它还检查发射 Rust 的用户部分，拒绝重复的 `.clone().clone()`。Map 回调返回 scalar 值：直接从 fan 分支返回 Option 目前会触发另一个结构化 `bind:unmapped` 拒绝，不宣称支持此情况。

Codopsy 2.2.0，配置不变：CaptureClone、capture bindings、CloneInsertion、调用参数 helper 各为 100/A；codegen crate 为 90/A；全部 crate 汇总为 83/B（两个文件未被 Codopsy 解析），因此仓库整体 A 仍未达成。
