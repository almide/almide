[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 固定到链接原文的、供人阅读权威代理指示的非规范副本。CLAUDE.reader 刻意避开自动加载指示的文件名，原本不变。不是新指示层、执行授权、自动同步或独立验证的实现保证。保留原文规则与限定，代码不变。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/CLAUDE.md)

<a id="spec-rules"></a>

# 规范规则

<a id="仕様の権威と実装の証拠"></a>

## 规范权威与实现证据

- 规范与实现同步更新。修改代码时，在同一提交中更新对应规范。
- 遵循 [SPEC 的规范地位](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md)。ALS 与实现不一致时，视为实现不符合规范。不能仅凭观察实现就改写规范性契约。
- 在规范中写明验证测试路径和确认范围。测试成功是该案例的证据，不是整个规范正确性的证明。
- 明示未验证部分及规范间矛盾，区分已采纳规则和当前实现状态。
- 更正过时的当前行为说明。不要仅因与实现不同就删除 ADR、历史或规范性契约。
- 说明实现现状的章节，应明确已实现行为与确认范围，不把推测或计划写成当前保证。
- 未采纳的未来提案放在 `docs/roadmap/`。已采纳 ALS 规范即使未实现或不符合，也应保留并明示状态。

<a id="フォーマット"></a>

## 格式

- 开头写 `> Last updated: YYYY-MM-DD`。
- 每节明确验证测试路径：“测试：`spec/integration/modules/diamond_test.almd`”。
- 规范描述附带代码示例，能够原样粘贴到 `.almd` 文件并编译。

<a id="現在の-spec"></a>

## 当前规范

| 文件 | 内容 |
|---|---|
| `language.md` | 语言规范：类型、声明、表达式、语句、模式、运算符、可见性、注释 |
| `type-system.md` | 类型系统：推断、泛型、记录、变体、协议、类型别名 |
| `effect-system.md` | 效应：fn vs effect fn、显式传播（仅 `!`——ADR-0008）、fan、权限、E006/E007/E008 |
| `codegen.md` | 代码生成：Nanopass pipeline、模板、Rust/WASM 输出、模块命名 |
| `cli.md` | CLI：全部命令、选项、错误码、Legacy Mode |
| `module-system.md` | 模块：import、子模块、菱形依赖、可见性、@extern |
| `edit-locality.md` | 编辑局部性：L1–L3 不变量、L4–L5 声明框架与组合、施行映射、反例搜索（2026-08-15）、附加条件、功能 gate |
| `package-system.md` | 包：依赖管理、MVS、版本共存、模块边界 |
| `result-option-effect.md` | Result/Option 和 `!` 的分情境规则、lambda 边界 |
| `effect-fn-call-semantics.md` | 效应调用与显式传播 |
| `scoped.md` | scoped 借用与验证范围 |
| `perceus.md` | RC 优化模型、实现、证据的边界 |
| `als/` | ALS 规范描述和已知一致性注记 |
