
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
    // #1503: negative dims clamp to the empty matrix (C-034/C-161), a negative
    // offset is OOB by definition, and the bounds compare is overflow-safe —
    // same discipline as the f32/f16 loaders in matrix.rs.
    let (r, c) = almide_rt_matrix_dims(rows, cols);
    let need = r * c * 8;
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
    if offset < 0 || (offset as u128) + (need as u128) > data.len() as u128 {
        for _ in 0..r { result.push(vec![0.0f64; c]); }
        return result.into();
    }
    let off = offset as usize;
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

// append_rows: row-wise concat — used for KV-cache accumulation.
pub fn almide_rt_matrix_append_rows(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    if a.is_empty() { return b.clone(); }
    if b.is_empty() { return a.clone(); }
    // Row-wise concat needs one width: a different one left the flat store
    // ragged (its assertion panic, exit 101).
    almide_rt_matrix_shape_eq(b.cols, a.cols);
    let mut out = Vec::<Vec<f64>>::with_capacity(a.len() + b.len());
    out.extend(a.iter().map(|r| r.to_vec()));
    out.extend(b.iter().map(|r| r.to_vec()));
    out.into()
}

// concat_rows (R-matrix-11): append_rows folded over the list, so an empty
// member is skipped and two non-empty members of different widths abort.
// Generic over the element so a list of shared (AlmideRcCow) matrices passes
// as it is, with no per-call-site element unwrapping.
pub fn almide_rt_matrix_concat_rows<M: std::ops::Deref<Target = AlmideMatrix>>(xs: &[M]) -> AlmideMatrix {
    xs.iter().fold(mk(0, 0, Vec::new()), |acc, m| almide_rt_matrix_append_rows(&acc, m))
}

// zip_with (R-matrix-09): f over same-position elements; any shape mismatch
// aborts (the R-matrix-05 rule), never a truncation to the overlap.
pub fn almide_rt_matrix_zip_with(a: &AlmideMatrix, b: &AlmideMatrix, f: std::rc::Rc<dyn Fn(f64, f64) -> f64>) -> AlmideMatrix {
    almide_rt_matrix_shape_eq(a.rows, b.rows);
    almide_rt_matrix_shape_eq(a.cols, b.cols);
    mk(a.rows, a.cols, a.data.iter().zip(b.data.iter()).map(|(x, y)| f(*x, *y)).collect())
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

// ── f32 bytes-resident path (L2-2): weights stay in the source GGUF buffer ──
//
// Mirror of the Q1_0 path for plain f32 tensors: no decoded AlmideMatrix is
// ever materialised for weights, so the model record (and its per-call deep
// clones — measured at ~13 GB per generated token) disappears entirely.

// Parallel sweep over output elements. Two implementations:
// - `almide_par` (cargo builds, see cli/mod.rs): rayon's persistent pool.
//   Per-call scoped threads measured 2.8 tok/s vs rayon's 4.9 — the spawn
//   cost per GEMV region beats the work for the small projections.
// - otherwise (raw-rustc test harness, no external crates): serial. Tests
//   need correctness, not throughput.
#[cfg(almide_par)]
fn par_out_rows(out: &mut [f64], f: impl Fn(usize, &mut f64) + Sync) {
    use rayon::prelude::*;
    if out.len() < 256 {
        for (j, o) in out.iter_mut().enumerate() {
            f(j, o);
        }
        return;
    }
    const CHUNK: usize = 64;
    out.par_chunks_mut(CHUNK).enumerate().for_each(|(ci, oc)| {
        let j0 = ci * CHUNK;
        for (dj, o) in oc.iter_mut().enumerate() {
            f(j0 + dj, o);
        }
    });
}

#[cfg(not(almide_par))]
fn par_out_rows(out: &mut [f64], f: impl Fn(usize, &mut f64) + Sync) {
    for (j, o) in out.iter_mut().enumerate() {
        f(j, o);
    }
}

/// The empty-extent answer of the `linear_*_row_no_bias` family: the all-zero
/// `rows(x) × w_rows` matrix, i.e. `zeros(rows(x), w_rows)` — so its shape
/// clamps and ceilings through `almide_rt_matrix_dims` like every constructor
/// (C-161) instead of reaching `vec!` unchecked (#2783).
fn linear_empty_out(x_rows: usize, w_rows: i64) -> AlmideMatrix {
    let (r, c) = almide_rt_matrix_dims(x_rows as i64, w_rows);
    mk(r, c, vec![0.0f64; r * c])
}

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
    let off = w_offset.max(0) as usize;
    // #2783: the empty-extent answer is `zeros(rows(x), w_rows)`, so its shape
    // goes through the shared C-161 ceiling; a huge `w_rows` took the raw
    // `capacity overflow` panic here (exit 101) or returned an over-ceiling
    // matrix, where wasm aborts with `matrix dimensions too large`.
    if x_rows == 0 || w_rows <= 0 || w_cols <= 0 {
        return linear_empty_out(x_rows, w_rows);
    }
    // The weight shape is a constructor's (`from_bytes_f32_le(.., w_rows,
    // w_cols)` in the self-hosted definition): ceilinged before the width check.
    let (out_cols, n_in) = almide_rt_matrix_dims(w_rows, w_cols);
    almide_rt_matrix_shape_eq(x.cols, n_in);
    // The weight window must be IN the buffer. Out of it, the answer is the
    // all-zero output — the same edge the unfused composition takes, because
    // `from_bytes_f32_le` over a window past the end is the all-zero matrix
    // (C-341) and a zero weight makes a zero product. Without the test the
    // slice below was a raw `range end index N out of range` panic (exit 101,
    // the form ALS-T6 forbids) on a buffer one element short.
    let need = out_cols.saturating_mul(n_in).saturating_mul(4);
    if off > w_bytes.len() || need > w_bytes.len() - off {
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
        for i in 0..x_rows {
            let xi = &x.data[i * n_in..(i + 1) * n_in];
            par_out_rows(&mut out[i * out_cols..(i + 1) * out_cols], |j, o| {
                let wj = &w_f32[j * n_in..(j + 1) * n_in];
                let mut acc = 0.0f64;
                for k in 0..n_in {
                    acc += xi[k] * wj[k] as f64;
                }
                *o = acc;
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
    // #1525: ceiling the output shape like every constructor — an over-ceiling
    // `cols` died in the raw capacity-overflow form here.
    let (_, c) = almide_rt_matrix_dims(row_ids.len() as i64, cols);
    let off = offset.max(0) as usize;
    let mut out = Vec::<f64>::with_capacity(row_ids.len() * c);
    for &rid in row_ids {
        // A row past the buffer is the all-zero row — the same defined
        // OOB→zeros edge `from_bytes_f32_le` (and the decoded-matrix
        // `select_rows`) contract; an unchecked slice here was a raw panic
        // while the wasm leg read undefined bytes (the 2026-08-12 fuzz
        // divergence, run 31515916427).
        let Some(base) = select_row_window(off, rid, c * 4, data.len()) else {
            out.extend(std::iter::repeat(0.0f64).take(c));
            continue;
        };
        for ch in data[base..base + c * 4].chunks_exact(4) {
            out.push(f32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]]) as f64);
        }
    }
    mk(row_ids.len(), c, out)
}

/// The byte start of selected row `rid` (clamped to 0) when its whole
/// `row_bytes` window `off + rid·row_bytes ..+ row_bytes` lies inside a
/// `len`-byte buffer; `None` = the all-zero row. Checked, not wrapped: an
/// `rid` near i64::MAX wrapped `rid·row_bytes` to just below `off`, passed the
/// bound, and sliced from a wrapped start (#2676, fuzz seed 579914833587
/// index 9386).
fn select_row_window(off: usize, rid: i64, row_bytes: usize, len: usize) -> Option<usize> {
    let base = (rid.max(0) as usize).checked_mul(row_bytes)?.checked_add(off)?;
    (base.checked_add(row_bytes)? <= len).then_some(base)
}
