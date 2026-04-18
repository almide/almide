// matrix extern — Rust native implementations
// Pure implementation using Vec<Vec<f64>> (no external dependencies)
// Will be replaced with ndarray for almide build, burn for --target cuda

pub type AlmideMatrix = Vec<Vec<f64>>;

pub fn almide_rt_matrix_zeros(rows: i64, cols: i64) -> AlmideMatrix {
    vec![vec![0.0; cols as usize]; rows as usize]
}

pub fn almide_rt_matrix_ones(rows: i64, cols: i64) -> AlmideMatrix {
    vec![vec![1.0; cols as usize]; rows as usize]
}

pub fn almide_rt_matrix_shape(m: &AlmideMatrix) -> (i64, i64) {
    let rows = m.len() as i64;
    let cols = if m.is_empty() { 0 } else { m[0].len() as i64 };
    (rows, cols)
}

pub fn almide_rt_matrix_rows(m: &AlmideMatrix) -> i64 { m.len() as i64 }
pub fn almide_rt_matrix_cols(m: &AlmideMatrix) -> i64 {
    if m.is_empty() { 0 } else { m[0].len() as i64 }
}

pub fn almide_rt_matrix_get(m: &AlmideMatrix, row: i64, col: i64) -> f64 {
    m[row as usize][col as usize]
}

pub fn almide_rt_matrix_transpose(m: &AlmideMatrix) -> AlmideMatrix {
    if m.is_empty() { return vec![]; }
    let rows = m.len();
    let cols = m[0].len();
    (0..cols).map(|c| (0..rows).map(|r| m[r][c]).collect()).collect()
}

pub fn almide_rt_matrix_from_lists(rows: &[Vec<f64>]) -> AlmideMatrix {
    rows.to_vec()
}

pub fn almide_rt_matrix_from_bytes_f32_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 4;
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
    if off + need > data.len() {
        for _ in 0..r { result.push(vec![0.0f64; c]); }
        return result;
    }
    let bytes = &data[off..off + need];
    for i in 0..r {
        let mut row: Vec<f64> = Vec::with_capacity(c);
        let row_base = i * c * 4;
        for j in 0..c {
            let p = row_base + j * 4;
            let v = f32::from_le_bytes([bytes[p], bytes[p+1], bytes[p+2], bytes[p+3]]);
            row.push(v as f64);
        }
        result.push(row);
    }
    result
}

pub fn almide_rt_matrix_from_bytes_f16_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    // IEEE-754 half-precision → f32 → f64, inlined to keep the matrix.rs
    // file a single contiguous block of `pub fn almide_rt_matrix_*` functions
    // (simplifies the runtime-stripping logic in src/cli/mod.rs).
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 2;
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
    if off + need > data.len() {
        for _ in 0..r { result.push(vec![0.0f64; c]); }
        return result;
    }
    let bytes = &data[off..off + need];
    for i in 0..r {
        let mut row: Vec<f64> = Vec::with_capacity(c);
        let row_base = i * c * 2;
        for j in 0..c {
            let p = row_base + j * 2;
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
            row.push(f32::from_bits(f32_bits) as f64);
        }
        result.push(row);
    }
    result
}

pub fn almide_rt_matrix_to_lists(m: &AlmideMatrix) -> Vec<Vec<f64>> {
    m.clone()
}

pub fn almide_rt_matrix_add(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    a.iter().zip(b.iter())
        .map(|(ar, br)| ar.iter().zip(br.iter()).map(|(x, y)| x + y).collect())
        .collect()
}

pub fn almide_rt_matrix_sub(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    a.iter().zip(b.iter())
        .map(|(ar, br)| ar.iter().zip(br.iter()).map(|(x, y)| x - y).collect())
        .collect()
}

pub fn almide_rt_matrix_div(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    a.iter().zip(b.iter())
        .map(|(ar, br)| ar.iter().zip(br.iter()).map(|(x, y)| x / y).collect())
        .collect()
}

pub fn almide_rt_matrix_neg(m: &AlmideMatrix) -> AlmideMatrix {
    m.iter().map(|r| r.iter().map(|x| -x).collect()).collect()
}

pub fn almide_rt_matrix_pow(m: &AlmideMatrix, exp: f64) -> AlmideMatrix {
    m.iter().map(|r| r.iter().map(|x| x.powf(exp)).collect()).collect()
}

pub fn almide_rt_matrix_mul(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    let m = a.len();
    let n = if a.is_empty() { 0 } else { a[0].len() };
    let p = if b.is_empty() { 0 } else { b[0].len() };
    // Flatten to contiguous arrays for cache-friendly access
    let a_flat: Vec<f64> = a.iter().flat_map(|r| r.iter().copied()).collect();
    let b_flat: Vec<f64> = b.iter().flat_map(|r| r.iter().copied()).collect();
    let mut c_flat = vec![0.0f64; m * p];
    // Tiled matmul: 32×32 blocks for L1 cache locality
    const TILE: usize = 32;
    let mut i0 = 0;
    while i0 < m {
        let i1 = if i0 + TILE < m { i0 + TILE } else { m };
        let mut k0 = 0;
        while k0 < n {
            let k1 = if k0 + TILE < n { k0 + TILE } else { n };
            let mut j0 = 0;
            while j0 < p {
                let j1 = if j0 + TILE < p { j0 + TILE } else { p };
                // Multiply tile A[i0..i1, k0..k1] × B[k0..k1, j0..j1]
                // Slice-based DAXPY: LLVM auto-vectorizes when inputs are
                // disjoint slices (c_row is a unique borrow, b_row is &).
                // On WASM this emits f64x2 SIMD when target-feature=+simd128.
                let mut i = i0;
                while i < i1 {
                    let c_base = i * p;
                    let c_row = &mut c_flat[c_base + j0..c_base + j1];
                    let mut k = k0;
                    while k < k1 {
                        let a_ik = a_flat[i * n + k];
                        let b_base = k * p;
                        let b_row = &b_flat[b_base + j0..b_base + j1];
                        // Plain mul+add (not mul_add) — WASM SIMD128 has no
                        // hardware FMA; mul_add falls back to a software
                        // polynomial that is 15-20x slower than mul+add.
                        for (c, &b) in c_row.iter_mut().zip(b_row.iter()) {
                            *c += a_ik * b;
                        }
                        k += 1;
                    }
                    i += 1;
                }
                j0 += TILE;
            }
            k0 += TILE;
        }
        i0 += TILE;
    }
    // Unflatten
    (0..m).map(|i| c_flat[i * p..(i + 1) * p].to_vec()).collect()
}

pub fn almide_rt_matrix_scale(m: &AlmideMatrix, s: f64) -> AlmideMatrix {
    m.iter().map(|row| row.iter().map(|x| x * s).collect()).collect()
}

/// Fused multiply-add: a*ka + b*kb in one pass, single allocation.
/// Equivalent to `add(scale(a, ka), scale(b, kb))` but reads each input
/// once and allocates once. Caller must ensure shapes match.
pub fn almide_rt_matrix_fma(a: &AlmideMatrix, ka: f64, b: &AlmideMatrix, kb: f64) -> AlmideMatrix {
    a.iter().zip(b.iter())
        .map(|(ar, br)| ar.iter().zip(br.iter()).map(|(x, y)| x * ka + y * kb).collect())
        .collect()
}

/// Three-term fused multiply-add: `a*ka + b*kb + c*kc` in one pass.
/// Target of the MatrixFusionPass tree-fuse rule for nested fma collapse.
pub fn almide_rt_matrix_fma3(
    a: &AlmideMatrix, ka: f64,
    b: &AlmideMatrix, kb: f64,
    c: &AlmideMatrix, kc: f64,
) -> AlmideMatrix {
    a.iter().zip(b.iter()).zip(c.iter())
        .map(|((ar, br), cr)| {
            ar.iter().zip(br.iter()).zip(cr.iter())
                .map(|((x, y), z)| x * ka + y * kb + z * kc)
                .collect()
        })
        .collect()
}

pub fn almide_rt_matrix_map(m: &AlmideMatrix, f: impl Fn(f64) -> f64) -> AlmideMatrix {
    m.iter().map(|row| row.iter().map(|x| f(*x)).collect()).collect()
}

pub fn almide_rt_matrix_broadcast_add_row(m: &AlmideMatrix, bias: &[f64]) -> AlmideMatrix {
    m.iter().map(|row| row.iter().zip(bias.iter()).map(|(x, b)| x + b).collect()).collect()
}

pub fn almide_rt_matrix_layer_norm_rows(m: &AlmideMatrix, gamma: &[f64], beta: &[f64], eps: f64) -> AlmideMatrix {
    m.iter().map(|row| {
        let n = row.len() as f64;
        let mut sum = 0.0;
        for &x in row { sum += x; }
        let mean = sum / n;
        let mut var = 0.0;
        for &x in row { let d = x - mean; var += d * d; }
        var /= n;
        let inv = (var + eps).sqrt().recip();
        row.iter().zip(gamma.iter()).zip(beta.iter())
            .map(|((x, g), b)| (x - mean) * inv * g + b)
            .collect()
    }).collect()
}

pub fn almide_rt_matrix_softmax_rows(m: &AlmideMatrix) -> AlmideMatrix {
    m.iter().map(|row| {
        let mut max = f64::NEG_INFINITY;
        for &x in row { if x > max { max = x; } }
        let mut exps: Vec<f64> = row.iter().map(|x| (x - max).exp()).collect();
        let sum: f64 = exps.iter().sum();
        let inv = 1.0 / sum;
        for e in exps.iter_mut() { *e *= inv; }
        exps
    }).collect()
}

pub fn almide_rt_matrix_gelu(m: &AlmideMatrix) -> AlmideMatrix {
    const K: f64 = 0.7978845608028654;
    m.iter().map(|row| row.iter().map(|&x| {
        let x3 = x * x * x;
        let inner = K * (x + 0.044715 * x3);
        0.5 * x * (1.0 + inner.tanh())
    }).collect()).collect()
}

pub fn almide_rt_matrix_fused_gemm_bias_scale_gelu(
    a: &AlmideMatrix,
    b: &AlmideMatrix,
    bias: &AlmideMatrix,
    alpha: f64,
) -> AlmideMatrix {
    let mul = almide_rt_matrix_mul(a, b);
    let added = almide_rt_matrix_add(&mul, bias);
    let scaled = almide_rt_matrix_scale(&added, alpha);
    almide_rt_matrix_gelu(&scaled)
}

pub fn almide_rt_matrix_attention_weights(
    q: &AlmideMatrix,
    kt: &AlmideMatrix,
    scale: f64,
) -> AlmideMatrix {
    let prod = almide_rt_matrix_mul(q, kt);
    let scaled = almide_rt_matrix_scale(&prod, scale);
    almide_rt_matrix_softmax_rows(&scaled)
}

pub fn almide_rt_matrix_split_cols_even(m: &AlmideMatrix, n: i64) -> Vec<AlmideMatrix> {
    let n = n as usize;
    if m.is_empty() || n == 0 { return vec![]; }
    let cols = m[0].len();
    let chunk = cols / n;
    (0..n).map(|h| {
        let start = h * chunk;
        let end = start + chunk;
        m.iter().map(|row| row[start..end].to_vec()).collect::<Vec<Vec<f64>>>()
    }).collect()
}

pub fn almide_rt_matrix_concat_cols_many(matrices: &[AlmideMatrix]) -> AlmideMatrix {
    if matrices.is_empty() { return vec![]; }
    let rows = matrices[0].len();
    if rows == 0 { return vec![vec![]]; }
    let total_cols: usize = matrices.iter().map(|m| if m.is_empty() { 0 } else { m[0].len() }).sum();
    (0..rows).map(|r| {
        let mut row = Vec::with_capacity(total_cols);
        for m in matrices {
            if r < m.len() {
                row.extend_from_slice(&m[r]);
            }
        }
        row
    }).collect()
}

pub fn almide_rt_matrix_causal_mask_add(m: &AlmideMatrix, mask_val: f64) -> AlmideMatrix {
    m.iter().enumerate().map(|(i, row)| {
        row.iter().enumerate().map(|(j, &x)| {
            if j > i { x + mask_val } else { x }
        }).collect()
    }).collect()
}

pub fn almide_rt_matrix_multi_head_attention(q: &AlmideMatrix, k: &AlmideMatrix, v: &AlmideMatrix, n_heads: i64) -> AlmideMatrix {
    almide_rt_matrix_mha_core(q, k, v, n_heads, false)
}

pub fn almide_rt_matrix_masked_multi_head_attention(q: &AlmideMatrix, k: &AlmideMatrix, v: &AlmideMatrix, n_heads: i64) -> AlmideMatrix {
    almide_rt_matrix_mha_core(q, k, v, n_heads, true)
}

pub fn almide_rt_matrix_mha_core(q: &AlmideMatrix, k: &AlmideMatrix, v: &AlmideMatrix, n_heads: i64, causal: bool) -> AlmideMatrix {
    let n_heads = n_heads as usize;
    if q.is_empty() || n_heads == 0 { return vec![]; }
    let sq = q.len();
    let sk = k.len();
    let d = q[0].len();
    let dh = d / n_heads;
    let scale = (dh as f64).sqrt().recip();

    let mut out = vec![vec![0.0f64; d]; sq];

    for h in 0..n_heads {
        let col0 = h * dh;
        let col1 = col0 + dh;

        // scores[i][j] = (sum_k q[i][col0+k] * k[j][col0+k]) * scale
        let mut scores = vec![vec![0.0f64; sk]; sq];
        for i in 0..sq {
            for j in 0..sk {
                let mut s = 0.0;
                for kk in col0..col1 {
                    s += q[i][kk] * k[j][kk];
                }
                scores[i][j] = s * scale;
                if causal && j > i {
                    scores[i][j] += -1.0e9;
                }
            }
        }

        // Row-wise softmax
        for row in scores.iter_mut() {
            let mut max = f64::NEG_INFINITY;
            for &x in row.iter() { if x > max { max = x; } }
            let mut sum = 0.0;
            for x in row.iter_mut() { *x = (*x - max).exp(); sum += *x; }
            let inv = 1.0 / sum;
            for x in row.iter_mut() { *x *= inv; }
        }

        // out[i][col0..col1] += sum_j scores[i][j] * v[j][col0..col1]
        for i in 0..sq {
            for j in 0..sk {
                let w = scores[i][j];
                if w == 0.0 { continue; }
                for kk in 0..dh {
                    out[i][col0 + kk] += w * v[j][col0 + kk];
                }
            }
        }
    }

    out
}

pub fn almide_rt_matrix_linear_row(x: &AlmideMatrix, weight: &AlmideMatrix, bias: &[f64]) -> AlmideMatrix {
    // y[i][j] = sum_k x[i][k] * weight[j][k] + bias[j]
    if x.is_empty() || weight.is_empty() { return vec![]; }
    let r = x.len();
    let n_in = x[0].len();
    let n_out = weight.len();
    let mut out = vec![vec![0.0f64; n_out]; r];
    for i in 0..r {
        let xi = &x[i];
        let oi = &mut out[i];
        for j in 0..n_out {
            let wj = &weight[j];
            let mut s = 0.0;
            for k in 0..n_in {
                s += xi[k] * wj[k];
            }
            oi[j] = s + bias[j];
        }
    }
    out
}

pub fn almide_rt_matrix_linear_row_gelu(
    x: &AlmideMatrix,
    weight: &AlmideMatrix,
    bias: &[f64],
) -> AlmideMatrix {
    let lin = almide_rt_matrix_linear_row(x, weight, bias);
    almide_rt_matrix_gelu(&lin)
}

pub fn almide_rt_matrix_pre_norm_linear(
    x: &AlmideMatrix,
    gamma: &[f64],
    beta: &[f64],
    eps: f64,
    weight: &AlmideMatrix,
    bias: &[f64],
) -> AlmideMatrix {
    let normed = almide_rt_matrix_layer_norm_rows(x, gamma, beta, eps);
    almide_rt_matrix_linear_row(&normed, weight, bias)
}

pub fn almide_rt_matrix_linear_row_no_bias(x: &AlmideMatrix, weight: &AlmideMatrix) -> AlmideMatrix {
    if x.is_empty() || weight.is_empty() { return vec![]; }
    let r = x.len();
    let n_in = x[0].len();
    let n_out = weight.len();
    let mut out = vec![vec![0.0f64; n_out]; r];
    for i in 0..r {
        let xi = &x[i];
        let oi = &mut out[i];
        for j in 0..n_out {
            let wj = &weight[j];
            let mut s = 0.0;
            for k in 0..n_in {
                s += xi[k] * wj[k];
            }
            oi[j] = s;
        }
    }
    out
}

pub fn almide_rt_matrix_slice_rows(m: &AlmideMatrix, start: i64, end: i64) -> AlmideMatrix {
    let s = start as usize;
    let e = (end as usize).min(m.len());
    if s >= e { return vec![]; }
    m[s..e].to_vec()
}

pub fn almide_rt_matrix_conv1d(input: &AlmideMatrix, weight: &AlmideMatrix, bias: &[f64], kernel: i64, stride: i64, padding: i64) -> AlmideMatrix {
    // input: (T, in_ch). weight: (out_ch, in_ch * kernel). bias: (out_ch,).
    // Output: (T_out, out_ch) where T_out = floor((T + 2P - K) / S) + 1.
    // Weight layout within a row: for c in 0..in_ch, for k in 0..kernel: weight[o][c*kernel + k].
    let t_in = input.len();
    if t_in == 0 || weight.is_empty() { return vec![]; }
    let in_ch = input[0].len();
    let out_ch = weight.len();
    let k = kernel as usize;
    let s = stride as usize;
    let p = padding as usize;
    let t_padded = t_in + 2 * p;
    if t_padded < k { return vec![]; }
    let t_out = (t_padded - k) / s + 1;
    let mut out = vec![vec![0.0f64; out_ch]; t_out];
    for t in 0..t_out {
        let base = t * s;  // start in padded coords
        for o in 0..out_ch {
            let wo = &weight[o];
            let mut sum = bias[o];
            for c in 0..in_ch {
                let w_base = c * k;
                for ki in 0..k {
                    let tp = base + ki;
                    if tp >= p && tp < p + t_in {
                        let tc = tp - p;
                        sum += wo[w_base + ki] * input[tc][c];
                    }
                }
            }
            out[t][o] = sum;
        }
    }
    out
}

pub fn almide_rt_matrix_to_bytes_f64_le(m: &AlmideMatrix) -> Vec<u8> {
    let rows = m.len();
    let cols = if rows == 0 { 0 } else { m[0].len() };
    let mut out: Vec<u8> = Vec::with_capacity(rows * cols * 8);
    for row in m {
        for v in row {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

pub fn almide_rt_matrix_to_bytes_f32_le(m: &AlmideMatrix) -> Vec<u8> {
    let rows = m.len();
    let cols = if rows == 0 { 0 } else { m[0].len() };
    let mut out: Vec<u8> = Vec::with_capacity(rows * cols * 4);
    for row in m {
        for v in row {
            out.extend_from_slice(&(*v as f32).to_le_bytes());
        }
    }
    out
}

pub fn almide_rt_matrix_from_bytes_f64_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 8;
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
    if off + need > data.len() {
        for _ in 0..r { result.push(vec![0.0f64; c]); }
        return result;
    }
    let bytes = &data[off..off + need];
    for i in 0..r {
        let mut row: Vec<f64> = Vec::with_capacity(c);
        let row_base = i * c * 8;
        for j in 0..c {
            let p = row_base + j * 8;
            let v = f64::from_le_bytes([bytes[p], bytes[p+1], bytes[p+2], bytes[p+3], bytes[p+4], bytes[p+5], bytes[p+6], bytes[p+7]]);
            row.push(v);
        }
        result.push(row);
    }
    result
}

pub fn almide_rt_matrix_gather_rows(m: &AlmideMatrix, indices: &[i64]) -> AlmideMatrix {
    if m.is_empty() { return vec![]; }
    let cols = m[0].len();
    indices.iter().map(|&idx| {
        let i = idx as usize;
        if i < m.len() { m[i].clone() } else { vec![0.0f64; cols] }
    }).collect()
}

pub fn almide_rt_matrix_row_dot(m: &AlmideMatrix, r: i64, vec: &[f64]) -> f64 {
    let r = r as usize;
    if r >= m.len() { return 0.0; }
    let row = &m[r];
    let n = row.len().min(vec.len());
    let mut s = 0.0;
    for k in 0..n { s += row[k] * vec[k]; }
    s
}

// matrix: f32 path stubs for WASM target. matrix.rs has no distinct f32
// matrix: storage (all Vec<Vec<f64>>), so these delegate to f64 ops.
// matrix: burn/matrix_burn.rs provides the real f32 path via cblas_sgemm.
pub fn almide_rt_matrix_zeros_f32(rows: i64, cols: i64) -> AlmideMatrix {
    almide_rt_matrix_zeros(rows, cols)
}
pub fn almide_rt_matrix_ones_f32(rows: i64, cols: i64) -> AlmideMatrix {
    almide_rt_matrix_ones(rows, cols)
}
pub fn almide_rt_matrix_mul_f32(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    almide_rt_matrix_mul(a, b)
}
pub fn almide_rt_matrix_mul_f32_scaled(a: &AlmideMatrix, alpha: f64, b: &AlmideMatrix) -> AlmideMatrix {
    almide_rt_matrix_scale(&almide_rt_matrix_mul(a, b), alpha)
}
pub fn almide_rt_matrix_mul_scaled(a: &AlmideMatrix, alpha: f64, b: &AlmideMatrix) -> AlmideMatrix {
    almide_rt_matrix_scale(&almide_rt_matrix_mul(a, b), alpha)
}
pub fn almide_rt_matrix_mul_f32_t(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    almide_rt_matrix_mul(a, &almide_rt_matrix_transpose(b))
}
pub fn almide_rt_matrix_mul_f32_t_scaled(a: &AlmideMatrix, alpha: f64, b: &AlmideMatrix) -> AlmideMatrix {
    almide_rt_matrix_scale(&almide_rt_matrix_mul_f32_t(a, b), alpha)
}
