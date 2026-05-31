<!-- description: Native build-speed — precompiled almide_rt runtime rlib, and recovering the shipping-build inlining gap with #[inline] -->
# Build Speed: Runtime rlib + Hot-Fn Inlining

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
