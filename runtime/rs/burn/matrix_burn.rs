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
    rows <= SMALL_THRESHOLD && cols <= SMALL_THRESHOLD
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

pub fn almide_rt_matrix_map(m: &AlmideMatrix, f: impl Fn(f64) -> f64) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            let out: Vec<f64> = data.iter().map(|&x| f(x)).collect();
            AlmideMatrix::Small { rows: *rows, cols: *cols, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            let out: Vec<f32> = data.iter().map(|&x| f(x as f64) as f32).collect();
            AlmideMatrix::SmallF32 { rows: *rows, cols: *cols, data: out }
        }
        AlmideMatrix::Burn(t) => {
            let [r, c] = t.dims();
            let data: Vec<f64> = t.clone().to_data().to_vec().unwrap();
            let mapped: Vec<f64> = data.into_iter().map(|x| f(x)).collect();
            wrap(Tensor::from_data(TensorData::new(mapped, [r, c]), &dev()))
        }
    }
}

pub fn almide_rt_matrix_broadcast_add_row(m: &AlmideMatrix, bias: &[f64]) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out: Vec<f64> = Vec::with_capacity(r * c);
            unsafe {
                let (src, dst, bp) = (data.as_ptr(), out.as_mut_ptr(), bias.as_ptr());
                for i in 0..r {
                    let base = i * c;
                    for j in 0..c {
                        *dst.add(base + j) = *src.add(base + j) + *bp.add(j);
                    }
                }
                out.set_len(r * c);
            }
            AlmideMatrix::Small { rows: r, cols: c, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out: Vec<f32> = Vec::with_capacity(r * c);
            unsafe {
                let (src, dst, bp) = (data.as_ptr(), out.as_mut_ptr(), bias.as_ptr());
                for i in 0..r {
                    let base = i * c;
                    for j in 0..c {
                        *dst.add(base + j) = *src.add(base + j) + (*bp.add(j) as f32);
                    }
                }
                out.set_len(r * c);
            }
            AlmideMatrix::SmallF32 { rows: r, cols: c, data: out }
        }
        AlmideMatrix::Burn(_) => {
            let bias_t: Tensor<B, 2> = Tensor::from_data(
                TensorData::new(bias.to_vec(), [1, bias.len()]), &dev());
            wrap(m.to_burn().add(bias_t))
        }
    }
}

pub fn almide_rt_matrix_layer_norm_rows(m: &AlmideMatrix, gamma: &[f64], beta: &[f64], eps: f64) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out: Vec<f64> = Vec::with_capacity(r * c);
            unsafe {
                let src = data.as_ptr();
                let dst = out.as_mut_ptr();
                for i in 0..r {
                    let base = i * c;
                    // Two-pass: mean, then variance. More stable than
                    // E[X²] - E[X]² (which cancels catastrophically).
                    let mut sum = 0.0f64;
                    for j in 0..c { sum += *src.add(base + j); }
                    let mean = sum / c as f64;
                    let mut var = 0.0f64;
                    for j in 0..c {
                        let d = *src.add(base + j) - mean;
                        var += d * d;
                    }
                    let inv_std = (var / c as f64 + eps).sqrt().recip();
                    for j in 0..c {
                        let x = *src.add(base + j);
                        *dst.add(base + j) = (x - mean) * inv_std * gamma[j] + beta[j];
                    }
                }
                out.set_len(r * c);
            }
            AlmideMatrix::Small { rows: r, cols: c, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out: Vec<f32> = vec![0.0f32; r * c];
            let gamma_f: Vec<f32> = gamma.iter().map(|&x| x as f32).collect();
            let beta_f: Vec<f32> = beta.iter().map(|&x| x as f32).collect();
            for i in 0..r {
                let row = &data[i * c..(i + 1) * c];
                // Slice-based iter: LLVM auto-vectorizes sum/dot to f32x4.
                let sum: f32 = row.iter().sum();
                let mean = sum / c as f32;
                let var: f32 = row.iter().map(|&x| { let d = x - mean; d * d }).sum::<f32>() / c as f32;
                let inv_std = 1.0 / (var + eps as f32).sqrt();
                let o = &mut out[i * c..(i + 1) * c];
                for j in 0..c {
                    o[j] = (row[j] - mean) * inv_std * gamma_f[j] + beta_f[j];
                }
            }
            AlmideMatrix::SmallF32 { rows: r, cols: c, data: out }
        }
        AlmideMatrix::Burn(_) => {
            let m_t = m.to_burn();
            let [_r, c] = m_t.dims();
            let mean = m_t.clone().mean_dim(1);
            let centered = m_t.sub(mean);
            let var = centered.clone().powf_scalar(2.0).mean_dim(1);
            let inv_std = var.add_scalar(eps).sqrt().recip();
            let normed = centered.mul(inv_std);
            let gamma_t: Tensor<B, 2> = Tensor::from_data(TensorData::new(gamma.to_vec(), [1, c]), &dev());
            let beta_t: Tensor<B, 2> = Tensor::from_data(TensorData::new(beta.to_vec(), [1, c]), &dev());
            wrap(normed.mul(gamma_t).add(beta_t))
        }
    }
}

pub fn almide_rt_matrix_softmax_rows(m: &AlmideMatrix) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out: Vec<f64> = vec![0.0; r * c];
            let c_i32 = c as i32;
            unsafe {
                let src = data.as_ptr();
                let dst = out.as_mut_ptr();
                for i in 0..r {
                    let base = i * c;
                    // Stable softmax: subtract max, vec-exp, normalize.
                    let mut max = f64::NEG_INFINITY;
                    for j in 0..c {
                        let v = *src.add(base + j);
                        if v > max { max = v; }
                    }
                    // Write (x - max) into dst, then vvexp in place.
                    for j in 0..c { *dst.add(base + j) = *src.add(base + j) - max; }
                    vvexp(dst.add(base), dst.add(base), &c_i32);
                    let mut sum = 0.0f64;
                    for j in 0..c { sum += *dst.add(base + j); }
                    let inv = 1.0 / sum;
                    for j in 0..c { *dst.add(base + j) *= inv; }
                }
            }
            AlmideMatrix::Small { rows: r, cols: c, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out: Vec<f32> = vec![0.0f32; r * c];
            let c_i32 = c as i32;
            for i in 0..r {
                let src_row = &data[i * c..(i + 1) * c];
                let dst_row = &mut out[i * c..(i + 1) * c];
                // Slice iter => LLVM vectorizes max/sum to f32x4 reductions.
                let max = src_row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                for (d, &s) in dst_row.iter_mut().zip(src_row.iter()) {
                    *d = s - max;
                }
                unsafe { vvexpf(dst_row.as_mut_ptr(), dst_row.as_ptr(), &c_i32); }
                let sum: f32 = dst_row.iter().sum();
                let inv = 1.0 / sum;
                for d in dst_row.iter_mut() { *d *= inv; }
            }
            AlmideMatrix::SmallF32 { rows: r, cols: c, data: out }
        }
        AlmideMatrix::Burn(_) => wrap(burn::tensor::activation::softmax(m.to_burn(), 1)),
    }
}

pub fn almide_rt_matrix_gelu(m: &AlmideMatrix) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            // Tanh-based GELU approximation: 0.5·x·(1 + tanh(√(2/π)·(x + 0.044715·x³)))
            const K: f64 = 0.7978845608028654;
            let n = data.len();
            let mut out: Vec<f64> = Vec::with_capacity(n);
            unsafe {
                let src = data.as_ptr();
                let dst = out.as_mut_ptr();
                for i in 0..n {
                    let x = *src.add(i);
                    let inner = K * (x + 0.044715 * x * x * x);
                    *dst.add(i) = 0.5 * x * (1.0 + inner.tanh());
                }
                out.set_len(n);
            }
            AlmideMatrix::Small { rows: *rows, cols: *cols, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            const K: f32 = 0.7978845608028654;
            let n = data.len();
            let mut out: Vec<f32> = Vec::with_capacity(n);
            unsafe {
                let src = data.as_ptr();
                let dst = out.as_mut_ptr();
                for i in 0..n {
                    let x = *src.add(i);
                    let inner = K * (x + 0.044715 * x * x * x);
                    *dst.add(i) = 0.5 * x * (1.0 + inner.tanh());
                }
                out.set_len(n);
            }
            AlmideMatrix::SmallF32 { rows: *rows, cols: *cols, data: out }
        }
        AlmideMatrix::Burn(_) => wrap(burn::tensor::activation::gelu(m.to_burn())),
    }
}

/// Fused: `gelu(alpha * (a @ b + bias))` in one pass.
///
/// `gemm` handles `alpha*A*B + beta*C` natively — seeding `C = bias` and
/// `beta = alpha` folds the `add` and `scale` stages into the BLAS call
/// itself. Only the GELU pass remains, run in-place on the output buffer.
///
/// Three intermediate allocations (mul → add → scale → gelu) collapse to
/// one. At 512² f64 the chain drops from ~978 µs to roughly the raw mul
/// time (~97 µs) — NumPy stays at ~3.4 ms for the same composition,
/// giving ≳30× structural advantage on fused chains.
pub fn almide_rt_matrix_fused_gemm_bias_scale_gelu(
    a: &AlmideMatrix,
    b: &AlmideMatrix,
    bias: &AlmideMatrix,
    alpha: f64,
) -> AlmideMatrix {
    match (a, b, bias) {
        (AlmideMatrix::Small { rows: m, cols: k, data: ad },
         AlmideMatrix::Small { rows: _, cols: n, data: bd },
         AlmideMatrix::Small { rows: br, cols: bc, data: biasd })
            if *br == *m && *bc == *n && bd.len() == *k * *n =>
        {
            let (m, k, n) = (*m, *k, *n);
            let mut c = run_gemm_f64(GemmF64 {
                m, k, n,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_NO_TRANS,
                a: ad, lda: k as i32,
                b: bd, ldb: n as i32,
                c_seed: CSeed::Uninit,
            });
            // Post: `v = alpha * (out + bias); out = gelu(v)` in a
            // single sweep. Alpha stays scalar and fuses with the bias
            // add; no intermediate buffer.
            const K: f64 = 0.7978845608028654;
            for (out, &bi) in c.iter_mut().zip(biasd.iter()) {
                let v = alpha * (*out + bi);
                let v3 = v * v * v;
                *out = 0.5 * v * (1.0 + (K * (v + 0.044715 * v3)).tanh());
            }
            AlmideMatrix::Small { rows: m, cols: n, data: c }
        }
        (AlmideMatrix::SmallF32 { rows: m, cols: k, data: ad },
         AlmideMatrix::SmallF32 { rows: _, cols: n, data: bd },
         AlmideMatrix::SmallF32 { rows: br, cols: bc, data: biasd })
            if *br == *m && *bc == *n && bd.len() == *k * *n =>
        {
            let (m, k, n) = (*m, *k, *n);
            let alpha_f = alpha as f32;
            let mut c = run_gemm_f32(GemmF32 {
                m, k, n,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_NO_TRANS,
                a: ad, lda: k as i32,
                b: bd, ldb: n as i32,
                c_seed: CSeed::Uninit,
            });
            const K: f32 = 0.7978845608028654;
            for (out, &bi) in c.iter_mut().zip(biasd.iter()) {
                let v = alpha_f * (*out + bi);
                let v3 = v * v * v;
                *out = 0.5 * v * (1.0 + (K * (v + 0.044715 * v3)).tanh());
            }
            AlmideMatrix::SmallF32 { rows: m, cols: n, data: c }
        }
        _ => {
            let mul = almide_rt_matrix_mul(a, b);
            let added = almide_rt_matrix_add(&mul, bias);
            let scaled = almide_rt_matrix_scale(&added, alpha);
            almide_rt_matrix_gelu(&scaled)
        }
    }
}

/// Fused attention weights: `softmax_rows(scale * (Q @ Kt))` in one pass.
///
/// The canonical scaled-dot-product-attention numerator. Implemented as:
/// 1. `cblas_dgemm(alpha=scale, Q, Kt, beta=0, C)` — one BLAS call.
/// 2. In-place row softmax with Accelerate `vvexp`.
///
/// Replaces the 3-op chain `softmax_rows(scale(mul(Q, Kt), s))` that
/// otherwise allocates two intermediate matrices and loops over them
/// separately. At seq=128 the fused path is ~2-3× faster than the
/// unfused chain; at seq=512 the chain allocates ~2 MiB per call that
/// this path skips entirely.
pub fn almide_rt_matrix_attention_weights(
    q: &AlmideMatrix,
    kt: &AlmideMatrix,
    scale: f64,
) -> AlmideMatrix {
    match (q, kt) {
        (AlmideMatrix::Small { rows: m, cols: k, data: qd },
         AlmideMatrix::Small { rows: _, cols: n, data: kd })
            if kd.len() == *k * *n =>
        {
            let (m, k, n) = (*m, *k, *n);
            let mut c = run_gemm_f64(GemmF64 {
                m, k, n,
                alpha: scale, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_NO_TRANS,
                a: qd, lda: k as i32,
                b: kd, ldb: n as i32,
                c_seed: CSeed::Uninit,
            });
            softmax_rows_inplace_f64(&mut c, m, n);
            AlmideMatrix::Small { rows: m, cols: n, data: c }
        }
        (AlmideMatrix::SmallF32 { rows: m, cols: k, data: qd },
         AlmideMatrix::SmallF32 { rows: _, cols: n, data: kd })
            if kd.len() == *k * *n =>
        {
            let (m, k, n) = (*m, *k, *n);
            let mut c = run_gemm_f32(GemmF32 {
                m, k, n,
                alpha: scale as f32, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_NO_TRANS,
                a: qd, lda: k as i32,
                b: kd, ldb: n as i32,
                c_seed: CSeed::Uninit,
            });
            softmax_rows_inplace_f32(&mut c, m, n);
            AlmideMatrix::SmallF32 { rows: m, cols: n, data: c }
        }
        _ => {
            let prod = almide_rt_matrix_mul(q, kt);
            let scaled = almide_rt_matrix_scale(&prod, scale);
            almide_rt_matrix_softmax_rows(&scaled)
        }
    }
}

/// SwiGLU FFN gate: `silu(x @ W_gate) ⊙ (x @ W_up)`.
///
/// silu(z) = z * sigmoid(z) = z / (1 + exp(-z)). Used in Llama / Mistral
/// FFN where the gate path drives an element-wise mul with the up
/// projection. Fused implementation: two cblas_dgemm(transB=Trans) into
/// separate buffers, then a single sweep applies silu to the gate
/// buffer and multiplies in the up buffer in place.
///
/// Shapes: x (r, d_model), W_gate (d_inner, d_model), W_up (d_inner, d_model)
///         → out (r, d_inner)
pub fn almide_rt_matrix_swiglu_gate(
    x: &AlmideMatrix,
    w_gate: &AlmideMatrix,
    w_up: &AlmideMatrix,
) -> AlmideMatrix {
    match (x, w_gate, w_up) {
        (AlmideMatrix::Small { rows: r, cols: d_in, data: xd },
         AlmideMatrix::Small { rows: d_out, cols: _, data: wgd },
         AlmideMatrix::Small { rows: d_out2, cols: _, data: wud })
            if *d_out == *d_out2
                && wgd.len() == *d_out * *d_in
                && wud.len() == *d_out * *d_in =>
        {
            let (r, d_in, d_out) = (*r, *d_in, *d_out);
            // Same x, different W: two independent GEMMs with transB=Trans
            // (W stored as (d_out, d_in), we want x @ W^T).
            let mut gate = run_gemm_f64(GemmF64 {
                m: r, k: d_in, n: d_out,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_TRANS,
                a: xd, lda: d_in as i32,
                b: wgd, ldb: d_in as i32,
                c_seed: CSeed::Uninit,
            });
            let up = run_gemm_f64(GemmF64 {
                m: r, k: d_in, n: d_out,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_TRANS,
                a: xd, lda: d_in as i32,
                b: wud, ldb: d_in as i32,
                c_seed: CSeed::Uninit,
            });
            // Post: silu(gate) ⊙ up, in place on `gate`.
            for (g, &u) in gate.iter_mut().zip(up.iter()) {
                let z = *g;
                let sig = 1.0 / (1.0 + (-z).exp());
                *g = z * sig * u;
            }
            AlmideMatrix::Small { rows: r, cols: d_out, data: gate }
        }
        (AlmideMatrix::SmallF32 { rows: r, cols: d_in, data: xd },
         AlmideMatrix::SmallF32 { rows: d_out, cols: _, data: wgd },
         AlmideMatrix::SmallF32 { rows: d_out2, cols: _, data: wud })
            if *d_out == *d_out2
                && wgd.len() == *d_out * *d_in
                && wud.len() == *d_out * *d_in =>
        {
            let (r, d_in, d_out) = (*r, *d_in, *d_out);
            let mut gate = run_gemm_f32(GemmF32 {
                m: r, k: d_in, n: d_out,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_TRANS,
                a: xd, lda: d_in as i32,
                b: wgd, ldb: d_in as i32,
                c_seed: CSeed::Uninit,
            });
            let up = run_gemm_f32(GemmF32 {
                m: r, k: d_in, n: d_out,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_TRANS,
                a: xd, lda: d_in as i32,
                b: wud, ldb: d_in as i32,
                c_seed: CSeed::Uninit,
            });
            for (g, &u) in gate.iter_mut().zip(up.iter()) {
                let z = *g;
                let sig = 1.0f32 / (1.0f32 + (-z).exp());
                *g = z * sig * u;
            }
            AlmideMatrix::SmallF32 { rows: r, cols: d_out, data: gate }
        }
        _ => {
            // Generic fallback: explicit chain via existing primitives.
            let g = almide_rt_matrix_linear_row_no_bias(x, w_gate);
            let u = almide_rt_matrix_linear_row_no_bias(x, w_up);
            // silu(g) ⊙ u — done element-wise with a couple intermediates.
            let g_silu = match &g {
                AlmideMatrix::Small { rows, cols, data } => {
                    let out: Vec<f64> = data.iter()
                        .map(|&z| z / (1.0 + (-z).exp()))
                        .collect();
                    AlmideMatrix::Small { rows: *rows, cols: *cols, data: out }
                }
                AlmideMatrix::SmallF32 { rows, cols, data } => {
                    let out: Vec<f32> = data.iter()
                        .map(|&z| z / (1.0 + (-z).exp()))
                        .collect();
                    AlmideMatrix::SmallF32 { rows: *rows, cols: *cols, data: out }
                }
                AlmideMatrix::Burn(t) => {
                    let z = t.clone();
                    let sig = z.clone().neg().exp().add_scalar(1.0).powf_scalar(-1.0);
                    wrap(z.mul(sig))
                }
            };
            // Element-wise mul. Implemented via burn for the generic path —
            // small Vec paths above never reach here.
            wrap(g_silu.to_burn().mul(u.to_burn()))
        }
    }
}

/// Root-Mean-Square Normalization (Llama / Mistral style):
///   y[i, j] = x[i, j] * gamma[j] / sqrt(mean(x[i, :]²) + eps)
///
/// Cheaper than full LayerNorm — no mean-subtraction, no beta. Modern
/// LLMs use it because the bias term and centering rarely help and can
/// hurt large-batch training. Per-row implementation mirrors
/// `layer_norm_rows` for consistency on the Small path.
pub fn almide_rt_matrix_rms_norm_rows(
    m: &AlmideMatrix,
    gamma: &[f64],
    eps: f64,
) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out: Vec<f64> = Vec::with_capacity(r * c);
            unsafe {
                let src = data.as_ptr();
                let dst = out.as_mut_ptr();
                for i in 0..r {
                    let base = i * c;
                    let mut sq = 0.0f64;
                    for j in 0..c {
                        let x = *src.add(base + j);
                        sq += x * x;
                    }
                    let inv_rms = (sq / c as f64 + eps).sqrt().recip();
                    for j in 0..c {
                        *dst.add(base + j) = *src.add(base + j) * inv_rms * gamma[j];
                    }
                }
                out.set_len(r * c);
            }
            AlmideMatrix::Small { rows: r, cols: c, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let gamma_f: Vec<f32> = gamma.iter().map(|&x| x as f32).collect();
            let eps_f = eps as f32;
            let mut out: Vec<f32> = vec![0.0f32; r * c];
            for i in 0..r {
                let row = &data[i * c..(i + 1) * c];
                let sq: f32 = row.iter().map(|&x| x * x).sum();
                let inv_rms = 1.0f32 / (sq / c as f32 + eps_f).sqrt();
                let o = &mut out[i * c..(i + 1) * c];
                for j in 0..c {
                    o[j] = row[j] * inv_rms * gamma_f[j];
                }
            }
            AlmideMatrix::SmallF32 { rows: r, cols: c, data: out }
        }
        AlmideMatrix::Burn(_) => {
            // Fallback: compute via the burn primitives. Llama-scale tensors
            // typically run on the Small path so this branch is rarely hit.
            let t = m.to_burn();
            let [_r, c_dim] = t.dims();
            let sq = t.clone().powf_scalar(2.0).mean_dim(1);
            let inv_rms = sq
                .add_scalar(eps)
                .sqrt()
                .powf_scalar(-1.0);
            let scaled = t.mul(inv_rms);
            let gamma_t: Tensor<B, 2> = Tensor::from_data(
                TensorData::new(gamma.to_vec(), [1, c_dim]),
                &dev(),
            );
            wrap(scaled.mul(gamma_t))
        }
    }
}

/// Scaled dot-product attention: `(softmax_rows(scale · Q @ Kt)) @ V`.
///
/// Two BLAS calls + one in-place row softmax. Compared to the unfused
/// chain (mul + scale + softmax + mul), this skips two intermediate
/// matrix allocations (the post-scale buffer and the explicit weights
/// matrix wrap), and the softmax pass runs over the seq×seq buffer
/// without an extra clone.
///
/// Shape contract:
///   Q  : (seq_q, d_head)
///   Kt : (d_head, seq_k)   — K already transposed
///   V  : (seq_k, d_v)      — typically d_v == d_head
///   out: (seq_q, d_v)
pub fn almide_rt_matrix_scaled_dot_product_attention(
    q: &AlmideMatrix,
    kt: &AlmideMatrix,
    v: &AlmideMatrix,
    scale: f64,
) -> AlmideMatrix {
    match (q, kt, v) {
        (AlmideMatrix::Small { rows: sq, cols: dq, data: qd },
         AlmideMatrix::Small { rows: _, cols: sk, data: kd },
         AlmideMatrix::Small { rows: _, cols: dv, data: vd })
            if kd.len() == *dq * *sk && vd.len() == *sk * *dv =>
        {
            let (sq, dq, sk, dv) = (*sq, *dq, *sk, *dv);
            // GEMM 1: scores = scale · Q @ K^T (shape sq × sk).
            let mut weights = run_gemm_f64(GemmF64 {
                m: sq, k: dq, n: sk,
                alpha: scale, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_NO_TRANS,
                a: qd, lda: dq as i32,
                b: kd, ldb: sk as i32,
                c_seed: CSeed::Uninit,
            });
            // In-place row softmax normalises without a second buffer.
            softmax_rows_inplace_f64(&mut weights, sq, sk);
            // GEMM 2: output = weights @ V (shape sq × dv).
            let out = run_gemm_f64(GemmF64 {
                m: sq, k: sk, n: dv,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_NO_TRANS,
                a: &weights, lda: sk as i32,
                b: vd, ldb: dv as i32,
                c_seed: CSeed::Uninit,
            });
            AlmideMatrix::Small { rows: sq, cols: dv, data: out }
        }
        (AlmideMatrix::SmallF32 { rows: sq, cols: dq, data: qd },
         AlmideMatrix::SmallF32 { rows: _, cols: sk, data: kd },
         AlmideMatrix::SmallF32 { rows: _, cols: dv, data: vd })
            if kd.len() == *dq * *sk && vd.len() == *sk * *dv =>
        {
            let (sq, dq, sk, dv) = (*sq, *dq, *sk, *dv);
            let mut weights = run_gemm_f32(GemmF32 {
                m: sq, k: dq, n: sk,
                alpha: scale as f32, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_NO_TRANS,
                a: qd, lda: dq as i32,
                b: kd, ldb: sk as i32,
                c_seed: CSeed::Uninit,
            });
            softmax_rows_inplace_f32(&mut weights, sq, sk);
            let out = run_gemm_f32(GemmF32 {
                m: sq, k: sk, n: dv,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_NO_TRANS,
                a: &weights, lda: sk as i32,
                b: vd, ldb: dv as i32,
                c_seed: CSeed::Uninit,
            });
            AlmideMatrix::SmallF32 { rows: sq, cols: dv, data: out }
        }
        _ => {
            let w = almide_rt_matrix_attention_weights(q, kt, scale);
            almide_rt_matrix_mul(&w, v)
        }
    }
}

pub fn almide_rt_matrix_split_cols_even(m: &AlmideMatrix, n: i64) -> Vec<AlmideMatrix> {
    let t = m.to_burn();
    let [r, c] = t.dims();
    let n = n as usize;
    if n == 0 { return vec![]; }
    let chunk = c / n;
    (0..n).map(|h| {
        let start = h * chunk;
        let end = start + chunk;
        wrap(t.clone().slice([0..r, start..end]))
    }).collect()
}

pub fn almide_rt_matrix_concat_cols_many(matrices: &Vec<AlmideMatrix>) -> AlmideMatrix {
    if matrices.is_empty() {
        return wrap(Tensor::zeros([0, 0], &dev()));
    }
    wrap(Tensor::cat(matrices.iter().map(|m| m.to_burn()).collect::<Vec<_>>(), 1))
}

pub fn almide_rt_matrix_causal_mask_add(m: &AlmideMatrix, mask_val: f64) -> AlmideMatrix {
    let t = m.to_burn();
    let [r, c] = t.dims();
    let mut flat = vec![0.0f64; r * c];
    for i in 0..r {
        for j in 0..c {
            if j > i { flat[i * c + j] = mask_val; }
        }
    }
    let mask: Tensor<B, 2> = Tensor::from_data(TensorData::new(flat, [r, c]), &dev());
    wrap(t.add(mask))
}

pub fn almide_rt_matrix_multi_head_attention(q: &AlmideMatrix, k: &AlmideMatrix, v: &AlmideMatrix, n_heads: i64) -> AlmideMatrix {
    almide_rt_matrix_mha_core_burn(&q.to_burn(), &k.to_burn(), &v.to_burn(), n_heads, false)
}

pub fn almide_rt_matrix_masked_multi_head_attention(q: &AlmideMatrix, k: &AlmideMatrix, v: &AlmideMatrix, n_heads: i64) -> AlmideMatrix {
    almide_rt_matrix_mha_core_burn(&q.to_burn(), &k.to_burn(), &v.to_burn(), n_heads, true)
}

fn almide_rt_matrix_mha_core_burn(q: &Tensor<B, 2>, k: &Tensor<B, 2>, v: &Tensor<B, 2>, n_heads: i64, causal: bool) -> AlmideMatrix {
    let [sq, d] = q.dims();
    let [sk, _] = k.dims();
    let h = n_heads as usize;
    let dh = d / h;
    let scale = (dh as f64).sqrt().recip();

    let q3: Tensor<B, 3> = q.clone().reshape([sq, h, dh]).swap_dims(0, 1);
    let k3: Tensor<B, 3> = k.clone().reshape([sk, h, dh]).swap_dims(0, 1);
    let v3: Tensor<B, 3> = v.clone().reshape([sk, h, dh]).swap_dims(0, 1);

    let k3t: Tensor<B, 3> = k3.swap_dims(1, 2);
    let mut scores: Tensor<B, 3> = q3.matmul(k3t).mul_scalar(scale);

    if causal {
        // KV-cache aware causal mask: Q row i maps to absolute position
        // (sk - sq) + i; attend to K rows j with j <= that position.
        let prev = sk.saturating_sub(sq);
        let mut flat = vec![0.0f64; h * sq * sk];
        for hi in 0..h {
            for i in 0..sq {
                for j in 0..sk {
                    if j > prev + i { flat[hi * sq * sk + i * sk + j] = -10000.0; }
                }
            }
        }
        let mask3: Tensor<B, 3> = Tensor::from_data(TensorData::new(flat, [h, sq, sk]), &dev());
        scores = scores.add(mask3);
    }

    let weights = burn::tensor::activation::softmax(scores, 2);
    let out3 = weights.matmul(v3);
    wrap(out3.swap_dims(0, 1).reshape([sq, d]))
}

pub fn almide_rt_matrix_linear_row(x: &AlmideMatrix, weight: &AlmideMatrix, bias: &[f64]) -> AlmideMatrix {
    let wt = weight.to_burn().swap_dims(0, 1);
    let bias_t: Tensor<B, 2> = Tensor::from_data(TensorData::new(bias.to_vec(), [1, bias.len()]), &dev());
    wrap(x.to_burn().matmul(wt).add(bias_t))
}

pub fn almide_rt_matrix_linear_row_no_bias(x: &AlmideMatrix, weight: &AlmideMatrix) -> AlmideMatrix {
    // Small fast path: cblas_dgemm(transB=Trans) directly. Without this
    // every `linear_row_no_bias` call rebuilds two burn Tensors (clone +
    // wrap) which costs ~50–100 µs per call at seq=256. Llama-style
    // blocks call this 3-5× per layer, so the Small dispatch dominates.
    match (x, weight) {
        (AlmideMatrix::Small { rows: r, cols: n_in, data: xd },
         AlmideMatrix::Small { rows: n_out, cols: _, data: wd })
            if wd.len() == *n_out * *n_in =>
        {
            let (r, n_in, n_out) = (*r, *n_in, *n_out);
            let c = run_gemm_f64(GemmF64 {
                m: r, k: n_in, n: n_out,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_TRANS,
                a: xd, lda: n_in as i32,
                b: wd, ldb: n_in as i32,
                c_seed: CSeed::Uninit,
            });
            AlmideMatrix::Small { rows: r, cols: n_out, data: c }
        }
        (AlmideMatrix::SmallF32 { rows: r, cols: n_in, data: xd },
         AlmideMatrix::SmallF32 { rows: n_out, cols: _, data: wd })
            if wd.len() == *n_out * *n_in =>
        {
            let (r, n_in, n_out) = (*r, *n_in, *n_out);
            let c = run_gemm_f32(GemmF32 {
                m: r, k: n_in, n: n_out,
                alpha: 1.0, beta: 0.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_TRANS,
                a: xd, lda: n_in as i32,
                b: wd, ldb: n_in as i32,
                c_seed: CSeed::Uninit,
            });
            AlmideMatrix::SmallF32 { rows: r, cols: n_out, data: c }
        }
        _ => {
            let wt = weight.to_burn().swap_dims(0, 1);
            wrap(x.to_burn().matmul(wt))
        }
    }
}

/// Fused: `gelu(x @ W^T + bias_row)` in one pass. Equivalent to
/// `matrix.gelu(matrix.linear_row(x, weight, bias))` but avoids the
/// intermediate (r × n_out) `linear_row` output and a second sweep
/// for GELU. Uses `cblas_dgemm(transB=Trans)` with the output seeded
/// row-wise from `bias`, then applies GELU in place on the same buffer.
///
/// For sizes above the Small threshold we fall back to the unfused
/// chain (burn matmul + add + gelu) since Burn's path has its own
/// dispatch tax that fusion can't easily short-circuit.
/// Fused: `linear_row(layer_norm_rows(x, γ, β, ε), W, b)` in one pass.
///
/// Transformer pre-norm residual block's first layer: LayerNorm then
/// linear projection. The naive chain allocates a full (r × n_in)
/// normalized buffer then feeds it to `linear_row` which round-trips
/// through burn for the matmul — this bypass does the norm inline and
/// calls cblas_dgemm directly, so the `linear_row` burn dispatch is
/// eliminated entirely on the Small path.
pub fn almide_rt_matrix_pre_norm_linear(
    x: &AlmideMatrix,
    gamma: &[f64],
    beta: &[f64],
    eps: f64,
    weight: &AlmideMatrix,
    bias: &[f64],
) -> AlmideMatrix {
    match (x, weight) {
        (AlmideMatrix::Small { rows: r, cols: n_in, data: xd },
         AlmideMatrix::Small { rows: n_out, cols: _, data: wd })
            if gamma.len() == *n_in && beta.len() == *n_in
                && bias.len() == *n_out && wd.len() == *n_out * *n_in =>
        {
            let (r, n_in, n_out) = (*r, *n_in, *n_out);
            // Build the normalised buffer in one sweep; LN is row-local
            // so no BLAS help is available here.
            let mut normalized: Vec<f64> = Vec::with_capacity(r * n_in);
            unsafe {
                normalized.set_len(r * n_in);
                let src = xd.as_ptr();
                let dst = normalized.as_mut_ptr();
                for i in 0..r {
                    let base = i * n_in;
                    let mut sum = 0.0f64;
                    for j in 0..n_in { sum += *src.add(base + j); }
                    let mean = sum / n_in as f64;
                    let mut var = 0.0f64;
                    for j in 0..n_in {
                        let d = *src.add(base + j) - mean;
                        var += d * d;
                    }
                    let inv_std = (var / n_in as f64 + eps).sqrt().recip();
                    for j in 0..n_in {
                        let x = *src.add(base + j);
                        *dst.add(base + j) = (x - mean) * inv_std * gamma[j] + beta[j];
                    }
                }
            }
            // GEMM with bias-seeded C + β=1 folds the row bias into the
            // matmul without a separate add pass.
            let c = run_gemm_f64(GemmF64 {
                m: r, k: n_in, n: n_out,
                alpha: 1.0, beta: 1.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_TRANS,
                a: &normalized, lda: n_in as i32,
                b: wd, ldb: n_in as i32,
                c_seed: CSeed::BroadcastRow(bias),
            });
            AlmideMatrix::Small { rows: r, cols: n_out, data: c }
        }
        _ => {
            let normed = almide_rt_matrix_layer_norm_rows(x, gamma, beta, eps);
            almide_rt_matrix_linear_row(&normed, weight, bias)
        }
    }
}

pub fn almide_rt_matrix_linear_row_gelu(
    x: &AlmideMatrix,
    weight: &AlmideMatrix,
    bias: &[f64],
) -> AlmideMatrix {
    match (x, weight) {
        (AlmideMatrix::Small { rows: r, cols: n_in, data: xd },
         AlmideMatrix::Small { rows: n_out, cols: _, data: wd })
            if bias.len() == *n_out && wd.len() == *n_out * *n_in =>
        {
            let (r, n_in, n_out) = (*r, *n_in, *n_out);
            let mut c = run_gemm_f64(GemmF64 {
                m: r, k: n_in, n: n_out,
                alpha: 1.0, beta: 1.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_TRANS,
                a: xd, lda: n_in as i32,
                b: wd, ldb: n_in as i32,
                c_seed: CSeed::BroadcastRow(bias),
            });
            gelu_inplace_f64(&mut c);
            AlmideMatrix::Small { rows: r, cols: n_out, data: c }
        }
        (AlmideMatrix::SmallF32 { rows: r, cols: n_in, data: xd },
         AlmideMatrix::SmallF32 { rows: n_out, cols: _, data: wd })
            if bias.len() == *n_out && wd.len() == *n_out * *n_in =>
        {
            let (r, n_in, n_out) = (*r, *n_in, *n_out);
            let bias_f: Vec<f32> = bias.iter().map(|&v| v as f32).collect();
            let mut c = run_gemm_f32(GemmF32 {
                m: r, k: n_in, n: n_out,
                alpha: 1.0, beta: 1.0,
                trans_a: CBLAS_NO_TRANS, trans_b: CBLAS_TRANS,
                a: xd, lda: n_in as i32,
                b: wd, ldb: n_in as i32,
                c_seed: CSeed::BroadcastRow(&bias_f),
            });
            gelu_inplace_f32(&mut c);
            AlmideMatrix::SmallF32 { rows: r, cols: n_out, data: c }
        }
        _ => {
            let lin = almide_rt_matrix_linear_row(x, weight, bias);
            almide_rt_matrix_gelu(&lin)
        }
    }
}

pub fn almide_rt_matrix_slice_rows(m: &AlmideMatrix, start: i64, end: i64) -> AlmideMatrix {
    let t = m.to_burn();
    let [r, _c] = t.dims();
    let s = (start as usize).min(r);
    let e = (end as usize).min(r);
    if s >= e {
        return wrap(Tensor::zeros([0, t.dims()[1]], &dev()));
    }
    wrap(t.clone().slice([s..e, 0..t.dims()[1]]))
}

pub fn almide_rt_matrix_conv1d(input: &AlmideMatrix, weight: &AlmideMatrix, bias: &[f64], kernel: i64, stride: i64, padding: i64) -> AlmideMatrix {
    let [t_in, in_ch] = input.dims2();
    let [out_ch, _] = weight.dims2();
    let k = kernel as usize;
    let s = stride as usize;
    let p = padding as usize;
    let t_padded = t_in + 2 * p;
    if t_padded < k {
        return wrap(Tensor::zeros([0, out_ch], &dev()));
    }
    let t_out = (t_padded - k) / s + 1;

    let x_flat = input.to_vec_f64();
    let w_flat = weight.to_vec_f64();
    let mut out_flat: Vec<f64> = vec![0.0; t_out * out_ch];

    for t in 0..t_out {
        let base = t * s;
        for o in 0..out_ch {
            let w_off = o * in_ch * k;
            let mut sum = bias[o];
            for c in 0..in_ch {
                let w_base = w_off + c * k;
                for ki in 0..k {
                    let tp = base + ki;
                    if tp >= p && tp < p + t_in {
                        let tc = tp - p;
                        sum += w_flat[w_base + ki] * x_flat[tc * in_ch + c];
                    }
                }
            }
            out_flat[t * out_ch + o] = sum;
        }
    }
    mk(t_out, out_ch, out_flat)
}

pub fn almide_rt_matrix_from_bytes_f64_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 8;
    let mut flat: Vec<f64> = Vec::with_capacity(r * c);
    if off + need > data.len() {
        flat.resize(r * c, 0.0);
    } else {
        let bytes = &data[off..off + need];
        let mut p = 0;
        for _ in 0..(r * c) {
            let v = f64::from_le_bytes([bytes[p], bytes[p+1], bytes[p+2], bytes[p+3], bytes[p+4], bytes[p+5], bytes[p+6], bytes[p+7]]);
            flat.push(v);
            p += 8;
        }
    }
    mk(r, c, flat)
}

pub fn almide_rt_matrix_gather_rows(m: &AlmideMatrix, indices: &Vec<i64>) -> AlmideMatrix {
    let [mr, c] = m.dims2();
    let n = indices.len();
    let mut flat: Vec<f64> = Vec::with_capacity(n * c);
    let mflat = m.to_vec_f64();
    for &idx in indices {
        let i = idx as usize;
        if i < mr {
            flat.extend_from_slice(&mflat[i * c..(i + 1) * c]);
        } else {
            flat.extend(std::iter::repeat(0.0).take(c));
        }
    }
    mk(n, c, flat)
}

pub fn almide_rt_matrix_row_dot(m: &AlmideMatrix, r: i64, vec: &Vec<f64>) -> f64 {
    let [mr, c] = m.dims2();
    let r = r as usize;
    if r >= mr { return 0.0; }
    let mflat = m.to_vec_f64();
    let mut s = 0.0;
    let n = c.min(vec.len());
    for k in 0..n { s += mflat[r * c + k] * vec[k]; }
    s
}

// ── f32 path ────────────────────────────────────────────────────────────────
// Same design as the f64 Small path, but backed by Vec<f32> and cblas_sgemm.
// Dispatched via explicit `matrix.*_f32` stdlib functions — values returned
// to Almide code are f64 (upcast) since the language has no f32 literal yet.

pub fn almide_rt_matrix_zeros_f32(rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    AlmideMatrix::SmallF32 { rows: r, cols: c, data: vec![0.0f32; r * c] }
}

pub fn almide_rt_matrix_ones_f32(rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    AlmideMatrix::SmallF32 { rows: r, cols: c, data: vec![1.0f32; r * c] }
}

/// Fused scale+mul: out = alpha * A @ B, via cblas_sgemm's alpha parameter.
/// Skips the intermediate Vec<f32> that `matrix.scale(a, alpha) |> matrix.mul_f32(_, b)`
/// would allocate — matters at 256-512² where alloc+scale cost dominates.
pub fn almide_rt_matrix_mul_f32_scaled(a: &AlmideMatrix, alpha: f64, b: &AlmideMatrix) -> AlmideMatrix {
    match (a, b) {
        (AlmideMatrix::SmallF32 { rows: m, cols: k, data: ad },
         AlmideMatrix::SmallF32 { rows: _, cols: n, data: bd }) => {
            let (m, k, n) = (*m, *k, *n);
            // In chained matmul pipelines, separate scale + mul beats
            // sgemm-with-alpha for square-ish shapes where Accelerate's
            // alpha=1 path is better tuned. Keep fusion for single-matmul
            // cases (skinny / non-square) where the alloc savings dominate.
            let square_ish = m == k && k == n;
            if square_ish && m > RAW_LOOP_MAX {
                return almide_rt_matrix_mul_f32(&almide_rt_matrix_scale(a, alpha), b);
            }
            if m.max(k).max(n) <= RAW_LOOP_MAX {
                let mut out = vec![0.0f32; m * n];
                let alpha = alpha as f32;
                for i in 0..m {
                    let a_row = &ad[i * k..(i + 1) * k];
                    let out_row = &mut out[i * n..(i + 1) * n];
                    for p in 0..k {
                        let aip = a_row[p] * alpha;
                        let b_row = &bd[p * n..(p + 1) * n];
                        for (o, &b) in out_row.iter_mut().zip(b_row.iter()) {
                            *o = aip.mul_add(b, *o);
                        }
                    }
                }
                AlmideMatrix::SmallF32 { rows: m, cols: n, data: out }
            } else {
                // Uninit buffer: cblas_sgemm with beta=0 writes every element.
                let mut out: Vec<f32> = Vec::with_capacity(m * n);
                unsafe {
                    cblas_sgemm(
                        101, 111, 111,
                        m as i32, n as i32, k as i32,
                        alpha as f32,
                        ad.as_ptr(), k as i32,
                        bd.as_ptr(), n as i32,
                        0.0,
                        out.as_mut_ptr(), n as i32,
                    );
                    out.set_len(m * n);
                }
                AlmideMatrix::SmallF32 { rows: m, cols: n, data: out }
            }
        }
        _ => almide_rt_matrix_scale(&almide_rt_matrix_mul_f32(a, b), alpha),
    }
}

/// a @ b^T without materialising a transposed copy (cblas transB).
pub fn almide_rt_matrix_mul_f32_t(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    match (a, b) {
        (AlmideMatrix::SmallF32 { rows: m, cols: k, data: ad },
         AlmideMatrix::SmallF32 { rows: n, cols: _, data: bd }) => {
            let (m, k, n) = (*m, *k, *n);
            let mut out: Vec<f32> = Vec::with_capacity(m * n);
            unsafe {
                cblas_sgemm(
                    101, 111, 112,  // RowMajor, NoTrans, Trans
                    m as i32, n as i32, k as i32,
                    1.0,
                    ad.as_ptr(), k as i32,
                    bd.as_ptr(), k as i32,  // b is (n, k) row-major
                    0.0,
                    out.as_mut_ptr(), n as i32,
                );
                out.set_len(m * n);
            }
            AlmideMatrix::SmallF32 { rows: m, cols: n, data: out }
        }
        _ => almide_rt_matrix_mul_f32(a, &almide_rt_matrix_transpose(b)),
    }
}

/// alpha * a @ b^T (scaled variant, e.g. attention scores).
pub fn almide_rt_matrix_mul_f32_t_scaled(a: &AlmideMatrix, alpha: f64, b: &AlmideMatrix) -> AlmideMatrix {
    match (a, b) {
        (AlmideMatrix::SmallF32 { rows: m, cols: k, data: ad },
         AlmideMatrix::SmallF32 { rows: n, cols: _, data: bd }) => {
            let (m, k, n) = (*m, *k, *n);
            let mut out: Vec<f32> = Vec::with_capacity(m * n);
            unsafe {
                cblas_sgemm(
                    101, 111, 112,
                    m as i32, n as i32, k as i32,
                    alpha as f32,
                    ad.as_ptr(), k as i32,
                    bd.as_ptr(), k as i32,
                    0.0,
                    out.as_mut_ptr(), n as i32,
                );
                out.set_len(m * n);
            }
            AlmideMatrix::SmallF32 { rows: m, cols: n, data: out }
        }
        _ => almide_rt_matrix_scale(&almide_rt_matrix_mul_f32_t(a, b), alpha),
    }
}

/// Fused scale+mul for f64 path (dgemm alpha).
pub fn almide_rt_matrix_mul_scaled(a: &AlmideMatrix, alpha: f64, b: &AlmideMatrix) -> AlmideMatrix {
    match (a, b) {
        (AlmideMatrix::Small { rows: m, cols: k, data: ad },
         AlmideMatrix::Small { rows: _, cols: n, data: bd }) => {
            let (m, k, n) = (*m, *k, *n);
            if m.max(k).max(n) > FUSED_ALPHA_MAX {
                // Large dgemm: alpha!=1 penalty > alloc savings. Do scale+mul.
                return almide_rt_matrix_mul(&almide_rt_matrix_scale(a, alpha), b);
            }
            if m.max(k).max(n) <= RAW_LOOP_MAX {
                let mut out = vec![0.0f64; m * n];
                for i in 0..m {
                    let a_row = &ad[i * k..(i + 1) * k];
                    let out_row = &mut out[i * n..(i + 1) * n];
                    for p in 0..k {
                        let aip = a_row[p] * alpha;
                        let b_row = &bd[p * n..(p + 1) * n];
                        for (o, &b) in out_row.iter_mut().zip(b_row.iter()) {
                            *o = aip.mul_add(b, *o);
                        }
                    }
                }
                mk(m, n, out)
            } else {
                // Skip the 0.0 init: cblas_dgemm with beta=0 writes every element.
                // At 1024² this saves ~1ms of zero-init (8MB write).
                let mut out: Vec<f64> = Vec::with_capacity(m * n);
                unsafe {
                    cblas_dgemm(
                        101, 111, 111,
                        m as i32, n as i32, k as i32,
                        alpha,
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
        _ => almide_rt_matrix_scale(&almide_rt_matrix_mul(a, b), alpha),
    }
}

pub fn almide_rt_matrix_mul_f32(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    match (a, b) {
        (AlmideMatrix::SmallF32 { rows: m, cols: k, data: ad },
         AlmideMatrix::SmallF32 { rows: _, cols: n, data: bd }) => {
            let (m, k, n) = (*m, *k, *n);
            if m.max(k).max(n) <= RAW_LOOP_MAX {
                let mut out = vec![0.0f32; m * n];
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
                AlmideMatrix::SmallF32 { rows: m, cols: n, data: out }
            } else {
                let mut out: Vec<f32> = Vec::with_capacity(m * n);
                unsafe {
                    cblas_sgemm(
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
                AlmideMatrix::SmallF32 { rows: m, cols: n, data: out }
            }
        }
        // Mixed: promote the non-f32 side (rare — only happens when user
        // mixes f64 and f32 matrices. Fall back to f64 path).
        _ => almide_rt_matrix_mul(a, b),
    }
}

// ── Q1_0 (1-bit) direct decode ──
// See comments in runtime/rs/src/matrix.rs — same implementation.
#[inline]
fn fp16_bits_to_f32(raw: u16) -> f32 {
    let sign = (raw >> 15) as u32;
    let exp = ((raw >> 10) & 0x1F) as u32;
    let mantissa = (raw & 0x3FF) as u32;
    let bits = if exp == 0 {
        if mantissa == 0 {
            sign << 31
        } else {
            let mut m = mantissa;
            let mut e = 1i32 - 15;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            let m = m & 0x3FF;
            let exp_f32 = (e + 127) as u32;
            (sign << 31) | (exp_f32 << 23) | (m << 13)
        }
    } else if exp == 31 {
        (sign << 31) | (0xFFu32 << 23) | (mantissa << 13)
    } else {
        (sign << 31) | ((exp + 112) << 23) | (mantissa << 13)
    };
    f32::from_bits(bits)
}

pub fn almide_rt_matrix_from_q1_0_bytes(
    data: &Vec<u8>,
    offset: i64,
    rows: i64,
    cols: i64,
) -> AlmideMatrix {
    let rows_u = rows.max(0) as usize;
    let cols_u = cols.max(0) as usize;
    let total = rows_u * cols_u;
    if total == 0 || data.is_empty() {
        return mk(rows_u, cols_u, vec![0.0f64; total]);
    }
    let off = offset.max(0) as usize;
    let mut flat = Vec::<f64>::with_capacity(total);
    let num_blocks = total / 128;
    for b in 0..num_blocks {
        let block_start = off + b * 18;
        let scale_raw = (data[block_start] as u16)
            | ((data[block_start + 1] as u16) << 8);
        let cur_scale = fp16_bits_to_f32(scale_raw) as f64;
        let cur_neg_scale = -cur_scale;
        let bits_start = block_start + 2;
        for i in 0..128usize {
            let byte = data[bits_start + (i >> 3)];
            let bit = (byte >> (i & 7)) & 1;
            flat.push(if bit == 1 { cur_scale } else { cur_neg_scale });
        }
    }
    mk(rows_u, cols_u, flat)
}

// ── RoPE (rotary positional embedding) ──
// Rotates (x[2i], x[2i+1]) pairs per head by `p * (1 / theta_base ^ (2i/head_dim))`
// where p is the row index. Standard transformer primitive — equal footing with
// rms_norm_rows / swiglu_gate. Adding as intrinsic keeps the 1 M-element rotation
// loop out of Almide's list/flat_map machinery which, although correct, pays
// heavy closure-allocation and list-concat costs per iteration.
pub fn almide_rt_matrix_rope_rotate(
    x: &AlmideMatrix,
    n_heads: i64,
    head_dim: i64,
    theta_base: f64,
) -> AlmideMatrix {
    almide_rt_matrix_rope_rotate_at(x, n_heads, head_dim, theta_base, 0)
}

pub fn almide_rt_matrix_rope_rotate_at(
    x: &AlmideMatrix,
    n_heads: i64,
    head_dim: i64,
    theta_base: f64,
    start_pos: i64,
) -> AlmideMatrix {
    let dims = x.dims2();
    let rows = dims[0];
    let cols = dims[1];
    let n_heads_u = n_heads.max(0) as usize;
    let head_dim_u = head_dim.max(0) as usize;
    let start = start_pos.max(0) as usize;
    let half = head_dim_u / 2;
    let mut inv_freqs = Vec::<f64>::with_capacity(half);
    for i in 0..half {
        let exp = (2 * i) as f64 / head_dim_u as f64;
        inv_freqs.push(1.0 / theta_base.powf(exp));
    }
    let flat_in = x.to_vec_f64();
    let mut flat_out = vec![0.0f64; rows * cols];
    for p in 0..rows {
        let pos_f = (start + p) as f64;
        let row_off = p * cols;
        for h in 0..n_heads_u {
            let head_start = row_off + h * head_dim_u;
            for i in 0..half {
                let j0 = head_start + 2 * i;
                let x0 = flat_in[j0];
                let x1 = flat_in[j0 + 1];
                let angle = pos_f * inv_freqs[i];
                let (s, c) = angle.sin_cos();
                flat_out[j0] = x0 * c - x1 * s;
                flat_out[j0 + 1] = x0 * s + x1 * c;
            }
        }
    }
    mk(rows, cols, flat_out)
}

pub fn almide_rt_matrix_append_rows(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    let da = a.dims2();
    let db = b.dims2();
    if da[0] == 0 { return b.clone(); }
    if db[0] == 0 { return a.clone(); }
    let cols = da[1].max(db[1]);
    let mut flat = Vec::<f64>::with_capacity((da[0] + db[0]) * cols);
    flat.extend(a.to_vec_f64());
    flat.extend(b.to_vec_f64());
    mk(da[0] + db[0], cols, flat)
}

pub fn almide_rt_matrix_select_rows(m: &AlmideMatrix, row_ids: &[i64]) -> AlmideMatrix {
    let dims = m.dims2();
    let cols = dims[1];
    let out_rows = row_ids.len();
    let mut flat = Vec::<f64>::with_capacity(out_rows * cols);
    let src = m.to_vec_f64();
    for &rid in row_ids {
        let r = rid.max(0) as usize;
        let start = r * cols;
        let end = start + cols;
        if end <= src.len() {
            flat.extend_from_slice(&src[start..end]);
        } else {
            flat.extend(std::iter::repeat(0.0f64).take(cols));
        }
    }
    mk(out_rows, cols, flat)
}

// Q1_0 direct matmul — see runtime/rs/src/matrix.rs for the detailed
// comment. The Burn variant flattens input once, runs the same loop,
// and wraps via `mk()`.
pub fn almide_rt_matrix_linear_q1_0_row_no_bias(
    x: &AlmideMatrix,
    w_bytes: &Vec<u8>,
    w_offset: i64,
    w_rows: i64,
    w_cols: i64,
) -> AlmideMatrix {
    let dims = x.dims2();
    let x_rows = dims[0];
    let n_in = w_cols.max(0) as usize;
    let out = w_rows.max(0) as usize;
    if x_rows == 0 || out == 0 || n_in == 0 {
        return mk(x_rows, out, vec![0.0f64; x_rows * out]);
    }
    let off = w_offset.max(0) as usize;
    let n_blocks_per_row = n_in / 128;
    let x_flat = x.to_vec_f64();
    let mut out_flat = vec![0.0f64; x_rows * out];
    for i in 0..x_rows {
        let x_off = i * n_in;
        for j in 0..out {
            let mut sum = 0.0f64;
            let row_off = off + j * n_blocks_per_row * 18;
            for b in 0..n_blocks_per_row {
                let block_start = row_off + b * 18;
                let scale_raw = (w_bytes[block_start] as u16)
                    | ((w_bytes[block_start + 1] as u16) << 8);
                let scale = fp16_bits_to_f32(scale_raw) as f64;
                let neg_scale = -scale;
                let bits_start = block_start + 2;
                for local_k in 0..128 {
                    let byte = w_bytes[bits_start + (local_k >> 3)];
                    let bit = (byte >> (local_k & 7)) & 1;
                    let w_val = if bit == 1 { scale } else { neg_scale };
                    sum += x_flat[x_off + b * 128 + local_k] * w_val;
                }
            }
            out_flat[i * out + j] = sum;
        }
    }
    mk(x_rows, out, out_flat)
}

pub fn almide_rt_matrix_silu_mul(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    let ad = a.dims2();
    let rows = ad[0];
    let cols = ad[1];
    let af = a.to_vec_f64();
    let bf = b.to_vec_f64();
    let mut out = vec![0.0f64; rows * cols];
    for i in 0..rows * cols {
        let x = af[i];
        let sig = 1.0f64 / (1.0 + (-x).exp());
        out[i] = x * sig * bf[i];
    }
    mk(rows, cols, out)
}

pub fn almide_rt_matrix_select_rows_q1_0(
    data: &Vec<u8>,
    offset: i64,
    cols: i64,
    row_ids: &[i64],
) -> AlmideMatrix {
    let cols_u = cols.max(0) as usize;
    let n_blocks = cols_u / 128;
    let off = offset.max(0) as usize;
    let n_rows = row_ids.len();
    let mut flat = vec![0.0f64; n_rows * cols_u];
    for (i, &rid) in row_ids.iter().enumerate() {
        let r = rid.max(0) as usize;
        let row_off = off + r * n_blocks * 18;
        let out_off = i * cols_u;
        for b in 0..n_blocks {
            let block_start = row_off + b * 18;
            let scale_raw = (data[block_start] as u16)
                | ((data[block_start + 1] as u16) << 8);
            let scale = fp16_bits_to_f32(scale_raw) as f64;
            let neg_scale = -scale;
            let bits_start = block_start + 2;
            for local_k in 0..128 {
                let byte = data[bits_start + (local_k >> 3)];
                let bit = (byte >> (local_k & 7)) & 1;
                flat[out_off + b * 128 + local_k] = if bit == 1 { scale } else { neg_scale };
            }
        }
    }
    mk(n_rows, cols_u, flat)
}
