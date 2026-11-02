<!-- description: Proposed Wasm exception lowering; upstream defaults are version- and engine-specific -->
# WASM Exception Handling

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](wasm-exception-handling.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-exception-handling.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-exception-handling.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-exception-handling.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


## Proposed lowering

Explore native `try_table` / `throw` / `exnref` for error propagation instead
of the current Result representation and explicit `!` handling. The original
plan's per-`?` terminology belongs to an older surface. Representation,
allocation and performance benefits require actual lowering and measurements;
this note does not claim EH has been implemented in Almide.

## Upstream prerequisite update (2026-10-31)

The blanket “Wasmtime defaults EH off” blocker is obsolete. In the source
pinned for Wasmtime 50.0.1, `Config::wasm_exceptions` defaults on except for
`Strategy::Winch`, where it defaults off. The API is gated by the `gc` build
feature and the configuration requires GC support. See the
[pinned configuration source](https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/config.rs#L1438-L1453).
This is a source/configuration observation, not an execution test.

Do not infer Spin, wasmCloud, Docker/Wasm or another embedded host's defaults
from Wasmtime alone: each host version, engine strategy, build features and
configuration must be checked. Almide's own tool/dependency pins also need
comparison before changing the shipping route.

## Resume criteria

Reassess required host coverage, error/catch semantics, cross-target fixtures
and whether a fallback route is needed. A changed upstream default alone
neither completes this feature nor authorizes adopting new language semantics.
The [upstream tracking issue](https://github.com/bytecodealliance/wasmtime/issues/3427)
is historical context, not a substitute for the pinned source and host tests.
