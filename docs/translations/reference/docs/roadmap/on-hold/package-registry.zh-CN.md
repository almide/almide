# 中央包注册表

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](package-registry.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](package-registry.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](package-registry.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-registry.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本。固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不会自动同步。状态、实现及测量描述均为原文时点的记录，并非翻译时的独立验证或现行保证；计划不代表已经实现。

说明：在现有基于 git 的包系统之外，提供集中的包发布、所有权和来源追溯。

## 现有基础

Almide 已经支持基于 git 的包获取、`almide add <name>` 简写、MVS 和固定到提交的锁文件。简写通过随附的包名映射或默认 GitHub 所有者解析；它**不是**中央注册表。当前行为和来源信任规则记录在 [包系统规范](../../../../../specs/package-system.md) 和 [CLI 参考](../../../../../specs/cli.md#almide-add) 中。

实现依据：`src/project_fetch.rs::resolve_add_target`、`resolve_package_spec`、`update_lock_file` 和 `update_locked_deps`；`src/almide_packages.txt`；`tests/mvs_resolution_test.rs`、`tests/dep_lock_pairing_test.rs` 和 `tests/lock_roundtrip_test.rs`。

锁文件生成和基本版本选择已不再是待办事项。剩余的依赖版本范围语义在 [依赖版本约束](../../../../../roadmap/on-hold/package-version-resolution.md) 中单独跟踪。

## 剩余提案 — 暂缓

- 注册表服务，包含包／版本元数据，并明确包名到已发布内容的映射。
- 发布流程（`almide publish` 是提议的命令，不是已经提供的接口），包括不可变版本和发布验证。
- 名称保留、所有权、转移和恢复规则。
- 注册表内容的完整性及来源验证，并明确划定与当前 git 所有者及原生代码授权不同的信任边界。
- 从显式 git 来源迁移，但不改变已锁定依赖的身份或可复现性。

托管和分发格式仍有待设计选择。不要宣传注册表端点，也不要把当前的名称简写当作注册表发布功能。
