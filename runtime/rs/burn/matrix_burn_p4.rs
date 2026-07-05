
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
// sign-byte → 8-lane ±1.0 lookup table (burn-runtime mirror of the one in
// `runtime/rs/src/matrix.rs`). Precomputed so the hot kernel can load a
// vector of signs per byte instead of branching per bit.
static SIGN_LUT_BURN: [[f64; 8]; 256] = {
    let mut t = [[0.0; 8]; 256];
    let mut i = 0;
    while i < 256 {
        let mut j = 0;
        while j < 8 {
            t[i][j] = if (i >> j) & 1 == 1 { 1.0 } else { -1.0 };
            j += 1;
        }
        i += 1;
    }
    t
};

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[inline]
unsafe fn q1_0_block_dot_neon_burn(xi: *const f64, sign_bytes: *const u8, scale: f64) -> f64 {
    use std::arch::aarch64::*;
    let mut a0 = vdupq_n_f64(0.0);
    let mut a1 = vdupq_n_f64(0.0);
    let mut a2 = vdupq_n_f64(0.0);
    let mut a3 = vdupq_n_f64(0.0);
    for byte_idx in 0..16 {
        let byte = *sign_bytes.add(byte_idx) as usize;
        let sig = SIGN_LUT_BURN[byte].as_ptr();
        let x = xi.add(byte_idx * 8);
        a0 = vfmaq_f64(a0, vld1q_f64(x),        vld1q_f64(sig));
        a1 = vfmaq_f64(a1, vld1q_f64(x.add(2)), vld1q_f64(sig.add(2)));
        a2 = vfmaq_f64(a2, vld1q_f64(x.add(4)), vld1q_f64(sig.add(4)));
        a3 = vfmaq_f64(a3, vld1q_f64(x.add(6)), vld1q_f64(sig.add(6)));
    }
    let acc = vaddq_f64(vaddq_f64(a0, a1), vaddq_f64(a2, a3));
    scale * vaddvq_f64(acc)
}

#[inline]
fn q1_0_block_dot_scalar_burn(xi: &[f64], sign_bytes: &[u8], scale: f64) -> f64 {
    let mut s = 0.0f64;
    for byte_idx in 0..16 {
        let byte = sign_bytes[byte_idx] as usize;
        let sig = &SIGN_LUT_BURN[byte];
        let off = byte_idx * 8;
        for k in 0..8 {
            s += xi[off + k] * sig[k];
        }
    }
    scale * s
}

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
    // Fast path: if x is already a Small (Vec<f64>) variant, borrow its data
    // directly instead of paying a full Vec::clone for every call.
    #[allow(unused_assignments)]
    let mut owned_fallback: Vec<f64> = Vec::new();
    let x_flat: &[f64] = match x {
        AlmideMatrix::Small { data, .. } => data.as_slice(),
        _ => {
            owned_fallback = x.to_vec_f64();
            owned_fallback.as_slice()
        }
    };
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
                let sign_slice = &w_bytes[block_start + 2..block_start + 18];
                let xi_block = &x_flat[x_off + b * 128..x_off + b * 128 + 128];
                #[cfg(target_arch = "aarch64")]
                let contrib = unsafe {
                    q1_0_block_dot_neon_burn(xi_block.as_ptr(), sign_slice.as_ptr(), scale)
                };
                #[cfg(not(target_arch = "aarch64"))]
                let contrib = q1_0_block_dot_scalar_burn(xi_block, sign_slice, scale);
                sum += contrib;
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

// ── Qwen3 block super-intrinsic (Q1_0 + KV cache, burn backend) ──
//
// Matches `runtime/rs/src/matrix.rs`'s scalar super-intrinsic but works
// against burn's AlmideMatrix enum by routing through the existing
// primitives (which already handle Small / SmallF32 / Tensor paths).

fn load_f32_to_f64_burn(w: &[u8], offset: usize, len: usize) -> Vec<f64> {
    let mut out = Vec::<f64>::with_capacity(len);
    for i in 0..len {
        let p = offset + i * 4;
        let bits = u32::from_le_bytes([w[p], w[p + 1], w[p + 2], w[p + 3]]);
        out.push(f32::from_bits(bits) as f64);
    }
    out
}

fn per_head_rms_norm_burn(
    x: &AlmideMatrix,
    gamma: &[f64],
    n_heads: i64,
    eps: f64,
) -> AlmideMatrix {
    let dims = x.dims2();
    let sq = dims[0];
    let d = dims[1];
    let n_heads_u = n_heads.max(1) as usize;
    if n_heads_u == 0 || d == 0 { return x.clone(); }
    let head_dim = d / n_heads_u;
    let flat_in = x.to_vec_f64();
    let mut flat_out = vec![0.0f64; sq * d];
    for i in 0..sq {
        let row_off = i * d;
        for h in 0..n_heads_u {
            let start = row_off + h * head_dim;
            let mut ss = 0.0f64;
            for k in 0..head_dim {
                let v = flat_in[start + k];
                ss += v * v;
            }
            let inv = 1.0f64 / (ss / head_dim as f64 + eps).sqrt();
            for k in 0..head_dim {
                flat_out[start + k] = flat_in[start + k] * inv * gamma[k];
            }
        }
    }
    mk(sq, d, flat_out)
}

fn repeat_kv_burn(kv: &AlmideMatrix, n_kv_heads: i64, n_rep: i64) -> AlmideMatrix {
    if n_rep <= 1 { return kv.clone(); }
    let dims = kv.dims2();
    let sq = dims[0];
    let d = dims[1];
    let n_kv = n_kv_heads.max(1) as usize;
    let n_rep_u = n_rep as usize;
    if n_kv == 0 || d == 0 { return kv.clone(); }
    let head_dim = d / n_kv;
    let out_cols = d * n_rep_u;
    let flat_in = kv.to_vec_f64();
    let mut flat_out = vec![0.0f64; sq * out_cols];
    for i in 0..sq {
        let src_row = i * d;
        let dst_row = i * out_cols;
        for h in 0..n_kv {
            let src = src_row + h * head_dim;
            for r in 0..n_rep_u {
                let dst = dst_row + (h * n_rep_u + r) * head_dim;
                flat_out[dst..dst + head_dim].copy_from_slice(&flat_in[src..src + head_dim]);
            }
        }
    }
    mk(sq, out_cols, flat_out)
}

pub fn almide_rt_matrix_qwen3_block_q1_0_kv(
    h: &AlmideMatrix,
    k_cache: &AlmideMatrix,
    v_cache: &AlmideMatrix,
    w: &Vec<u8>,
    gamma_offs: &[i64],
    weight_offs: &[i64],
    start_pos: i64,
    n_q_heads: i64,
    n_kv_heads: i64,
    head_dim: i64,
    ffn_hidden: i64,
    rope_theta: f64,
    eps: f64,
) -> (AlmideMatrix, AlmideMatrix, AlmideMatrix) {
    let hidden = (n_q_heads * head_dim) as usize;
    let kv_hidden = (n_kv_heads * head_dim) as usize;
    let n_rep = n_q_heads / n_kv_heads;

    let gamma_attn = load_f32_to_f64_burn(w, gamma_offs[0].max(0) as usize, hidden);
    let gamma_q = load_f32_to_f64_burn(w, gamma_offs[1].max(0) as usize, head_dim.max(0) as usize);
    let gamma_k = load_f32_to_f64_burn(w, gamma_offs[2].max(0) as usize, head_dim.max(0) as usize);
    let gamma_ffn = load_f32_to_f64_burn(w, gamma_offs[3].max(0) as usize, hidden);

    let h_normed = almide_rt_matrix_rms_norm_rows(h, &gamma_attn, eps);

    let q_proj = almide_rt_matrix_linear_q1_0_row_no_bias(
        &h_normed, w, weight_offs[0], hidden as i64, hidden as i64);
    let k_proj = almide_rt_matrix_linear_q1_0_row_no_bias(
        &h_normed, w, weight_offs[1], kv_hidden as i64, hidden as i64);
    let v_proj = almide_rt_matrix_linear_q1_0_row_no_bias(
        &h_normed, w, weight_offs[2], kv_hidden as i64, hidden as i64);

    let q_normed = per_head_rms_norm_burn(&q_proj, &gamma_q, n_q_heads, eps);
    let k_normed = per_head_rms_norm_burn(&k_proj, &gamma_k, n_kv_heads, eps);

    let q_rot = almide_rt_matrix_rope_rotate_at(&q_normed, n_q_heads, head_dim, rope_theta, start_pos);
    let k_rot = almide_rt_matrix_rope_rotate_at(&k_normed, n_kv_heads, head_dim, rope_theta, start_pos);

    let k_full_kv = almide_rt_matrix_append_rows(k_cache, &k_rot);
    let v_full_kv = almide_rt_matrix_append_rows(v_cache, &v_proj);

    let k_full = repeat_kv_burn(&k_full_kv, n_kv_heads, n_rep);
    let v_full = repeat_kv_burn(&v_full_kv, n_kv_heads, n_rep);

    let attn_out = almide_rt_matrix_masked_multi_head_attention(&q_rot, &k_full, &v_full, n_q_heads);
    let attn_proj = almide_rt_matrix_linear_q1_0_row_no_bias(
        &attn_out, w, weight_offs[3], hidden as i64, hidden as i64);
    let x_attn = almide_rt_matrix_add(h, &attn_proj);

    let h2 = almide_rt_matrix_rms_norm_rows(&x_attn, &gamma_ffn, eps);
    let gate = almide_rt_matrix_linear_q1_0_row_no_bias(
        &h2, w, weight_offs[4], ffn_hidden, hidden as i64);
    let up = almide_rt_matrix_linear_q1_0_row_no_bias(
        &h2, w, weight_offs[5], ffn_hidden, hidden as i64);
    let gated = almide_rt_matrix_silu_mul(&gate, &up);
    let ffn_out = almide_rt_matrix_linear_q1_0_row_no_bias(
        &gated, w, weight_offs[6], hidden as i64, ffn_hidden);

    let h_out = almide_rt_matrix_add(&x_attn, &ffn_out);
    (h_out, k_full_kv, v_full_kv)
}
