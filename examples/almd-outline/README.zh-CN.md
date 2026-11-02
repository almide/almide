# almd-outline

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](./README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](./README.zh-CN.md)

使用自身语言的演示：用 Almide 自身重新实现 `almide ide outline` 的格式化器。

主工具链的 `almide ide outline <file>` 执行三项工作：

1. 解析源文件并进行类型检查（通过编译器内部实现 — Rust）。
2. 提取 `ModuleInterface`（通过 `almide-tools::interface::extract`）。
3. 将该接口渲染为每项声明占一行的文本。

本示例涵盖 **第3步** — 渲染 — 完全使用 Almide 编写。
输入是 `almide compile --json <file>` 输出的 JSON。

## 运行

```sh
# 1. Generate a ModuleInterface JSON for some source file
almide compile --json /path/to/your.almd > /tmp/iface.json

# 2. Render it with the Almide-written formatter
almide run examples/almd-outline/src/main.almd -- /tmp/iface.json
```

对于相同的源文件，输出与 `almide ide outline /path/to/your.almd` 逐位一致。

## 为什么这很重要

Almide 的使命是成为“LLM 能最准确编写的语言”。如果一个涉及核心模式（JSON 解析、模式匹配、递归渲染、字符串插值、带效应的 I/O）的非平凡 Almide 程序可以自然地编写出来，这就比微基准测试更有说服力。该文件约140行，涵盖：

- `import json / env / fs`，以及对带效应模块采用显式导入的规则
- `Value` + `value.as_*`（Option 形式使用 `?`）／`json.get_*` 遍历
- 对标签字符串进行 `match`，并在分支中使用字符串插值
- 管道链：`xs |> list.map(f) |> list.join(", ")`
- 递归渲染（经由记录类型，`render_type` → `render_inner` → `render_type`）
- 用 `guard cond else err("msg")` 实现 `effect fn` 的提前返回
- 用 `expr ?? default` 和 `expr!` 处理 `Option`／`Result`

## 编写过程中发现的历史缺口

以下记录的是最初的实现经历，而非当前尚未解决的问题列表。特别是，[llms.txt](../../llms.txt) 现在已经存在。在将历史缺口视为尚未实现之前，请先检查当前诊断和接口源码。

1. **`guard cond else { println(...) }`**（只有副作用，没有返回值）应当在检查器层面报错，提示“guard else must return or err”，而不是泄漏到 rustc，变成 `expected Result<(), String>, found ()`。[rustc-leak wrap](../../src/cli/mod.rs) 当前仅在 `almide test` 时触发，在 `almide run`／`almide build` 时不会触发，应予扩展。

2. **`effect` 是保留关键字** — 不能用作变量名。值得将其明确列入 `expect_ident` 的诊断列表，避免 LLM 反复犯错。

3. **JSON 字段名本身不足以说明含义。** `ModuleInterface` 将函数返回类型序列化为 `"return"`（不是 `"ret"`），将常量类型序列化为 `"type"`（不是 `"ty"`）。在没有规范的情况下编写读取器，需要先猜测再纠正。应通过 [crates/almide-tools/src/interface.rs](../../crates/almide-tools/src/interface.rs) 中的文档注释和公开 JSON 模式固定该模式，或者发布 `llms.txt`（Phase 3-3），使后续重新实现无需逆向推断。

尽管如此，总体体验是：**这门语言足以胜任这类工作**。在了解上述缺口之后，没有感觉到缺少功能，也不需要别扭的变通办法。
