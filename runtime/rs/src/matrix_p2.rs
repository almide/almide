
pub fn almide_rt_matrix_to_bytes_f64_le(m: &AlmideMatrix) -> Vec<u8> {
    let rows = m.len();
    let cols = if rows == 0 { 0 } else { m[0].len() };
    let mut out: Vec<u8> = Vec::with_capacity(rows * cols * 8);
    for row in m.iter() {
        for v in row {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.into()
}

pub fn almide_rt_matrix_to_bytes_f32_le(m: &AlmideMatrix) -> Vec<u8> {
    let rows = m.len();
    let cols = if rows == 0 { 0 } else { m[0].len() };
    let mut out: Vec<u8> = Vec::with_capacity(rows * cols * 4);
    for row in m.iter() {
        for v in row {
            out.extend_from_slice(&(*v as f32).to_le_bytes());
        }
    }
    out.into()
}

pub fn almide_rt_matrix_from_bytes_f64_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    let r = rows as usize;
    let c = cols as usize;
    let off = offset as usize;
    let need = r * c * 8;
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
    if off + need > data.len() {
        for _ in 0..r { result.push(vec![0.0f64; c]); }
        return result.into();
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
    result.into()
}

pub fn almide_rt_matrix_gather_rows(m: &AlmideMatrix, indices: &[i64]) -> AlmideMatrix {
    if m.is_empty() { return vec![].into(); }
    let cols = m[0].len();
    indices.iter().map(|&idx| {
        let i = idx as usize;
        if i < m.len() { m[i].to_vec() } else { vec![0.0f64; cols] }
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

// ── Q1_0 (1-bit) direct decode ──
//
// Used by bonsai-almide / Qwen3 family GGUFs: 18 bytes per 128 weights,
// first 2 bytes fp16 scale, next 16 bytes sign bits (0 → -scale, 1 →
// +scale, LSB-first within each byte). The pure-Almide decode loop paid
// ~5 s per 2 M-weight tensor because every weight round-tripped through
// Option-returning stdlib helpers (`bytes.get`, `int.band`, `int.bshr`,
// `list.push`) the LLVM backend couldn't fully constant-fold through.
// This primitive takes the packed bytes directly and emits a
// Vec<Vec<f64>> ready for subsequent matrix ops — same role as
// `from_lists`: a format converter, not an algorithmic shortcut.

// Inlined IEEE-754 half-precision → f32 conversion, so this file stays
// self-contained (no `half` crate dep leaking into spec-test builds).
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
        return vec![vec![0.0f64; cols_u]; rows_u].into();
    }
    let off = offset.max(0) as usize;
    let mut flat = Vec::<f64>::with_capacity(total);
    let num_blocks = total / 128;
    for b in 0..num_blocks {
        let block_start = off + b * 18;
        let scale_raw = (data[block_start] as u16) | ((data[block_start + 1] as u16) << 8);
        let scale = fp16_bits_to_f32(scale_raw) as f64;
        let neg_scale = -scale;
        let bits_start = block_start + 2;
        for i in 0..128usize {
            let byte = data[bits_start + (i >> 3)];
            let bit = (byte >> (i & 7)) & 1;
            flat.push(if bit == 1 { scale } else { neg_scale });
        }
    }
    // Reshape flat → Vec<Vec<f64>> of shape (rows, cols).
    let mut out = Vec::<Vec<f64>>::with_capacity(rows_u);
    for r in 0..rows_u {
        let start = r * cols_u;
        let end = start + cols_u;
        out.push(flat[start..end].to_vec());
    }
    out.into()
}

// RoPE (rotary positional embedding) — see comments in the burn variant.
pub fn almide_rt_matrix_rope_rotate(
    x: &AlmideMatrix,
    n_heads: i64,
    head_dim: i64,
    theta_base: f64,
) -> AlmideMatrix {
    almide_rt_matrix_rope_rotate_at(x, n_heads, head_dim, theta_base, 0)
}

// KV-cache variant: row p is treated as absolute position `start_pos + p`,
// so during gen steps (1 new token at a time) the cached K has already
// seen positions 0..start_pos and the new row gets position start_pos.
pub fn almide_rt_matrix_rope_rotate_at(
    x: &AlmideMatrix,
    n_heads: i64,
    head_dim: i64,
    theta_base: f64,
    start_pos: i64,
) -> AlmideMatrix {
    let rows = x.len();
    let cols = if rows == 0 { 0 } else { x[0].len() };
    let n_heads_u = n_heads.max(0) as usize;
    let head_dim_u = head_dim.max(0) as usize;
    let start = start_pos.max(0) as usize;
    let half = head_dim_u / 2;
    let mut inv_freqs = Vec::<f64>::with_capacity(half);
    for i in 0..half {
        let exp = (2 * i) as f64 / head_dim_u as f64;
        inv_freqs.push(1.0 / theta_base.powf(exp));
    }
    let mut out = Vec::<Vec<f64>>::with_capacity(rows);
    for p in 0..rows {
        let pos_f = (start + p) as f64;
        let row = &x[p];
        let mut new_row = vec![0.0f64; cols];
        for h in 0..n_heads_u {
            let head_start = h * head_dim_u;
            for i in 0..half {
                let j0 = head_start + 2 * i;
                let x0 = row[j0];
                let x1 = row[j0 + 1];
                let angle = pos_f * inv_freqs[i];
                let (s, c) = angle.sin_cos();
                new_row[j0] = x0 * c - x1 * s;
                new_row[j0 + 1] = x0 * s + x1 * c;
            }
        }
        out.push(new_row);
    }
    out.into()
}

// append_rows: row-wise concat — used for KV-cache accumulation.
pub fn almide_rt_matrix_append_rows(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    if a.is_empty() { return b.clone(); }
    if b.is_empty() { return a.clone(); }
    let mut out = Vec::<Vec<f64>>::with_capacity(a.len() + b.len());
    out.extend(a.iter().map(|r| r.to_vec()));
    out.extend(b.iter().map(|r| r.to_vec()));
    out.into()
}

// select_rows: gather a small number of rows from a big matrix into a
// new matrix. Avoids the `to_lists` round-trip for the common case of
// embedding lookups (LLM inference).
pub fn almide_rt_matrix_select_rows(m: &AlmideMatrix, row_ids: &[i64]) -> AlmideMatrix {
    let cols = if m.is_empty() { 0 } else { m[0].len() };
    let mut out = Vec::<Vec<f64>>::with_capacity(row_ids.len());
    for &rid in row_ids {
        let r = rid.max(0) as usize;
        if r < m.len() { out.push(m[r].to_vec()); }
        else { out.push(vec![0.0f64; cols]); }
    }
    out.into()
}

// Q1_0 direct matmul: `y[i, j] = Σ_k x[i, k] * W[j, k]` where W is a
// packed Q1_0 tensor still sitting in its source GGUF byte buffer.
// No intermediate decoded matrix is allocated, so per-layer memory
// stays on the order of the activation matrices (kilobytes) instead of
// the ~400 MB that full decode produces for a 2048×2048 weight.
//
// Layout assumption: W has shape (w_rows, w_cols) with `w_cols`
// divisible by 128; blocks are stored row-major
//   [row 0 : n_blocks × 18 B][row 1 : n_blocks × 18 B] ... ,
// starting at `w_offset` within `w_bytes`. Each block = 2 B fp16 scale
// + 16 B sign bits (LSB-first).
// sign-byte → 8-lane ±1.0 lookup table. Precomputed so the hot kernel
// can do a single 64-byte load instead of 8 branches per byte.
static SIGN_LUT: [[f64; 8]; 256] = {
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

// Block dot: Σ_k xi[k] * (±scale) over k ∈ [0, 128), reading 16 sign
// bytes at `sign_bytes` and 128 f64 at `xi`. `scale` is applied once
// at the end so the inner loop does only ±1 FMA.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[inline]
unsafe fn q1_0_block_dot_neon(xi: *const f64, sign_bytes: *const u8, scale: f64) -> f64 {
    use std::arch::aarch64::*;
    let mut a0 = vdupq_n_f64(0.0);
    let mut a1 = vdupq_n_f64(0.0);
    let mut a2 = vdupq_n_f64(0.0);
    let mut a3 = vdupq_n_f64(0.0);
    for byte_idx in 0..16 {
        let byte = *sign_bytes.add(byte_idx) as usize;
        let sig = SIGN_LUT[byte].as_ptr();
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
fn q1_0_block_dot_scalar(xi: &[f64], sign_bytes: &[u8], scale: f64) -> f64 {
    let mut s = 0.0f64;
    for byte_idx in 0..16 {
        let byte = sign_bytes[byte_idx] as usize;
        let sig = &SIGN_LUT[byte];
        let off = byte_idx * 8;
        for k in 0..8 {
            s += xi[off + k] * sig[k];
        }
    }
    scale * s
}

// Q1_0 direct matmul: `y[i, j] = Σ_k x[i, k] * W[j, k]` where W is a
// packed Q1_0 tensor still sitting in its source GGUF byte buffer.
// No intermediate decoded matrix is allocated, so per-layer memory
// stays on the order of the activation matrices (kilobytes) instead of
// the ~400 MB that full decode produces for a 2048×2048 weight.
//
// Layout assumption: W has shape (w_rows, w_cols) with `w_cols`
// divisible by 128; blocks are stored row-major
//   [row 0 : n_blocks × 18 B][row 1 : n_blocks × 18 B] ... ,
// starting at `w_offset` within `w_bytes`. Each block = 2 B fp16 scale
// + 16 B sign bits (LSB-first).
pub fn almide_rt_matrix_linear_q1_0_row_no_bias(
    x: &AlmideMatrix,
    w_bytes: &Vec<u8>,
    w_offset: i64,
    w_rows: i64,
    w_cols: i64,
) -> AlmideMatrix {
    // Routed to almide-kernel: flat x.data straight in, AVX f64 SIMD bit-unpack +
    // signed sum (Almide's own dot is scalar on x86 — NEON-only — so this beats
    // Almide itself on native). Quantized matmul is outside BLAS, so this is a
    // BLAS blind spot almide-kernel owns.
    let x_rows = x.rows;
    let n_in = w_cols.max(0) as usize;
    let out_cols = w_rows.max(0) as usize;
    if x_rows == 0 || out_cols == 0 || n_in == 0 {
        return mk(x_rows, out_cols, vec![0.0f64; x_rows * out_cols]);
    }
    let mut out = vec![0.0f64; x_rows * out_cols];
    almide_kernel::q1_0_packed::linear_q1_0_packed(
        &x.data,
        x_rows,
        n_in,
        w_bytes,
        w_offset.max(0) as usize,
        out_cols,
        &mut out,
    );
    mk(x_rows, out_cols, out)
}

// Elementwise: `y[i, j] = silu(a[i, j]) * b[i, j]` where silu(x) = x * σ(x).
// Used to decompose `swiglu_gate` when we want to feed it through two
// `linear_q1_0_row_no_bias` calls (one for gate, one for up) instead of
// going through the full decoded weight matrices.
pub fn almide_rt_matrix_silu_mul(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    // Routed to almide-kernel: flat a.data/b.data, SIMD fast-exp sigmoid (AVX f64
    // + FMA). exp is the autovec wall (scalar libm call per element), so the SIMD
    // path beats Almide's scalar — fused silu*mul is outside BLAS entirely. 3.58x
    // measured. within-tolerance (exp approximated ~1e-7).
    if a.rows == b.rows && a.cols == b.cols && a.data.len() == b.data.len() {
        let mut out = vec![0.0f64; a.data.len()];
        almide_kernel::silu::silu_mul(&a.data, &b.data, &mut out);
        return mk(a.rows, a.cols, out);
    }
    // shape mismatch fallback (ragged): keep the elementwise definition
    let rows = a.len();
    let mut out = Vec::<Vec<f64>>::with_capacity(rows);
    for i in 0..rows {
        let ai = &a[i];
        let bi = &b[i];
        let cols = ai.len().min(bi.len());
        let mut row = vec![0.0f64; cols];
        for j in 0..cols {
            let x = ai[j];
            let sig = 1.0f64 / (1.0 + (-x).exp());
            row[j] = x * sig * bi[j];
        }
        out.push(row);
    }
    out.into()
}

// Per-row Q1_0 decoder: extract only the requested row ids directly
// from the packed bytes, without ever materialising the full matrix.
// For 5 prompt tokens this allocates ~80 KB instead of the 2.5 GB a
// full `token_embd` decode needs.
pub fn almide_rt_matrix_select_rows_q1_0(
    data: &Vec<u8>,
    offset: i64,
    cols: i64,
    row_ids: &[i64],
) -> AlmideMatrix {
    let cols_u = cols.max(0) as usize;
    let n_blocks = cols_u / 128;
    let off = offset.max(0) as usize;
    let mut out = Vec::<Vec<f64>>::with_capacity(row_ids.len());
    for &rid in row_ids {
        let r = rid.max(0) as usize;
        let row_off = off + r * n_blocks * 18;
        let mut row = vec![0.0f64; cols_u];
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
                row[b * 128 + local_k] = if bit == 1 { scale } else { neg_scale };
            }
        }
        out.push(row);
    }
    out.into()
}

// ── Qwen3 block super-intrinsic (Q1_0 + KV cache) ──
//
// One decoder layer of a Qwen3 / Bonsai-1.7B model, expressed as a
// single Rust fn so the middle Matrix allocations that Almide-level
// composition otherwise pays for disappear. Mirrors
// `nn.qwen3.qwen3_block` but wires in the KV-cache accumulator
// (append_rows) and accepts weights as raw bytes + offsets to keep the
// caller's argument list manageable.

// Load a packed f32 sub-range from w_bytes as f64, for gamma-style
// 1-D tensors that don't go through a Matrix intrinsic.
fn load_f32_to_f64(w: &[u8], offset: usize, len: usize) -> Vec<f64> {
    let mut out = Vec::<f64>::with_capacity(len);
    for i in 0..len {
        let p = offset + i * 4;
        let bits = u32::from_le_bytes([w[p], w[p + 1], w[p + 2], w[p + 3]]);
        out.push(f32::from_bits(bits) as f64);
    }
    out.into()
}

fn per_head_rms_norm(
    x: &AlmideMatrix,
    gamma: &[f64],
    n_heads: i64,
    eps: f64,
) -> AlmideMatrix {
    let n_heads = n_heads.max(1) as usize;
    if x.is_empty() { return vec![].into(); }
    let d = x[0].len();
    let head_dim = d / n_heads;
    let mut out = Vec::<Vec<f64>>::with_capacity(x.len());
    for row in x.iter() {
        let mut out_row = vec![0.0f64; d];
        for h in 0..n_heads {
            let start = h * head_dim;
            let mut ss = 0.0f64;
            for k in 0..head_dim {
                let v = row[start + k];
                ss += v * v;
            }
            let inv = 1.0f64 / (ss / head_dim as f64 + eps).sqrt();
            for k in 0..head_dim {
                out_row[start + k] = row[start + k] * inv * gamma[k];
            }
        }
        out.push(out_row);
    }
    out.into()
}

fn repeat_kv(kv: &AlmideMatrix, n_kv_heads: i64, n_rep: i64) -> AlmideMatrix {
    if n_rep <= 1 { return kv.clone(); }
    let n_kv = n_kv_heads.max(1) as usize;
    let n_rep_u = n_rep as usize;
    if kv.is_empty() { return vec![].into(); }
    let d = kv[0].len();
    let head_dim = d / n_kv;
    let out_cols = d * n_rep_u;
    let mut out = Vec::<Vec<f64>>::with_capacity(kv.len());
    for row in kv.iter() {
        let mut out_row = vec![0.0f64; out_cols];
        for h in 0..n_kv {
            let src = h * head_dim;
            for r in 0..n_rep_u {
                let dst = (h * n_rep_u + r) * head_dim;
                out_row[dst..dst + head_dim].copy_from_slice(&row[src..src + head_dim]);
            }
        }
        out.push(out_row);
    }
    out.into()
}

pub fn almide_rt_matrix_qwen3_block_q1_0_kv(
    h: &AlmideMatrix,
    k_cache: &AlmideMatrix,
    v_cache: &AlmideMatrix,
    w: &Vec<u8>,
    gamma_offs: &[i64],   // [attn_norm, q_norm, k_norm, ffn_norm]
    weight_offs: &[i64],  // [q, k, v, o, gate, up, down]
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

    let gamma_attn = load_f32_to_f64(w, gamma_offs[0].max(0) as usize, hidden);
    let gamma_q = load_f32_to_f64(w, gamma_offs[1].max(0) as usize, head_dim.max(0) as usize);
    let gamma_k = load_f32_to_f64(w, gamma_offs[2].max(0) as usize, head_dim.max(0) as usize);
    let gamma_ffn = load_f32_to_f64(w, gamma_offs[3].max(0) as usize, hidden);

    let h_normed = almide_rt_matrix_rms_norm_rows(h, &gamma_attn, eps);

    let q_proj = almide_rt_matrix_linear_q1_0_row_no_bias(
        &h_normed, w, weight_offs[0], hidden as i64, hidden as i64);
    let k_proj = almide_rt_matrix_linear_q1_0_row_no_bias(
        &h_normed, w, weight_offs[1], kv_hidden as i64, hidden as i64);
    let v_proj = almide_rt_matrix_linear_q1_0_row_no_bias(
        &h_normed, w, weight_offs[2], kv_hidden as i64, hidden as i64);

    let q_normed = per_head_rms_norm(&q_proj, &gamma_q, n_q_heads, eps);
    let k_normed = per_head_rms_norm(&k_proj, &gamma_k, n_kv_heads, eps);

    let q_rot = almide_rt_matrix_rope_rotate_at(&q_normed, n_q_heads, head_dim, rope_theta, start_pos);
    let k_rot = almide_rt_matrix_rope_rotate_at(&k_normed, n_kv_heads, head_dim, rope_theta, start_pos);

    let k_full_kv = almide_rt_matrix_append_rows(k_cache, &k_rot);
    let v_full_kv = almide_rt_matrix_append_rows(v_cache, &v_proj);

    let k_full = repeat_kv(&k_full_kv, n_kv_heads, n_rep);
    let v_full = repeat_kv(&v_full_kv, n_kv_heads, n_rep);

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


// ── f32 bytes-resident path (L2-2): weights stay in the source GGUF buffer ──
//
// Mirror of the Q1_0 path for plain f32 tensors: no decoded AlmideMatrix is
// ever materialised for weights, so the model record (and its per-call deep
// clones — measured at ~13 GB per generated token) disappears entirely.

/// `y = x @ Wᵀ` where W is f32 row-major (out, in) still sitting in the
/// source byte buffer at `w_offset`.
pub fn almide_rt_matrix_linear_f32_row_no_bias(
    x: &AlmideMatrix,
    w_bytes: &Vec<u8>,
    w_offset: i64,
    w_rows: i64,
    w_cols: i64,
) -> AlmideMatrix {
    let x_rows = x.rows;
    let n_in = w_cols.max(0) as usize;
    let out_cols = w_rows.max(0) as usize;
    let off = w_offset.max(0) as usize;
    if x_rows == 0 || out_cols == 0 || n_in == 0 {
        return mk(x_rows, out_cols, vec![0.0f64; x_rows * out_cols]);
    }
    let mut out = vec![0.0f64; x_rows * out_cols];
    // Fast path: reinterpret the (4-byte-aligned, little-endian) weight bytes
    // as &[f32] so the inner dot auto-vectorizes (cvtps2pd + fma), and split
    // the output row across threads — GEMV is embarrassingly parallel over
    // output elements. Falls back to per-element decode when misaligned.
    let w_all = &w_bytes[off..off + out_cols * n_in * 4];
    let (head, w_f32, _) = unsafe { w_all.align_to::<f32>() };
    if head.is_empty() && w_f32.len() == out_cols * n_in {
        use rayon::prelude::*;
        // Chunked parallelism: one task per ~64 output elements (≈64·n_in
        // MACs ≈ 30–130 µs). Per-ELEMENT tasks measured ~50% of samples in
        // condvar parking — the dot is only ~0.5 µs, below rayon's dispatch
        // overhead.
        const CHUNK: usize = 64;
        for i in 0..x_rows {
            let xi = &x.data[i * n_in..(i + 1) * n_in];
            out[i * out_cols..(i + 1) * out_cols]
                .par_chunks_mut(CHUNK)
                .enumerate()
                .for_each(|(ci, oc)| {
                    let j0 = ci * CHUNK;
                    for (dj, o) in oc.iter_mut().enumerate() {
                        let j = j0 + dj;
                        let wj = &w_f32[j * n_in..(j + 1) * n_in];
                        let mut acc = 0.0f64;
                        for k in 0..n_in {
                            acc += xi[k] * wj[k] as f64;
                        }
                        *o = acc;
                    }
                });
        }
    } else {
        for j in 0..out_cols {
            let row_off = off + j * n_in * 4;
            let wj = &w_bytes[row_off..row_off + n_in * 4];
            for i in 0..x_rows {
                let xi = &x.data[i * n_in..(i + 1) * n_in];
                let mut acc = 0.0f64;
                for (k, c) in wj.chunks_exact(4).enumerate() {
                    acc += xi[k] * f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64;
                }
                out[i * out_cols + j] = acc;
            }
        }
    }
    mk(x_rows, out_cols, out)
}

/// Gather rows from an f32 tensor still in the source byte buffer
/// (embedding lookup without decoding the 600 MB table).
pub fn almide_rt_matrix_select_rows_f32(
    data: &Vec<u8>,
    offset: i64,
    cols: i64,
    row_ids: &[i64],
) -> AlmideMatrix {
    let c = cols.max(0) as usize;
    let off = offset.max(0) as usize;
    let mut out = Vec::<f64>::with_capacity(row_ids.len() * c);
    for &rid in row_ids {
        let base = off + (rid.max(0) as usize) * c * 4;
        for ch in data[base..base + c * 4].chunks_exact(4) {
            out.push(f32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]]) as f64);
        }
    }
    mk(row_ids.len(), c, out)
}

/// NeoX / HF-convention RoPE: rotates pairs (j, j+half) within each head,
/// angle = (start_pos + row) * theta^(-2j/head_dim). With this variant the
/// loader-side weight-row permutation (NeoX → interleaved) is unnecessary —
/// weights are consumed exactly as stored.
pub fn almide_rt_matrix_rope_rotate_neox_at(
    x: &AlmideMatrix,
    n_heads: i64,
    head_dim: i64,
    theta_base: f64,
    start_pos: i64,
) -> AlmideMatrix {
    let rows = x.len();
    let cols = if rows == 0 { 0 } else { x[0].len() };
    let n_heads_u = n_heads.max(0) as usize;
    let head_dim_u = head_dim.max(0) as usize;
    let start = start_pos.max(0) as usize;
    let half = head_dim_u / 2;
    let mut inv_freqs = Vec::<f64>::with_capacity(half);
    for j in 0..half {
        let exp = (2 * j) as f64 / head_dim_u as f64;
        inv_freqs.push(1.0 / theta_base.powf(exp));
    }
    let mut out = Vec::<Vec<f64>>::with_capacity(rows);
    for p in 0..rows {
        let pos_f = (start + p) as f64;
        let row = &x[p];
        let mut new_row = vec![0.0f64; cols];
        for h in 0..n_heads_u {
            let head_start = h * head_dim_u;
            for j in 0..half {
                let j0 = head_start + j;
                let j1 = head_start + half + j;
                let x0 = row[j0];
                let x1 = row[j1];
                let angle = pos_f * inv_freqs[j];
                let (s, c) = angle.sin_cos();
                new_row[j0] = x0 * c - x1 * s;
                new_row[j1] = x0 * s + x1 * c;
            }
        }
        out.push(new_row);
    }
    out.into_iter().collect()
}

/// One Qwen3 decoder layer over f32 weights resident in the source buffer.
/// Mirror of `qwen3_block_q1_0_kv` with two fixes for real Qwen3 checkpoints:
/// hidden_size is taken from `h` (Qwen3-0.6B has hidden 1024 ≠ n_heads*head_dim
/// = 2048), and RoPE uses the NeoX convention so weights need no permutation.
/// Handles both prefill (multi-row h, causal mask) and decode (single row) —
/// the masked-MHA offset rule `j > (sk - sq) + i` covers both.
pub fn almide_rt_matrix_qwen3_block_f32_kv(
    h: &AlmideMatrix,
    k_cache: &AlmideMatrix,
    v_cache: &AlmideMatrix,
    w: &Vec<u8>,
    gamma_offs: &[i64],   // [attn_norm, q_norm, k_norm, ffn_norm]
    weight_offs: &[i64],  // [q, k, v, o, gate, up, down]
    start_pos: i64,
    n_q_heads: i64,
    n_kv_heads: i64,
    head_dim: i64,
    ffn_hidden: i64,
    rope_theta: f64,
    eps: f64,
) -> (AlmideMatrix, AlmideMatrix, AlmideMatrix) {
    let hidden = if h.rows > 0 { h.cols } else { 0 };
    let q_hidden = (n_q_heads * head_dim) as usize;
    let kv_hidden = (n_kv_heads * head_dim) as usize;
    let n_rep = n_q_heads / n_kv_heads;

    let gamma_attn = load_f32_to_f64(w, gamma_offs[0].max(0) as usize, hidden);
    let gamma_q = load_f32_to_f64(w, gamma_offs[1].max(0) as usize, head_dim.max(0) as usize);
    let gamma_k = load_f32_to_f64(w, gamma_offs[2].max(0) as usize, head_dim.max(0) as usize);
    let gamma_ffn = load_f32_to_f64(w, gamma_offs[3].max(0) as usize, hidden);

    let h_normed = almide_rt_matrix_rms_norm_rows(h, &gamma_attn, eps);

    let q_proj = almide_rt_matrix_linear_f32_row_no_bias(
        &h_normed, w, weight_offs[0], q_hidden as i64, hidden as i64);
    let k_proj = almide_rt_matrix_linear_f32_row_no_bias(
        &h_normed, w, weight_offs[1], kv_hidden as i64, hidden as i64);
    let v_proj = almide_rt_matrix_linear_f32_row_no_bias(
        &h_normed, w, weight_offs[2], kv_hidden as i64, hidden as i64);

    let q_normed = per_head_rms_norm(&q_proj, &gamma_q, n_q_heads, eps);
    let k_normed = per_head_rms_norm(&k_proj, &gamma_k, n_kv_heads, eps);

    let q_rot = almide_rt_matrix_rope_rotate_neox_at(&q_normed, n_q_heads, head_dim, rope_theta, start_pos);
    let k_rot = almide_rt_matrix_rope_rotate_neox_at(&k_normed, n_kv_heads, head_dim, rope_theta, start_pos);

    let k_full_kv = if k_cache.rows == 0 { k_rot } else { almide_rt_matrix_append_rows(k_cache, &k_rot) };
    let v_full_kv = if v_cache.rows == 0 { v_proj } else { almide_rt_matrix_append_rows(v_cache, &v_proj) };

    let k_full = repeat_kv(&k_full_kv, n_kv_heads, n_rep);
    let v_full = repeat_kv(&v_full_kv, n_kv_heads, n_rep);

    let attn_out = almide_rt_matrix_masked_multi_head_attention(&q_rot, &k_full, &v_full, n_q_heads);
    let attn_proj = almide_rt_matrix_linear_f32_row_no_bias(
        &attn_out, w, weight_offs[3], hidden as i64, q_hidden as i64);
    let x_attn = almide_rt_matrix_add(h, &attn_proj);

    let h2 = almide_rt_matrix_rms_norm_rows(&x_attn, &gamma_ffn, eps);
    let gate = almide_rt_matrix_linear_f32_row_no_bias(
        &h2, w, weight_offs[4], ffn_hidden, hidden as i64);
    let up = almide_rt_matrix_linear_f32_row_no_bias(
        &h2, w, weight_offs[5], ffn_hidden, hidden as i64);
    let gated = almide_rt_matrix_silu_mul(&gate, &up);
    let ffn_out = almide_rt_matrix_linear_f32_row_no_bias(
        &gated, w, weight_offs[6], hidden as i64, ffn_hidden);

    let h_out = almide_rt_matrix_add(&x_attn, &ffn_out);
    (h_out, k_full_kv, v_full_kv)
}
