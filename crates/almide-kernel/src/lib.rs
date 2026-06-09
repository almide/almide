//! # almide-kernel
//!
//! Verified SIMD numeric kernels for Almide. Each kernel is a Rust port of a
//! Wyve-proven kernel (Racket/Lean is the research lab: it designs and proves
//! bitwise-exactness; this crate is the factory: the production Rust SIMD impl).
//! Differential tests pin each impl bitwise-exact to a naive reference — the
//! same reference Wyve proves against — so the proof carries to the port.
//!
//! No dependencies, no prelude injection: builds and tests standalone, then
//! `almide_rt` calls it for data-movement ops (transpose, shuffles) where the
//! explicit SIMD wins even on native.

pub mod bridge;
pub mod gelu;
pub mod matmul;
pub mod q1_0;
pub mod q1_0_packed;
pub mod scale;
pub mod silu;
pub mod softmax;
pub mod transpose;
pub mod transpose_f64;
