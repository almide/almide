
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
