[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative, source-pinned human reader copy of repository agent instructions. The authoritative CLAUDE.md remains unchanged. This CLAUDE.reader.md family deliberately does not use an auto-loaded instruction filename. These are historical source instructions, not a new instruction layer or independently verified current implementation/security guarantees; no automatic synchronization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-lang/CLAUDE.md)

# almide-lang

Facade crate. Re-exports `almide-syntax` (AST, lexer, parser) and `almide-types` (Ty, stdlib_info) as a single dependency.

## Rules

- **No logic here.** This crate only re-exports. If you need shared logic, put it in the appropriate leaf crate.
- **Downstream crates depend on this** instead of separately depending on both almide-syntax and almide-types.
