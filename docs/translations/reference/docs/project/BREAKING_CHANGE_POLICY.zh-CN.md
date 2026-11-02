<a id="breaking-change-policy"></a>

# 破坏性变更政策

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](BREAKING_CHANGE_POLICY.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](BREAKING_CHANGE_POLICY.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](BREAKING_CHANGE_POLICY.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BREAKING_CHANGE_POLICY.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> Python 2→3 的教训：静默的行为变更会给生态系统带来沉重负担。
> Swift 1→2→3 的教训：连续的破坏性发布会增加用户的迁移成本。
> Go 1 的教训：“今天能编译的代码永远能编译”是其最大优势。

---

<a id="現行実装と-post-10-方針の区別"></a>

## 区分当前实现与 post-1.0 政策

以下 post-1.0 / edition 政策是未来的运作设计，并非当前已实现的功能。当前 `src/project.rs::PACKAGE_METADATA_KEYS` 中的 `edition` 仅作为不改变行为的 metadata 被接受。按 edition 切换 parser 的机制和 `almide migrate` 子命令均未实现。目前，已移除语法与迁移提示由 `proofs/dialect-epochs.toml` 和 checker 处理。以下保留未来政策本身。

<a id="原則"></a>

## 原则

<a id="1-silent-な挙動変更は絶対禁止"></a>

### 1. 严禁静默改变行为

post-1.0 不进行改变现有代码含义的变更。如果必须改变类型或运算符的行为：
- 旧代码必须产生**编译错误**，而不是静默返回不同结果。
- 错误消息必须包含**迁移提示**。
- 新行为只能在新 edition 中启用。

<a id="2-api-削除は-2-minor-version-の-deprecation-後"></a>

### 2. API 弃用 2 个次版本后才能移除

移除或重命名 stdlib 函数：
1. **v1.x**：发出 `#[deprecated]` 警告（仍可编译）。
2. **v1.(x+1)**：继续发出警告。
3. **v1.(x+2)**：允许移除，但必须通过编译错误 + 提示引导用户使用替代函数。

<a id="3-構文変更は-edition-で吸収post-10-構想"></a>

### 3. 通过 edition 承接语法变更（post-1.0 构想）

新增关键字或语法变更由 `edition` 字段控制：
- `edition = "2026"` 的代码按 2026 的规则编译。
- `edition = "2027"` 启用新语法。
- 不同 edition 的模块可以互操作（与 Rust editions 相同）。
- 通过 `almide migrate --edition 2027` 提供自动转换工具。

<a id="4-コア型のシグネチャは不変"></a>

### 4. 核心类型的签名保持不变

post-1.0 不改变以下类型已有方法／函数的签名：
- `String`, `Int`, `Float`, `Bool`
- `List[T]`, `Map[K, V]`
- `Option[T]`, `Result[T, E]`
- `Tuple`, `Record`, `Variant`

可以**新增**函数。禁止**修改已有函数的参数类型或返回类型**。

---

<a id="10-前の-breaking-change"></a>

## 1.0 之前的破坏性变更

1.0 之前允许破坏性变更，但必须：
- 在 CHANGELOG 中明确记录。
- 尽可能发出附带迁移提示的编译错误。
- 在 1.0 之前完成 stdlib-verb-system 改革。

---

<a id="edition-の仕組み未実装の設計"></a>

## edition 的机制（尚未实现的设计）

```toml
# almide.toml
[package]
name = "myapp"
version = "0.1.0"
edition = "2026"
```

- 省略 `edition` 时，默认使用编译器的最新 edition。
- 不同 edition 的依赖包可以无障碍混用。
- 每 2-3 年更新一次 edition（参照 Rust 的 3 年周期）。
