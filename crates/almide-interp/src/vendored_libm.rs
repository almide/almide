// The oracle's transcendental floor: the SAME vendored musl-libm source the
// native runtime compiles (`runtime/rs/src/libm.rs` + its p2/p3/p4 parts,
// libm 0.2.16 / FreeBSD msun / Sun fdlibm), included at SOURCE level.
//
// Why include! and not a copy or a crate dep (both were the recorded reasons
// the interp abstained on transcendentals before this): a copy drifts silently
// the moment the runtime's file changes, and depending on the `almide_rt`
// CRATE would drag its TLS/runtime machinery into this deliberately lean
// evaluator. Including the one file gives the third oracle the bit-identical
// algorithm both backends run — the same "one source, both sides" discipline
// the scalar-read audit's shared enumeration script uses (#1176's lesson).
//
// The wasm leg mirrors this file function-for-function in the self-hosted
// `stdlib/math_{trig,exp,log}.almd`, and the cross-target byte gate holds them
// together; so an interp answer computed HERE is the consensus answer, not a
// platform answer, and the 3-way oracle regains its third vote on every
// fixture that touches sin/cos/tan/exp/log/log2/log10/pow/atan/tanh/expm1.
//
// Only names the vendored file actually provides are bridged. `asin`/`acos`/
// `atan2` are NOT here: the runtime's own `almide_rt_math_{asin,acos,atan2}`
// still delegate to the PLATFORM libm, so they have no stable oracle — they
// are also unreachable from Almide today (no `@intrinsic` in stdlib/math.almd),
// and the interp keeps abstaining on those names.
//
// The include reads the in-crate snapshot `src/generated/libm/` (#3361):
// `runtime/rs/` is outside this package, so a vendored almide-interp could not
// reach it. `build.rs` refreshes the snapshot from `runtime/rs/src` on every
// build inside a checkout, and `scripts/check-build-inputs-in-package.sh`
// fails a commit whose snapshot differs — so it is still one source, not a
// copy that can drift.
#![allow(dead_code, clippy::all)]

include!("generated/libm/libm.rs");
