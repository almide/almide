<a id="make-verify--the-trust-demo"></a>

# make-verify——信任演示

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



> 原文：[demo/make-verify/README.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/demo/make-verify/README.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，英语原文）。这是面向读者的非规范性快照译文，不会自动同步。保留原文的时间、适用范围与主张；翻译时未进行独立测量或实现验证。 原文中约 100%、8 个案例的检出数、“最常见”“一步修复”等表述，作为原文报告的有限实验与解释记录保留。本译文未重新运行，也未独立确立一般 AI 正确率、与整个 Python 生态的优劣或所有逻辑错误的可检测性。

**主张不是“AI 会写出正确代码”。主张是：当 AI 的代码_错误_时——本演示试验了几种这样的错误——Almide 能使失败_清晰_且_可修复_。独立的证书检查在文档规定的覆盖范围内确立特定的建模性质，并不证明所有可编译代码都正确或完全安全。**

这些示例比较的是具体编译器诊断与 Python 的 `py_compile` 门禁，不是其他语言中所有可用工具或安全技术。

<a id="why-this-framing-not-modification-survival-rate"></a>

## 为什么采用这一框架（而非“modification survival rate”）

我们直接测量过 modification-survival（LLM 修改正常工作的代码后，是否仍通过？）。强模型（Claude 级别）处理中等任务时，在**每种**语言上都约为 100%——这是天花板效应：有能力的模型根本不犯这些错误。那不是合适的测量对象。

真正的区别是**当_确实_有错误时会发生什么**，并且：

- **模型群不只有 Claude。** Agent 集群运行更便宜/更小/开放/更旧的模型，它们在大得多的变更中更频繁出错。
- 因此，如实且与模型无关的测量方式是**注入错误**，询问_语言_而非模型：*失败是可见且可修复的，还是悄无声息的？*

这种响应是**语言 + 编译器 + proof spine**的性质，不会随着模型变强而消失。

<a id="the-reported-eight-case-experiment"></a>

## 所报告的八案例实验

原始演示实验注入了 **8 个由 AI 编写的修改错误**（每个由独立 agent 分别在 Almide 和 Python 中编写），并在编写/CI 时询问*语言*：能检测并修复，还是默默通过？

- **Almide 在编译时检测到 6/8**，并给出可操作诊断。未检测到的 2 个中，一个是纯逻辑 off-by-one（对照项——类型系统无法检测），另一个暴露了正在跟踪的真实 Almide 诊断缺口（旧字段名被报告为内部错误，而非“unknown field”）。
- **Python 的编译门禁（`py_compile`）检测到 0/8。** 这些案例通过语法编译门禁，随后产生错误值或运行时错误。这不是对所有 Python linter 或测试的测量。

`./run.sh` 用真实编译器输出现场复现其中三个（下文为第一个）。

<a id="the-scenarios"></a>

## 场景

`./run.sh` 依次展示三个场景：（1）非穷尽 match（Python 默默产出错误的**值**），（2）签名改变后漏改调用点，（3）使用 Option/None 结果时未处理缺失情况（Python 通过编译门禁，随后在运行时崩溃）。三者中，Almide 都拒绝构建错误程序并给出可操作诊断。以下详细说明 #1：

要求 AI“给 `Shape` 添加 `Triangle` variant”。它更新了**类型**，却忘了在 `area()` 处理——这是最常见的修改错误。

自行运行：

```
./run.sh
```

<a id="1-correct-program--fully-type-checked-compiles-runs"></a>

### 1. 正确程序——完整类型检查、编译、运行

```
$ almide run shape.almd
12
12
```

它接受穷尽的类型检查（注意步骤 2——缺失 case 是编译错误，而非运行时意外）。更深层的**证明**层在下文说明。

<a id="2-the-wrong-modification-in-almide--caught-at-compile-with-the-fix"></a>

### 2. **Almide** 中的错误修改——编译时检测，并给出修复

```
$ almide run shape_buggy.almd
error[E010]: non-exhaustive match: missing Triangle(_, _)
  --> shape_buggy.almd:5:34
  in match
  here: fn area(s: Shape) -> Int = match s {
  hint: add arms for Triangle(_, _):
  Triangle(arg1, arg2) => _
Or use `_ => todo()` to compile incrementally.
  |
5 | fn area(s: Shape) -> Int = match s {
  |                                  ^
compile failed
```

失败是**清晰的**（精确指出缺失 case）、**可定位的**（插入符号）、**可修复的**（提供要添加的 arm——Elm 级别）。模型读到它就能一步修复，人类也是如此。

<a id="3-the-same-wrong-modification-in-python--ships-silently"></a>

### 3. **Python** 中相同的错误修改——默默发布

```
$ python3 shape_buggy.py
None
(exit 0)
```

它编译、运行、以 0 退出，并返回 `None` 作为三角形面积。没有错误。bug 进入生产，若最终被发现，也是由下游测试或最终用户发现。

<a id="the-two-trust-layers-this-shows"></a>

## 此处展示的两个信任层

1. **失败清晰且可修复**（本演示）：在编译时用可操作诊断抓住错误变更。与模型无关——对每个会犯错的模型都有价值。
2. **检查建模的性质**（trust spine——独立层）：`almide verify` 产生结构化 Wasm witness，并将检查交给独立的 `almide-verify`。受支持的 core artifact 可通过 SHA-256 绑定到 bundle；此路径不支持 component。所有权平衡、名称和 capability 边界有覆盖范围与信任假设。字节绑定已实现（#4011），但不确立完整内存安全或功能正确性。参见 [差距表](../../../../design/MEMORY-SAFETY.md) 和 [proven vs trusted](../../../../contracts/proven-vs-trusted.md)。

演示的区别是：在这些示例中，可操作诊断与静默失败或后期失败之间的区别。证书证据是单独且有边界的主张，不证明每个错误修改都会被检测。

<a id="honest-scope"></a>

## 如实界定范围

本演示单独展示*失败清晰度*这条轴（最清楚、可立即复现的轴）。跨目标比较与形式化 witness bundle 是另有声明范围的证据层；参见 `docs/` 和 `proofs/`。这里的重点是信任_体验_，用真实编译器输出展示，而不是模型样稿。
