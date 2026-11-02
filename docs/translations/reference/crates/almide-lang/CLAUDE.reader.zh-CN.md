[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 仓库代理指示的非规范、固定原文的人类读者副本。权威 CLAUDE.md 不变。此 CLAUDE.reader.md 系列刻意不使用会自动加载指示的文件名。这些是历史原文指示，不是新的指示层，也不是独立验证的当前实现或安全保证；不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-lang/CLAUDE.md)

# almide-lang

门面 crate。将 `almide-syntax`（AST、词法分析器、解析器）和 `almide-types`（Ty、stdlib_info）重新导出为单一依赖。

<a id="rules"></a>

## 规则

- **此处不放逻辑。** 此 crate 只负责重新导出。需要共享逻辑时，放入适当的叶子 crate。
- **下游 crate 依赖此 crate**，而不是分别依赖 almide-syntax 和 almide-types。
