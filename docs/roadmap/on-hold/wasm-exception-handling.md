<!-- description: Proposed Wasm exception lowering; upstream defaults are version- and engine-specific -->
# WASM Exception Handling

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
