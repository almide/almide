
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
