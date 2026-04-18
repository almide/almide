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

pub fn almide_rt_matrix_broadcast_add_row(m: &AlmideMatrix, bias: &Vec<f64>) -> AlmideMatrix {
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
                TensorData::new(bias.clone(), [1, bias.len()]), &dev());
            wrap(m.to_burn().add(bias_t))
        }
    }
}

pub fn almide_rt_matrix_layer_norm_rows(m: &AlmideMatrix, gamma: &Vec<f64>, beta: &Vec<f64>, eps: f64) -> AlmideMatrix {
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
            let gamma_t: Tensor<B, 2> = Tensor::from_data(TensorData::new(gamma.clone(), [1, c]), &dev());
            let beta_t: Tensor<B, 2> = Tensor::from_data(TensorData::new(beta.clone(), [1, c]), &dev());
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
            // dgemm with beta=0 is write-only — skips the initial read of
            // C and lets BLAS use its fastest code path. We then fuse the
            // scale, bias-add, and GELU into a single element-wise sweep
            // over the output, so total memory BW is 1(dgemm) + 2(fused
            // post-pass) vs 2+2 for the beta=alpha+bias.clone() approach.
            let mut c: Vec<f64> = Vec::with_capacity(m * n);
            unsafe {
                c.set_len(m * n);
                cblas_dgemm(
                    101, 111, 111,
                    m as i32, n as i32, k as i32,
                    1.0,
                    ad.as_ptr(), k as i32,
                    bd.as_ptr(), n as i32,
                    0.0,
                    c.as_mut_ptr(), n as i32,
                );
            }
            const K: f64 = 0.7978845608028654;
            for (out, &bi) in c.iter_mut().zip(biasd.iter()) {
                let v = alpha * (*out + bi);
                let v3 = v * v * v;
                let inner = K * (v + 0.044715 * v3);
                *out = 0.5 * v * (1.0 + inner.tanh());
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
            let mut c: Vec<f32> = Vec::with_capacity(m * n);
            unsafe {
                c.set_len(m * n);
                cblas_sgemm(
                    101, 111, 111,
                    m as i32, n as i32, k as i32,
                    1.0,
                    ad.as_ptr(), k as i32,
                    bd.as_ptr(), n as i32,
                    0.0,
                    c.as_mut_ptr(), n as i32,
                );
            }
            const K: f32 = 0.7978845608028654;
            for (out, &bi) in c.iter_mut().zip(biasd.iter()) {
                let v = alpha_f * (*out + bi);
                let v3 = v * v * v;
                let inner = K * (v + 0.044715 * v3);
                *out = 0.5 * v * (1.0 + inner.tanh());
            }
            AlmideMatrix::SmallF32 { rows: m, cols: n, data: c }
        }
        _ => {
            // Shape or variant mismatch — fall back to the naive chain
            // so the semantics match what the user wrote even when the
            // fast path can't apply.
            let mul = almide_rt_matrix_mul(a, b);
            let added = almide_rt_matrix_add(&mul, bias);
            let scaled = almide_rt_matrix_scale(&added, alpha);
            almide_rt_matrix_gelu(&scaled)
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

    if causal && sq == sk {
        let mut flat = vec![0.0f64; h * sq * sk];
        for hi in 0..h {
            for i in 0..sq {
                for j in 0..sk {
                    if j > i { flat[hi * sq * sk + i * sk + j] = -10000.0; }
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

pub fn almide_rt_matrix_linear_row(x: &AlmideMatrix, weight: &AlmideMatrix, bias: &Vec<f64>) -> AlmideMatrix {
    let wt = weight.to_burn().swap_dims(0, 1);
    let bias_t: Tensor<B, 2> = Tensor::from_data(TensorData::new(bias.clone(), [1, bias.len()]), &dev());
    wrap(x.to_burn().matmul(wt).add(bias_t))
}

pub fn almide_rt_matrix_linear_row_no_bias(x: &AlmideMatrix, weight: &AlmideMatrix) -> AlmideMatrix {
    let wt = weight.to_burn().swap_dims(0, 1);
    wrap(x.to_burn().matmul(wt))
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

pub fn almide_rt_matrix_conv1d(input: &AlmideMatrix, weight: &AlmideMatrix, bias: &Vec<f64>, kernel: i64, stride: i64, padding: i64) -> AlmideMatrix {
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
