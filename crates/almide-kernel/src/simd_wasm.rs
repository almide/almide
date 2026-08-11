//! The crate's only raw-pointer boundary on the wasm32 SIMD path.
//!
//! `v128_load` / `v128_store` take raw pointers, so every kernel used to spell
//! its own `unsafe` block plus the offset arithmetic that justified it. These
//! four helpers take FIXED-SIZE array windows instead: a `&[f64; 2]` is exactly
//! the 16 bytes the instruction touches, so the safety argument is carried by
//! the type and no caller can get it wrong. Callers carve those windows with
//! `slice::as_chunks`, which also hands back the ragged tail for the scalar
//! remainder loop.
//!
//! Both instructions are UNALIGNED, so the natural alignment of `f32`/`f64` is
//! sufficient — no window needs to be 16-byte aligned.

use std::arch::wasm32::{v128, v128_load, v128_store};

/// The two f64 lanes of `w`, as a `v128`.
#[inline(always)]
pub(crate) fn load_f64x2(w: &[f64; 2]) -> v128 {
    // SAFETY: `w` is exactly 2 × 8 = 16 bytes, the width `v128_load` reads,
    // and the load is unaligned.
    unsafe { v128_load(w.as_ptr() as *const v128) }
}

/// Write the two f64 lanes of `v` into `w`.
#[inline(always)]
pub(crate) fn store_f64x2(w: &mut [f64; 2], v: v128) {
    // SAFETY: same 16-byte, unaligned-store argument as [`load_f64x2`].
    unsafe { v128_store(w.as_mut_ptr() as *mut v128, v) };
}

/// The four f32 lanes of `w`, as a `v128`.
#[inline(always)]
pub(crate) fn load_f32x4(w: &[f32; 4]) -> v128 {
    // SAFETY: `w` is exactly 4 × 4 = 16 bytes, the width `v128_load` reads,
    // and the load is unaligned.
    unsafe { v128_load(w.as_ptr() as *const v128) }
}

/// Write the four f32 lanes of `v` into `w`.
#[inline(always)]
#[allow(dead_code)] // used by the transpose kernel only
pub(crate) fn store_f32x4(w: &mut [f32; 4], v: v128) {
    // SAFETY: same 16-byte, unaligned-store argument as [`load_f32x4`].
    unsafe { v128_store(w.as_mut_ptr() as *mut v128, v) };
}
