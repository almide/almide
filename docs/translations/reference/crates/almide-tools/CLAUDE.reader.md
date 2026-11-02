[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative, source-pinned human reader copy of repository agent instructions. The authoritative CLAUDE.md remains unchanged. This CLAUDE.reader.md family deliberately does not use an auto-loaded instruction filename. These are historical source instructions, not a new instruction layer or independently verified current implementation/security guarantees; no automatic synchronization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-tools/CLAUDE.md)

# almide-tools

Developer-facing tools: formatter, module interface extraction, and `.almdi` artifacts.

## Formatter (`fmt.rs`)

- `format_program(program) -> String` — AST → formatted source.
- Preserves comments, infers short/long formatting by expression complexity.
- `auto_imports()` — Add missing stdlib imports, remove unused.

## Module Interface (`interface.rs`)

- `extract_module_interface(ir_program) -> ModuleInterface` — Produces JSON API description.
- Includes type signatures, docs, deprecation flags, ABI layout for C FFI.
- Used by export tooling to generate pip/npm/gem packages.

## Module Artifacts (`almdi.rs`)

- `.almdi` serialization: public interface plus type-checked IR.
- The language server is implemented by the root CLI in `src/cli/lsp*.rs`.

## Rules

- **Formatter must be idempotent.** `format(format(x)) == format(x)` always.
- **Interface JSON is the contract.** External tools parse this — breaking changes require version bumps.
- **ABI layout must match codegen.** Field offsets in `AbiLayout` must agree with what `almide-codegen` actually emits. If codegen changes record layout, update interface extraction.
