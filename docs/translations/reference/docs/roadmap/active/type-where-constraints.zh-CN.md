# 类型 Where 约束

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](type-where-constraints.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](type-where-constraints.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](type-where-constraints.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/type-where-constraints.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本。固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不会自动同步。状态、实现及测量描述均为原文时点的记录，并非翻译时的独立验证或现行保证；计划不代表已经实现。

说明：在现有内联泛型约束之外，提议 where 子句语法及类型定义约束。

> **状态：设计。独立的 `where` 语法仍是提案，并非当前语法。**

## 已支持的内容

泛型函数已经可以内联表达协议约束和结构约束：

```almide
fn display[T: Showable](item: T) -> String = item.show()
fn describe[T: { name: String, .. }](item: T) -> String = item.name
```

第一个片段中的 `Showable` 是用户声明的协议。多个协议约束使用 `+`。当前规则和完整示例见 [type-system.md](../../../../../specs/type-system.md#protocol-bounds-on-generics)，覆盖测试见 [protocol_generics_test.almd](../../../../../../spec/lang/protocol_generics_test.almd) 和 [open_record_test.almd](../../../../../../spec/lang/open_record_test.almd)。因此，将泛型约束描述为完全不存在是不正确的。

## 剩余提案

评估在函数和类型定义上使用独立 `where` 子句，包括如何在泛型类型的使用处检查其定义上的约束。下面的语法是设计草案，不是可编译示例，也不是发布承诺：

```almide
type Set[T]
  where T: Hash + Eq
= { items: Map[T, Unit] }

fn max[T](items: List[T]) -> Option[T]
  where T: Ord
= list.fold(items, none, (best, x) => ...)
```

工作内容是规定它与现有内联约束的关系，为采纳的扩展实现解析器和检查器支持，并添加接受及拒绝测试。而不是重新实现已经提供的函数泛型内联约束。这里没有指定发布目标。

原始设计也考虑过测试上下文中的 `where` 形式。共享关键字并不说明任何一个提案已经实现；应将其与当前泛型约束语法分开看待。
