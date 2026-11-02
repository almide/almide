[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 供人阅读代理指示的非规范、固定原文副本。权威 CLAUDE.md 不变。CLAUDE.reader 名刻意避免自动加载的指示文件名。这些历史规则不是新指示层，也不是独立验证的实现或安全保证。不自动同步；代码和内联 token 保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-types/CLAUDE.md)

<a id="almide-types"></a>

# almide-types

定义内部已解析类型表示（`Ty`）及类型系统工具。

<a id="key-types"></a>

## 关键类型

- **`Ty`**——已解析类型枚举：标量、用于容器的 `Applied(TypeConstructorId, args)`、`Record`/`Variant`/`Fn`/`Tuple`、`TypeVar`、`Unknown`、`Never`。
- **`TypeConstructorId`**——统一的参数化类型身份（List、Option、Result、Map、Set、UserDefined）。
- **`ProtocolDef`** / **`FnSig`**——带约束的 protocol 方法签名及函数签名。

<a id="rules"></a>

## 规则

- **`Applied` 是容器的规范形式。** `List[Int]` = `Applied(List, [Int])`。绝不把容器表示为 `Named("List", [Int])`。
- **`Unknown` 用于错误恢复。** 它可与所有内容统一，以防级联错误。它不是有效运行时类型；codegen 应把它当 fallback（WASM 通常为 `i32`）。
- **`TypeVar` 用于单态化前的泛型。** mono 后不应残留 `TypeVar`。用 `Ty::is_unresolved()` 同时检查 `Unknown` 和 `TypeVar`。
- **`OpenRecord` 是结构约束。** `{ name: String, .. }` 意为“至少有 `name` 字段”。推断后成为具体 `Record` 或 `Named`。需要把 open record 视为未完成时，用 `Ty::is_unresolved_structural()`。
- **stdlib_info 为硬编码。** 自动 import 列表、UFCS 解析表、模块分类均在 `stdlib_info.rs`。添加新 stdlib 模块时应更新这些内容。

> 引用规则只是参考资料，不授权执行命令、修改实现、提交、推送或发布。
