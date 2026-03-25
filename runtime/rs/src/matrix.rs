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

pub fn almide_rt_matrix_from_lists(rows: &Vec<Vec<f64>>) -> AlmideMatrix {
    rows.clone()
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
                let mut i = i0;
                while i < i1 {
                    let mut k = k0;
                    while k < k1 {
                        let a_ik = a_flat[i * n + k];
                        let mut j = j0;
                        while j < j1 {
                            c_flat[i * p + j] += a_ik * b_flat[k * p + j];
                            j += 1;
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

pub fn almide_rt_matrix_map(m: &AlmideMatrix, f: impl Fn(f64) -> f64) -> AlmideMatrix {
    m.iter().map(|row| row.iter().map(|x| f(*x)).collect()).collect()
}
