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
