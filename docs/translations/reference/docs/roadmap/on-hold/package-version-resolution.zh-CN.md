# 依赖版本约束

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](package-version-resolution.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](package-version-resolution.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](package-version-resolution.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-version-resolution.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本。固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不会自动同步。状态、实现及测量描述均为原文时点的记录，并非翻译时的独立验证或现行保证；计划不代表已经实现。

说明：在已提供的 MVS 解析器之外，尚余的依赖范围语义和可用版本选择。

## 已提供的基础

MVS、semver 排序、锁文件和 `almide update` 已经实现。当前行为应归入 [包系统规范](../../../../../specs/package-system.md)，而不是另一份实施计划：

- 对于同一包身份，选择请求的最大版本；不同主版本可以共存。`src/project_fetch.rs::semver_of` 和依赖遍历实现了这一点。`tests/mvs_resolution_test.rs` 固定测试选择行为及其与声明顺序无关的性质。
- `almide.lock` 为每个直接依赖记录选定检出内容的来源、ref 和提交。`tests/dep_lock_pairing_test.rs` 和 `tests/lock_roundtrip_test.rs` 覆盖所选版本的配对和格式。
- `almide update [dep]` 推进浮动 ref；固定的标签不会推进。参见 [CLI 参考](../../../../../specs/cli.md#almide-update) 和 `src/project_fetch.rs::update_locked_deps`。

## 剩余提案 — 暂缓

已提供的最大请求版本规则**不是**通用依赖约束求解器。`resolve_dep_version` 去掉前导非数字并补齐版本分量；它不求解脱字符、波浪号或复合范围。获取仍通过 `dep_ref_name` 选择 `tag`、`branch` 或 `main`。不能将 `version` 字段描述为保证会查找并选择匹配标签。

- 定义支持的范围语法，以及它与 MVS、1.0 之前的身份、预发布版本和不兼容上限之间的关系。
- 从 git 标签（或未来的注册表）发现可用版本，并选择满足声明约束的版本；对无效或不可满足的请求，给出指明依赖路径的诊断并拒绝。
- 规定支持范围的 `add` 和 `update` 如何与现有锁及精确标签固定交互，再通过可复现性测试覆盖这些规则。
- 决定新选版本所需的完整性元数据；注册表来源追溯由 [中央注册表工作](../../../../../roadmap/on-hold/package-registry.md) 负责。

不要仅为实现此提案就添加第二个 semver 解析器或替换现有锁格式。复用现有基础并扩展其可执行测试。

## 设计参考

- [Go MVS](https://research.swtch.com/vgo-mvs) — 所请求最小值中的最大值
- [Cargo resolver](https://doc.rust-lang.org/cargo/reference/resolver.html) — 范围解析比较
