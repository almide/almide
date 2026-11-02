# Almide 标准库

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



> 原文：[stdlib/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/stdlib/README.md)。这是供读者参考的非规范性快照译文，不会自动同步。未对测量值及主张进行独立复核，以原文为准。

stdlib 是**自托管**的：所有函数均在 `stdlib/*.almd` 中使用纯 Almide 声明。旧的 `stdlib/defs/*.toml` 流水线已经退役（Stdlib Declarative Unification arc）；不再使用 TOML，也不再在构建时从 TOML 生成分派。

## 架构概览

```
stdlib/<module>.almd            Module body: signatures + @intrinsic decls +
                                @inline_rust templates + pure-Almide impls
stdlib/<module>_<part>.almd     Split files: pure-Almide implementations used
                                by the WASM self-hosting path
        │
        ├─▶ crates/almide-types/src/stdlib_info.rs
        │     bundled_source(): include_str! of module bodies.
        │     The frontend extracts FnSigs; codegen extracts @inline_rust.
        │
        ├─▶ crates/almide-types/src/self_host_registry.rs
        │     self_host_runtime(): (SRC_<STEM>, [(impl_fn, "module.fn")]) registrations:
        │     pure-Almide impls linked into IR and emitted by almide-wasm.
        │     An unlinked stdlib call is a wall (hard error) in the renderer.
        │
        └─▶ runtime/rs/src/<module>.rs
              Hand-written almide_rt_* natives, targeted by
              @intrinsic("almide_rt_*") declarations (native target only).
```

## 添加或修改函数

1. **用 Almide 实现** — 将函数添加到 `stdlib/<module>.almd`（或为 WASM 路径新增 `stdlib/<module>_<part>.almd` 拆分文件）。
2. **Wasm/自托管覆盖** — 用 `@implements("module.fn")` 声明新实现。`self_host_runtime()` 将声明的函数体与只减不增的旧表合并；不要再添加一份手工维护的名称映射。参见 `crates/almide-types/src/implements.rs`。
3. **Native intrinsic（仅在纯 Almide 不够用时）** — 在 `runtime/rs/src/<module>.rs` 中实现 `almide_rt_*`，并在模块体中用 `@intrinsic("almide_rt_*")` 声明它。
4. **新模块** — 更新 `crates/almide-types/src/stdlib_info.rs` 中的 `STDLIB_MODULES`、`BUNDLED_MODULES`、`bundled_source()`（自动导入模块还需更新 `AUTO_IMPORT_BUNDLED`）。
5. **测试** — 添加 `spec/stdlib/*_test.almd`（或内联 `test` 块）。解释器会在其支持范围内自动求值自托管函数体。仅在该范围无法承载函数时添加解释器衔接代码，并确认 3-way oracle 不会悄悄丢失覆盖；参见 `crates/almide-interp/CLAUDE.md`。

## 自动导入

自动导入模块是两个权威列表的并集：

- `crates/almide-frontend/src/import_table.rs` 中的 seed 列表
- `crates/almide-types/src/stdlib_info.rs` 中的 `AUTO_IMPORT_BUNDLED`

定义自动导入的是这些列表，而不是复制的模块清单。并集以外的模块需要显式 `import`。生成的 [stdlib 参考](../../../stdlib) 列出了可用的公开模块。

## 文档

- 面向语言使用者的参考：[docs/CHEATSHEET.md](../../../CHEATSHEET.md)
- 各模块说明：[docs/stdlib/](../../../stdlib)
- 跨目标语义清单（CI 检查，请勿手动编辑）：`docs/stdlib/semantics-manifest.toml`
- 任意模块的接口：`almide compile <module> --json`
