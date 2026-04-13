// matrix_burn extern — burn-backed Matrix implementations
// Used when building with `almide build` (auto-detected when Matrix is used).
// Drop-in replacement for matrix.rs (same function signatures, same f64 precision).
// BLAS: Apple Accelerate on macOS, OpenBLAS on Linux/Windows (auto-selected via Cargo.toml).

extern crate blas_src;
use burn::tensor::Tensor;
use burn::backend::NdArray;

type B = NdArray<f64>;
pub type AlmideMatrix = Tensor<B, 2>;

fn dev() -> <B as burn::tensor::backend::Backend>::Device { Default::default() }

pub fn almide_rt_matrix_zeros(rows: i64, cols: i64) -> AlmideMatrix {
    Tensor::zeros([rows as usize, cols as usize], &dev())
}

pub fn almide_rt_matrix_ones(rows: i64, cols: i64) -> AlmideMatrix {
    Tensor::ones([rows as usize, cols as usize], &dev())
}

pub fn almide_rt_matrix_shape(m: &AlmideMatrix) -> (i64, i64) {
    let d = m.dims();
    (d[0] as i64, d[1] as i64)
}

pub fn almide_rt_matrix_rows(m: &AlmideMatrix) -> i64 { m.dims()[0] as i64 }
pub fn almide_rt_matrix_cols(m: &AlmideMatrix) -> i64 { m.dims()[1] as i64 }

pub fn almide_rt_matrix_get(m: &AlmideMatrix, row: i64, col: i64) -> f64 {
    let data = m.clone().slice([row as usize..row as usize + 1, col as usize..col as usize + 1]);
    data.into_scalar() as f64
}

pub fn almide_rt_matrix_transpose(m: &AlmideMatrix) -> AlmideMatrix {
    m.clone().transpose()
}

pub fn almide_rt_matrix_from_lists(rows: &Vec<Vec<f64>>) -> AlmideMatrix {
    if rows.is_empty() {
        return Tensor::zeros([0, 0], &dev());
    }
    let nrows = rows.len();
    let ncols = rows[0].len();
    let flat: Vec<f64> = rows.iter().flat_map(|r| r.iter().copied()).collect();
    Tensor::from_data(
        burn::tensor::TensorData::new(flat, [nrows, ncols]),
        &dev(),
    )
}

pub fn almide_rt_matrix_from_bytes_f32_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 4;
    if off + need > data.len() {
        return Tensor::zeros([r, c], &dev());
    }
    let mut flat: Vec<f64> = Vec::with_capacity(r * c);
    let bytes = &data[off..off + need];
    let mut p = 0;
    for _ in 0..(r * c) {
        let v = f32::from_le_bytes([bytes[p], bytes[p+1], bytes[p+2], bytes[p+3]]);
        flat.push(v as f64);
        p += 4;
    }
    Tensor::from_data(burn::tensor::TensorData::new(flat, [r, c]), &dev())
}

pub fn almide_rt_matrix_from_bytes_f16_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 2;
    if off + need > data.len() {
        return Tensor::zeros([r, c], &dev());
    }
    let mut flat: Vec<f64> = Vec::with_capacity(r * c);
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
    Tensor::from_data(burn::tensor::TensorData::new(flat, [r, c]), &dev())
}

pub fn almide_rt_matrix_to_lists(m: &AlmideMatrix) -> Vec<Vec<f64>> {
    let d = m.dims();
    let data = m.clone().to_data();
    let flat: Vec<f64> = data.to_vec().unwrap();
    (0..d[0]).map(|i| {
        flat[i * d[1]..(i + 1) * d[1]].to_vec()
    }).collect()
}

pub fn almide_rt_matrix_add(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    a.clone().add(b.clone())
}

pub fn almide_rt_matrix_sub(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    a.clone().sub(b.clone())
}

pub fn almide_rt_matrix_mul(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    a.clone().matmul(b.clone())
}

pub fn almide_rt_matrix_scale(m: &AlmideMatrix, s: f64) -> AlmideMatrix {
    m.clone().mul_scalar(s)
}

pub fn almide_rt_matrix_map(m: &AlmideMatrix, f: impl Fn(f64) -> f64) -> AlmideMatrix {
    let d = m.dims();
    let data = m.clone().to_data();
    let flat: Vec<f64> = data.to_vec().unwrap();
    let mapped: Vec<f64> = flat.into_iter().map(|x| f(x)).collect();
    Tensor::from_data(
        burn::tensor::TensorData::new(mapped, [d[0], d[1]]),
        &dev(),
    )
}

pub fn almide_rt_matrix_broadcast_add_row(m: &AlmideMatrix, bias: &Vec<f64>) -> AlmideMatrix {
    let bias_t: Tensor<B, 2> = Tensor::from_data(
        burn::tensor::TensorData::new(bias.clone(), [1, bias.len()]),
        &dev(),
    );
    m.clone().add(bias_t)
}

pub fn almide_rt_matrix_layer_norm_rows(m: &AlmideMatrix, gamma: &Vec<f64>, beta: &Vec<f64>, eps: f64) -> AlmideMatrix {
    let [_r, c] = m.dims();
    let mean = m.clone().mean_dim(1);
    let centered = m.clone().sub(mean.clone());
    let var = centered.clone().powf_scalar(2.0).mean_dim(1);
    let inv_std = var.add_scalar(eps).sqrt().recip();
    let normed = centered.mul(inv_std);
    let gamma_t: Tensor<B, 2> = Tensor::from_data(
        burn::tensor::TensorData::new(gamma.clone(), [1, c]), &dev());
    let beta_t: Tensor<B, 2> = Tensor::from_data(
        burn::tensor::TensorData::new(beta.clone(), [1, c]), &dev());
    normed.mul(gamma_t).add(beta_t)
}

pub fn almide_rt_matrix_softmax_rows(m: &AlmideMatrix) -> AlmideMatrix {
    burn::tensor::activation::softmax(m.clone(), 1)
}

pub fn almide_rt_matrix_gelu(m: &AlmideMatrix) -> AlmideMatrix {
    burn::tensor::activation::gelu(m.clone())
}

pub fn almide_rt_matrix_split_cols_even(m: &AlmideMatrix, n: i64) -> Vec<AlmideMatrix> {
    let [r, c] = m.dims();
    let n = n as usize;
    if n == 0 { return vec![]; }
    let chunk = c / n;
    (0..n).map(|h| {
        let start = h * chunk;
        let end = start + chunk;
        m.clone().slice([0..r, start..end])
    }).collect()
}

pub fn almide_rt_matrix_concat_cols_many(matrices: &Vec<AlmideMatrix>) -> AlmideMatrix {
    if matrices.is_empty() {
        return Tensor::zeros([0, 0], &dev());
    }
    Tensor::cat(matrices.iter().cloned().collect::<Vec<_>>(), 1)
}

pub fn almide_rt_matrix_causal_mask_add(m: &AlmideMatrix, mask_val: f64) -> AlmideMatrix {
    let [r, c] = m.dims();
    let mut flat = vec![0.0f64; r * c];
    for i in 0..r {
        for j in 0..c {
            if j > i { flat[i * c + j] = mask_val; }
        }
    }
    let mask: Tensor<B, 2> = Tensor::from_data(
        burn::tensor::TensorData::new(flat, [r, c]), &dev());
    m.clone().add(mask)
}

pub fn almide_rt_matrix_multi_head_attention(q: &AlmideMatrix, k: &AlmideMatrix, v: &AlmideMatrix, n_heads: i64) -> AlmideMatrix {
    almide_rt_matrix_mha_core_burn(q, k, v, n_heads, false)
}

pub fn almide_rt_matrix_masked_multi_head_attention(q: &AlmideMatrix, k: &AlmideMatrix, v: &AlmideMatrix, n_heads: i64) -> AlmideMatrix {
    almide_rt_matrix_mha_core_burn(q, k, v, n_heads, true)
}

pub fn almide_rt_matrix_mha_core_burn(q: &AlmideMatrix, k: &AlmideMatrix, v: &AlmideMatrix, n_heads: i64, causal: bool) -> AlmideMatrix {
    let [sq, d] = q.dims();
    let [sk, _] = k.dims();
    let h = n_heads as usize;
    let dh = d / h;
    let scale = (dh as f64).sqrt().recip();

    // (S, D) → (S, H, Dh) → (H, S, Dh)
    let q3: Tensor<B, 3> = q.clone().reshape([sq, h, dh]).swap_dims(0, 1);
    let k3: Tensor<B, 3> = k.clone().reshape([sk, h, dh]).swap_dims(0, 1);
    let v3: Tensor<B, 3> = v.clone().reshape([sk, h, dh]).swap_dims(0, 1);

    // scores = Q @ K^T * scale: (H, S, S)
    let k3t: Tensor<B, 3> = k3.swap_dims(1, 2); // (H, Dh, S)
    let mut scores: Tensor<B, 3> = q3.matmul(k3t).mul_scalar(scale);

    if causal && sq == sk {
        // Build an explicit (h, sq, sk) upper-triangular mask — avoids
        // relying on burn's broadcast-add semantics which silently produce
        // wrong output for 1-sized dims in some burn releases.
        let mut flat = vec![0.0f64; h * sq * sk];
        for hi in 0..h {
            for i in 0..sq {
                for j in 0..sk {
                    if j > i { flat[hi * sq * sk + i * sk + j] = -10000.0; }
                }
            }
        }
        let mask3: Tensor<B, 3> = Tensor::from_data(
            burn::tensor::TensorData::new(flat, [h, sq, sk]), &dev());
        scores = scores.add(mask3);
    }

    let weights = burn::tensor::activation::softmax(scores, 2);

    // out3 = weights @ V: (H, S, Dh)
    let out3 = weights.matmul(v3);

    // (H, S, Dh) → (S, H, Dh) → (S, D)
    out3.swap_dims(0, 1).reshape([sq, d])
}

pub fn almide_rt_matrix_linear_row(x: &AlmideMatrix, weight: &AlmideMatrix, bias: &Vec<f64>) -> AlmideMatrix {
    let wt: Tensor<B, 2> = weight.clone().swap_dims(0, 1);
    let bias_t: Tensor<B, 2> = Tensor::from_data(
        burn::tensor::TensorData::new(bias.clone(), [1, bias.len()]),
        &dev(),
    );
    x.clone().matmul(wt).add(bias_t)
}

pub fn almide_rt_matrix_linear_row_no_bias(x: &AlmideMatrix, weight: &AlmideMatrix) -> AlmideMatrix {
    let wt: Tensor<B, 2> = weight.clone().swap_dims(0, 1);
    x.clone().matmul(wt)
}

pub fn almide_rt_matrix_slice_rows(m: &AlmideMatrix, start: i64, end: i64) -> AlmideMatrix {
    let [r, _c] = m.dims();
    let s = (start as usize).min(r);
    let e = (end as usize).min(r);
    if s >= e {
        return Tensor::zeros([0, m.dims()[1]], &dev());
    }
    m.clone().slice([s..e, 0..m.dims()[1]])
}

pub fn almide_rt_matrix_conv1d(input: &AlmideMatrix, weight: &AlmideMatrix, bias: &Vec<f64>, kernel: i64, stride: i64, padding: i64) -> AlmideMatrix {
    // Flatten to Vec<f64>, do scalar conv (Whisper tiny is small enough),
    // return as Tensor. burn's conv module is module-based (trainable) so
    // we use a direct scalar impl here for simplicity.
    let [t_in, in_ch] = input.dims();
    let [out_ch, _] = weight.dims();
    let k = kernel as usize;
    let s = stride as usize;
    let p = padding as usize;
    let t_padded = t_in + 2 * p;
    if t_padded < k {
        return Tensor::zeros([0, out_ch], &dev());
    }
    let t_out = (t_padded - k) / s + 1;

    let x_flat: Vec<f64> = input.clone().to_data().to_vec().unwrap();
    let w_flat: Vec<f64> = weight.clone().to_data().to_vec().unwrap();
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
    Tensor::from_data(burn::tensor::TensorData::new(out_flat, [t_out, out_ch]), &dev())
}

pub fn almide_rt_matrix_from_bytes_f64_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 8;
    if off + need > data.len() {
        return Tensor::zeros([r, c], &dev());
    }
    let mut flat: Vec<f64> = Vec::with_capacity(r * c);
    let bytes = &data[off..off + need];
    let mut p = 0;
    for _ in 0..(r * c) {
        let v = f64::from_le_bytes([bytes[p], bytes[p+1], bytes[p+2], bytes[p+3], bytes[p+4], bytes[p+5], bytes[p+6], bytes[p+7]]);
        flat.push(v);
        p += 8;
    }
    Tensor::from_data(burn::tensor::TensorData::new(flat, [r, c]), &dev())
}

pub fn almide_rt_matrix_gather_rows(m: &AlmideMatrix, indices: &Vec<i64>) -> AlmideMatrix {
    let [_r, c] = m.dims();
    let n = indices.len();
    let mut flat: Vec<f64> = Vec::with_capacity(n * c);
    let data = m.clone().to_data();
    let mflat: Vec<f64> = data.to_vec().unwrap();
    for &idx in indices {
        let i = idx as usize;
        if i < m.dims()[0] {
            flat.extend_from_slice(&mflat[i * c..(i + 1) * c]);
        } else {
            flat.extend(std::iter::repeat(0.0).take(c));
        }
    }
    Tensor::from_data(burn::tensor::TensorData::new(flat, [n, c]), &dev())
}

pub fn almide_rt_matrix_row_dot(m: &AlmideMatrix, r: i64, vec: &Vec<f64>) -> f64 {
    let [_, c] = m.dims();
    let r = r as usize;
    if r >= m.dims()[0] { return 0.0; }
    let data = m.clone().to_data();
    let mflat: Vec<f64> = data.to_vec().unwrap();
    let mut s = 0.0;
    let n = c.min(vec.len());
    for k in 0..n { s += mflat[r * c + k] * vec[k]; }
    s
}
