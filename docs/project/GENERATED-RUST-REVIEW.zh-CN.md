# 审查生成的 Rust (#572)

[English](./GENERATED-RUST-REVIEW.md) | [日本語](./GENERATED-RUST-REVIEW.ja.md) | [简体中文](./GENERATED-RUST-REVIEW.zh-CN.md)

在经过资格鉴定的代码生成器模型中（以 SCADE KCG 为先例），生成的源码是可审查的产物。这一资格鉴定模型并不表示 Almide 或其当前构建已通过资格鉴定。本文是 Almide `--target rust` 输出的审查指南。

## 对应关系映射

```
almide app.almd --target rust --trace-map > app.rs
```

该命令在每个渲染出的函数上方输出 `// almd: fn <name> @ line <N>`，并写出 `app.trace.json`，其中各行为 `{fn, almd_line, rust_line}`，通过扫描实际交付的文本生成（映射不会与该文本不一致；门禁为 `tests/trace_map_test.rs`）。锚点指向函数体的源码行。从自托管标准库链接进来的函数携带其各自的标准库源码行；其文件身份由函数名点分前缀中的标准库模块名标识。默认输出（不带该标志）与已提交的输出基线逐字节一致；不带标志时绝不会出现锚点。

## 审查者需要对齐什么

1. **函数**是跟踪映射的对应单位：保留下来的 `.almd` 函数通常渲染为 Rust 函数（单态化的泛型为每次实例化生成一个函数，并以具体类型作为后缀）。内联、死代码消除与辅助函数合成可能改变一一对应关系。遍历器按顺序渲染最终 IR，但 LICM、流融合等优化在此之前执行；仅凭这一事实不能推断源码语句的顺序。请依据各遍的契约和输出文本审查可观察的执行顺序。
2. **稳定模式**：effect 的结果使用推断或声明的 Result 错误通道，并不总是 String。后缀 `!` 遵循其外围上下文（[上下文表](../specs/result-option-effect.md)）；在适用时，`==` 首先通过 Secret／约定相等性进行 lowering；普通的 `BinOp::Eq` 渲染为 `almide_eq!`（前端 `lower/expressions_access.rs`）；字符串／列表的 `+` → `AlmideConcat`；`mut` 参数 → `&mut`；变体 → 具有相同分支名的 Rust enum；记录 → 具有相同字段名的 struct。检查器接受什么就渲染什么，代码生成不会重新推断类型（TypeMap 是权威来源）。
3. **无警告**：生成的 Rust 必须能够无警告地编译（常设测试规则）；整个 run-manifest 语料集每周都在所记录的 Ferrocene 上游兼容性固定版本下构建（`ferrocene-lane.yml`，#573）。该通道目前安装的是标准 rustc，而非经过资格鉴定的 Ferrocene 二进制文件。

## 认证框架

拟议的资格鉴定分工将源码生成放在此处，并在下游使用经过资格鉴定的编译器。目前的标准 rustc 兼容性通道并不能确立这一资格鉴定。证据及剩余缺口列于 [TOOL-QUALIFICATION](../../proofs/TOOL-QUALIFICATION.md)（#574）。
