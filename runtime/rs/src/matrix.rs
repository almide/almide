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

pub fn almide_rt_matrix_mul(a: &AlmideMatrix, b: &AlmideMatrix) -> AlmideMatrix {
    let rows_a = a.len();
    let cols_a = if a.is_empty() { 0 } else { a[0].len() };
    let cols_b = if b.is_empty() { 0 } else { b[0].len() };
    let mut result = vec![vec![0.0; cols_b]; rows_a];
    for i in 0..rows_a {
        for j in 0..cols_b {
            let mut sum = 0.0;
            for k in 0..cols_a {
                sum += a[i][k] * b[k][j];
            }
            result[i][j] = sum;
        }
    }
    result
}

pub fn almide_rt_matrix_scale(m: &AlmideMatrix, s: f64) -> AlmideMatrix {
    m.iter().map(|row| row.iter().map(|x| x * s).collect()).collect()
}

pub fn almide_rt_matrix_map(m: &AlmideMatrix, f: impl Fn(f64) -> f64) -> AlmideMatrix {
    m.iter().map(|row| row.iter().map(|x| f(*x)).collect()).collect()
}
