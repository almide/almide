
pub fn almide_rt_matrix_map(m: &AlmideMatrix, f: impl Fn(f64) -> f64) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            let out: Vec<f64> = data.iter().map(|&x| f(x)).collect();
            AlmideMatrix::Small { rows: *rows, cols: *cols, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            let out: Vec<f32> = data.iter().map(|&x| f(x as f64) as f32).collect();
            AlmideMatrix::SmallF32 { rows: *rows, cols: *cols, data: out }
        }
        AlmideMatrix::Burn(t) => {
            let [r, c] = t.dims();
            let data: Vec<f64> = t.clone().to_data().to_vec().unwrap();
            let mapped: Vec<f64> = data.into_iter().map(|x| f(x)).collect();
            wrap(Tensor::from_data(TensorData::new(mapped, [r, c]), &dev()))
        }
    }
}

pub fn almide_rt_matrix_broadcast_add_row(m: &AlmideMatrix, bias: &[f64]) -> AlmideMatrix {
    match m {
        AlmideMatrix::Small { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out: Vec<f64> = Vec::with_capacity(r * c);
            unsafe {
                let (src, dst, bp) = (data.as_ptr(), out.as_mut_ptr(), bias.as_ptr());
                for i in 0..r {
                    let base = i * c;
                    for j in 0..c {
                        *dst.add(base + j) = *src.add(base + j) + *bp.add(j);
                    }
                }
                out.set_len(r * c);
            }
            AlmideMatrix::Small { rows: r, cols: c, data: out }
        }
        AlmideMatrix::SmallF32 { rows, cols, data } => {
            let (r, c) = (*rows, *cols);
            let mut out: Vec<f32> = Vec::with_capacity(r * c);
            unsafe {
                let (src, dst, bp) = (data.as_ptr(), out.as_mut_ptr(), bias.as_ptr());
                for i in 0..r {
                    let base = i * c;
                    for j in 0..c {
                        *dst.add(base + j) = *src.add(base + j) + (*bp.add(j) as f32);
                    }
                }
                out.set_len(r * c);
            }
            AlmideMatrix::SmallF32 { rows: r, cols: c, data: out }
        }
        AlmideMatrix::Burn(_) => {
            let bias_t: Tensor<B, 2> = Tensor::from_data(
                TensorData::new(bias.to_vec(), [1, bias.len()]), &dev());
            wrap(m.to_burn().add(bias_t))
        }
    }
}

