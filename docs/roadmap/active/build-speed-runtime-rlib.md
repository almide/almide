<!-- description: Runtime rlib build path: current fat-LTO release linking and the historical inlining experiment -->
# Build Speed: Runtime rlib + Hot-Fn Inlining

## Current implementation (2026-10-31)

The release fast path now links with **fat LTO and one codegen unit**;
the runtime rlib itself is built with LTO off and four codegen units.
`src/cli/cargo_build.rs` records the October 9 experiment: adding `#[inline]`
to wrappers alone would not recover the observed non-LTO gap. The cache key
includes source, toolchain identity, opt level, CPU and codegen flags, under
`build_dir::cache_base()` rather than a fixed public temporary path.

Cross builds, matrix use, runtime-crate dependencies, native dependencies and
a source root bypass the run/build fast path. See
[cargo_build.rs](../../../src/cli/cargo_build.rs) for the actual predicates.
The timings and no-LTO/inline plan below describe the earlier implementation;
they are retained as measurement and design history, not a current prescription.
Native-dependency support in the rlib fast path and prebuilt distribution remain separate proposals.

### The v1 render's runtime crate (owner ruling 2026-11-02)

A program the v1 trust spine renders (#4006) carries no runtime boundary
marker, so it takes the cargo path, not the rlib path. Its runtime part — the
shared string buffer (`almide-str`) and the runtime bodies the render takes
along (`almide_mir::native_floor`) — is rendered as one module,
`almide_v1_rt`, between two marker lines. The build
(`cargo_toml::split_runtime_crate`) compiles that module as a crate of its
own, and the manifest pins its profile under
`[profile.dev.package.almide_v1_rt]`:

- `debug-assertions = false`, as v0's runtime rlib is built (`rustc` without
  `-C debug-assertions`);
- `overflow-checks = false`, pinned explicitly.

On the cargo route the program's own code keeps `[profile.dev]` unchanged,
its debug assertions included, as v0's cargo route does.

On the bare-rustc route (`v1_rustc_build`, the one a dependency-free render
takes) the program crate compiles as v0's rlib fast path compiles its program
(owner ruling 2026-11-09): opt-level 1, no debug assertions (rustc's default
above opt-level 0, which v0 relies on), overflow checks off. The runtime
generics the program crate instantiates are compiled there, so debug
assertions on it cost `map_index_order` 1.069x the standard build; with them
off it read 1.020x (probe run 42500091710). A unit test resolves both routes'
flags to their effective settings and holds them equal.

Why: through cargo's dev profile the spliced runtime ran std's `ub_checks`
inside every runtime loop. A per-codepoint string scan read 1.29x the standard
build's instructions with them on and 1.05x with them off.

What still holds:

- The runtime's `unsafe` is judged by Miri and ASan on the single-file render,
  as before (`scripts/miri-asan-slice.sh`).
- A fan program keeps one crate, because the fan runtime's output macros and
  exit routing must reach the runtime's items.

`tests/native_runtime_profile_test.rs` checks this on a real build's project:

- both profile lines are present, and overflow wraps in the program and
  through the runtime;
- a `debug_assert!` planted in the program crate does not fire, as in v0's
  build;
- nor does the same assertion planted in the runtime crate.

## Earlier implementation and inlining proposal

> Goal: Go-like build speed. The dev/test/run loop and shipping builds should not pay to recompile the runtime every time.

## Status

**Shipped (branch `build-speed-parallel-rustc`):** the `almide_rt` runtime is
compiled once into an `.rlib` (`codegen::emit_runtime_crate()`), cached under
`$TMPDIR/almide-rtlib-<hash>` keyed by runtime-source + rustc-version + opt-level.
Per-file/per-build rustc emits a slim main (`#[macro_use] extern crate almide_rt;
use almide_rt::*;` + user code, split on `RT_BOUNDARY_MARKER`) and links the rlib
with `--extern almide_rt=<rlib>`. Falls back to the inline/cargo path on any
failure; `ALMIDE_NO_RTLIB=1` disables it.

Wired into:
- **Test path** (`cargo_build_test_with_native`): rlib at opt1 + slim main at **opt0** (tests don't need optimized user code) → **2.45x/file**, spec/lang 116 files **2.26x wall / 3.1x CPU**.
- **Run/build path** (`cargo_build_generated_with_native`): rlib + slim main at the binary's opt-level (1 debug, 3 release). Shipping `--release` builds **~20x** (27–33s → 1–2s), output verified identical.

Excluded: `http`/`zlib`/`sse` (non-std deps) stay on the cargo path.

## The remaining gap (this roadmap item): shipping-build inlining

Because the runtime is a separate crate compiled **without LTO**, non-generic,
non-`#[inline]` runtime fns are **not inlined across the crate boundary**. On a
string/list-heavy hot loop (measured: 200k-row CSV through `csv-to-json`) the
shipped binary runs **~10% slower** than the monolithic build (typical programs:
2–5%). Generic runtime fns (most `list`/`map`/`option`/`result` ops) and the
existing `#[inline(always)]` helpers are unaffected — they inline cross-crate
already.

Rejected alternative — **ThinLTO**: recovers the inlining but re-optimizes the
runtime's bitcode at link time, erasing the build-speed win (≈ monolithic build
time). Measured: no compile win, so not worth it.

### Plan: `#[inline]` the hot non-generic runtime fns

Mark the small set of hot, non-generic runtime fns — primarily `string.*`
(`trim`, `split`, `lines`, `concat`, `len`, slicing) and any frequently-called
scalar helpers — with `#[inline]` in `runtime/rs/src/*.rs`. With their MIR
exposed in the rlib, rustc inlines them across the crate boundary even without
LTO, recovering the lost performance while keeping the ~20x build win and the
"separate runtime crate, no LTO" model. This mirrors Go's "small functions are
auto-inlined" stance.

Steps:
1. Profile a runtime-heavy workload (e.g. the 200k-CSV `csv-to-json` benchmark) to identify which runtime fns dominate the rlib-vs-monolithic gap.
2. Add `#[inline]` to those fns (prefer targeted `#[inline]` over blanket `#[inline(always)]` to avoid bloating caller compile time).
3. Re-measure: confirm shipping runtime regression → ~0% while build stays ~20x.
4. Guard against over-inlining: watch that the slim-main compile time doesn't creep back up (more inline candidates = more work for the final rustc).

### Open follow-ups
- Wire the rlib into `almide build` for projects with **native deps** (currently cargo-only) — would need the rlib passed through cargo as a path dependency or `RUSTFLAGS --extern`.
- Consider shipping a prebuilt rlib with the compiler release so the first build in a fresh environment skips the one-time rlib compile.
