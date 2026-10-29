// matrix extern — Rust native implementations
// Pure implementation using Vec<Vec<f64>> (no external dependencies)
// Will be replaced with ndarray for almide build, burn for --target cuda

// Flat, row-major matrix — the ABI almide-kernel needs (no nested→flat copy).
// Compatibility traits below keep existing `m[r][c]`, `m.iter()`, and
// `collect()` call sites working, so the 64 ops mostly stay as written.
#[derive(Clone, Debug, PartialEq)]
pub struct AlmideMatrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>, // row-major, length rows*cols
}

#[inline]
pub fn mk(rows: usize, cols: usize, data: Vec<f64>) -> AlmideMatrix {
    debug_assert_eq!(data.len(), rows * cols);
    AlmideMatrix { rows, cols, data }
}

impl AlmideMatrix {
    #[inline]
    pub fn len(&self) -> usize { self.rows } // row count (matches old Vec<Vec<>> semantics)
    #[inline]
    pub fn is_empty(&self) -> bool { self.rows == 0 }
    /// The rows, in order — exactly `rows` of them. NOT `data.chunks(cols)`:
    /// an r×0 matrix (`zeros(3, 0)`, a `from_bytes_*` with a clamped
    /// negative width) has empty data, and `chunks` yielded ZERO rows for
    /// it while `rows()` still said r — `to_lists` printed `[]` where the
    /// wasm leg printed `[[], [], []]` (differential fuzz, seed 544254236199
    /// index 170). Indexing by row keeps the shape the header declares.
    #[inline]
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &[f64]> + ExactSizeIterator + '_ {
        (0..self.rows).map(move |r| &self[r])
    }
}

impl std::ops::Index<usize> for AlmideMatrix {
    type Output = [f64];
    #[inline]
    fn index(&self, r: usize) -> &[f64] {
        &self.data[r * self.cols..(r + 1) * self.cols]
    }
}

impl std::ops::IndexMut<usize> for AlmideMatrix {
    #[inline]
    fn index_mut(&mut self, r: usize) -> &mut [f64] {
        let c = self.cols;
        &mut self.data[r * c..(r + 1) * c]
    }
}

// `(0..rows).map(|r| (0..cols).map(..).collect::<Vec<f64>>()).collect()` keeps working.
impl FromIterator<Vec<f64>> for AlmideMatrix {
    fn from_iter<I: IntoIterator<Item = Vec<f64>>>(iter: I) -> Self {
        let rows: Vec<Vec<f64>> = iter.into_iter().collect();
        let r = rows.len();
        let c = if r > 0 { rows[0].len() } else { 0 };
        mk(r, c, rows.into_iter().flatten().collect())
    }
}

// `vec![vec![x; cols]; rows].into()` keeps working.
impl From<Vec<Vec<f64>>> for AlmideMatrix {
    fn from(v: Vec<Vec<f64>>) -> Self {
        v.into_iter().collect()
    }
}

// Almide-literal repr for compound string interpolation (a derived record
// `Repr` over a Matrix field calls `self.w.almide_repr()`). Almide has no
// matrix literal, so a matrix renders in CONSTRUCTOR form
// `matrix.from_lists([[1, 2], [3, 4]])` — the Set precedent
// (`set.from_list([…])`) — rows in row-major order, elements via the same
// Display path as bare `${f}` interpolation.
impl AlmideRepr for AlmideMatrix {
    fn almide_repr(&self) -> String {
        let mut o = String::from("matrix.from_lists([");
        for r in 0..self.rows {
            if r > 0 { o.push_str(", "); }
            o.push('[');
            for c in 0..self.cols {
                if c > 0 { o.push_str(", "); }
                o.push_str(&format!("{}", self.data[r * self.cols + c]));
            }
            o.push(']');
        }
        o.push_str("])");
        o
    }
}

/// Largest element count a dimension-taking matrix constructor will allocate:
/// 2^28 f64 = 2 GiB, comfortably inside wasm32's 4 GiB address space so BOTH
/// targets accept exactly the same dimensions. Above it, both abort in the T6
/// form instead of diverging into a native `capacity overflow` panic (exit 101)
/// vs a wasm out-of-bounds trap (exit 134) — #849. Keep in sync with
/// `__mx_dims` in stdlib/matrix_core.almd.
pub const ALMIDE_MATRIX_MAX_ELEMS: i64 = 1 << 28;

/// Normalize a constructor's `(rows, cols)`: a NEGATIVE dimension clamps to 0
/// (the empty matrix), matching the C-034 signed-clamp rule the `repeat` family
/// already follows; an element count over the shared ceiling aborts.
pub fn almide_rt_matrix_dims(rows: i64, cols: i64) -> (usize, usize) {
    let r = rows.max(0);
    let c = cols.max(0);
    // The ROW COUNT is bounded ALONE, not only the r*c product: `zeros(2^31, -7)`
    // has 0 elements after the cols clamp but still asks for 2^31 ROW headers —
    // 16 GiB of row-pointer array on wasm (OOM kill) while native's unused-value
    // allocation was elided by LLVM (ran clean): two verdicts for one program
    // (differential fuzz, 2026-08-10 night). Rows over the element ceiling can
    // never produce an in-ceiling matrix anyway (cols >= 1 overflows the product;
    // cols = 0 is useless at that scale), so both targets die in the T6 form.
    if r > ALMIDE_MATRIX_MAX_ELEMS || r.saturating_mul(c) > ALMIDE_MATRIX_MAX_ELEMS {
        almide_abort("matrix dimensions too large");
    }
    (r as usize, c as usize)
}

/// The SHAPE precondition of the two-operand kernels (#2481 / #2482 / #2483):
/// two extents that the kernel indexes against each other must be EQUAL —
/// `cols(a) == rows(b)` for `mul`, `cols(x) == cols(weight)` and
/// `len(bias) == rows(weight)` for `linear_row`, the
/// weight width and bias length of `conv1d`, every member's row count in
/// `concat_cols`, and `cols(extra) == cols(base)` in `append_rows`. Violating
/// it used to be a RAW slice panic natively (exit 101, the form ALS-T6
/// forbids) against a silently TRUNCATED product (`mul` clamped the inner sum
/// to `min(cols(a), rows(b))`), a missing bias read as 0.0, or an out-of-block
/// read on the self-hosted leg — a shape that does not fit never has a
/// plausible answer, so it aborts in the unified T6 form on both targets, the
/// same family as the head-count (C-198) and index (C-282) domains. The
/// EMPTY-operand short-circuits each kernel already had stay in front of the
/// guard (C-278's "the empty matrix has no row to violate"): `mul(2x3, 0x0)`
/// is still the 2x0 matrix, not an abort.
#[inline]
pub fn almide_rt_matrix_shape_eq(a: usize, b: usize) {
    if a != b {
        almide_abort("matrix shape mismatch");
    }
}

/// `matrix.from_lists` (#2482): every row must have the FIRST row's width — a
/// ragged list of lists is not a matrix. The flat store asserted
/// `data.len() == rows * cols` (a raw `assertion left == right failed` panic,
/// exit 101) while the wasm leg, a list of rows with no shape invariant, built
/// the ragged value and read on. C-282 already stated that no public
/// constructor builds a ragged matrix; this is the guard that makes it so.
pub fn almide_rt_matrix_rows_uniform(rows: &[Vec<f64>]) {
    if let Some(first) = rows.first() {
        if rows.iter().any(|r| r.len() != first.len()) {
            almide_abort("matrix rows must have equal length");
        }
    }
}

pub fn almide_rt_matrix_zeros(rows: i64, cols: i64) -> AlmideMatrix {
    let (r, c) = almide_rt_matrix_dims(rows, cols);
    // Row-lazy on purpose (#1532 confirm night, seed 514359535655/2163):
    // `vec![vec![0.0; c]; r]` evaluates the ROW expression once even at
    // r == 0, and (0, i64::MAX) passes the dims guard with zero elements —
    // the eager row was a raw capacity-overflow panic where the wasm leg
    // answered the empty matrix.
    (0..r).map(|_| vec![0.0; c]).collect::<Vec<_>>().into()
}

pub fn almide_rt_matrix_ones(rows: i64, cols: i64) -> AlmideMatrix {
    let (r, c) = almide_rt_matrix_dims(rows, cols);
    // Row-lazy, same as `zeros` — see the note there.
    (0..r).map(|_| vec![1.0; c]).collect::<Vec<_>>().into()
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

/// The INDEX-DOMAIN rule for the element accessor (C-282): `Some(value)` in
/// range, `None` out of range, NEGATIVE included. An out-of-range row/col used
/// to be a RAW index on both legs: a Rust slice panic (exit 101) natively
/// against a wasm OOB trap (exit 134), and for a NEGATIVE index the wasm leg
/// returned whatever heap bytes it landed on, exit 0. Compared as i64 BEFORE
/// any cast, so a negative can never wrap past the test. (Before stdlib
/// redesign R-matrix-08 an out-of-range index aborted.)
#[inline]
pub fn almide_rt_matrix_in_range(idx: i64, extent: usize) -> bool {
    idx >= 0 && (idx as u64) < extent as u64
}

pub fn almide_rt_matrix_get(m: &AlmideMatrix, row: i64, col: i64) -> Option<f64> {
    if !almide_rt_matrix_in_range(row, m.len()) { return None; }
    let r = &m[row as usize];
    // The COL extent is this row's own width, so a ragged matrix is exact.
    if !almide_rt_matrix_in_range(col, r.len()) { return None; }
    Some(r[col as usize])
}

/// C-341: the byte window fits exactly when `0 <= offset <= len` and
/// `need <= len - offset` — judged on the buffer side, never as
/// `offset + need`, which wraps for an offset near i64::MAX.
#[inline]
pub fn almide_rt_matrix_window_fits(len: usize, offset: i64, need: usize) -> bool {
    offset >= 0 && (offset as u64) <= len as u64 && need as u64 <= len as u64 - offset as u64
}

pub fn almide_rt_matrix_transpose(m: &AlmideMatrix) -> AlmideMatrix {
    // flat → almide-kernel f64 SIMD directly (no nested conversion): AVX f64x4 /
    // wasm simd128 / naive, bitwise-exact, statically proven the transpose
    // permutation for all inputs. This is the flat ABI win — the kernel reads
    // m.data straight, no copy.
    if m.rows == 0 || m.cols == 0 {
        return mk(0, 0, vec![]);
    }
    let mut out = vec![0.0f64; m.rows * m.cols];
    almide_kernel::transpose_f64::transpose_matrix_f64(&m.data, m.rows, m.cols, &mut out);
    mk(m.cols, m.rows, out) // transposed shape is cols × rows
}

pub fn almide_rt_matrix_from_lists(rows: &[Vec<f64>]) -> AlmideMatrix {
    almide_rt_matrix_rows_uniform(rows);
    rows.to_vec().into()
}

pub fn almide_rt_matrix_from_bytes_f32_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> Option<AlmideMatrix> {
    // #1503: a negative dimension clamps to the empty matrix (the C-034 /
    // C-161 rule the constructors already follow) and a negative offset is
    // out of bounds by definition — the raw `as usize` casts turned either
    // into a multi-exabyte size and died in the forbidden T6 form (capacity
    // overflow, exit 101). `almide_rt_matrix_dims` also enforces the element
    // ceiling, and the window is judged buffer-side (C-341).
    let (r, c) = almide_rt_matrix_dims(rows, cols);
    let need = r * c * 4;
    if !almide_rt_matrix_window_fits(data.len(), offset, need) {
        return None;
    }
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
    let off = offset as usize;
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
    Some(result.into())
}

pub fn almide_rt_matrix_from_bytes_f16_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> Option<AlmideMatrix> {
    // IEEE-754 half-precision → f32 → f64, inlined to keep the matrix.rs
    // file a single contiguous block of `pub fn almide_rt_matrix_*` functions
    // (simplifies the runtime-stripping logic in src/cli/mod.rs).
    // #1503: a negative dimension clamps to the empty matrix (the C-034 /
    // C-161 rule the constructors already follow) and a negative offset is
    // out of bounds by definition — the raw `as usize` casts turned either
    // into a multi-exabyte size and died in the forbidden T6 form (capacity
    // overflow, exit 101). `almide_rt_matrix_dims` also enforces the element
    // ceiling, and the window is judged buffer-side (C-341).
    let (r, c) = almide_rt_matrix_dims(rows, cols);
    let need = r * c * 2;
    if !almide_rt_matrix_window_fits(data.len(), offset, need) {
        return None;
    }
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
    let off = offset as usize;
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
    Some(result.into())
}

pub fn almide_rt_matrix_to_lists(m: &AlmideMatrix) -> Vec<Vec<f64>> {
    m.iter().map(|r| r.to_vec()).collect()
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


pub fn almide_rt_matrix_mul(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    // Dense matmul: BLAS if linked (have_blas → Accelerate cblas_dgemm, ~4.9x
    // beyond register tiling), else almide-kernel's register-tiled SIMD (5.23x
    // over Almide's tiled-scalar, used where BLAS isn't linked: wasm, etc.).
    // Either way it's the fastest available — BLAS for dense, almide-kernel for
    // BLAS's blind spots (quant/fused/attention/data-movement).
    // #3651: a 0-row operand's stored width is unobservable (`shape` and `cols`
    // report 0), so the kernel reads the observable width too. Its own
    // `mk(0, n)` result kept `cols = n`, and that hidden width then failed the
    // inner-dimension check below as a right operand — C-278 exempts empty
    // operands on every entry, and the wasm leg answered r×0.
    let m = a.rows;
    let k = if m == 0 { 0 } else { a.cols };
    let n = if b.rows == 0 { 0 } else { b.cols };
    if m == 0 || k == 0 || n == 0 {
        return mk(m, n, vec![0.0f64; m * n]);
    }
    // #2481: the inner dimension is a precondition, not a clamp — `b` was
    // indexed as if it had `k` rows (a raw slice panic past its data) while
    // the wasm body summed over `min(k, rows(b))` and printed a product.
    almide_rt_matrix_shape_eq(k, b.rows);
    let mut out = vec![0.0f64; m * n];
    #[cfg(have_blas)]
    {
        // cblas_dgemm, through sys.rs.
        almide_rt_sys_ffi::dgemm(m, n, k, &a.data, &b.data, &mut out);
    }
    #[cfg(not(have_blas))]
    {
        almide_kernel::matmul::matmul(&a.data, m, k, &b.data, n, &mut out);
    }
    mk(m, n, out)
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

pub fn almide_rt_matrix_map(m: &AlmideMatrix, f: std::rc::Rc<dyn Fn(f64) -> f64>) -> AlmideMatrix {
    let f = move |x| f(x);
    m.iter().map(|row| row.iter().map(|x| f(*x)).collect()).collect()
}

pub fn almide_rt_matrix_broadcast_add_row(m: &AlmideMatrix, bias: &[f64]) -> AlmideMatrix {
    m.iter().map(|row| row.iter().zip(bias.iter()).map(|(x, b)| x + b).collect()).collect()
}

pub fn almide_rt_matrix_split_cols_even(m: &AlmideMatrix, n: i64) -> Vec<AlmideMatrix> {
    // #2480: a part count of 0 or less is the EMPTY list, the same answer
    // `n == 0` has always had here and on the self-hosted leg (the C-034
    // signed-clamp reading: a negative count is no parts). Cast raw, -1
    // became 18446744073709551615 parts and the collect died in a raw
    // `capacity overflow` panic (exit 101) where wasm printed 0.
    if m.is_empty() || n <= 0 { return vec![].into(); }
    let n = n as usize;
    let cols = m[0].len();
    let chunk = cols / n;
    (0..n).map(|h| {
        let start = h * chunk;
        let end = start + chunk;
        m.iter().map(|row| row[start..end].to_vec()).collect::<Vec<Vec<f64>>>().into()
    }).collect()
}

pub fn almide_rt_matrix_concat_cols_many(matrices: &[AlmideMatrix]) -> AlmideMatrix {
    if matrices.is_empty() { return vec![].into(); }
    let rows = matrices[0].len();
    if rows == 0 { return vec![vec![]].into(); }
    // #2482: every NON-EMPTY member must have the first member's row count —
    // a member with fewer rows left the output ragged (the flat store's
    // assertion panic) while the wasm leg built the ragged value; a member
    // with more rows silently lost its tail on both. An EMPTY member
    // contributes no columns and no shape (C-278's empty-operand rule).
    for m in matrices {
        if !m.is_empty() {
            almide_rt_matrix_shape_eq(m.len(), rows);
        }
    }
    let total_cols: usize = matrices.iter().map(|m| if m.is_empty() { 0 } else { m[0].len() }).sum();
    (0..rows).map(|r| {
        let mut row = Vec::with_capacity(total_cols);
        for m in matrices {
            if !m.is_empty() {
                row.extend_from_slice(&m[r]);
            }
        }
        row
    }).collect()
}



pub fn almide_rt_matrix_slice_rows(m: &AlmideMatrix, start: i64, end: i64) -> AlmideMatrix {
    // `list.slice` over the rows: both bounds clamp into [0, len] — a
    // negative bound is 0 (#4147; it used to follow the `as usize` wrap, a
    // negative start empty and a negative end `len`) — and `start >= end` is
    // the 0-row matrix (#2475 — the self-hosted leg clamped only `end` and
    // read the list header as a row pointer).
    let len = m.len();
    let e = (end.max(0) as u64).min(len as u64) as usize;
    let s = (start.max(0) as u64).min(len as u64) as usize;
    if s >= e { return vec![].into(); }
    mk(e - s, m.cols, m.data[s * m.cols..e * m.cols].to_vec())
}


include!("matrix_p2.rs");
