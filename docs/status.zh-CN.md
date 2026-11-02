# 项目现状

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./status.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](./status.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](./status.zh-CN.md)

这里介绍 Almide 目前的状态。带有记录时点的汇总值（标准库规模、测试文件、契约、变异测试分数）显示在[测量页面](./benchmarks/README.md#derived-counts)上，CI 会将它们与各自的台账核对。

| 类别 | 状态 |
|----------|--------|
| 成熟度 | 尚未达到 1.0，正在 `develop` 上积极开发；面向 LLM 的表层规范只能通过带有迁移诊断的 dialect epoch 变更，遵循 [STABILITY.md](./STABILITY.md)（于 2026-08-20 宣布） |
| 支持 | 1.0 之前仅支持最新发布系列。政策和版本管理保证：[SUPPORT.md](../SUPPORT.zh-CN.md) · 漏洞：[SECURITY.md](../SECURITY.zh-CN.md) |
| 编译器 | 纯 Rust 编译器；发行包还附带独立的 `almide-verify` 证书检查器 |
| 目标 | Rust（原生）、WASM（直接输出的 structural leg，见[工作原理](./how-it-works.zh-CN.md)） |
| 带验证的代码生成 | structural wasm 路径会为每次构建输出所有权、边界、名称和能力的验证证据（witness）；`almide verify` 检查通过 SHA-256 绑定到交付 wasm 文件的包（#4011）。这并不证明语义正确性或整个产物的内存安全：覆盖范围限制和剩余的信任假设明确列于[已证明与依赖信任的部分](./contracts/proven-vs-trusted.md)和[内存安全缺口表](./design/MEMORY-SAFETY.md#the-gap-table-2026-10-24-4011)。旧 MIR→WAT 渲染器已在 #2761 中退役 |
| 代码生成 | Rust：Nanopass + TOML 模板；wasm：structural engine → 直接输出（v0 发射器和旧 MIR→WAT 渲染器均已退役；遇到不支持的边界就报错，绝不回退） |
| 标准库检查 | 标准库的每个公开函数都在 [`proofs/stdlib-inspection.toml`](../proofs/stdlib-inspection.toml) 中带有 `inspected` 标记：原生与 wasm 的字节相同、每次调用成本不超过声明的复杂度阶、codopsy A、新一轮模糊测试组合，以及经过菱形模块依赖的调用。截至 2026-10-19（almide 0.68.0），未检查数为 0；`tests/stdlib_inspection_test.rs` 会在签名或函数体变化时使标记失效，未检查数量只允许下降 |
| 产物 | 通过 `almide compile` 生成的 `.almdi` 模块接口文件 |
| Playground | [在线体验](https://almide.github.io/playground/) — 编译器以 WASM 形式在浏览器中运行 |

发布历史：[CHANGELOG.md](../CHANGELOG.md) · 计划：[路线图](./roadmap/README.md)。
