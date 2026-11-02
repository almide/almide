[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 仓库代理指示的非规范、固定原文的人类读者副本。权威 CLAUDE.md 不变。此 CLAUDE.reader.md 系列刻意不使用会自动加载指示的文件名。这些是历史原文指示，不是新的指示层，也不是独立验证的当前实现或安全保证；不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-base/CLAUDE.md)

# almide-base

基础 crate。所有其他 crate 都依赖它。

<a id="provides"></a>

## 提供内容

- **`Sym`**——指向全局线程安全字符串驻留器（`lasso::ThreadedRodeo`）的 `Copy` 句柄。编译器中的所有标识符、类型名和字段名都是 `Sym`。
- **`Span`**——源码位置（line、col、end_col）。
- **`Diagnostic`**——带上下文、提示及基于 Levenshtein 的“你是否想写……”建议的编译器错误／警告。

<a id="rules"></a>

## 规则

- **绝不按字符串比较标识符。** 使用 `Sym` 相等比较（`==`）。相等比较为 O(1)（比较驻留 ID）。`Sym::Ord`（`<`、`>`）为确定性排序而有意比较字符串内容；walker 按字母顺序排列记录字段，以保持 Rust 结构体布局稳定。
- **用 `sym(s)` 驻留，用 `.as_str()` 读取。** `resolve()` 函数返回 `&'static str`；由于驻留器从不释放，因此是安全的。
- **诊断必须可操作。** 每个错误应尽可能包含提示或建议。模糊匹配使用 `Diagnostic::suggest(name, candidates)`。
- **不要在此添加运行时负担重的依赖。** 此 crate 在所有 crate 的依赖链中，必须快速编译。
