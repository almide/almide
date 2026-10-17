

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
