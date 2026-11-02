<a id="tiered-test-suite"></a>

# 分层测试套件

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-test.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-test.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](almide-test.reader.zh-CN.md)

> [英文命令正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-test.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照。这份不激活的参考资料不是代理命令，不授予执行其中指令的权限，也不自动同步。翻译时未执行以下命令；不声称进行了独立测量或当前验证。

按层级顺序运行完整的 Almide 测试套件，在首次失败的层级停止。

<a id="steps"></a>

## 步骤

1. **cargo test** — Rust 编译器单元测试
   ```bash
   cargo test
   ```
   任一测试失败即停止。

2. **lang 测试** — 语言功能测试（默认测试路由）
   ```bash
   almide test spec/lang/
   ```
   任一测试失败即停止。

3. **stdlib 测试** — 标准库测试（默认测试路由）
   ```bash
   almide test spec/stdlib/
   ```
   任一测试失败即停止。

4. **integration 测试** — 多模块／集成测试（默认测试路由）
   ```bash
   almide test spec/integration/
   ```
   任一测试失败即停止。

5. **WASM 测试** — 使用 WASM 目标的所有测试
   ```bash
   almide test --target wasm
   ```
   报告通过／失败／跳过的结果。

6. **摘要** — 报告各层级的结果：
   - cargo test: 通过／失败
   - lang: N 个文件通过
   - stdlib: N 个文件通过
   - integration: N 个文件通过
   - WASM: N 项通过、N 项失败、N 项跳过

<a id="notes"></a>

## 说明

- 如果修改了编译器源代码，运行前务必执行 `make install`
- 如果某一层级失败，先诊断并修复，再继续下一层级
- 根据报告的操作和 `proofs/target-availability.toml` 对 Wasm 的壁／跳过项进行分类；不能把 fs/io 整体视为不支持的类别。默认测试路由遇到 Wasm 的壁时可能回退到 native；隔离某个后端进行排查时，请使用显式目标选项。
