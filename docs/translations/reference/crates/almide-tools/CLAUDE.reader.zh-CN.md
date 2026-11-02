[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 仓库代理指示的非规范、固定原文的人类读者副本。权威 CLAUDE.md 不变。此 CLAUDE.reader.md 系列刻意不使用会自动加载指示的文件名。这些是历史原文指示，不是新的指示层，也不是独立验证的当前实现或安全保证；不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-tools/CLAUDE.md)

# almide-tools

面向开发者的工具：格式化器、模块接口提取和 `.almdi` 产物。

<a id="formatter-fmtrs"></a>

## 格式化器（`fmt.rs`）

- `format_program(program) -> String`——AST → 格式化源码。
- 保留注释，根据表达式复杂度判断采用短或长格式。
- `auto_imports()`——添加缺少的 stdlib import，移除未使用的 import。

<a id="module-interface-interfacers"></a>

## 模块接口（`interface.rs`）

- `extract_module_interface(ir_program) -> ModuleInterface`——生成 JSON API 描述。
- 包含类型签名、文档、弃用标志和 C FFI 的 ABI 布局。
- 导出工具用它生成 pip/npm/gem 包。

<a id="module-artifacts-almdirs"></a>

## 模块产物（`almdi.rs`）

- `.almdi` 序列化：公开接口加上已做类型检查的 IR。
- language server 由根 CLI 的 `src/cli/lsp*.rs` 实现。

<a id="rules"></a>

## 规则

- **格式化器必须幂等。** 始终满足 `format(format(x)) == format(x)`。
- **interface JSON 就是契约。** 外部工具会解析它，破坏性更改必须提升版本。
- **ABI 布局必须匹配 codegen。** `AbiLayout` 中字段的 offset 必须与 `almide-codegen` 实际发射的一致。若 codegen 改变记录布局，应更新接口提取。
