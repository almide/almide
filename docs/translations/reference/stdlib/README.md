# Almide Standard Library

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/stdlib/README.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


The stdlib is **self-hosted**: every function is declared in pure Almide in
`stdlib/*.almd`. The old `stdlib/defs/*.toml` pipeline was retired (the
Stdlib Declarative Unification arc) — there is no TOML, and no build-time
dispatch generation from TOML.

## Architecture Overview

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

## Adding or Changing a Function

1. **Implement in Almide** — add the function to `stdlib/<module>.almd` (or a
   new `stdlib/<module>_<part>.almd` split file for the WASM path).
2. **Wasm/self-hosted coverage** — declare a new implementation with
   `@implements("module.fn")`. `self_host_runtime()` combines declared bodies
   with its shrink-only legacy table; do not add a second hand-maintained
   name mapping. See `crates/almide-types/src/implements.rs`.
3. **Native intrinsic (only when pure Almide won't do)** — implement
   `almide_rt_*` in `runtime/rs/src/<module>.rs` and declare it in the module
   body with `@intrinsic("almide_rt_*")`.
4. **New module** — update `STDLIB_MODULES`, `BUNDLED_MODULES`,
   `bundled_source()` (and `AUTO_IMPORT_BUNDLED` for auto-imported modules) in
   `crates/almide-types/src/stdlib_info.rs`.
5. **Test** — add a `spec/stdlib/*_test.almd` (or inline `test` block). The interpreter evaluates self-hosted bodies over its supported floor
   automatically. Add interpreter glue only where that floor cannot serve the
   function, and verify the 3-way oracle does not silently lose coverage — see
   `crates/almide-interp/CLAUDE.md`.

## Auto-Import

Auto-imported modules are the union of two authoritative lists:

- the seed list in `crates/almide-frontend/src/import_table.rs`
- `AUTO_IMPORT_BUNDLED` in `crates/almide-types/src/stdlib_info.rs`

These lists, rather than a copied module inventory, define auto-imports.
Modules outside their union require an explicit `import`. The generated
[stdlib reference](../../../stdlib) lists the available public modules.

## Documentation

- Language-facing reference: [docs/CHEATSHEET.md](../../../CHEATSHEET.md)
- Per-module notes: [docs/stdlib/](../../../stdlib)
- Cross-target semantics manifest (CI-checked, do not edit by hand):
  `docs/stdlib/semantics-manifest.toml`
- Module interface for any module: `almide compile <module> --json`
