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
    #[inline]
    pub fn iter(&self) -> std::slice::Chunks<'_, f64> {
        self.data.chunks(self.cols.max(1))
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
        eprintln!("Error: matrix dimensions too large");
        std::process::exit(1);
    }
    (r as usize, c as usize)
}

/// Normalize a HEAD COUNT. A head count below 1 has no meaning — it is not a
/// degenerate case with an obvious answer the way a negative dimension is (that
/// clamps to the empty matrix, C-034) — so it aborts with the unified
/// `Error: <msg>` + exit 1 that `list.chunk` / `list.windows` / `int.clamp`
/// already use for their domain violations.
///
/// This exists because the family had THREE answers to the same question:
/// `almide_rt_matrix_rope_rotate_at` clamped with `.max(0)` (negative → no
/// heads), `almide_rt_matrix_rms_norm_heads` clamped with `.max(1)` (negative →
/// one head), and `almide_rt_matrix_mha_core` did not guard at all — it cast
/// straight to `usize`, so `n_heads = i32::MIN` became 18446744071562067968 and
/// `for h in 0..n_heads` hung the process (Wave 5 R2). The self-hosted wasm
/// bodies diverged from all three, dividing by a zero head count.
///
/// Every head-count-taking entry point goes through here, and
/// `matrix_head_count_domain_test.rs` is the matrix gate that keeps it that way.
pub fn almide_rt_matrix_head_count(n: i64) -> usize {
    if n < 1 {
        eprintln!("Error: head count must be positive");
        std::process::exit(1);
    }
    n as usize
}

/// #1419: the head GEOMETRY rule for the rope family — `n_heads * head_dim`
/// must fit the row. Exceeding it used to index straight past the row (a raw
/// slice panic natively, an OOB memory trap on the self-hosted leg: exit 101
/// vs 134, fuzz seed 508666777783 index 2120). Same abort family as the
/// head-count domain above; the division form cannot overflow on a huge head
/// count. An empty matrix has no row to violate and stays a no-op. The MHA /
/// rms_norm entries derive `head_dim = cols / n_heads` internally and cannot
/// express the mismatch — only the rope entries take both numbers, and
/// `matrix_head_count_domain_test.rs` keeps every one of them routed here.
pub fn almide_rt_matrix_head_geometry(n_heads_u: usize, head_dim_u: usize, rows: usize, cols: usize) {
    if rows > 0 && head_dim_u > 0 && n_heads_u > cols / head_dim_u {
        eprintln!("Error: head geometry exceeds row width");
        std::process::exit(1);
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

/// The INDEX-DOMAIN rule for the element accessor (#1423 night findings, C-282).
/// `matrix.get` is indexing — the matrix analogue of `xs[i]`, which aborts with
/// `Error: index out of bounds` (the `almide_index` macro) rather than inventing
/// a value. An out-of-range row/col used to be a RAW index on both legs: a Rust
/// slice panic (exit 101) natively against a wasm OOB trap (exit 134), and for a
/// NEGATIVE index the wasm leg did not even fail — `row as usize` wrapped and it
/// returned whatever heap bytes it landed on, exit 0 (the silent-wrong class,
/// found by extending the fuzzer's positive-OOB finding to the whole family).
/// Compared as i64 BEFORE any cast, so a negative can never wrap past the test.
#[inline]
pub fn almide_rt_matrix_bounds(idx: i64, extent: usize) {
    if idx < 0 || (idx as u64) >= extent as u64 {
        eprintln!("Error: matrix index out of bounds");
        std::process::exit(1);
    }
}

pub fn almide_rt_matrix_get(m: &AlmideMatrix, row: i64, col: i64) -> f64 {
    almide_rt_matrix_bounds(row, m.len());
    let r = &m[row as usize];
    // The COL extent is this row's own width, so a ragged matrix is exact.
    almide_rt_matrix_bounds(col, r.len());
    r[col as usize]
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
    rows.to_vec().into()
}

pub fn almide_rt_matrix_from_bytes_f32_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    // #1503: a negative dimension clamps to the empty matrix (the C-034 /
    // C-161 rule the constructors already follow) and a negative offset is
    // out of bounds by definition — the raw `as usize` casts turned either
    // into a multi-exabyte size and died in the forbidden T6 form (capacity
    // overflow, exit 101). `almide_rt_matrix_dims` also enforces the element
    // ceiling, and the bounds compare is overflow-safe in u128.
    let (r, c) = almide_rt_matrix_dims(rows, cols);
    let need = r * c * 4;
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
    if offset < 0 || (offset as u128) + (need as u128) > data.len() as u128 {
        for _ in 0..r { result.push(vec![0.0f64; c]); }
        return result.into();
    }
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
    result.into()
}

pub fn almide_rt_matrix_from_bytes_f16_le(data: &Vec<u8>, offset: i64, rows: i64, cols: i64) -> AlmideMatrix {
    // IEEE-754 half-precision → f32 → f64, inlined to keep the matrix.rs
    // file a single contiguous block of `pub fn almide_rt_matrix_*` functions
    // (simplifies the runtime-stripping logic in src/cli/mod.rs).
    // #1503: a negative dimension clamps to the empty matrix (the C-034 /
    // C-161 rule the constructors already follow) and a negative offset is
    // out of bounds by definition — the raw `as usize` casts turned either
    // into a multi-exabyte size and died in the forbidden T6 form (capacity
    // overflow, exit 101). `almide_rt_matrix_dims` also enforces the element
    // ceiling, and the bounds compare is overflow-safe in u128.
    let (r, c) = almide_rt_matrix_dims(rows, cols);
    let need = r * c * 2;
    let mut result: Vec<Vec<f64>> = Vec::with_capacity(r);
    if offset < 0 || (offset as u128) + (need as u128) > data.len() as u128 {
        for _ in 0..r { result.push(vec![0.0f64; c]); }
        return result.into();
    }
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
    result.into()
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

pub fn almide_rt_matrix_pow(m: &AlmideMatrix, exp: f64) -> AlmideMatrix {
    m.iter().map(|r| r.iter().map(|x| almide_rt_libm_pow(*x, exp)).collect()).collect()
}

pub fn almide_rt_matrix_mul(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    // Dense matmul: BLAS if linked (have_blas → Accelerate cblas_dgemm, ~4.9x
    // beyond register tiling), else almide-kernel's register-tiled SIMD (5.23x
    // over Almide's tiled-scalar, used where BLAS isn't linked: wasm, etc.).
    // Either way it's the fastest available — BLAS for dense, almide-kernel for
    // BLAS's blind spots (quant/fused/attention/data-movement).
    let m = a.rows;
    let k = a.cols;
    let n = b.cols;
    if m == 0 || k == 0 || n == 0 {
        return mk(m, n, vec![0.0f64; m * n]);
    }
    let mut out = vec![0.0f64; m * n];
    #[cfg(have_blas)]
    {
        // CblasRowMajor=101, CblasNoTrans=111. C = 1.0·A·B + 0.0·C.
        extern "C" {
            fn cblas_dgemm(
                order: i32, transa: i32, transb: i32, m: i32, n: i32, k: i32,
                alpha: f64, a: *const f64, lda: i32, b: *const f64, ldb: i32,
                beta: f64, c: *mut f64, ldc: i32,
            );
        }
        unsafe {
            cblas_dgemm(
                101, 111, 111, m as i32, n as i32, k as i32, 1.0,
                a.data.as_ptr(), k as i32, b.data.as_ptr(), n as i32,
                0.0, out.as_mut_ptr(), n as i32,
            );
        }
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
    // Routed to almide-kernel: SIMD fast-exp softmax (attention's core). exp is
    // the autovec wall; 2.74x over scalar libm. within-tolerance, each row sums
    // to 1 by construction. Flat data straight in.
    if m.rows == 0 || m.cols == 0 {
        return mk(m.rows, m.cols, vec![0.0f64; m.rows * m.cols]);
    }
    let mut out = vec![0.0f64; m.data.len()];
    almide_kernel::softmax::softmax_rows(&m.data, m.rows, m.cols, &mut out);
    mk(m.rows, m.cols, out)
}

pub fn almide_rt_matrix_gelu(m: &AlmideMatrix) -> AlmideMatrix {
    // SIMD tanh via the shared fast-exp (tanh(y)=1-2/(exp(2y)+1)): 7.94x over
    // scalar libm. within-tolerance. Flat data straight in.
    let mut out = vec![0.0f64; m.data.len()];
    almide_kernel::gelu::gelu(&m.data, &mut out);
    mk(m.rows, m.cols, out)
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

pub fn almide_rt_matrix_rms_norm_rows(
    m: &AlmideMatrix,
    gamma: &[f64],
    eps: f64,
) -> AlmideMatrix {
    m.iter().map(|row| {
        let n = row.len() as f64;
        let mut sq = 0.0;
        for &x in row { sq += x * x; }
        let inv = 1.0 / (sq / n + eps).sqrt();
        row.iter().zip(gamma.iter()).map(|(x, g)| x * inv * g).collect()
    }).collect()
}

pub fn almide_rt_matrix_swiglu_gate(
    x: &AlmideMatrix,
    w_gate: &AlmideMatrix,
    w_up: &AlmideMatrix,
) -> AlmideMatrix {
    if x.is_empty() || w_gate.is_empty() || w_up.is_empty() { return vec![].into(); }
    let r = x.len();
    let d_in = x[0].len();
    let d_out = w_gate.len();
    let mut out = vec![vec![0.0f64; d_out]; r];
    for i in 0..r {
        let xi = &x[i];
        for j in 0..d_out {
            let wg = &w_gate[j];
            let wu = &w_up[j];
            let mut g = 0.0;
            let mut u = 0.0;
            for k in 0..d_in {
                g += xi[k] * wg[k];
                u += xi[k] * wu[k];
            }
            let sig = 1.0 / (1.0 + almide_rt_libm_exp(-g));
            out[i][j] = g * sig * u;
        }
    }
    out.into()
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

pub fn almide_rt_matrix_scaled_dot_product_attention(
    q: &AlmideMatrix,
    kt: &AlmideMatrix,
    v: &AlmideMatrix,
    scale: f64,
) -> AlmideMatrix {
    let w = almide_rt_matrix_attention_weights(q, kt, scale);
    almide_rt_matrix_mul(&w, v)
}

pub fn almide_rt_matrix_split_cols_even(m: &AlmideMatrix, n: i64) -> Vec<AlmideMatrix> {
    let n = n as usize;
    if m.is_empty() || n == 0 { return vec![].into(); }
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
    let n_heads = almide_rt_matrix_head_count(n_heads);
    if q.is_empty() { return vec![].into(); }
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
                // causal mask: Q row i (new tokens) attends to K row j
                // if j is at-or-before the i-th new token's absolute
                // position. When sq == sk this reduces to j <= i; when
                // sq < sk (KV-cache gen step), the sk - sq past rows are
                // always visible.
                if causal && j > (sk - sq) + i {
                    scores[i][j] += -1.0e9;
                }
            }
        }

        // Row-wise softmax
        for row in scores.iter_mut() {
            let mut max = f64::NEG_INFINITY;
            for &x in row.iter() { if x > max { max = x; } }
            let mut sum = 0.0;
            for x in row.iter_mut() { *x = almide_rt_libm_exp(*x - max); sum += *x; }
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

    out.into()
}

pub fn almide_rt_matrix_linear_row(x: &AlmideMatrix, weight: &AlmideMatrix, bias: &[f64]) -> AlmideMatrix {
    // y[i][j] = sum_k x[i][k] * weight[j][k] + bias[j]
    if x.is_empty() || weight.is_empty() { return vec![].into(); }
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
    out.into()
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
    if x.is_empty() || weight.is_empty() { return vec![].into(); }
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
    out.into()
}

pub fn almide_rt_matrix_slice_rows(m: &AlmideMatrix, start: i64, end: i64) -> AlmideMatrix {
    let s = start as usize;
    let e = (end as usize).min(m.len());
    if s >= e { return vec![].into(); }
    mk(e - s, m.cols, m.data[s * m.cols..e * m.cols].to_vec())
}

pub fn almide_rt_matrix_conv1d(input: &AlmideMatrix, weight: &AlmideMatrix, bias: &[f64], kernel: i64, stride: i64, padding: i64) -> AlmideMatrix {
    // input: (T, in_ch). weight: (out_ch, in_ch * kernel). bias: (out_ch,).
    // Output: (T_out, out_ch) where T_out = floor((T + 2P - K) / S) + 1.
    // Weight layout within a row: for c in 0..in_ch, for k in 0..kernel: weight[o][c*kernel + k].
    let t_in = input.len();
    if t_in == 0 || weight.is_empty() { return vec![].into(); }
    let in_ch = input[0].len();
    let out_ch = weight.len();
    let k = kernel as usize;
    let s = stride as usize;
    let p = padding as usize;
    let t_padded = t_in + 2 * p;
    if t_padded < k { return vec![].into(); }
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
    out.into()
}

include!("matrix_p2.rs");
