// matrix_burn extern — burn-backed Matrix with small-matrix fast path.
// AlmideMatrix is an enum: Small holds raw Vec<f64> (no burn dispatch),
// Burn wraps a Tensor (BLAS-backed for large shapes).
// Small threshold = 64×64; ops that stay Small skip burn entirely,
// which wins heavily where burn's ~1µs dispatch dominates the FLOPs.

extern crate blas_src;
use burn::tensor::{Tensor, TensorData};
use burn::backend::NdArray;

type B = NdArray<f64>;

// Small storage threshold: matrices with both dims ≤ this stay as Vec<f64>,
// bypassing burn entirely. Above this, burn Tensor (needed for large-matrix
// BLAS and higher-dim ops like MHA).
const SMALL_THRESHOLD: usize = 2048;

// Raw-loop vs cblas_dgemm crossover for Small matmul:
//   max(m,k,n) ≤ RAW_LOOP_MAX → inline Rust loop (~14 GFLOPS, beats BLAS setup)
//   else → direct cblas_dgemm (skip burn wrapper, match NumPy BLAS throughput)
const RAW_LOOP_MAX: usize = 16;

// For mul_scaled (fused alpha*A*B): at small/medium sizes the cblas alpha
// param saves an intermediate scale allocation. At large sizes alpha!=1.0
// hits a slight BLAS perf penalty, so do scale-then-mul (NumPy-style) instead.
// 512 chosen because fused wins at ≤512 and loses at ≥768 for f64.
const FUSED_ALPHA_MAX: usize = 512;

#[cfg_attr(target_os = "macos", link(name = "Accelerate", kind = "framework"))]
extern "C" {
    fn cblas_dgemm(
        layout: i32, transa: i32, transb: i32,
        m: i32, n: i32, k: i32,
        alpha: f64,
        a: *const f64, lda: i32,
        b: *const f64, ldb: i32,
        beta: f64,
        c: *mut f64, ldc: i32,
    );
    fn cblas_sgemm(
        layout: i32, transa: i32, transb: i32,
        m: i32, n: i32, k: i32,
        alpha: f32,
        a: *const f32, lda: i32,
        b: *const f32, ldb: i32,
        beta: f32,
        c: *mut f32, ldc: i32,
    );
    // vForce vectorized elementwise exp — 10-30× faster than scalar .exp()
    // on Apple Silicon. Critical for softmax throughput at seq ≥ 128.
    fn vvexpf(y: *mut f32, x: *const f32, n: *const i32);
    fn vvexp(y: *mut f64, x: *const f64, n: *const i32);
}

#[derive(Clone)]
pub enum AlmideMatrix {
    Small { rows: usize, cols: usize, data: Vec<f64> },
    SmallF32 { rows: usize, cols: usize, data: Vec<f32> },
    Burn(Tensor<B, 2>),
}

fn dev() -> <B as burn::tensor::backend::Backend>::Device { Default::default() }

fn is_small(rows: usize, cols: usize) -> bool {
    // Gen-step single-row path: the whole (1, N) activation column fits in a
    // Vec<f64> of at most ~100 KB (N up to ~12k for Qwen3 FFN), so staying on
    // the Small variant lets to_vec_f64 collapse to a data.clone() instead of
    // a Tensor → TensorData → Vec<f64> round-trip. For multi-row (prompt eval,
    // training) we still hand off to burn once either dim exceeds
    // SMALL_THRESHOLD.
    (rows <= SMALL_THRESHOLD && cols <= SMALL_THRESHOLD)
        || (rows == 1 && cols <= 16384)
}

fn mk(rows: usize, cols: usize, data: Vec<f64>) -> AlmideMatrix {
    if is_small(rows, cols) {
        AlmideMatrix::Small { rows, cols, data }
    } else {
        AlmideMatrix::Burn(Tensor::from_data(TensorData::new(data, [rows, cols]), &dev()))
    }
}

fn wrap(t: Tensor<B, 2>) -> AlmideMatrix { AlmideMatrix::Burn(t) }

// ── Fused-matmul helpers ────────────────────────────────────────────
//
// The fused runtime fns below (fused_gemm_bias_scale_gelu,
// attention_weights, scaled_dot_product_attention, linear_row_gelu,
// pre_norm_linear, swiglu_gate) all share the same skeleton:
//
//   1. Build a fresh output buffer, optionally seeded from `bias` / a
//      previous result for `β != 0` GEMM use.
//   2. Call `cblas_{d,s}gemm` with a particular `α / β / trans` combo.
//   3. Sweep the result once to apply an element-wise post-op (gelu,
//      softmax, silu⊙up, bias-scale).
//
// These helpers pull step 1+2 out so each fused fn only has to declare
// its GEMM config and write its post loop. The helper signatures are
// intentionally close to what a future `@fused_runtime` attribute
// schema would serialize — one `GemmConfig` entry per fused call, post
// loop in the runtime DSL. When that arc lands the translation from
// attribute metadata to these structs is direct.
//
// NB: both helpers take the `C` buffer ownership path (Uninit / Copy /
// BroadcastRow) explicitly because `β=0` wants an uninitialized buffer
// for BLAS-owned writes while `β=1` needs a seeded one; hiding that in
// the helper would force unnecessary zero-initialization on the hot
// path.

/// BLAS transpose convention, i32-compatible with CBLAS enum values.
pub const CBLAS_NO_TRANS: i32 = 111;
pub const CBLAS_TRANS: i32 = 112;

/// How to initialize the output buffer `C` before `cblas_*gemm`.
pub enum CSeed<'a, T> {
    /// Uninit. Use with `β = 0` so every element is written by GEMM.
    Uninit,
    /// Copy `src` into C. Use when C participates in the result
    /// (`β != 0`), e.g. residual add as part of GEMM.
    Copy(&'a [T]),
    /// Broadcast `row` (length = n) to every output row. Use for
    /// bias-seeded GEMM with `β = 1`: the GEMM's `β·C` contributes the
    /// bias row to every output row without a separate add pass.
    BroadcastRow(&'a [T]),
}

/// GEMM config, f64 variant. `ldc` is always `n` (row-major tight).
pub struct GemmF64<'a> {
    pub m: usize,
    pub k: usize,
    pub n: usize,
    pub alpha: f64,
    pub beta: f64,
    pub trans_a: i32,
    pub trans_b: i32,
    pub a: &'a [f64],
    pub lda: i32,
    pub b: &'a [f64],
    pub ldb: i32,
    pub c_seed: CSeed<'a, f64>,
}

/// Run a single `cblas_dgemm` according to `cfg` and return the result
/// buffer. Safety: the caller guarantees `a.len() >= (trans_a ? k*m : m*k)`,
/// `b.len() >= (trans_b ? n*k : k*n)`, and (when seeded) the seed slice
/// lengths match `m*n` / `n`.
pub fn run_gemm_f64(cfg: GemmF64<'_>) -> Vec<f64> {
    let mut c = match cfg.c_seed {
        CSeed::Uninit => {
            let mut v = Vec::with_capacity(cfg.m * cfg.n);
            unsafe { v.set_len(cfg.m * cfg.n); }
            v
        }
        CSeed::Copy(src) => src.to_vec(),
        CSeed::BroadcastRow(row) => {
            let mut v = Vec::with_capacity(cfg.m * cfg.n);
            for _ in 0..cfg.m { v.extend_from_slice(row); }
            v
        }
    };
    unsafe {
        cblas_dgemm(
            101,
            cfg.trans_a, cfg.trans_b,
            cfg.m as i32, cfg.n as i32, cfg.k as i32,
            cfg.alpha,
            cfg.a.as_ptr(), cfg.lda,
            cfg.b.as_ptr(), cfg.ldb,
            cfg.beta,
            c.as_mut_ptr(), cfg.n as i32,
        );
    }
    c
}

/// GEMM config, f32 variant (mirror of `GemmF64`).
pub struct GemmF32<'a> {
    pub m: usize,
    pub k: usize,
    pub n: usize,
    pub alpha: f32,
    pub beta: f32,
    pub trans_a: i32,
    pub trans_b: i32,
    pub a: &'a [f32],
    pub lda: i32,
    pub b: &'a [f32],
    pub ldb: i32,
    pub c_seed: CSeed<'a, f32>,
}

pub fn run_gemm_f32(cfg: GemmF32<'_>) -> Vec<f32> {
    let mut c = match cfg.c_seed {
        CSeed::Uninit => {
            let mut v = Vec::with_capacity(cfg.m * cfg.n);
            unsafe { v.set_len(cfg.m * cfg.n); }
            v
        }
        CSeed::Copy(src) => src.to_vec(),
        CSeed::BroadcastRow(row) => {
            let mut v = Vec::with_capacity(cfg.m * cfg.n);
            for _ in 0..cfg.m { v.extend_from_slice(row); }
            v
        }
    };
    unsafe {
        cblas_sgemm(
            101,
            cfg.trans_a, cfg.trans_b,
            cfg.m as i32, cfg.n as i32, cfg.k as i32,
            cfg.alpha,
            cfg.a.as_ptr(), cfg.lda,
            cfg.b.as_ptr(), cfg.ldb,
            cfg.beta,
            c.as_mut_ptr(), cfg.n as i32,
        );
    }
    c
}

/// In-place row-wise stable softmax over an `m × n` row-major buffer.
/// Uses vForce's `vvexp` for vectorised exponential — critical for
/// softmax throughput at seq ≥ 128.
pub fn softmax_rows_inplace_f64(buf: &mut [f64], m: usize, n: usize) {
    let n_i32 = n as i32;
    let dst = buf.as_mut_ptr();
    unsafe {
        for i in 0..m {
            let base = i * n;
            let mut max = f64::NEG_INFINITY;
            for j in 0..n {
                let x = *dst.add(base + j);
                if x > max { max = x; }
            }
            for j in 0..n { *dst.add(base + j) -= max; }
            vvexp(dst.add(base), dst.add(base), &n_i32);
            let mut sum = 0.0f64;
            for j in 0..n { sum += *dst.add(base + j); }
            let inv = 1.0 / sum;
            for j in 0..n { *dst.add(base + j) *= inv; }
        }
    }
}

pub fn softmax_rows_inplace_f32(buf: &mut [f32], m: usize, n: usize) {
    let n_i32 = n as i32;
    for i in 0..m {
        let row = &mut buf[i * n..(i + 1) * n];
        let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        for v in row.iter_mut() { *v -= max; }
        unsafe { vvexpf(row.as_mut_ptr(), row.as_ptr(), &n_i32); }
        let sum: f32 = row.iter().sum();
        let inv = 1.0f32 / sum;
        for v in row.iter_mut() { *v *= inv; }
    }
}

/// Tanh-based GELU approximation: `0.5·x·(1 + tanh(√(2/π)·(x + 0.044715·x³)))`.
/// Applied in place, one coefficient choice for both f64 and f32.
pub fn gelu_inplace_f64(buf: &mut [f64]) {
    const K: f64 = 0.7978845608028654;
    for v in buf.iter_mut() {
        let x = *v;
        let x3 = x * x * x;
        let inner = K * (x + 0.044715 * x3);
        *v = 0.5 * x * (1.0 + inner.tanh());
    }
}

pub fn gelu_inplace_f32(buf: &mut [f32]) {
    const K: f32 = 0.7978845608028654;
    for v in buf.iter_mut() {
        let x = *v;
        let x3 = x * x * x;
        let inner = K * (x + 0.044715 * x3);
        *v = 0.5 * x * (1.0 + inner.tanh());
    }
}

impl AlmideMatrix {
    fn dims2(&self) -> [usize; 2] {
        match self {
            AlmideMatrix::Small { rows, cols, .. } => [*rows, *cols],
            AlmideMatrix::SmallF32 { rows, cols, .. } => [*rows, *cols],
            AlmideMatrix::Burn(t) => t.dims(),
        }
    }
    fn to_burn(&self) -> Tensor<B, 2> {
        match self {
            AlmideMatrix::Small { rows, cols, data } =>
                Tensor::from_data(TensorData::new(data.clone(), [*rows, *cols]), &dev()),
            AlmideMatrix::SmallF32 { rows, cols, data } => {
                let d64: Vec<f64> = data.iter().map(|&x| x as f64).collect();
                Tensor::from_data(TensorData::new(d64, [*rows, *cols]), &dev())
            }
            AlmideMatrix::Burn(t) => t.clone(),
        }
    }
    fn to_vec_f64(&self) -> Vec<f64> {
        match self {
            AlmideMatrix::Small { data, .. } => data.clone(),
            AlmideMatrix::SmallF32 { data, .. } => data.iter().map(|&x| x as f64).collect(),
            AlmideMatrix::Burn(t) => t.clone().to_data().to_vec().unwrap(),
        }
    }
}

pub fn almide_rt_matrix_zeros(rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    if is_small(r, c) {
        AlmideMatrix::Small { rows: r, cols: c, data: vec![0.0; r * c] }
    } else {
        wrap(Tensor::zeros([r, c], &dev()))
    }
}

pub fn almide_rt_matrix_ones(rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    if is_small(r, c) {
        AlmideMatrix::Small { rows: r, cols: c, data: vec![1.0; r * c] }
    } else {
        wrap(Tensor::ones([r, c], &dev()))
    }
}

pub fn almide_rt_matrix_shape(m: &AlmideMatrix) -> (i64, i64) {
    let [r, c] = m.dims2();
    (r as i64, c as i64)
}

pub fn almide_rt_matrix_rows(m: &AlmideMatrix) -> i64 { m.dims2()[0] as i64 }
pub fn almide_rt_matrix_cols(m: &AlmideMatrix) -> i64 { m.dims2()[1] as i64 }

pub fn almide_rt_matrix_get(m: &AlmideMatrix, row: i64, col: i64) -> f64 {
    let r = row as usize;
    let c = col as usize;
    match m {
        AlmideMatrix::Small { cols, data, .. } => data[r * cols + c],
        AlmideMatrix::SmallF32 { cols, data, .. } => data[r * cols + c] as f64,
        AlmideMatrix::Burn(t) => {
            let s = t.clone().slice([r..r + 1, c..c + 1]);
            s.into_scalar() as f64
        }
    }
}

pub fn almide_rt_matrix_transpose(m: &AlmideMatrix) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out = vec![0.0f64; r * c];
            for i in 0..r {
                for j in 0..c {
                    out[j * r + i] = data[i * c + j];
                }
            }
            AlmideMatrix::Small { rows: c, cols: r, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out = vec![0.0f32; r * c];
            for i in 0..r {
                for j in 0..c {
                    out[j * r + i] = data[i * c + j];
                }
            }
            AlmideMatrix::SmallF32 { rows: c, cols: r, data: out }
        }
        AlmideMatrix::Burn(t) => wrap(t.clone().transpose()),
    }
}

pub fn almide_rt_matrix_from_lists(rows: &Vec<Vec<f64>>) -> AlmideMatrix {
    if rows.is_empty() {
        return AlmideMatrix::Small { rows: 0, cols: 0, data: vec![] };
    }
    let nrows = rows.len();
    let ncols = rows[0].len();
    let flat: Vec<f64> = rows.iter().flat_map(|r| r.iter().copied()).collect();
    mk(nrows, ncols, flat)
}

pub fn almide_rt_matrix_from_bytes_f32_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 4;
    let mut flat: Vec<f64> = Vec::with_capacity(r * c);
    if off + need > data.len() {
        flat.resize(r * c, 0.0);
    } else {
        let bytes = &data[off..off + need];
        let mut p = 0;
        for _ in 0..(r * c) {
            let v = f32::from_le_bytes([bytes[p], bytes[p+1], bytes[p+2], bytes[p+3]]);
            flat.push(v as f64);
            p += 4;
        }
    }
    mk(r, c, flat)
}

pub fn almide_rt_matrix_from_bytes_f16_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 2;
    let mut flat: Vec<f64> = Vec::with_capacity(r * c);
    if off + need > data.len() {
        flat.resize(r * c, 0.0);
    } else {
        let bytes = &data[off..off + need];
        let mut p = 0;
        for _ in 0..(r * c) {
            let bits = u16::from_le_bytes([bytes[p], bytes[p+1]]);
            let sign = ((bits >> 15) & 0x1) as u32;
            let exp = ((bits >> 10) & 0x1f) as u32;
            let mant = (bits & 0x3ff) as u32;
            let f32_bits: u32 = if exp == 0 {
                if mant == 0 {
                    sign << 31
                } else {
                    let mut e: i32 = -14;
                    let mut m = mant;
                    while (m & 0x400) == 0 { m <<= 1; e -= 1; }
                    m &= 0x3ff;
                    (sign << 31) | (((e + 127) as u32) << 23) | (m << 13)
                }
            } else if exp == 0x1f {
                (sign << 31) | (0xff << 23) | (mant << 13)
            } else {
                (sign << 31) | (((exp + 112) as u32) << 23) | (mant << 13)
            };
            flat.push(f32::from_bits(f32_bits) as f64);
            p += 2;
        }
    }
    mk(r, c, flat)
}

pub fn almide_rt_matrix_to_lists(m: &AlmideMatrix) -> Vec<Vec<f64>> {
    let [r, c] = m.dims2();
    let flat = m.to_vec_f64();
    (0..r).map(|i| flat[i * c..(i + 1) * c].to_vec()).collect()
}

pub fn almide_rt_matrix_add(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    match (a, b) {
        (AlmideMatrix::Small { rows, cols, data: ad },
         AlmideMatrix::Small { data: bd, .. }) => {
            let n = ad.len();
            let mut out: Vec<f64> = Vec::with_capacity(n);
            unsafe {
                let (s1, s2, d) = (ad.as_ptr(), bd.as_ptr(), out.as_mut_ptr());
                for i in 0..n { *d.add(i) = *s1.add(i) + *s2.add(i); }
                out.set_len(n);
            }
            AlmideMatrix::Small { rows: *rows, cols: *cols, data: out }
        }
        (AlmideMatrix::SmallF32 { rows, cols, data: ad },
         AlmideMatrix::SmallF32 { data: bd, .. }) => {
            let n = ad.len();
            let mut out: Vec<f32> = Vec::with_capacity(n);
            unsafe {
                let (s1, s2, d) = (ad.as_ptr(), bd.as_ptr(), out.as_mut_ptr());
                for i in 0..n { *d.add(i) = *s1.add(i) + *s2.add(i); }
                out.set_len(n);
            }
            AlmideMatrix::SmallF32 { rows: *rows, cols: *cols, data: out }
        }
        _ => wrap(a.to_burn().add(b.to_burn())),
    }
}

// Fused multiply-add: a*ka + b*kb in one pass with one allocation.
// Falls back to scale+add via burn for non-Small cases.
pub fn almide_rt_matrix_fma(a: &AlmideMatrix, ka: f64, b: &AlmideMatrix, kb: f64) -> AlmideMatrix {
    match (a, b) {
        (AlmideMatrix::Small { rows, cols, data: ad },
         AlmideMatrix::Small { data: bd, .. }) => {
            let n = ad.len();
            let mut out: Vec<f64> = Vec::with_capacity(n);
            unsafe {
                let (s1, s2, d) = (ad.as_ptr(), bd.as_ptr(), out.as_mut_ptr());
                for i in 0..n { *d.add(i) = *s1.add(i) * ka + *s2.add(i) * kb; }
                out.set_len(n);
            }
            AlmideMatrix::Small { rows: *rows, cols: *cols, data: out }
        }
        (AlmideMatrix::SmallF32 { rows, cols, data: ad },
         AlmideMatrix::SmallF32 { data: bd, .. }) => {
            let n = ad.len();
            let ka32 = ka as f32;
            let kb32 = kb as f32;
            let mut out: Vec<f32> = Vec::with_capacity(n);
            unsafe {
                let (s1, s2, d) = (ad.as_ptr(), bd.as_ptr(), out.as_mut_ptr());
                for i in 0..n { *d.add(i) = *s1.add(i) * ka32 + *s2.add(i) * kb32; }
                out.set_len(n);
            }
            AlmideMatrix::SmallF32 { rows: *rows, cols: *cols, data: out }
        }
        _ => almide_rt_matrix_add(
            &almide_rt_matrix_scale(a, ka),
            &almide_rt_matrix_scale(b, kb),
        ),
    }
}

// Three-term fused multiply-add: a*ka + b*kb + c*kc in one pass.
// Target of the MatrixFusionPass tree-fuse rule.
pub fn almide_rt_matrix_fma3(
    a: &AlmideMatrix, ka: f64,
    b: &AlmideMatrix, kb: f64,
    c: &AlmideMatrix, kc: f64,
) -> AlmideMatrix {
    match (a, b, c) {
        (AlmideMatrix::Small { rows, cols, data: ad },
         AlmideMatrix::Small { data: bd, .. },
         AlmideMatrix::Small { data: cd, .. }) => {
            let n = ad.len();
            let mut out: Vec<f64> = Vec::with_capacity(n);
            unsafe {
                let (s1, s2, s3, d) = (ad.as_ptr(), bd.as_ptr(), cd.as_ptr(), out.as_mut_ptr());
                for i in 0..n { *d.add(i) = *s1.add(i) * ka + *s2.add(i) * kb + *s3.add(i) * kc; }
                out.set_len(n);
            }
            AlmideMatrix::Small { rows: *rows, cols: *cols, data: out }
        }
        (AlmideMatrix::SmallF32 { rows, cols, data: ad },
         AlmideMatrix::SmallF32 { data: bd, .. },
         AlmideMatrix::SmallF32 { data: cd, .. }) => {
            let n = ad.len();
            let (ka32, kb32, kc32) = (ka as f32, kb as f32, kc as f32);
            let mut out: Vec<f32> = Vec::with_capacity(n);
            unsafe {
                let (s1, s2, s3, d) = (ad.as_ptr(), bd.as_ptr(), cd.as_ptr(), out.as_mut_ptr());
                for i in 0..n { *d.add(i) = *s1.add(i) * ka32 + *s2.add(i) * kb32 + *s3.add(i) * kc32; }
                out.set_len(n);
            }
            AlmideMatrix::SmallF32 { rows: *rows, cols: *cols, data: out }
        }
        _ => almide_rt_matrix_add(
            &almide_rt_matrix_fma(a, ka, b, kb),
            &almide_rt_matrix_scale(c, kc),
        ),
    }
}

pub fn almide_rt_matrix_sub(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    match (a, b) {
        (AlmideMatrix::Small { rows, cols, data: ad },
         AlmideMatrix::Small { data: bd, .. }) => {
            let n = ad.len();
            let mut out: Vec<f64> = Vec::with_capacity(n);
            unsafe {
                let (s1, s2, d) = (ad.as_ptr(), bd.as_ptr(), out.as_mut_ptr());
                for i in 0..n { *d.add(i) = *s1.add(i) - *s2.add(i); }
                out.set_len(n);
            }
            AlmideMatrix::Small { rows: *rows, cols: *cols, data: out }
        }
        (AlmideMatrix::SmallF32 { rows, cols, data: ad },
         AlmideMatrix::SmallF32 { data: bd, .. }) => {
            let n = ad.len();
            let mut out: Vec<f32> = Vec::with_capacity(n);
            unsafe {
                let (s1, s2, d) = (ad.as_ptr(), bd.as_ptr(), out.as_mut_ptr());
                for i in 0..n { *d.add(i) = *s1.add(i) - *s2.add(i); }
                out.set_len(n);
            }
            AlmideMatrix::SmallF32 { rows: *rows, cols: *cols, data: out }
        }
        _ => wrap(a.to_burn().sub(b.to_burn())),
    }
}

pub fn almide_rt_matrix_mul(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    match (a, b) {
        (AlmideMatrix::Small { rows: m, cols: k, data: ad },
         AlmideMatrix::Small { rows: _, cols: n, data: bd }) => {
            let (m, k, n) = (*m, *k, *n);
            if m.max(k).max(n) <= RAW_LOOP_MAX {
                // Tiny path: raw ikj loop, dispatch-free (beats NumPy @ 3-16²).
                let mut out = vec![0.0f64; m * n];
                for i in 0..m {
                    let a_row = &ad[i * k..(i + 1) * k];
                    let out_row = &mut out[i * n..(i + 1) * n];
                    for p in 0..k {
                        let aip = a_row[p];
                        let b_row = &bd[p * n..(p + 1) * n];
                        for (o, &b) in out_row.iter_mut().zip(b_row.iter()) {
                            *o = aip.mul_add(b, *o);
                        }
                    }
                }
                mk(m, n, out)
            } else {
                // Uninit buffer: cblas_dgemm with beta=0 writes every element.
                // At 1024² this saves ~1ms of zero-init (8MB write).
                let mut out: Vec<f64> = Vec::with_capacity(m * n);
                unsafe {
                    cblas_dgemm(
                        101, 111, 111,
                        m as i32, n as i32, k as i32,
                        1.0,
                        ad.as_ptr(), k as i32,
                        bd.as_ptr(), n as i32,
                        0.0,
                        out.as_mut_ptr(), n as i32,
                    );
                    out.set_len(m * n);
                }
                mk(m, n, out)
            }
        }
        _ => wrap(a.to_burn().matmul(b.to_burn())),
    }
}

pub fn almide_rt_matrix_scale(m: &AlmideMatrix, s: f64) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            let n = data.len();
            // Uninit + unrolled writes: saves an 8MB zero-init on large sizes.
            let mut out: Vec<f64> = Vec::with_capacity(n);
            let chunks = n - n % 4;
            unsafe {
                let src = data.as_ptr();
                let dst = out.as_mut_ptr();
                let mut j = 0;
                while j < chunks {
                    *dst.add(j)     = *src.add(j)     * s;
                    *dst.add(j + 1) = *src.add(j + 1) * s;
                    *dst.add(j + 2) = *src.add(j + 2) * s;
                    *dst.add(j + 3) = *src.add(j + 3) * s;
                    j += 4;
                }
                while j < n { *dst.add(j) = *src.add(j) * s; j += 1; }
                out.set_len(n);
            }
            AlmideMatrix::Small { rows: *rows, cols: *cols, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            let sf = s as f32;
            let n = data.len();
            let mut out = vec![0.0f32; n];
            // 4-wide unroll for NEON/AVX auto-vectorization
            let chunks = n - n % 4;
            let mut j = 0;
            while j < chunks {
                out[j] = data[j] * sf;
                out[j + 1] = data[j + 1] * sf;
                out[j + 2] = data[j + 2] * sf;
                out[j + 3] = data[j + 3] * sf;
                j += 4;
            }
            while j < n { out[j] = data[j] * sf; j += 1; }
            AlmideMatrix::SmallF32 { rows: *rows, cols: *cols, data: out }
        }
        AlmideMatrix::Burn(t) => wrap(t.clone().mul_scalar(s)),
    }
}
include!("matrix_burn_p2.rs");
include!("matrix_burn_p3.rs");
include!("matrix_burn_p4.rs");
