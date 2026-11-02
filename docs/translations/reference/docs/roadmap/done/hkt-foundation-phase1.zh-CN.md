<a id="hkt-foundation--phase-1-4--stream-fusion-all-6-laws"></a>

# HKT 基础 — 阶段 1–4 + 流融合（全部 6 条定律）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](hkt-foundation-phase1.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](hkt-foundation-phase1.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](hkt-foundation-phase1.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/hkt-foundation-phase1.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本。固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不会自动同步。状态、实现及测量描述均为原文时点的记录，并非翻译时的独立验证或现行保证；计划不代表已经实现。

说明：包含类型构造器和代数定律的 HKT 基础阶段 1–4。
记录中的完成日期：2026-03-19。

**完成日期：** 2026-03-19
**PR：** #49

<a id="implementation-details"></a>

## 实现细节

<a id="phase-1-ty-helpers--typeconstructor-foundation"></a>

### 阶段 1：Ty 辅助方法 + TypeConstructor 基础
- `TypeConstructorId`、`Kind`、`AlgebraicLaw` 类型定义
- `TypeConstructorRegistry` — 自动注册内置类型和用户定义类型
- `Ty::children()`、`map_children()`、`map_children_mut()` — 统一的类型遍历
- `Ty::constructor_id()`、`type_args()`、`constructor_name()`、`is_container()`、`any_child_recursive()`、`all_children_recursive()`
- `Ty::list()`、`option()`、`result()`、`map_of()` 智能构造器
- `Ty::inner()`、`inner2()`、`is_list()`、`is_option()`、`is_result()`、`is_map()` 访问器
- 在 IrProgram 中添加 `type_registry` 字段，在 lowering 期间自动注册
- 简化 14 个函数中的 match 分支（-120 行）

<a id="phase-2-algebraic-law-table"></a>

### 阶段 2：代数定律表
- 6 条定律：FunctorComposition、FunctorIdentity、FilterComposition、MapFoldFusion、MapFilterFusion、MonadAssociativity
- List：Functor + Filterable + Foldable
- Option：Functor + Monad
- Result：Functor

<a id="phase-3-stream-fusion--ir-rewrite-for-all-6-algebraic-laws"></a>

### 阶段 3：流融合 — 全部 6 条代数定律的 IR 重写
- 管道链检测（嵌套调用和 let 绑定链）
- **FunctorComposition**：`map(map(x,f),g)` → `map(x, f>>g)` — 消除中间 List
- **FunctorIdentity**：`map(x, id)` → `x` — 消除 map 本身
- **FilterComposition**：`filter(filter(x,p),q)` → `filter(x, p&&q)`
- **MapFoldFusion**：`fold(map(x,f),init,g)` → `fold(x, init, g∘f)` — 消除 map，单次遍历
- **MapFilterFusion**：`filter(map(x,f),p)` → `filter_map(x, ...)` — 单次遍历
- **MonadAssociativity**：`flat_map(flat_map(x,f),g)` → `flat_map(x, f>>=g)`
- Lambda 组合 + 谓词组合 + 变量替换
- 通过 ALMIDE_DEBUG_FUSION=1 输出分析结果

<a id="phase-4-tyapplied-unification"></a>

### 阶段 4：统一为 Ty::Applied
- 移除 `Ty::List`、`Ty::Option`、`Ty::Result`、`Ty::Map`
- 统一为 `Ty::Applied(TypeConstructorId, Vec<Ty>)`
- 342 处构造位置 → 改用智能构造器
- 约 200 个 match 分支 → 迁移到 Applied 模式
- 更改 23 个文件
- 更新 build.rs（generated/ 也自动处理）

<a id="remaining-at-completion-2026-03-19"></a>

## 完成时（2026-03-19）的剩余事项

原始 `active/hkt-foundation.md` 计划已不存在。以下是完成时记录的剩余工作快照，不是当前任务清单。

- 阶段 5：编译器内部的效应信息注解（无语法变更）→ 2.x
- 阶段 6：Trait 集成 → 2.x

<a id="tests"></a>

## 测试
- 617+ 个 Rust 单元测试（56→617）
- 110/110 almide 测试
- 0 个警告
