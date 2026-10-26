
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

// f64 -> IEEE half bits, round-to-nearest-even straight from the f64 (no f32
// double rounding); overflow is +-inf, a NaN is the canonical quiet 0x7E00.
// stdlib/matrix_shape.almd's self-host runs the same integer steps.
fn almide_f64_to_f16_bits(x: f64) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 63) as u16) << 15;
    let exp = ((bits >> 52) & 0x7ff) as i64;
    let mant = bits & ((1u64 << 52) - 1);
    if exp == 0x7ff {
        return if mant != 0 { 0x7E00 } else { sign | 0x7C00 };
    }
    if exp == 0 {
        return sign; // f64 subnormals are far below the f16 range
    }
    let e = exp - 1023 + 15; // the f16 biased exponent
    let m = (1u64 << 52) | mant; // 53-bit significand
    // A normal result keeps 11 significant bits; a subnormal keeps fewer.
    let shift = if e >= 1 { 42 } else { 42 + (1 - e) };
    if shift > 53 {
        return sign; // below half the smallest subnormal
    }
    let q = m >> shift;
    let r = m & ((1u64 << shift) - 1);
    let half = 1u64 << (shift - 1);
    let q = if r > half || (r == half && (q & 1) == 1) { q + 1 } else { q };
    let out = if e >= 1 { (((e as u64) << 10) + (q - (1 << 10))) as u64 } else { q };
    if out >= 0x7C00 { sign | 0x7C00 } else { sign | out as u16 }
}

// to_bytes_f16_le (R-matrix): row-major half floats, 2 bytes each, rounded.
pub fn almide_rt_matrix_to_bytes_f16_le(m: &AlmideMatrix) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(m.data.len() * 2);
    for v in m.data.iter() {
        out.extend_from_slice(&almide_f64_to_f16_bits(*v).to_le_bytes());
    }
    out.into()
}

pub fn almide_rt_matrix_from_bytes_f64_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> Option<AlmideMatrix> {
    // #1503: negative dims clamp to the empty matrix (C-034/C-161), a negative
    // offset is OOB by definition, and the window is judged buffer-side
    // (C-341) — same discipline as the f32/f16 loaders in matrix.rs.
    let (r, c) = almide_rt_matrix_dims(rows, cols);
    let need = r * c * 8;
    if !almide_rt_matrix_window_fits(data.len(), offset, need) {
        return None;
    }
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
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
    Some(result.into())
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

pub fn almide_rt_matrix_mul_scaled(a: &AlmideMatrix, alpha: f64, b: &AlmideMatrix) -> AlmideMatrix {
    almide_rt_matrix_scale(&almide_rt_matrix_mul(a, b), alpha)
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
